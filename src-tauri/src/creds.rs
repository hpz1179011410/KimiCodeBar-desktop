//! 凭证存储：
//! - OAuth 凭证（平台分叉）：
//!   - Windows：JSON pretty → UTF-8 → DPAPI 密文 → credentials.json（原子写）；
//!     读取兼容明文旧文件并透明迁移为密文；DPAPI 失败宁可报错不落明文。
//!   - macOS / Linux：JSON 序列化后整体存 keyring（service "KimiCodeBar"，
//!     key "oauth_credentials"），与 api_key / web_token 同 service，不落盘文件。
//! - API Key、Kimi 网页令牌、OpenCode Go Workspace 凭证 → keyring。

use serde::{Deserialize, Serialize};
#[cfg(windows)]
use std::fs;
use std::path::Path;
#[cfg(windows)]
use std::path::PathBuf;

#[cfg(windows)]
use crate::kimi::dpapi;
use crate::kimi::identity::DeviceIdentity;
use crate::kimi::oauth::{self, OAuthError};
#[cfg(windows)]
use crate::storage;
use crate::storage::{AppSettings, LoginMethod};

const KEYRING_SERVICE: &str = "KimiCodeBar";
const KEYRING_USER: &str = "api_key";
const KEYRING_WEB_TOKEN: &str = "web_token";
const KEYRING_OPENCODE_GO: &str = "opencode_go_credentials";
#[cfg(not(windows))]
const KEYRING_OAUTH: &str = "oauth_credentials";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthCredentials {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenCodeGoCredentials {
    pub workspace_id: String,
    pub auth_cookie: String,
}

#[cfg(windows)]
pub fn credentials_path(config_dir: &Path) -> PathBuf {
    config_dir.join("credentials.json")
}

// ---- OAuth 凭证：Windows 走 DPAPI 密文文件 ----

#[cfg(windows)]
pub fn save_oauth_credentials(config_dir: &Path, creds: &OAuthCredentials) -> Result<(), String> {
    let json = serde_json::to_string_pretty(creds).map_err(|e| e.to_string())?;
    // DPAPI 失败时直接报错，绝不回退落明文
    let blob = dpapi::protect(json.as_bytes()).map_err(|e| e.to_string())?;
    storage::atomic_write(&credentials_path(config_dir), &blob).map_err(|e| e.to_string())
}

#[cfg(windows)]
pub fn load_oauth_credentials(config_dir: &Path) -> Option<OAuthCredentials> {
    let data = fs::read(credentials_path(config_dir)).ok()?;
    // 优先按 DPAPI 密文解读
    if let Ok(plain) = dpapi::unprotect(&data) {
        return serde_json::from_slice::<OAuthCredentials>(&plain).ok();
    }
    // 回退明文（旧版本文件），成功后透明迁移为密文
    if let Ok(creds) = serde_json::from_slice::<OAuthCredentials>(&data) {
        let _ = save_oauth_credentials(config_dir, &creds);
        return Some(creds);
    }
    None
}

#[cfg(windows)]
pub fn delete_oauth_credentials(config_dir: &Path) -> Result<(), String> {
    match fs::remove_file(credentials_path(config_dir)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

// ---- OAuth 凭证：macOS / Linux 走 keyring（config_dir 参数仅为保持签名一致） ----

#[cfg(not(windows))]
fn oauth_entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_OAUTH).map_err(|e| e.to_string())
}

#[cfg(not(windows))]
pub fn save_oauth_credentials(_config_dir: &Path, creds: &OAuthCredentials) -> Result<(), String> {
    let json = serde_json::to_string_pretty(creds).map_err(|e| e.to_string())?;
    oauth_entry()?
        .set_password(&json)
        .map_err(|e| e.to_string())
}

#[cfg(not(windows))]
pub fn load_oauth_credentials(_config_dir: &Path) -> Option<OAuthCredentials> {
    let json = oauth_entry().ok()?.get_password().ok()?;
    serde_json::from_str::<OAuthCredentials>(&json).ok()
}

#[cfg(not(windows))]
pub fn delete_oauth_credentials(_config_dir: &Path) -> Result<(), String> {
    match oauth_entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

// ---- API Key（keyring） ----

fn api_key_entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER).map_err(|e| e.to_string())
}

pub fn set_api_key(key: &str) -> Result<(), String> {
    api_key_entry()?
        .set_password(key)
        .map_err(|e| e.to_string())
}

pub fn get_api_key() -> Option<String> {
    api_key_entry().ok()?.get_password().ok()
}

pub fn delete_api_key() -> Result<(), String> {
    match api_key_entry()?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

// ---- 网页端令牌（kimi-auth cookie，keyring） ----

fn web_token_entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_WEB_TOKEN).map_err(|e| e.to_string())
}

pub fn save_web_token(token: &str) -> Result<(), String> {
    web_token_entry()?
        .set_password(token)
        .map_err(|e| e.to_string())
}

pub fn load_web_token() -> Option<String> {
    web_token_entry().ok()?.get_password().ok()
}

pub fn delete_web_token() -> Result<(), String> {
    match web_token_entry()?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

// ---- OpenCode Go Workspace 凭证（keyring） ----

fn opencode_go_entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_OPENCODE_GO).map_err(|e| e.to_string())
}

pub fn save_opencode_go_credentials(credentials: &OpenCodeGoCredentials) -> Result<(), String> {
    let json = serde_json::to_string(credentials).map_err(|e| e.to_string())?;
    opencode_go_entry()?
        .set_password(&json)
        .map_err(|e| e.to_string())
}

pub fn load_opencode_go_credentials() -> Option<OpenCodeGoCredentials> {
    let json = opencode_go_entry().ok()?.get_password().ok()?;
    serde_json::from_str(&json).ok()
}

pub fn delete_opencode_go_credentials() -> Result<(), String> {
    match opencode_go_entry()?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

/// 掩码显示：长度 <= 8 全掩码，否则保留前 3 + 后 4。
pub fn mask_api_key(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    if chars.len() <= 8 {
        return "****".into();
    }
    let head: String = chars[..3].iter().collect();
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("{head}…{tail}")
}

/// 按 login_method 取当前可用 token。
/// OAuth：临期（剩余 <300s）自动刷新并回写；NotAuthorized 时删除凭证并返回 None。
pub async fn get_active_token(
    client: &reqwest::Client,
    device: &DeviceIdentity,
    config_dir: &Path,
    settings: &AppSettings,
) -> Option<String> {
    match settings.login_method {
        LoginMethod::ApiKey => get_api_key(),
        LoginMethod::Oauth => {
            let creds = load_oauth_credentials(config_dir)?;
            let now = chrono::Utc::now().timestamp();
            if !oauth::is_expiring_soon(creds.expires_at, now) {
                return Some(creds.access_token);
            }
            let Some(refresh_token) = creds.refresh_token.clone() else {
                // 无 refresh_token：先用着，等 401 再走重新登录
                return Some(creds.access_token);
            };
            match oauth::refresh_access_token(client, device, &refresh_token).await {
                Ok(set) => {
                    let updated = OAuthCredentials {
                        access_token: set.access_token.clone(),
                        refresh_token: set.refresh_token.or(creds.refresh_token),
                        expires_at: set.expires_at,
                    };
                    let _ = save_oauth_credentials(config_dir, &updated);
                    Some(set.access_token)
                }
                Err(OAuthError::NotAuthorized) => {
                    let _ = delete_oauth_credentials(config_dir);
                    None
                }
                Err(_) => {
                    // 网络等临时错误：沿用现有 token，等下次轮询再试
                    Some(creds.access_token)
                }
            }
        }
    }
}
