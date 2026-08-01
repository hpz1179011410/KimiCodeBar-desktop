//! 配额 API 响应的 wire 结构。
//!
//! 响应为 camelCase proto3 JSON：
//! - 数值字段全是字符串（limit/used/remaining/amountLeft/priceInCents）
//! - proto3 会省略 false / 空值 → 字段全部 Option
//! - membership 结构不确定，先用 Value 兜底，解析层兼容字符串或对象

use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UsagesResponse {
    /// 周窗口
    pub usage: Option<UsageWindowWire>,
    /// 各类限额窗口（含 5 小时窗口）
    pub limits: Vec<UsageLimitWire>,
    pub total_quota: Option<TotalQuotaWire>,
    pub booster_wallet: Option<BoosterWalletWire>,
    /// 用户信息（membership 在其下）；老结构可能在顶层 membership，解析层做回退
    pub user: Option<UserWire>,
    /// 顶层会员信息（旧结构兜底，解析层兼容 string / { level: string }）
    pub membership: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UserWire {
    pub membership: Option<MembershipWire>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MembershipWire {
    /// 形如 LEVEL_FREE / LEVEL_INTERMEDIATE
    pub level: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UsageWindowWire {
    pub limit: Option<String>,
    pub used: Option<String>,
    pub remaining: Option<String>,
    /// RFC3339 时间
    pub reset_time: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UsageLimitWire {
    pub window: Option<WindowSpecWire>,
    /// 真实响应：配额明细嵌套在 detail 里；顶层平铺字段保留作兜底
    pub detail: Option<UsageWindowWire>,
    pub limit: Option<String>,
    pub used: Option<String>,
    pub remaining: Option<String>,
    pub reset_time: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WindowSpecWire {
    /// 窗口长度（分钟）。可能是数字或字符串，用 Value 兜底。
    pub duration: Option<serde_json::Value>,
}

impl WindowSpecWire {
    pub fn duration_minutes(&self) -> Option<i64> {
        match &self.duration {
            Some(serde_json::Value::Number(n)) => n.as_i64(),
            Some(serde_json::Value::String(s)) => s.trim().parse::<i64>().ok(),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TotalQuotaWire {
    pub limit: Option<String>,
    pub used: Option<String>,
    pub remaining: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BoosterWalletWire {
    pub status: Option<String>,
    /// 余额对象，amountLeft 单位 1e-8 元（未启用时服务端可能不返回该字段）
    pub balance: Option<BoosterBalanceWire>,
    /// 月度消费上限
    pub monthly_charge_limit: Option<MoneyWire>,
    /// 月度已消费
    pub monthly_used: Option<MoneyWire>,
    /// 充值上限
    pub topup_limit: Option<MoneyWire>,
    /// 顶层平铺字段（旧结构兜底）：余额 1e-8 元 / 价格 分
    pub amount_left: Option<String>,
    pub price_in_cents: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BoosterBalanceWire {
    pub amount_left: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MoneyWire {
    pub currency: Option<String>,
    /// 单位：分
    pub price_in_cents: Option<String>,
}
