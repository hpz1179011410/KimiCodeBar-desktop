//! 会话归档：扫描 sessions/<工作区>/<会话>/state.json，写 archived 标记。
//!
//! - archived 同时写顶层与 custom 下各一份
//! - 刷新 updatedAt 时保持原格式（ISO 字符串或毫秒时间戳）
//! - 自动归档：超过阈值天数未更新的未归档会话批量归档，每小时定时触发

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Manager};

use crate::storage;
use crate::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct SessionInfo {
    /// "<工作区目录名>/<会话目录名>"
    pub id: String,
    pub title: Option<String>,
    /// updatedAt 原文（用于展示）
    pub updated_at: Option<String>,
    /// updatedAt 归一化为毫秒时间戳（用于排序/比较）
    pub updated_at_ms: Option<i64>,
    pub archived: bool,
    pub work_dir: Option<String>,
    /// state.json 绝对路径
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkdirGroup {
    pub work_dir: String,
    pub sessions: Vec<SessionInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArchiveOverview {
    pub total: usize,
    pub archived: usize,
    pub groups: Vec<WorkdirGroup>,
}

pub fn sessions_root(kimi_home: &Path) -> PathBuf {
    kimi_home.join("sessions")
}

/// updatedAt 兼容两种格式：RFC3339 字符串 / 毫秒时间戳数字。
fn parse_updated_at(value: &Value) -> Option<i64> {
    match value {
        Value::String(s) => DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|dt| dt.timestamp_millis()),
        Value::Number(n) => n.as_i64(),
        _ => None,
    }
}

fn updated_at_display(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::String(s)) => Some(s.clone()),
        Some(Value::Number(n)) => Some(n.to_string()),
        _ => None,
    }
}

pub fn scan_sessions(kimi_home: &Path) -> Vec<SessionInfo> {
    let mut out = Vec::new();
    let root = sessions_root(kimi_home);
    let Ok(ws_entries) = fs::read_dir(&root) else {
        return out;
    };
    for ws in ws_entries.flatten() {
        let ws_path = ws.path();
        if !ws_path.is_dir() {
            continue;
        }
        let Ok(sess_entries) = fs::read_dir(&ws_path) else {
            continue;
        };
        for sess in sess_entries.flatten() {
            let sess_path = sess.path();
            if !sess_path.is_dir() {
                continue;
            }
            let state_path = sess_path.join("state.json");
            if !state_path.is_file() {
                continue;
            }
            let Ok(text) = fs::read_to_string(&state_path) else {
                continue;
            };
            let Ok(v) = serde_json::from_str::<Value>(&text) else {
                continue;
            };
            let id = format!(
                "{}/{}",
                ws.file_name().to_string_lossy(),
                sess.file_name().to_string_lossy()
            );
            let title = v.get("title").and_then(|t| t.as_str()).map(String::from);
            let updated_raw = v.get("updatedAt").cloned();
            let archived = v.get("archived").and_then(|a| a.as_bool()).unwrap_or(false)
                || v.get("custom")
                    .and_then(|c| c.get("archived"))
                    .and_then(|a| a.as_bool())
                    .unwrap_or(false);
            let work_dir = v
                .get("workDir")
                .and_then(|w| w.as_str())
                .map(String::from)
                .or_else(|| {
                    v.get("custom")
                        .and_then(|c| c.get("cwd"))
                        .and_then(|w| w.as_str())
                        .map(String::from)
                });
            out.push(SessionInfo {
                id,
                title,
                updated_at: updated_at_display(updated_raw.as_ref()),
                updated_at_ms: updated_raw.as_ref().and_then(parse_updated_at),
                archived,
                work_dir,
                path: state_path.to_string_lossy().into_owned(),
            });
        }
    }
    // 最近更新的排前面（无 updatedAt 的排最后）
    out.sort_by_key(|s| std::cmp::Reverse(s.updated_at_ms));
    out
}

/// 归档 / 取消归档：写 archived（顶层 + custom），刷新 updatedAt 且保持原格式。
pub fn set_archived(state_path: &Path, archived: bool) -> Result<(), String> {
    let text = fs::read_to_string(state_path).map_err(|e| e.to_string())?;
    let mut v: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let obj = v
        .as_object_mut()
        .ok_or_else(|| "state.json 不是 JSON 对象".to_string())?;

    obj.insert("archived".into(), Value::Bool(archived));
    if let Some(Value::Object(custom)) = obj.get_mut("custom") {
        custom.insert("archived".into(), Value::Bool(archived));
    }

    let now = Utc::now();
    let is_string = matches!(obj.get("updatedAt"), Some(Value::String(_)));
    let is_number = matches!(obj.get("updatedAt"), Some(Value::Number(_)));
    if is_string {
        obj.insert("updatedAt".into(), Value::String(now.to_rfc3339()));
    } else if is_number {
        obj.insert(
            "updatedAt".into(),
            serde_json::json!(now.timestamp_millis()),
        );
    }

    let out = serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?;
    storage::atomic_write(state_path, out.as_bytes()).map_err(|e| e.to_string())
}

/// 由 id（"<ws>/<session>"）解析 state.json 路径，拒绝路径穿越。
pub fn session_state_path(kimi_home: &Path, id: &str) -> Option<PathBuf> {
    let mut parts = id.split('/');
    let ws = parts.next()?;
    let sess = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    for part in [ws, sess] {
        if part.is_empty() || part == ".." || part.contains(['\\', ':']) {
            return None;
        }
    }
    Some(
        sessions_root(kimi_home)
            .join(ws)
            .join(sess)
            .join("state.json"),
    )
}

/// 批量归档，返回成功数。
pub fn archive_sessions(kimi_home: &Path, ids: &[String]) -> u32 {
    let mut count = 0;
    for id in ids {
        if let Some(path) = session_state_path(kimi_home, id) {
            if set_archived(&path, true).is_ok() {
                count += 1;
            }
        }
    }
    count
}

/// 自动归档：超过 threshold_days 未更新且未归档的会话批量归档，返回归档数。
pub fn run_auto_archive(kimi_home: &Path, threshold_days: u64) -> u32 {
    let cutoff = Utc::now().timestamp_millis() - (threshold_days as i64) * 86_400_000;
    let mut count = 0;
    for session in scan_sessions(kimi_home) {
        if session.archived {
            continue;
        }
        let Some(ms) = session.updated_at_ms else {
            continue;
        };
        if ms < cutoff && set_archived(Path::new(&session.path), true).is_ok() {
            count += 1;
        }
    }
    count
}

/// 统计 + 按工作目录分组。
pub fn build_overview(kimi_home: &Path) -> ArchiveOverview {
    let sessions = scan_sessions(kimi_home);
    let total = sessions.len();
    let archived = sessions.iter().filter(|s| s.archived).count();
    let mut grouped: BTreeMap<String, Vec<SessionInfo>> = BTreeMap::new();
    for session in sessions {
        grouped
            .entry(
                session
                    .work_dir
                    .clone()
                    .unwrap_or_else(|| "(未知目录)".into()),
            )
            .or_default()
            .push(session);
    }
    let groups = grouped
        .into_iter()
        .map(|(work_dir, sessions)| WorkdirGroup { work_dir, sessions })
        .collect();
    ArchiveOverview {
        total,
        archived,
        groups,
    }
}

/// 每小时触发一次自动归档（开关与阈值读设置）。
pub fn start_archive_timer(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(3600)).await;
            let (enabled, threshold, home) = {
                let state = app.state::<AppState>();
                let settings = state.settings.read().unwrap();
                (
                    settings.auto_archive_enabled,
                    settings.auto_archive_threshold_days,
                    state.kimi_home.clone(),
                )
            };
            if !enabled {
                continue;
            }
            let _ = tokio::task::spawn_blocking(move || run_auto_archive(&home, threshold)).await;
        }
    });
}
