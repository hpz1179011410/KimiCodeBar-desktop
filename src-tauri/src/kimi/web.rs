//! 网页端月度用量：POST www.kimi.com GetSubscriptionStats（kimi-auth cookie 认证）。
//!
//! - 只解析 subscriptionBalance，忽略 ratelimitCode5h/7d；
//! - ratio ≤1.0 视为小数 ×100，>1 原样视为百分数；负数/非有限视为缺失；
//! - token 为三段 JWT 且 payload 含非空 device_id/ssid/sub 时才附加 x-msh-* / x-traffic-id 头；
//! - 日志纪律：不记录 token 和响应原文。

use serde::Serialize;
use serde_json::Value;
use std::time::Duration;

use super::USER_AGENT;

pub const SUBSCRIPTION_STATS_URL: &str =
    "https://www.kimi.com/apiv2/kimi.gateway.membership.v2.MembershipService/GetSubscriptionStats";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const NORMALIZE_ERR: &str = "无法识别的 token 格式，请直接粘贴 kimi-auth 的值";

#[derive(Debug, thiserror::Error)]
pub enum WebError {
    #[error("网页登录态无效或已过期 (HTTP {0})")]
    Unauthorized(u16),
    #[error("网络错误: {0}")]
    Http(String),
    #[error("月度数据解析失败: {0}")]
    Parse(String),
}

#[derive(Debug, Clone, Serialize)]
pub struct MonthlyInfo {
    /// 月度总额度已用百分比
    pub total_pct: f64,
    /// 其中 Kimi（网页/客户端）占用百分比
    pub kimi_pct: f64,
    /// 其中 Kimi Code 占用百分比
    pub code_pct: f64,
    /// expireTime 原样透传
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset_time: Option<String>,
}

/// 规范化用户粘贴的 token：
/// 去首尾空白 → 剥成对引号 → 剥 Authorization: 前缀（大小写不敏感）
/// → 剥 Bearer 前缀（仅当后跟空白，可叠加）→ 若含 kimi-auth=（大小写不敏感）提取其值。
/// 拒绝：空、含内部空白/换行、含 `;` 但找不到 kimi-auth、kimi-auth 值为空。
pub fn normalize_web_token(input: &str) -> Result<String, String> {
    let mut s = input.trim();

    // 剥成对引号
    s = strip_paired_quotes(s).trim();

    // 剥 Authorization: 前缀（大小写不敏感）
    const AUTH_PREFIX: &str = "authorization:";
    if s.get(..AUTH_PREFIX.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(AUTH_PREFIX))
    {
        s = s[AUTH_PREFIX.len()..].trim_start();
    }

    // 剥 Bearer 前缀（仅当后跟空白，可叠加）
    while s.len() > "Bearer".len()
        && s.starts_with("Bearer")
        && s.as_bytes()["Bearer".len()].is_ascii_whitespace()
    {
        s = s["Bearer".len()..].trim_start();
    }

    // 若含 kimi-auth=（大小写不敏感）：提取其值到 `;` 或结尾并去引号
    if let Some(idx) = find_ascii_case_insensitive(s, "kimi-auth=") {
        let rest = &s[idx + "kimi-auth=".len()..];
        let end = rest.find(';').unwrap_or(rest.len());
        let value = strip_paired_quotes(rest[..end].trim()).trim();
        if value.is_empty() {
            return Err(NORMALIZE_ERR.into());
        }
        s = value;
    } else if s.contains(';') {
        // 看起来像 cookie 串但找不到 kimi-auth
        return Err(NORMALIZE_ERR.into());
    }

    if s.is_empty() || s.chars().any(|c| c.is_whitespace()) {
        return Err(NORMALIZE_ERR.into());
    }
    Ok(s.to_string())
}

fn strip_paired_quotes(s: &str) -> &str {
    if s.len() >= 2 {
        let bytes = s.as_bytes();
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return &s[1..s.len() - 1];
        }
    }
    s
}

/// 大小写不敏感的 ASCII 子串查找，返回字节下标。
fn find_ascii_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    if needle.len() > haystack.len() {
        return None;
    }
    haystack
        .as_bytes()
        .windows(needle.len())
        .position(|w| w.eq_ignore_ascii_case(needle.as_bytes()))
}

/// 手写 base64url 解码：容忍缺失的 `=` 填充；非法字符返回 None。
fn base64url_decode(input: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let mut acc: u32 = 0;
    let mut nbits: u32 = 0;
    for &b in input.as_bytes() {
        let v = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            b'=' => continue,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(v);
        nbits += 6;
        if nbits >= 8 {
            nbits -= 8;
            out.push((acc >> nbits) as u8);
        }
    }
    Some(out)
}

/// token 是三段 JWT 且 payload 同时含非空 device_id/ssid/sub 时返回
/// (device_id, ssid, sub)，否则 None（缺任一即全省略）。
pub fn jwt_identity_headers(token: &str) -> Option<(String, String, String)> {
    let mut parts = token.split('.');
    let (Some(_header), Some(payload), Some(signature)) =
        (parts.next(), parts.next(), parts.next())
    else {
        return None;
    };
    if parts.next().is_some() || signature.is_empty() {
        return None;
    }
    let payload: Value = serde_json::from_slice(&base64url_decode(payload)?).ok()?;
    let get = |key: &str| {
        payload
            .get(key)
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    Some((get("device_id")?, get("ssid")?, get("sub")?))
}

/// 拉取月度用量。
pub async fn fetch_subscription_stats(
    client: &reqwest::Client,
    token: &str,
) -> Result<MonthlyInfo, WebError> {
    let mut req = client
        .post(SUBSCRIPTION_STATS_URL)
        .timeout(REQUEST_TIMEOUT)
        .header(reqwest::header::AUTHORIZATION, format!("Bearer {token}"))
        .header(reqwest::header::COOKIE, format!("kimi-auth={token}"))
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header(reqwest::header::ACCEPT, "application/json")
        .header(reqwest::header::ORIGIN, "https://www.kimi.com")
        .header(
            reqwest::header::REFERER,
            "https://www.kimi.com/code/console",
        )
        .header("connect-protocol-version", "1")
        .header("x-msh-platform", "web")
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .body("{}");

    // 条件附加头：仅当 JWT payload 同时含 device_id/ssid/sub
    if let Some((device_id, ssid, sub)) = jwt_identity_headers(token) {
        req = req
            .header("x-msh-device-id", device_id)
            .header("x-msh-session-id", ssid)
            .header("x-traffic-id", sub);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| WebError::Http(e.to_string()))?;
    let status = resp.status().as_u16();
    if status == 401 || status == 403 {
        return Err(WebError::Unauthorized(status));
    }
    if !(200..300).contains(&status) {
        return Err(WebError::Http(format!("HTTP {status}")));
    }
    let body = resp
        .text()
        .await
        .map_err(|e| WebError::Http(e.to_string()))?;
    parse_subscription_stats(&body)
}

/// 解析 GetSubscriptionStats 响应体。
pub fn parse_subscription_stats(body: &str) -> Result<MonthlyInfo, WebError> {
    let root: Value = serde_json::from_str(body).map_err(|e| WebError::Parse(e.to_string()))?;
    // 顶层有 data 对象则下钻一层
    let root = match root.get("data").filter(|d| d.is_object()) {
        Some(data) => data,
        None => &root,
    };

    let balance = root
        .get("subscriptionBalance")
        .or_else(|| root.get("subscription_balance"))
        .and_then(|v| v.as_object())
        .ok_or_else(|| WebError::Parse("缺少 subscriptionBalance".to_string()))?;

    // feature / type 必须可信（缺失放行，不匹配报错）
    if let Some(feature) = balance.get("feature").and_then(|v| v.as_str()) {
        if feature != "FEATURE_OMNI" {
            return Err(WebError::Parse(format!("非预期的 feature: {feature}")));
        }
    }
    if let Some(ty) = balance.get("type").and_then(|v| v.as_str()) {
        if ty != "SUBSCRIPTION" {
            return Err(WebError::Parse(format!("非预期的 type: {ty}")));
        }
    }

    let total_pct = ratio_pct(
        balance,
        &[
            "amountUsedRatio",
            "amount_used_ratio",
            "usedRatio",
            "used_ratio",
        ],
    )
    .ok_or_else(|| WebError::Parse("缺少 amountUsedRatio".to_string()))?;
    let code_pct =
        ratio_pct(balance, &["kimiCodeUsedRatio", "kimi_code_used_ratio"]).unwrap_or(0.0);
    let kimi_pct = (total_pct - code_pct).max(0.0);
    let reset_time = balance
        .get("expireTime")
        .or_else(|| balance.get("expire_time"))
        .and_then(|v| v.as_str())
        .map(str::to_string);

    Ok(MonthlyInfo {
        total_pct,
        kimi_pct,
        code_pct,
        reset_time,
    })
}

/// ratio 解析：数字或数字字符串均可；≤1.0 视为小数 ×100，>1 视为百分数原样；
/// 负数/非有限值视为缺失（继续尝试下一个别名）。
fn ratio_pct(obj: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<f64> {
    for key in keys {
        let Some(value) = obj.get(*key) else {
            continue;
        };
        let raw = match value {
            Value::Number(n) => n.as_f64(),
            Value::String(s) => s.trim().parse::<f64>().ok(),
            _ => None,
        };
        if let Some(raw) = raw {
            if raw.is_finite() && raw >= 0.0 {
                return Some(if raw <= 1.0 { raw * 100.0 } else { raw });
            }
        }
    }
    None
}
