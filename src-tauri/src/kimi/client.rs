//! 配额 API 客户端：GET {API_BASE}/coding/v1/usages（Bearer 认证，API Key 与 OAuth token 通用）。

use super::identity::{self, DeviceIdentity};
use super::models::UsagesResponse;
use super::{API_BASE, HTTP_TIMEOUT, USER_AGENT};

#[derive(Debug, thiserror::Error)]
pub enum QuotaError {
    #[error("未授权或登录已过期")]
    Unauthorized,
    #[error("网络请求失败: {0}")]
    Http(#[from] reqwest::Error),
    #[error("服务器错误 HTTP {0}")]
    Status(u16),
    #[error("响应解析失败: {0}")]
    Parse(String),
}

/// 共享 HTTP 客户端：仅 https，30s 超时，固定 UA。
pub fn build_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .https_only(true)
        .timeout(HTTP_TIMEOUT)
        .user_agent(USER_AGENT)
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

pub async fn fetch_usages(
    client: &reqwest::Client,
    token: &str,
    device: &DeviceIdentity,
) -> Result<UsagesResponse, QuotaError> {
    let req = client
        .get(format!("{API_BASE}/coding/v1/usages"))
        .bearer_auth(token);
    let resp = identity::apply_headers(req, device).send().await?;
    let status = resp.status().as_u16();
    if status == 401 || status == 403 {
        return Err(QuotaError::Unauthorized);
    }
    if !(200..300).contains(&status) {
        return Err(QuotaError::Status(status));
    }
    resp.json::<UsagesResponse>()
        .await
        .map_err(|e| QuotaError::Parse(e.to_string()))
}
