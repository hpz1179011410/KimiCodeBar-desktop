//! 更新检查：
//! - CLI 更新：候选路径找 kimi 可执行 → `kimi --version`（5s 超时）→ 对比官方 changelog 首个版本号
//! - App 更新：GitHub releases/latest 302 Location 解析 tag（API 作回退），缓存在 settings.json

use serde::Serialize;
use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::kimi::{HTTP_TIMEOUT, USER_AGENT};
use crate::AppState;

// GitHub 仓库（如需换仓库改这里）
pub const APP_REPO_OWNER: &str = "hpz1179011410";
pub const APP_REPO_NAME: &str = "KimiCodeBar-desktop";

const CLI_CMD_TIMEOUT: Duration = Duration::from_secs(5);

/// 成功缓存 6 小时，失败缓存 10 分钟。
pub const APP_UPDATE_CACHE_OK_SECS: i64 = 6 * 3600;
pub const APP_UPDATE_CACHE_ERR_SECS: i64 = 600;

// 注意：Kimi Code 官方 changelog 的实际 URL 无法离线验证，以下候选 URL 待联网验证后修正。
const CLI_CHANGELOG_URLS: [&str; 2] = [
    "https://moonshotai.github.io/kimi-code/changelog.md",
    "https://moonshotai.github.io/kimi-code/CHANGELOG.md",
];

#[derive(Debug, Clone, Serialize)]
pub struct CliUpdateInfo {
    pub current: Option<String>,
    pub latest: Option<String>,
    pub update_available: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppUpdateInfo {
    pub current: String,
    pub latest: Option<String>,
    pub update_available: bool,
    pub release_url: String,
    pub error: Option<String>,
}

/// 从文本中提取首个 x.y.z 版本号（允许带 v 前缀）。
pub fn parse_version(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                i += 1;
            }
            let candidate = &text[start..i];
            let parts: Vec<&str> = candidate.split('.').collect();
            if parts.len() >= 3
                && parts[..3]
                    .iter()
                    .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
            {
                return Some(parts[..3].join("."));
            }
        } else {
            i += 1;
        }
    }
    None
}

fn version_segments(v: &str) -> Vec<u64> {
    v.trim()
        .trim_start_matches(['v', 'V'])
        .split('.')
        .map(|p| p.trim().parse::<u64>().unwrap_or(0))
        .collect()
}

/// 版本比较：剥 v 前缀，分段数字比较。
pub fn compare_versions(a: &str, b: &str) -> Ordering {
    let sa = version_segments(a);
    let sb = version_segments(b);
    for i in 0..sa.len().max(sb.len()) {
        let x = sa.get(i).copied().unwrap_or(0);
        let y = sb.get(i).copied().unwrap_or(0);
        match x.cmp(&y) {
            Ordering::Equal => continue,
            other => return other,
        }
    }
    Ordering::Equal
}

/// 依次尝试候选路径找 kimi 可执行文件：where.exe → kimi_code_home/bin → npm 全局目录。
#[cfg(windows)]
pub async fn find_kimi_executable(kimi_home: &Path) -> Option<PathBuf> {
    // 1. where.exe kimi：可能返回多行（无扩展名的 shell 脚本 + .cmd）。
    //    Windows 上无扩展名的脚本无法被 CreateProcess 直接执行，优先选 .exe/.cmd。
    if let Ok(Ok(out)) = tokio::time::timeout(
        CLI_CMD_TIMEOUT,
        tokio::process::Command::new("where.exe")
            .arg("kimi")
            .output(),
    )
    .await
    {
        if out.status.success() {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let paths: Vec<PathBuf> = stdout
                .lines()
                .map(|l| PathBuf::from(l.trim()))
                .filter(|p| p.is_file())
                .collect();
            let executable = paths.iter().find(|p| {
                p.extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.eq_ignore_ascii_case("exe") || e.eq_ignore_ascii_case("cmd"))
                    .unwrap_or(false)
            });
            if let Some(p) = executable.or_else(|| paths.first()) {
                return Some(p.clone());
            }
        }
    }
    // 2. {kimi_code_home}/bin
    for name in ["kimi.exe", "kimi.cmd", "kimi"] {
        let path = kimi_home.join("bin").join(name);
        if path.is_file() {
            return Some(path);
        }
    }
    // 3. npm 全局目录（Windows 上 npm 是 npm.cmd，Command 不会做 PATHEXT 解析，显式指定）
    for npm in ["npm.cmd", "npm"] {
        if let Ok(Ok(out)) = tokio::time::timeout(
            CLI_CMD_TIMEOUT,
            tokio::process::Command::new(npm)
                .arg("prefix")
                .arg("-g")
                .output(),
        )
        .await
        {
            if out.status.success() {
                let prefix = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if prefix.is_empty() {
                    continue;
                }
                for name in ["kimi.cmd", "kimi.exe", "kimi"] {
                    let path = Path::new(&prefix).join(name);
                    if path.is_file() {
                        return Some(path);
                    }
                }
            }
            break; // npm 找到了但没搜到 kimi，不必再试别名
        }
    }
    None
}

/// Unix（macOS / Linux）候选路径：`sh -c "command -v kimi"` → kimi_code_home/bin → npm 全局 bin。
/// Unix 上可执行权限即文件本身，无需扩展名判断；`command -v` 已覆盖 PATH 解析。
#[cfg(not(windows))]
pub async fn find_kimi_executable(kimi_home: &Path) -> Option<PathBuf> {
    // 1. sh -c "command -v kimi"
    if let Ok(Ok(out)) = tokio::time::timeout(
        CLI_CMD_TIMEOUT,
        tokio::process::Command::new("sh")
            .args(["-c", "command -v kimi"])
            .output(),
    )
    .await
    {
        if out.status.success() {
            let path = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
            if path.is_file() {
                return Some(path);
            }
        }
    }
    // 2. {kimi_code_home}/bin
    let path = kimi_home.join("bin").join("kimi");
    if path.is_file() {
        return Some(path);
    }
    // 3. npm 全局目录（Unix 上可执行文件在 {prefix}/bin 下）
    if let Ok(Ok(out)) = tokio::time::timeout(
        CLI_CMD_TIMEOUT,
        tokio::process::Command::new("npm")
            .arg("prefix")
            .arg("-g")
            .output(),
    )
    .await
    {
        if out.status.success() {
            let prefix = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !prefix.is_empty() {
                let path = Path::new(&prefix).join("bin").join("kimi");
                if path.is_file() {
                    return Some(path);
                }
            }
        }
    }
    None
}

/// `kimi --version`，5 秒超时（避免悬挂）。
/// .cmd/.bat 不能直接 CreateProcess，需经 cmd.exe 执行；
/// 命令行用 libuv 同款 `cmd /d /s /c ""path" args"` 形式 + raw_arg 原样传递
/// （Rust 的自动转义会破坏内层引号，导致 cmd 把引号当成程序名的一部分）。
#[cfg(windows)]
pub async fn current_cli_version(exe: &Path) -> Option<String> {
    let exe = exe.to_path_buf();
    let needs_cmd = exe
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("cmd") || e.eq_ignore_ascii_case("bat"))
        .unwrap_or(false);
    let out = tokio::time::timeout(
        CLI_CMD_TIMEOUT,
        tokio::task::spawn_blocking(move || {
            if needs_cmd {
                use std::os::windows::process::CommandExt;
                std::process::Command::new("cmd.exe")
                    .args(["/d", "/s", "/c"])
                    .raw_arg(format!("\"\"{}\" --version\"", exe.display()))
                    .output()
            } else {
                std::process::Command::new(&exe).arg("--version").output()
            }
        }),
    )
    .await
    .ok()?
    .ok()?
    .ok()?;
    if !out.status.success() {
        return None;
    }
    parse_version(&String::from_utf8_lossy(&out.stdout))
}

/// Unix（macOS / Linux）：无 .cmd 包装问题，直接执行。
#[cfg(not(windows))]
pub async fn current_cli_version(exe: &Path) -> Option<String> {
    let exe = exe.to_path_buf();
    let out = tokio::time::timeout(
        CLI_CMD_TIMEOUT,
        tokio::task::spawn_blocking(move || {
            std::process::Command::new(&exe).arg("--version").output()
        }),
    )
    .await
    .ok()?
    .ok()?
    .ok()?;
    if !out.status.success() {
        return None;
    }
    parse_version(&String::from_utf8_lossy(&out.stdout))
}

fn parse_changelog_version(text: &str) -> Option<String> {
    // 优先取标题行（## x.y.z）里的版本号
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            if let Some(v) = parse_version(trimmed) {
                return Some(v);
            }
        }
    }
    parse_version(text)
}

/// 从官方 changelog 页取最新版本号：Range 请求前 8192 字节，多候选 URL 回退。
pub async fn latest_cli_version(client: &reqwest::Client) -> Option<String> {
    for url in CLI_CHANGELOG_URLS {
        let Ok(resp) = client
            .get(url)
            .header(reqwest::header::RANGE, "bytes=0-8192")
            .send()
            .await
        else {
            continue;
        };
        let status = resp.status().as_u16();
        // 200（忽略 Range）与 206（部分内容）都接受
        if !(200..300).contains(&status) {
            continue;
        }
        if let Ok(text) = resp.text().await {
            if let Some(v) = parse_changelog_version(&text) {
                return Some(v);
            }
        }
    }
    None
}

pub async fn check_cli_update(kimi_home: &Path, client: &reqwest::Client) -> CliUpdateInfo {
    let exe = find_kimi_executable(kimi_home).await;
    let current = match &exe {
        Some(path) => current_cli_version(path).await,
        None => None,
    };
    let latest = latest_cli_version(client).await;
    let update_available = match (&current, &latest) {
        (Some(c), Some(l)) => compare_versions(c, l) == Ordering::Less,
        _ => false,
    };
    CliUpdateInfo {
        current,
        latest,
        update_available,
    }
}

/// App 最新版本 tag：先走 releases/latest 的 302 Location（避开 API 限流），GitHub API 作回退。
pub async fn fetch_latest_app_release() -> Result<String, String> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(HTTP_TIMEOUT)
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| e.to_string())?;

    let url = format!("https://github.com/{APP_REPO_OWNER}/{APP_REPO_NAME}/releases/latest");
    if let Ok(resp) = client.get(&url).send().await {
        if resp.status() == reqwest::StatusCode::FOUND {
            let location = resp
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());
            if let Some(location) = location {
                if let Some(tag) = location.rsplit('/').next().filter(|t| !t.is_empty()) {
                    return Ok(tag.trim_start_matches(['v', 'V']).to_string());
                }
            }
            return Err("302 响应缺少有效 Location".into());
        }
    }

    // 回退：GitHub API（有速率限制）
    let api =
        format!("https://api.github.com/repos/{APP_REPO_OWNER}/{APP_REPO_NAME}/releases/latest");
    let resp = client.get(&api).send().await.map_err(|e| e.to_string())?;
    let v: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    v.get("tag_name")
        .and_then(|t| t.as_str())
        .map(|t| t.trim_start_matches(['v', 'V']).to_string())
        .ok_or_else(|| "API 响应缺少 tag_name".to_string())
}

pub fn build_app_update_info(latest: Option<String>, error: Option<String>) -> AppUpdateInfo {
    let current = env!("CARGO_PKG_VERSION").to_string();
    let update_available = latest
        .as_deref()
        .map(|l| compare_versions(&current, l) == Ordering::Less)
        .unwrap_or(false);
    AppUpdateInfo {
        current,
        latest,
        update_available,
        release_url: format!("https://github.com/{APP_REPO_OWNER}/{APP_REPO_NAME}/releases"),
        error,
    }
}

/// 启动时后台检查一次更新（CLI + App），有新版发系统通知。不向外抛错。
pub async fn background_update_check(app: &AppHandle) {
    let (home, client) = {
        let state = app.state::<AppState>();
        (state.kimi_home.clone(), state.http.clone())
    };

    let cli = check_cli_update(&home, &client).await;
    if cli.update_available {
        let body = format!(
            "Kimi Code CLI 有新版本 {}（当前 {}）",
            cli.latest.unwrap_or_default(),
            cli.current.unwrap_or_else(|| "未知".into())
        );
        let _ = app
            .notification()
            .builder()
            .title("KimiCodeBar 更新提示")
            .body(body)
            .show();
    }

    if let Ok(latest) = fetch_latest_app_release().await {
        if compare_versions(env!("CARGO_PKG_VERSION"), &latest) == Ordering::Less {
            let _ = app
                .notification()
                .builder()
                .title("KimiCodeBar 更新提示")
                .body(format!(
                    "KimiCodeBar 有新版本 {latest}，请前往 Releases 页下载"
                ))
                .show();
        }
    }
}
