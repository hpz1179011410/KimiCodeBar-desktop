//! OAuth Device Flow：设备授权、轮询取 token、refresh_token 刷新。
//!
//! 端点（均在 AUTH_BASE 下）：
//! - POST /api/oauth/device_authorization（form: client_id）
//! - POST /api/oauth/token（grant_type=device_code / refresh_token）

use serde::Deserialize;
use std::time::{Duration, Instant};
use tokio::sync::watch;

use super::identity::{self, DeviceIdentity};
use super::{AUTH_BASE, EXPIRING_SOON_THRESHOLD_SECS, OAUTH_CLIENT_ID};

const DEVICE_CODE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";
const REFRESH_GRANT: &str = "refresh_token";
const DEFAULT_POLL_INTERVAL_SECS: u64 = 5;
const SLOW_DOWN_EXTRA_SECS: u64 = 5;
/// 轮询总预算 15 分钟。
const POLL_BUDGET: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, thiserror::Error)]
pub enum OAuthError {
    #[error("网络请求失败: {0}")]
    Http(#[from] reqwest::Error),
    #[error("服务器返回错误: {0}")]
    Api(String),
    #[error("授权已过期，请重新发起登录")]
    Expired,
    #[error("用户拒绝了授权")]
    Denied,
    #[error("登录状态已失效，请重新授权")]
    NotAuthorized,
    #[error("登录已取消")]
    Cancelled,
    #[error("服务器响应缺少必要字段")]
    MalformedResponse,
}

/// 设备码轮询错误分类（纯函数，便于单测）。
pub enum PollErrorClass {
    /// 用户尚未完成授权，继续轮询
    Pending,
    /// 服务器要求放慢，间隔 +5s
    SlowDown,
    /// 终止流程
    Fatal(OAuthError),
}

pub fn classify_poll_error(code: &str) -> PollErrorClass {
    match code {
        "authorization_pending" => PollErrorClass::Pending,
        "slow_down" => PollErrorClass::SlowDown,
        "expired_token" => PollErrorClass::Fatal(OAuthError::Expired),
        "access_denied" => PollErrorClass::Fatal(OAuthError::Denied),
        other => PollErrorClass::Fatal(OAuthError::Api(other.to_string())),
    }
}

/// refresh 错误分类：401/403 或 invalid_grant → 需要重新登录。
pub fn classify_refresh_error(status: u16, code: Option<&str>) -> OAuthError {
    if status == 401 || status == 403 {
        return OAuthError::NotAuthorized;
    }
    match code {
        Some("invalid_grant") => OAuthError::NotAuthorized,
        Some(other) => OAuthError::Api(format!("HTTP {status}: {other}")),
        None => OAuthError::Api(format!("HTTP {status}")),
    }
}

/// 临期判断：剩余有效期 < 300s 视为临期；无过期时间（长期 token）不算临期。
pub fn is_expiring_soon(expires_at: Option<i64>, now_epoch: i64) -> bool {
    match expires_at {
        Some(t) => t - now_epoch < EXPIRING_SOON_THRESHOLD_SECS,
        None => false,
    }
}

#[derive(Debug, Clone)]
pub struct DeviceAuthorization {
    pub user_code: String,
    pub device_code: String,
    pub verification_uri_complete: Option<String>,
    pub expires_in: u64,
    pub interval: u64,
}

#[derive(Debug, Clone)]
pub struct TokenSet {
    pub access_token: String,
    pub refresh_token: Option<String>,
    /// epoch 秒；无 expires_in 时为 None（视为长期有效）
    pub expires_at: Option<i64>,
}

// ---- wire 结构（snake_case） ----

#[derive(Debug, Deserialize)]
struct DeviceAuthWire {
    user_code: Option<String>,
    device_code: Option<String>,
    verification_uri_complete: Option<String>,
    expires_in: Option<u64>,
    interval: Option<u64>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TokenWire {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
    error: Option<String>,
}

fn now_epoch() -> i64 {
    chrono::Utc::now().timestamp()
}

fn token_set_from_wire(wire: TokenWire) -> Result<TokenSet, OAuthError> {
    let access_token = wire.access_token.ok_or(OAuthError::MalformedResponse)?;
    Ok(TokenSet {
        access_token,
        refresh_token: wire.refresh_token,
        expires_at: wire.expires_in.map(|secs| now_epoch() + secs),
    })
}

/// 第一步：请求设备授权，拿到 user_code / device_code。
pub async fn start_device_authorization(
    client: &reqwest::Client,
    device: &DeviceIdentity,
) -> Result<DeviceAuthorization, OAuthError> {
    let req = client
        .post(format!("{AUTH_BASE}/api/oauth/device_authorization"))
        .form(&[("client_id", OAUTH_CLIENT_ID)]);
    let resp = identity::apply_headers(req, device).send().await?;
    let status = resp.status().as_u16();
    let body = resp.text().await?;
    if !(200..300).contains(&status) {
        return Err(OAuthError::Api(format!("HTTP {status}: {body}")));
    }
    let wire: DeviceAuthWire =
        serde_json::from_str(&body).map_err(|e| OAuthError::Api(format!("响应解析失败: {e}")))?;
    if let Some(err) = wire.error {
        return Err(OAuthError::Api(err));
    }
    Ok(DeviceAuthorization {
        user_code: wire.user_code.ok_or(OAuthError::MalformedResponse)?,
        device_code: wire.device_code.ok_or(OAuthError::MalformedResponse)?,
        verification_uri_complete: wire.verification_uri_complete,
        expires_in: wire.expires_in.unwrap_or(0),
        interval: wire.interval.unwrap_or(DEFAULT_POLL_INTERVAL_SECS),
    })
}

/// 第二步：轮询 token 端点直到成功 / 过期 / 拒绝 / 取消。
pub async fn poll_for_token(
    client: &reqwest::Client,
    device: &DeviceIdentity,
    auth: &DeviceAuthorization,
    cancel: &mut watch::Receiver<bool>,
) -> Result<TokenSet, OAuthError> {
    let mut interval = Duration::from_secs(auth.interval.max(1));
    let deadline = Instant::now() + POLL_BUDGET;

    loop {
        if *cancel.borrow() {
            return Err(OAuthError::Cancelled);
        }
        tokio::select! {
            _ = tokio::time::sleep(interval) => {}
            _ = cancel.changed() => {
                return Err(OAuthError::Cancelled);
            }
        }
        if Instant::now() >= deadline {
            return Err(OAuthError::Expired);
        }

        let req = client.post(format!("{AUTH_BASE}/api/oauth/token")).form(&[
            ("client_id", OAUTH_CLIENT_ID),
            ("device_code", auth.device_code.as_str()),
            ("grant_type", DEVICE_CODE_GRANT),
        ]);
        let resp = identity::apply_headers(req, device).send().await?;
        let status = resp.status().as_u16();
        let body = resp.text().await?;
        let wire: TokenWire = serde_json::from_str(&body)
            .map_err(|e| OAuthError::Api(format!("响应解析失败: {e}")))?;

        if (200..300).contains(&status) && wire.error.is_none() {
            return token_set_from_wire(wire);
        }

        match classify_poll_error(wire.error.as_deref().unwrap_or("unknown_error")) {
            PollErrorClass::Pending => {}
            PollErrorClass::SlowDown => interval += Duration::from_secs(SLOW_DOWN_EXTRA_SECS),
            PollErrorClass::Fatal(e) => return Err(e),
        }
    }
}

/// refresh_token 刷新。成功但无新 refresh_token 时沿用旧的。
pub async fn refresh_access_token(
    client: &reqwest::Client,
    device: &DeviceIdentity,
    refresh_token: &str,
) -> Result<TokenSet, OAuthError> {
    let req = client.post(format!("{AUTH_BASE}/api/oauth/token")).form(&[
        ("client_id", OAUTH_CLIENT_ID),
        ("grant_type", REFRESH_GRANT),
        ("refresh_token", refresh_token),
    ]);
    let resp = identity::apply_headers(req, device).send().await?;
    let status = resp.status().as_u16();
    let body = resp.text().await?;
    let wire: TokenWire =
        serde_json::from_str(&body).map_err(|e| OAuthError::Api(format!("响应解析失败: {e}")))?;

    if !(200..300).contains(&status) || wire.error.is_some() {
        return Err(classify_refresh_error(status, wire.error.as_deref()));
    }

    let mut set = token_set_from_wire(wire)?;
    if set.refresh_token.is_none() {
        set.refresh_token = Some(refresh_token.to_string());
    }
    Ok(set)
}
