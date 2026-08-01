//! 配额解析：把 wire 结构解析为领域结构 QuotaInfo。
//!
//! 规则：
//! - 周窗口 = 顶层 usage；5 小时窗口 = limits[] 中 window.duration == 300
//! - remaining 一律用 limit - used 反推（不信任两端不一致数据）
//! - totalQuota.used = limit - remaining 反推
//! - booster：status ∈ {STATUS_ACTIVE, STATUS_ENABLED} 才启用；未启用余额显示 0；
//!   amountLeft 单位 1e-8 元；priceInCents/100 = 元
//! - 低额告警：任一窗口剩余 < 20%

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::kimi::models::{BoosterWalletWire, UsagesResponse};

pub const LOW_WARNING_THRESHOLD: f64 = 0.2;
const BOOSTER_ENABLED_STATUSES: [&str; 2] = ["STATUS_ACTIVE", "STATUS_ENABLED"];
const FIVE_HOUR_WINDOW_MINUTES: i64 = 300;

#[derive(Debug, Clone, Serialize)]
pub struct QuotaWindow {
    pub limit: i64,
    pub used: i64,
    pub remaining: i64,
    /// remaining / limit（limit 为 0 时为 0）
    pub remaining_percent: f64,
    /// RFC3339
    pub reset_time: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BoosterInfo {
    pub enabled: bool,
    /// 余额（元），未启用时为 0
    pub amount_left_yuan: f64,
    /// 价格（元）
    pub price_yuan: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct QuotaInfo {
    pub weekly: Option<QuotaWindow>,
    pub five_hour: Option<QuotaWindow>,
    pub total: Option<QuotaWindow>,
    pub booster: Option<BoosterInfo>,
    pub membership_level: Option<String>,
    /// 本次获取时间（RFC3339）
    pub fetched_at: String,
    pub low_warning: bool,
}

impl QuotaInfo {
    /// 任一窗口剩余 < 20%（limit>0 才参与判断）。
    pub fn needs_low_warning(&self) -> bool {
        [&self.weekly, &self.five_hour, &self.total]
            .into_iter()
            .flatten()
            .any(|w| w.limit > 0 && w.remaining_percent < LOW_WARNING_THRESHOLD)
    }
}

/// proto3 字符串数字解析：容忍首尾空白与浮点写法。
fn parse_num(value: &Option<String>) -> Option<i64> {
    let s = value.as_deref()?.trim();
    if s.is_empty() {
        return None;
    }
    s.parse::<i64>()
        .ok()
        .or_else(|| s.parse::<f64>().ok().map(|f| f as i64))
}

/// resetTime 用 RFC3339 解析，统一输出 UTC RFC3339；解析失败为 None。
fn parse_reset_time(value: &Option<String>) -> Option<String> {
    let s = value.as_deref()?;
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.with_timezone(&Utc).to_rfc3339())
}

/// limit + used → 窗口，remaining 用 limit - used 反推。
fn window_from_limit_used(
    limit: Option<i64>,
    used: Option<i64>,
    reset_time: Option<String>,
) -> Option<QuotaWindow> {
    let limit = limit?;
    let used = used.unwrap_or(0);
    let remaining = (limit - used).max(0);
    let remaining_percent = if limit > 0 {
        remaining as f64 / limit as f64
    } else {
        0.0
    };
    Some(QuotaWindow {
        limit,
        used,
        remaining,
        remaining_percent,
        reset_time,
    })
}

fn parse_booster(wire: &BoosterWalletWire) -> BoosterInfo {
    let enabled = wire
        .status
        .as_deref()
        .map(|s| BOOSTER_ENABLED_STATUSES.contains(&s))
        .unwrap_or(false);
    // 余额：balance.amountLeft（嵌套，真实结构）优先，顶层 amountLeft 兜底；未启用显示 0
    let amount_left_raw = wire
        .balance
        .as_ref()
        .and_then(|b| b.amount_left.as_ref())
        .or(wire.amount_left.as_ref());
    let amount_left_yuan = if enabled {
        parse_num(&amount_left_raw.cloned())
            .map(|v| v as f64 / 1e8)
            .unwrap_or(0.0)
    } else {
        0.0
    };
    // 月度消费上限（分→元）；顶层 priceInCents 兜底
    let price_yuan = wire
        .monthly_charge_limit
        .as_ref()
        .and_then(|m| parse_num(&m.price_in_cents))
        .or_else(|| parse_num(&wire.price_in_cents))
        .map(|v| v as f64 / 100.0);
    BoosterInfo {
        enabled,
        amount_left_yuan,
        price_yuan,
    }
}

/// 会员等级：优先 user.membership.level（真实结构），回退顶层 membership
/// （兼容 "pro" 或 { "level": "pro" } 两种旧形态）。
fn parse_membership_level(wire: &UsagesResponse) -> Option<String> {
    if let Some(level) = wire
        .user
        .as_ref()
        .and_then(|u| u.membership.as_ref())
        .and_then(|m| m.level.as_deref())
    {
        return Some(level.to_string());
    }
    let v = wire.membership.as_ref()?;
    if let Some(s) = v.as_str() {
        return Some(s.to_string());
    }
    v.get("level")
        .and_then(|l| l.as_str())
        .map(|s| s.to_string())
}

pub fn parse_usage(wire: &UsagesResponse) -> QuotaInfo {
    let weekly = wire.usage.as_ref().and_then(|w| {
        window_from_limit_used(
            parse_num(&w.limit),
            parse_num(&w.used),
            parse_reset_time(&w.reset_time),
        )
    });

    let five_hour = wire
        .limits
        .iter()
        .find(|l| {
            l.window.as_ref().and_then(|w| w.duration_minutes()) == Some(FIVE_HOUR_WINDOW_MINUTES)
        })
        .and_then(|l| {
            // 真实响应明细嵌套在 detail 里；顶层平铺字段作兜底
            let (limit, used, reset_time) = match &l.detail {
                Some(d) => (&d.limit, &d.used, &d.reset_time),
                None => (&l.limit, &l.used, &l.reset_time),
            };
            window_from_limit_used(
                parse_num(limit),
                parse_num(used),
                parse_reset_time(reset_time),
            )
        });

    // totalQuota.used = limit - remaining 反推
    let total = wire.total_quota.as_ref().and_then(|t| {
        let limit = parse_num(&t.limit)?;
        let remaining_wire = parse_num(&t.remaining).unwrap_or(0);
        let used = (limit - remaining_wire).max(0);
        window_from_limit_used(Some(limit), Some(used), None)
    });

    let booster = wire.booster_wallet.as_ref().map(parse_booster);

    let mut info = QuotaInfo {
        weekly,
        five_hour,
        total,
        booster,
        membership_level: parse_membership_level(wire),
        fetched_at: Utc::now().to_rfc3339(),
        low_warning: false,
    };
    info.low_warning = info.needs_low_warning();
    info
}
