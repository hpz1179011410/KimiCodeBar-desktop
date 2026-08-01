//! Kimi API 协议层：常量与各子模块。

pub mod client;
pub mod dpapi;
pub mod identity;
pub mod models;
pub mod oauth;
pub mod web;

use std::time::Duration;

pub const API_BASE: &str = "https://api.kimi.com";
pub const AUTH_BASE: &str = "https://auth.kimi.com";
pub const OAUTH_CLIENT_ID: &str = "17e5f671-d194-4dfb-9706-5516cb48c098";
pub const USER_AGENT: &str = "KimiCodeBar/1.0";
pub const HTTP_TIMEOUT: Duration = Duration::from_secs(30);

/// OAuth token 临期阈值：剩余有效期不足 300s 即触发刷新。
pub const EXPIRING_SOON_THRESHOLD_SECS: i64 = 300;
