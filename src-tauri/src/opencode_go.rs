//! OpenCode Go 订阅配额：抓取已登录的 Workspace Go 页面并解析 SSR 数据。
//!
//! OpenCode Go 目前没有可用 API Key 查询的公开配额接口；控制台页面会在
//! SolidJS SSR hydration 数据里输出 rollingUsage / weeklyUsage / monthlyUsage。
//! 本模块只请求用户明确配置的 Workspace 页面，不记录 Cookie 与响应原文。

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::Serialize;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::oneshot;

use crate::kimi::USER_AGENT;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const DASHBOARD_ORIGIN: &str = "https://opencode.ai";
const ECB_DAILY_RATES_URL: &str = "https://www.ecb.europa.eu/stats/eurofxref/eurofxref-daily.xml";
const EXCHANGE_RATE_CACHE_TTL: Duration = Duration::from_secs(6 * 60 * 60);
const EXCHANGE_RATE_INITIAL_WAIT: Duration = Duration::from_secs(2);
const EXCHANGE_RATE_RETRY_DELAY: Duration = Duration::from_secs(5 * 60);
const FIVE_HOUR_LIMIT_USD: f64 = 12.0;
const WEEKLY_LIMIT_USD: f64 = 30.0;
const MONTHLY_LIMIT_USD: f64 = 60.0;

#[derive(Debug, thiserror::Error)]
pub enum OpenCodeGoError {
    #[error("OpenCode Go 登录态无效或已过期")]
    Unauthorized,
    #[error("OpenCode Go 网络错误: {0}")]
    Http(String),
    #[error("OpenCode Go 配额解析失败: {0}")]
    Parse(String),
}

#[derive(Debug, Clone, Serialize)]
pub struct OpenCodeGoWindow {
    /// 套餐窗口额度（美元计价）。
    pub limit_usd: f64,
    /// 按控制台百分比折算的已用美元值。
    pub used_usd: f64,
    /// 控制台报告的已用百分比（0-100）。
    pub used_percent: f64,
    /// 剩余百分比（0-1），与 Kimi QuotaWindow 保持一致。
    pub remaining_percent: f64,
    /// 按 resetInSec 换算的 RFC3339 时间。
    pub reset_time: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpenCodeGoUsage {
    pub five_hour: Option<OpenCodeGoWindow>,
    pub weekly: Option<OpenCodeGoWindow>,
    pub monthly: Option<OpenCodeGoWindow>,
    pub fetched_at: String,
    pub low_warning: bool,
    pub exchange_rate: Option<UsdCnyExchangeRate>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UsdCnyExchangeRate {
    /// 1 美元对应的人民币参考值。
    pub usd_cny: f64,
    /// 欧洲央行参考汇率日期（YYYY-MM-DD，周末通常沿用最近工作日）。
    pub reference_date: String,
}

#[derive(Debug, Default)]
pub struct ExchangeRateCache {
    value: Option<UsdCnyExchangeRate>,
    checked_at: Option<Instant>,
    last_attempt_at: Option<Instant>,
    refreshing: bool,
}

/// 接受原始 Workspace ID，也接受完整的 `/workspace/{id}/go` 页面 URL。
pub fn normalize_workspace_id(input: &str) -> Result<String, String> {
    let input = input.trim();
    let candidate = if let Some(index) = input.find("/workspace/") {
        input[index + "/workspace/".len()..]
            .split('/')
            .next()
            .unwrap_or_default()
    } else {
        input
    };
    if !candidate.starts_with("wrk_")
        || candidate.len() <= "wrk_".len()
        || !candidate
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err("Workspace ID 格式无效，应为 wrk_… 或完整的 Go 页面 URL".into());
    }
    Ok(candidate.to_string())
}

/// 接受 auth 原始值、`auth=...` 或完整 Cookie 请求头。
pub fn normalize_auth_cookie(input: &str) -> Result<String, String> {
    let mut input = input.trim();
    if input.len() >= 2 {
        let bytes = input.as_bytes();
        if (bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\'')
        {
            input = input[1..input.len() - 1].trim();
        }
    }
    if input
        .get(.."cookie:".len())
        .is_some_and(|head| head.eq_ignore_ascii_case("cookie:"))
    {
        input = input["cookie:".len()..].trim_start();
    }

    let mut value = None;
    for part in input.split(';') {
        let part = part.trim();
        if let Some((name, candidate)) = part.split_once('=') {
            if name.trim().eq_ignore_ascii_case("auth") {
                value = Some(candidate.trim());
                break;
            }
        }
    }
    let value = value.unwrap_or(input).trim();
    if value.is_empty()
        || value.chars().any(|c| c.is_whitespace() || c.is_control())
        || value.contains(';')
    {
        return Err("无法识别 auth Cookie，请粘贴 auth 的值".into());
    }
    Ok(value.to_string())
}

fn extract_number(segment: &str, key: &str) -> Option<f64> {
    let start = segment.find(key)? + key.len();
    let tail = segment[start..].trim_start();
    let tail = tail.strip_prefix(':')?.trim_start();
    let end = tail
        .find(|c: char| !(c.is_ascii_digit() || matches!(c, '-' | '+' | '.' | 'e' | 'E')))
        .unwrap_or(tail.len());
    let number = tail[..end].parse::<f64>().ok()?;
    number.is_finite().then_some(number)
}

fn parse_window(
    html: &str,
    name: &str,
    limit_usd: f64,
    now: DateTime<Utc>,
) -> Option<OpenCodeGoWindow> {
    let mut search_from = 0;
    while let Some(relative_index) = html[search_from..].find(name) {
        let name_index = search_from + relative_index;
        let value_tail = html[name_index + name.len()..].trim_start();
        let Some(object_tail) = value_tail.strip_prefix(':') else {
            search_from = name_index + name.len();
            continue;
        };
        if let Some(object_start) = object_tail.find('{') {
            let reference = &object_tail[..object_start];
            if reference.chars().any(|c| {
                !(c.is_ascii_alphanumeric()
                    || c.is_ascii_whitespace()
                    || matches!(c, '$' | '[' | ']' | '_' | '-' | '.' | '='))
            }) {
                search_from = name_index + name.len();
                continue;
            }
            let object_tail = &object_tail[object_start + 1..];
            if let Some(object_end) = object_tail.find('}') {
                let object = &object_tail[..object_end];
                if let (Some(used_percent), Some(reset_seconds)) = (
                    extract_number(object, "usagePercent"),
                    extract_number(object, "resetInSec"),
                ) {
                    let used_percent = used_percent.clamp(0.0, 100.0);
                    let reset_seconds = reset_seconds.max(0.0).round() as i64;
                    let reset_time = now
                        .checked_add_signed(ChronoDuration::seconds(reset_seconds))
                        .unwrap_or(now)
                        .to_rfc3339();
                    return Some(OpenCodeGoWindow {
                        limit_usd,
                        used_usd: limit_usd * used_percent / 100.0,
                        used_percent,
                        remaining_percent: (100.0 - used_percent) / 100.0,
                        reset_time,
                    });
                }
            }
        }
        search_from = name_index + name.len();
    }
    None
}

pub fn parse_dashboard_at(
    html: &str,
    now: DateTime<Utc>,
) -> Result<OpenCodeGoUsage, OpenCodeGoError> {
    let five_hour = parse_window(html, "rollingUsage", FIVE_HOUR_LIMIT_USD, now);
    let weekly = parse_window(html, "weeklyUsage", WEEKLY_LIMIT_USD, now);
    let monthly = parse_window(html, "monthlyUsage", MONTHLY_LIMIT_USD, now);
    if five_hour.is_none() && weekly.is_none() && monthly.is_none() {
        return Err(OpenCodeGoError::Parse(
            "页面中未找到 rollingUsage / weeklyUsage / monthlyUsage".into(),
        ));
    }
    let low_warning = [&five_hour, &weekly, &monthly]
        .into_iter()
        .flatten()
        .any(|window| window.remaining_percent < 0.2);
    Ok(OpenCodeGoUsage {
        five_hour,
        weekly,
        monthly,
        fetched_at: now.to_rfc3339(),
        low_warning,
        exchange_rate: None,
    })
}

pub fn parse_dashboard(html: &str) -> Result<OpenCodeGoUsage, OpenCodeGoError> {
    parse_dashboard_at(html, Utc::now())
}

pub async fn fetch_usage(
    client: &reqwest::Client,
    workspace_id: &str,
    auth_cookie: &str,
) -> Result<OpenCodeGoUsage, OpenCodeGoError> {
    let workspace_id = normalize_workspace_id(workspace_id).map_err(OpenCodeGoError::Parse)?;
    let auth_cookie = normalize_auth_cookie(auth_cookie).map_err(OpenCodeGoError::Parse)?;
    let expected_path = format!("/workspace/{workspace_id}/go");
    let url = format!("{DASHBOARD_ORIGIN}{expected_path}");
    let response = client
        .get(&url)
        .timeout(REQUEST_TIMEOUT)
        .header(reqwest::header::COOKIE, format!("auth={auth_cookie}"))
        .header(reqwest::header::ACCEPT, "text/html")
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .send()
        .await
        .map_err(|e| OpenCodeGoError::Http(e.to_string()))?;

    let status = response.status().as_u16();
    if status == 401 || status == 403 {
        return Err(OpenCodeGoError::Unauthorized);
    }
    if !(200..300).contains(&status) {
        return Err(OpenCodeGoError::Http(format!("HTTP {status}")));
    }
    if response.url().path() != expected_path {
        return Err(OpenCodeGoError::Unauthorized);
    }
    let html = response
        .text()
        .await
        .map_err(|e| OpenCodeGoError::Http(e.to_string()))?;
    parse_dashboard(&html)
}

fn xml_attribute<'a>(segment: &'a str, name: &str) -> Option<&'a str> {
    for quote in ['\'', '"'] {
        let marker = format!("{name}={quote}");
        if let Some(start) = segment.find(&marker) {
            let value = &segment[start + marker.len()..];
            if let Some(end) = value.find(quote) {
                return Some(&value[..end]);
            }
        }
    }
    None
}

/// 欧洲央行以 EUR 为基准同时给出 USD、CNY，交叉相除得到 CNY/USD。
pub fn parse_ecb_exchange_rate(xml: &str) -> Result<UsdCnyExchangeRate, String> {
    let mut reference_date = None;
    let mut usd_per_eur = None;
    let mut cny_per_eur = None;

    for segment in xml.split('<') {
        if reference_date.is_none() {
            reference_date = xml_attribute(segment, "time").map(str::to_string);
        }
        let Some(currency) = xml_attribute(segment, "currency") else {
            continue;
        };
        let rate = xml_attribute(segment, "rate").and_then(|value| value.parse::<f64>().ok());
        match currency {
            "USD" => usd_per_eur = rate,
            "CNY" => cny_per_eur = rate,
            _ => {}
        }
    }

    let reference_date = reference_date.ok_or_else(|| "欧洲央行汇率缺少日期".to_string())?;
    let usd_per_eur = usd_per_eur
        .filter(|value| value.is_finite() && *value > 0.0)
        .ok_or_else(|| "欧洲央行汇率缺少有效 USD 值".to_string())?;
    let cny_per_eur = cny_per_eur
        .filter(|value| value.is_finite() && *value > 0.0)
        .ok_or_else(|| "欧洲央行汇率缺少有效 CNY 值".to_string())?;
    let usd_cny = cny_per_eur / usd_per_eur;
    if !usd_cny.is_finite() || usd_cny <= 0.0 {
        return Err("欧洲央行 USD/CNY 交叉汇率无效".into());
    }

    Ok(UsdCnyExchangeRate {
        usd_cny,
        reference_date,
    })
}

async fn fetch_ecb_exchange_rate(client: &reqwest::Client) -> Result<UsdCnyExchangeRate, String> {
    let response = client
        .get(ECB_DAILY_RATES_URL)
        .timeout(REQUEST_TIMEOUT)
        .header(reqwest::header::ACCEPT, "application/xml,text/xml")
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(format!("欧洲央行汇率请求失败: HTTP {status}"));
    }
    let xml = response.text().await.map_err(|e| e.to_string())?;
    parse_ecb_exchange_rate(&xml)
}

async fn get_exchange_rate(
    client: &reqwest::Client,
    cache: &Arc<Mutex<ExchangeRateCache>>,
) -> Option<UsdCnyExchangeRate> {
    let cached = {
        let mut guard = cache.lock().ok()?;
        if let (Some(value), Some(checked_at)) = (&guard.value, guard.checked_at) {
            if checked_at.elapsed() < EXCHANGE_RATE_CACHE_TTL {
                return Some(value.clone());
            }
        }
        let cached = guard.value.clone();
        if guard.refreshing {
            return cached;
        }
        if guard
            .last_attempt_at
            .is_some_and(|attempt| attempt.elapsed() < EXCHANGE_RATE_RETRY_DELAY)
        {
            return cached;
        }
        guard.refreshing = true;
        guard.last_attempt_at = Some(Instant::now());
        cached
    };

    let (sender, receiver) = oneshot::channel();
    let client = client.clone();
    let cache = Arc::clone(cache);
    tokio::spawn(async move {
        let fetched = fetch_ecb_exchange_rate(&client).await.ok();
        if let Ok(mut guard) = cache.lock() {
            if let Some(value) = fetched.as_ref() {
                guard.value = Some(value.clone());
                guard.checked_at = Some(Instant::now());
            }
            guard.refreshing = false;
        }
        let _ = sender.send(fetched);
    });

    if cached.is_some() {
        return cached;
    }

    match tokio::time::timeout(EXCHANGE_RATE_INITIAL_WAIT, receiver).await {
        Ok(Ok(value)) => value,
        _ => None,
    }
}

/// OpenCode 额度与每日参考汇率并行获取；汇率失败不影响美元额度。
pub async fn fetch_usage_with_exchange_rate(
    client: &reqwest::Client,
    workspace_id: &str,
    auth_cookie: &str,
    cache: &Arc<Mutex<ExchangeRateCache>>,
) -> Result<OpenCodeGoUsage, OpenCodeGoError> {
    let (usage, exchange_rate) = tokio::join!(
        fetch_usage(client, workspace_id, auth_cookie),
        get_exchange_rate(client, cache)
    );
    let mut usage = usage?;
    usage.exchange_rate = exchange_rate;
    Ok(usage)
}
