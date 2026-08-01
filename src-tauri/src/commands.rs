//! 前端可调用的全部 Tauri 命令（snake_case，返回 Result<T, String>）。

use serde::Serialize;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;
use tokio::sync::watch;

use crate::kimi::{self, oauth};
use crate::local_usage::{self, LocalUsageReport};
use crate::opencode_go::{self, OpenCodeGoUsage};
use crate::quota::QuotaInfo;
use crate::storage::{self, AppSettings, LoginMethod, UpdateCheckCache};
use crate::update::{self, AppUpdateInfo, CliUpdateInfo};
use crate::{archive, creds, polling, skills, AppState};

pub const EVENT_LOGIN_SUCCESS: &str = "login-success";
pub const EVENT_LOGIN_ERROR: &str = "login-error";
/// 退出登录并清空全部账号凭证后广播，供三个窗口同步清除旧数据。
pub const EVENT_CREDENTIALS_CLEARED: &str = "credentials-cleared";
/// 设置保存成功后广播（widget 窗口监听，即时刷新卡片配置）
pub const EVENT_SETTINGS_CHANGED: &str = "settings-changed";

// ---- 设置 ----

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    Ok(state.settings.read().unwrap().clone())
}

#[tauri::command]
pub fn save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: AppSettings,
) -> Result<(), String> {
    let mut normalized = settings.normalized();
    {
        let mut current = state.settings.write().unwrap();
        // 坐标以内存为唯一权威（拖拽时 WindowEvent::Moved 处理器是唯一写者）：
        // 设置窗口打开期间用户拖动 widget 后，设置页保存会携带挂载时的旧坐标，
        // 这里用内存中当前坐标覆盖入参值，避免覆盖拖拽记忆。
        normalized.widget_position = current.widget_position;
        *current = normalized.clone();
    }
    // 锁外统一从内存落盘（persist_settings）+ 广播 + 开关即时生效
    crate::persist_settings(&state).map_err(|e| e.to_string())?;
    let _ = app.emit(EVENT_SETTINGS_CHANGED, &normalized);
    crate::apply_widget_visibility(&app, normalized.widget_enabled);
    // 轮询间隔可能变了，重启本轮等待
    let _ = state.poll_restart.send(());
    Ok(())
}

// ---- 登录 ----

#[derive(Debug, Clone, Serialize)]
pub struct LoginState {
    pub method: LoginMethod,
    pub logged_in: bool,
    pub masked_key: Option<String>,
}

#[tauri::command]
pub fn get_login_state(state: State<'_, AppState>) -> Result<LoginState, String> {
    let method = state.settings.read().unwrap().login_method;
    match method {
        LoginMethod::Oauth => Ok(LoginState {
            method,
            logged_in: creds::load_oauth_credentials(&state.config_dir).is_some(),
            masked_key: None,
        }),
        LoginMethod::ApiKey => {
            let key = creds::get_api_key();
            Ok(LoginState {
                method,
                logged_in: key.is_some(),
                masked_key: key.map(|k| creds::mask_api_key(&k)),
            })
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceLoginStart {
    pub user_code: String,
    pub verification_uri_complete: Option<String>,
}

/// 发起 Device Flow：立即返回 user_code，后台轮询结果。
/// 成功 emit `login-success`，失败/取消/过期 emit `login-error`。
#[tauri::command]
pub async fn start_device_login(app: AppHandle) -> Result<DeviceLoginStart, String> {
    let state = app.state::<AppState>();
    {
        let guard = state.login_cancel.lock().unwrap();
        if guard.is_some() {
            return Err("已有登录流程进行中，请先取消".into());
        }
    }

    let auth = oauth::start_device_authorization(&state.http, &state.device)
        .await
        .map_err(|e| e.to_string())?;
    let start = DeviceLoginStart {
        user_code: auth.user_code.clone(),
        verification_uri_complete: auth.verification_uri_complete.clone(),
    };

    let (tx, mut rx) = watch::channel(false);
    *state.login_cancel.lock().unwrap() = Some(tx);

    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let (http, device) = {
            let state = app_handle.state::<AppState>();
            (state.http.clone(), state.device.clone())
        };
        let result = oauth::poll_for_token(&http, &device, &auth, &mut rx).await;

        let state = app_handle.state::<AppState>();
        match result {
            Ok(set) => {
                let credentials = creds::OAuthCredentials {
                    access_token: set.access_token,
                    refresh_token: set.refresh_token,
                    expires_at: set.expires_at,
                };
                match creds::save_oauth_credentials(&state.config_dir, &credentials) {
                    Ok(()) => {
                        {
                            let mut settings = state.settings.write().unwrap();
                            settings.login_method = LoginMethod::Oauth;
                            let _ = storage::save_settings(&state.config_dir, &settings);
                        }
                        let _ = app_handle.emit(EVENT_LOGIN_SUCCESS, ());
                        // 登录成功后立刻刷一次配额
                        let _ = polling::poll_once(&app_handle).await;
                    }
                    Err(e) => {
                        let _ = app_handle.emit(EVENT_LOGIN_ERROR, e);
                    }
                }
            }
            Err(e) => {
                let _ = app_handle.emit(EVENT_LOGIN_ERROR, e.to_string());
            }
        }
        *state.login_cancel.lock().unwrap() = None;
    });

    Ok(start)
}

#[tauri::command]
pub fn cancel_login(state: State<'_, AppState>) -> Result<(), String> {
    if let Some(tx) = state.login_cancel.lock().unwrap().take() {
        let _ = tx.send(true);
    }
    Ok(())
}

#[tauri::command]
pub fn logout(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    // 四类凭证彼此独立，全部尝试删除，避免其中一项失败时跳过后续清理。
    let results = [
        (
            "Kimi OAuth",
            creds::delete_oauth_credentials(&state.config_dir),
        ),
        ("Kimi API Key", creds::delete_api_key()),
        ("Kimi 网页端令牌", creds::delete_web_token()),
        (
            "OpenCode Go 订阅凭证",
            creds::delete_opencode_go_credentials(),
        ),
    ];
    *state.quota.write().unwrap() = None;
    crate::tray::reset_icon(state.inner());
    let _ = app.emit(EVENT_CREDENTIALS_CLEARED, ());

    let errors: Vec<String> = results
        .into_iter()
        .filter_map(|(name, result)| result.err().map(|error| format!("{name}: {error}")))
        .collect();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!("部分凭证清除失败：{}", errors.join("；")))
    }
}

#[tauri::command]
pub fn set_api_key(key: String) -> Result<(), String> {
    let key = key.trim();
    if key.is_empty() {
        return Err("API Key 不能为空".into());
    }
    creds::set_api_key(key)
}

#[tauri::command]
pub fn get_masked_api_key() -> Result<Option<String>, String> {
    Ok(creds::get_api_key().map(|k| creds::mask_api_key(&k)))
}

#[tauri::command]
pub fn set_login_method(state: State<'_, AppState>, method: LoginMethod) -> Result<(), String> {
    {
        let mut settings = state.settings.write().unwrap();
        settings.login_method = method;
        storage::save_settings(&state.config_dir, &settings).map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ---- 月度用量（网页端令牌） ----

/// 保存 web token：先规范化，再在线校验（真实调接口），成功才存 keyring。
#[tauri::command]
pub async fn set_web_token(
    state: State<'_, AppState>,
    token: String,
) -> Result<kimi::web::MonthlyInfo, String> {
    let token = kimi::web::normalize_web_token(&token)?;
    match kimi::web::fetch_subscription_stats(&state.http, &token).await {
        Ok(info) => {
            creds::save_web_token(&token)?;
            Ok(info)
        }
        Err(kimi::web::WebError::Unauthorized(_)) => {
            Err("网页登录态无效或已过期，请重新复制 kimi-auth 的值".into())
        }
        Err(kimi::web::WebError::Http(_)) => Err("网络错误，校验失败".into()),
        Err(e @ kimi::web::WebError::Parse(_)) => Err(e.to_string()),
    }
}

/// 清除 web token（不存在也算成功）。
#[tauri::command]
pub fn clear_web_token() -> Result<(), String> {
    creds::delete_web_token()
}

#[tauri::command]
pub fn get_web_token_configured() -> Result<bool, String> {
    Ok(creds::load_web_token().is_some())
}

/// 获取月度用量；未配置 token 或请求失败返回 Err
/// （401/403 的错误文案以"网页登录态无效或已过期"开头，前端据此区分）。
#[tauri::command]
pub async fn get_monthly(state: State<'_, AppState>) -> Result<kimi::web::MonthlyInfo, String> {
    let token = creds::load_web_token().ok_or_else(|| "未配置网页端令牌".to_string())?;
    kimi::web::fetch_subscription_stats(&state.http, &token)
        .await
        .map_err(|e| e.to_string())
}

// ---- OpenCode Go 订阅配额（Workspace Dashboard） ----

/// 在线校验 Workspace ID + auth Cookie，成功后整体存入系统钥匙串。
#[tauri::command]
pub async fn set_opencode_go_credentials(
    state: State<'_, AppState>,
    workspace_id: String,
    auth_cookie: String,
) -> Result<OpenCodeGoUsage, String> {
    let workspace_id = opencode_go::normalize_workspace_id(&workspace_id)?;
    let auth_cookie = opencode_go::normalize_auth_cookie(&auth_cookie)?;
    let usage = opencode_go::fetch_usage_with_exchange_rate(
        &state.http,
        &workspace_id,
        &auth_cookie,
        &state.opencode_go_exchange_rate,
    )
    .await
    .map_err(|e| e.to_string())?;
    creds::save_opencode_go_credentials(&creds::OpenCodeGoCredentials {
        workspace_id,
        auth_cookie,
    })?;
    Ok(usage)
}

#[tauri::command]
pub fn clear_opencode_go_credentials() -> Result<(), String> {
    creds::delete_opencode_go_credentials()
}

#[tauri::command]
pub fn get_opencode_go_configured() -> Result<bool, String> {
    Ok(creds::load_opencode_go_credentials().is_some())
}

#[tauri::command]
pub async fn get_opencode_go_usage(state: State<'_, AppState>) -> Result<OpenCodeGoUsage, String> {
    let credentials = creds::load_opencode_go_credentials()
        .ok_or_else(|| "未配置 OpenCode Go 订阅".to_string())?;
    opencode_go::fetch_usage_with_exchange_rate(
        &state.http,
        &credentials.workspace_id,
        &credentials.auth_cookie,
        &state.opencode_go_exchange_rate,
    )
    .await
    .map_err(|e| e.to_string())
}

// ---- 配额 ----

/// 立即刷新配额，返回最新 QuotaInfo。
#[tauri::command]
pub async fn refresh_quota(app: AppHandle) -> Result<QuotaInfo, String> {
    polling::poll_once(&app).await
}

/// 读取缓存的配额（可能为空）。
#[tauri::command]
pub fn get_quota(state: State<'_, AppState>) -> Result<Option<QuotaInfo>, String> {
    Ok(state.quota.read().unwrap().clone())
}

// ---- 本地用量 ----

#[tauri::command]
pub fn get_local_usage(state: State<'_, AppState>) -> Result<Option<LocalUsageReport>, String> {
    Ok(state.local_usage.read().unwrap().clone())
}

/// 触发增量扫描（进程内 180s 节流；节流期内返回缓存）。
#[tauri::command]
pub async fn refresh_local_usage(state: State<'_, AppState>) -> Result<LocalUsageReport, String> {
    {
        let last = state.local_usage_scan_at.lock().unwrap();
        if let Some(instant) = *last {
            if instant.elapsed() < Duration::from_secs(local_usage::SCAN_THROTTLE_SECS) {
                if let Some(report) = state.local_usage.read().unwrap().clone() {
                    return Ok(report);
                }
            }
        }
    }

    let home = state.kimi_home.clone();
    let state_path = local_usage::scan_state_path(&state.config_dir);
    let scanned = tokio::task::spawn_blocking(move || {
        local_usage::scan_and_update(&local_usage::sessions_dir(&home), &state_path)
    })
    .await
    .map_err(|e| e.to_string())??;

    let report = local_usage::build_report(&scanned);
    *state.local_usage.write().unwrap() = Some(report.clone());
    *state.local_usage_scan_at.lock().unwrap() = Some(Instant::now());
    Ok(report)
}

// ---- 归档 ----

#[tauri::command]
pub async fn get_archive_overview(
    state: State<'_, AppState>,
) -> Result<archive::ArchiveOverview, String> {
    let home = state.kimi_home.clone();
    tokio::task::spawn_blocking(move || archive::build_overview(&home))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn archive_sessions(state: State<'_, AppState>, ids: Vec<String>) -> Result<u32, String> {
    let home = state.kimi_home.clone();
    tokio::task::spawn_blocking(move || archive::archive_sessions(&home, &ids))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn unarchive_session(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let home = state.kimi_home.clone();
    tokio::task::spawn_blocking(move || {
        let path =
            archive::session_state_path(&home, &id).ok_or_else(|| "非法会话 id".to_string())?;
        archive::set_archived(&path, false)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn run_auto_archive_now(state: State<'_, AppState>) -> Result<u32, String> {
    let (home, threshold) = {
        let settings = state.settings.read().unwrap();
        (
            state.kimi_home.clone(),
            settings.auto_archive_threshold_days,
        )
    };
    tokio::task::spawn_blocking(move || archive::run_auto_archive(&home, threshold))
        .await
        .map_err(|e| e.to_string())
}

// ---- 技能 ----

#[tauri::command]
pub fn get_skills(state: State<'_, AppState>) -> Result<Vec<skills::SkillInfo>, String> {
    Ok(skills::list_skills(&state.kimi_home))
}

#[tauri::command]
pub fn read_skill(state: State<'_, AppState>, name: String) -> Result<String, String> {
    skills::read_skill_content(&state.kimi_home, &name)
}

#[tauri::command]
pub fn reveal_in_explorer(app: AppHandle, path: String) -> Result<(), String> {
    app.opener()
        .reveal_item_in_dir(&path)
        .map_err(|e| e.to_string())
}

// ---- 更新检查 ----

#[tauri::command]
pub async fn check_cli_update(state: State<'_, AppState>) -> Result<CliUpdateInfo, String> {
    Ok(update::check_cli_update(&state.kimi_home, &state.http).await)
}

/// App 更新检查：成功缓存 6h、失败缓存 10min（缓存写 settings.json）；force 跳过缓存。
#[tauri::command]
pub async fn check_app_update(
    state: State<'_, AppState>,
    force: Option<bool>,
) -> Result<AppUpdateInfo, String> {
    let now = chrono::Utc::now().timestamp();

    if !force.unwrap_or(false) {
        let settings = state.settings.read().unwrap();
        let cache = &settings.app_update_check;
        if let Some(checked_at) = cache.last_checked_at {
            let ttl = if cache.latest_version.is_some() {
                update::APP_UPDATE_CACHE_OK_SECS
            } else {
                update::APP_UPDATE_CACHE_ERR_SECS
            };
            if now - checked_at < ttl {
                return Ok(update::build_app_update_info(
                    cache.latest_version.clone(),
                    cache.last_error.clone(),
                ));
            }
        }
    }

    match update::fetch_latest_app_release().await {
        Ok(latest) => {
            {
                let mut settings = state.settings.write().unwrap();
                settings.app_update_check = UpdateCheckCache {
                    last_checked_at: Some(now),
                    latest_version: Some(latest.clone()),
                    last_error: None,
                };
                let _ = storage::save_settings(&state.config_dir, &settings);
            }
            Ok(update::build_app_update_info(Some(latest), None))
        }
        Err(e) => {
            {
                let mut settings = state.settings.write().unwrap();
                settings.app_update_check.last_checked_at = Some(now);
                settings.app_update_check.latest_version = None;
                settings.app_update_check.last_error = Some(e.clone());
                let _ = storage::save_settings(&state.config_dir, &settings);
            }
            Ok(update::build_app_update_info(None, Some(e)))
        }
    }
}

// ---- 其他 ----

/// 退出应用（面板"退出"按钮使用）。
#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
pub fn open_url(app: AppHandle, url: String) -> Result<(), String> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("仅支持 http/https 链接".into());
    }
    app.opener()
        .open_url(&url, None::<&str>)
        .map_err(|e| e.to_string())
}
