//! 托盘图标更新：配额刷新后按低额状态切换静态图标（低额红 / 普通蓝）。
//!
//! tooltip 不走前端 i18n（托盘属原生层），固定中文。

use std::sync::atomic::Ordering;

use tauri::tray::TrayIcon;

use crate::quota::QuotaInfo;
use crate::{AppState, TRAY_ICON_NORMAL, TRAY_ICON_WARN};

const DEFAULT_TOOLTIP: &str = "KimiCodeBar";

/// 百分比文本：四舍五入为整数（CLI 接口本身是整数口径）。
fn percent_text(p: f64) -> String {
    format!("{}%", (p * 100.0).round() as i64)
}

/// 配额刷新成功后调用：按 low_warning 切换静态图标，tooltip 显示完整配额信息。
pub fn set_quota_icon(state: &AppState, quota: &QuotaInfo) {
    state.low_quota.store(quota.low_warning, Ordering::Relaxed);
    let guard = state.tray.lock().unwrap();
    let Some(tray) = guard.as_ref() else {
        return;
    };
    set_static_icon(tray, quota.low_warning);

    let mut parts = Vec::new();
    if let Some(w) = &quota.weekly {
        parts.push(format!("本周剩余 {}", percent_text(w.remaining_percent)));
    }
    if let Some(w) = &quota.five_hour {
        parts.push(format!("5小时剩余 {}", percent_text(w.remaining_percent)));
    }
    let tooltip = if parts.is_empty() {
        DEFAULT_TOOLTIP.to_string()
    } else {
        parts.join(" · ")
    };
    let _ = tray.set_tooltip(Some(&tooltip));
}

/// 退出登录 / 登录过期（401）时调用：恢复静态普通图标与默认 tooltip。
pub fn reset_icon(state: &AppState) {
    state.low_quota.store(false, Ordering::Relaxed);
    let guard = state.tray.lock().unwrap();
    let Some(tray) = guard.as_ref() else {
        return;
    };
    set_static_icon(tray, false);
    let _ = tray.set_tooltip(Some(DEFAULT_TOOLTIP));
}

fn set_static_icon(tray: &TrayIcon<tauri::Wry>, low: bool) {
    let bytes = if low {
        TRAY_ICON_WARN
    } else {
        TRAY_ICON_NORMAL
    };
    if let Ok(icon) = tauri::image::Image::from_bytes(bytes) {
        let _ = tray.set_icon(Some(icon));
    }
}
