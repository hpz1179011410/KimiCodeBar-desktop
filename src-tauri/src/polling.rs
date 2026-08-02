//! 配额定时轮询：按 refresh_interval_secs 周期获取配额，缓存到共享状态，
//! 向 main 窗口 emit `quota-updated`，并按低额状态切换托盘图标；
//! 401 时清凭证、重置托盘图标并 emit `login-expired`。

use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

use crate::kimi::client::{self, QuotaError};
use crate::quota::{self, QuotaInfo};
use crate::storage::{LoginMethod, MIN_REFRESH_INTERVAL_SECS};
use crate::{creds, tray, AppState};

pub const EVENT_QUOTA_UPDATED: &str = "quota-updated";
pub const EVENT_LOGIN_EXPIRED: &str = "login-expired";

/// 启动轮询任务。设置变更（间隔调整）通过 watch 通道重启本轮等待。
pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        // 启动时先刷一次
        let _ = poll_once(&app).await;
        loop {
            let interval = {
                let state = app.state::<AppState>();
                let secs = state.settings.read().unwrap().refresh_interval_secs;
                secs
            }
            .max(MIN_REFRESH_INTERVAL_SECS);
            let mut restart_rx = app.state::<AppState>().poll_restart.subscribe();
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(interval)) => {
                    let _ = poll_once(&app).await;
                }
                changed = restart_rx.changed() => {
                    if changed.is_err() {
                        break; // sender 已销毁（应用退出）
                    }
                }
            }
        }
    });
}

/// 立即执行一次配额刷新。返回最新 QuotaInfo 或错误描述。
pub async fn poll_once(app: &AppHandle) -> Result<QuotaInfo, String> {
    let requested_at = Instant::now();
    let state = app.state::<AppState>();
    let _refresh_guard = state.quota_refresh_lock.lock().await;

    // 若等待锁期间另一请求已经刷新成功，直接复用结果，避免定时轮询与手动刷新
    // 在同一时刻连续访问配额接口。
    let refreshed_after_request = state
        .quota_refresh_at
        .lock()
        .unwrap()
        .is_some_and(|completed_at| completed_at >= requested_at);
    if refreshed_after_request {
        if let Some(info) = state.quota.read().unwrap().clone() {
            return Ok(info);
        }
    }

    let (http, device, config_dir, settings) = {
        let settings = state.settings.read().unwrap().clone();
        (
            state.http.clone(),
            state.device.clone(),
            state.config_dir.clone(),
            settings,
        )
    };

    let token = creds::get_active_token(&http, &device, &config_dir, &settings)
        .await
        .ok_or_else(|| "未登录".to_string())?;

    match client::fetch_usages(&http, &token, &device).await {
        Ok(wire) => {
            let info = quota::parse_usage(&wire);
            {
                *state.quota.write().unwrap() = Some(info.clone());
                *state.quota_refresh_at.lock().unwrap() = Some(Instant::now());
                tray::set_quota_icon(&state, &info);
            }
            let _ = app.emit(EVENT_QUOTA_UPDATED, &info);
            Ok(info)
        }
        Err(QuotaError::Unauthorized) => {
            match settings.login_method {
                LoginMethod::Oauth => {
                    let _ = creds::delete_oauth_credentials(&config_dir);
                }
                LoginMethod::ApiKey => {
                    let _ = creds::delete_api_key();
                }
            }
            {
                *state.quota.write().unwrap() = None;
                tray::reset_icon(&state);
            }
            let _ = app.emit(EVENT_LOGIN_EXPIRED, ());
            Err("登录已过期，请重新登录".to_string())
        }
        Err(e) => Err(e.to_string()),
    }
}
