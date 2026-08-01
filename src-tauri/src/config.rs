//! 路径解析：应用配置目录与 kimi-code 主目录。
//!
//! 优先级（各平台一致，环境变量覆盖最高优先）：
//! - 配置目录：`KIMICODEBAR_CONFIG_DIR` > 平台默认目录 > 临时目录兜底
//!   - Windows：`%APPDATA%\KimiCodeBar`
//!   - macOS：`$HOME/Library/Application Support/KimiCodeBar`
//!   - Linux：`$XDG_CONFIG_HOME/KimiCodeBar`（未设置则 `~/.config/KimiCodeBar`）
//! - kimi-code 主目录：`KIMI_CODE_HOME` > 平台默认目录 > 临时目录兜底
//!   - Windows：`%USERPROFILE%\.kimi-code`
//!   - macOS / Linux：`$HOME/.kimi-code`

use std::path::PathBuf;

pub const CONFIG_DIR_ENV: &str = "KIMICODEBAR_CONFIG_DIR";
pub const KIMI_HOME_ENV: &str = "KIMI_CODE_HOME";

/// 环境变量取值（空白视为未设置）。
fn env_var_non_empty(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.trim().is_empty())
}

/// 应用自身的配置目录（settings.json / credentials.json / scan-state.json 所在目录）。
pub fn config_dir() -> PathBuf {
    if let Some(dir) = env_var_non_empty(CONFIG_DIR_ENV) {
        return PathBuf::from(dir);
    }
    platform_config_dir().unwrap_or_else(|| std::env::temp_dir().join("KimiCodeBar"))
}

#[cfg(windows)]
fn platform_config_dir() -> Option<PathBuf> {
    env_var_non_empty("APPDATA").map(|appdata| PathBuf::from(appdata).join("KimiCodeBar"))
}

#[cfg(target_os = "macos")]
fn platform_config_dir() -> Option<PathBuf> {
    env_var_non_empty("HOME").map(|home| {
        PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("KimiCodeBar")
    })
}

#[cfg(target_os = "linux")]
fn platform_config_dir() -> Option<PathBuf> {
    if let Some(xdg) = env_var_non_empty("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(xdg).join("KimiCodeBar"));
    }
    env_var_non_empty("HOME").map(|home| PathBuf::from(home).join(".config").join("KimiCodeBar"))
}

/// kimi-code CLI 的主目录（与 CLI 共享，包含 device_id / sessions / skills 等）。
pub fn kimi_code_home() -> PathBuf {
    if let Some(dir) = env_var_non_empty(KIMI_HOME_ENV) {
        return PathBuf::from(dir);
    }
    platform_kimi_home().unwrap_or_else(|| std::env::temp_dir().join(".kimi-code"))
}

#[cfg(windows)]
fn platform_kimi_home() -> Option<PathBuf> {
    env_var_non_empty("USERPROFILE").map(|profile| PathBuf::from(profile).join(".kimi-code"))
}

#[cfg(not(windows))]
fn platform_kimi_home() -> Option<PathBuf> {
    env_var_non_empty("HOME").map(|home| PathBuf::from(home).join(".kimi-code"))
}
