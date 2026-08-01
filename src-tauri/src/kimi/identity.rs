//! X-Msh-* 请求头构造：设备标识（device_id 与 kimi-code CLI 共享）。
//!
//! 平台分叉：
//! - Windows：COMPUTERNAME 取设备名，RtlGetVersion 取系统版本，型号区分 Windows 10/11
//! - macOS：型号固定 "macOS"，版本用 `sw_vers -productVersion`，设备名 HOSTNAME / `scutil --get ComputerName`
//! - Linux：型号固定 "Linux"，版本用 `uname -r`，设备名 /etc/hostname / HOSTNAME

use std::fs;
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct DeviceIdentity {
    pub device_id: String,
    pub device_name: String,
    pub device_model: String,
    pub os_version: String,
}

impl DeviceIdentity {
    pub fn load(kimi_home: &Path) -> Self {
        Self {
            device_id: load_or_create_device_id(kimi_home),
            device_name: device_name(),
            device_model: device_model(),
            os_version: os_version_string(),
        }
    }
}

/// 读取 `{kimi_code_home}/device_id`，不存在则生成 UUID v4 写入。（跨平台共用）
fn load_or_create_device_id(kimi_home: &Path) -> String {
    let path = kimi_home.join("device_id");
    if let Ok(content) = fs::read_to_string(&path) {
        let trimmed = content.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    let id = Uuid::new_v4().to_string();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(&path, &id);
    id
}

// ---- 设备名 ----

#[cfg(windows)]
fn device_name() -> String {
    std::env::var("COMPUTERNAME").unwrap_or_else(|_| "unknown".into())
}

/// macOS：环境变量 HOSTNAME → `scutil --get ComputerName` → "Mac"。
#[cfg(target_os = "macos")]
fn device_name() -> String {
    if let Ok(name) = std::env::var("HOSTNAME") {
        let name = name.trim();
        if !name.is_empty() {
            return name.to_string();
        }
    }
    command_stdout("scutil", &["--get", "ComputerName"]).unwrap_or_else(|| "Mac".into())
}

/// Linux：/etc/hostname → 环境变量 HOSTNAME → "Linux"。
#[cfg(target_os = "linux")]
fn device_name() -> String {
    if let Ok(name) = fs::read_to_string("/etc/hostname") {
        let name = name.trim();
        if !name.is_empty() {
            return name.to_string();
        }
    }
    if let Ok(name) = std::env::var("HOSTNAME") {
        let name = name.trim();
        if !name.is_empty() {
            return name.to_string();
        }
    }
    "Linux".into()
}

// ---- 系统版本 ----

/// 通过 ntdll 的 RtlGetVersion 获取真实系统版本（GetVersionEx 会被兼容性 shim 欺骗）。
/// 直接 extern 链接 ntdll，避免额外引入 windows-sys 的 LibraryLoader feature。
#[cfg(windows)]
mod ntdll {
    use windows_sys::Win32::System::SystemInformation::OSVERSIONINFOW;

    #[link(name = "ntdll")]
    extern "C" {
        pub fn RtlGetVersion(info: *mut OSVERSIONINFOW) -> i32;
    }
}

#[cfg(windows)]
fn os_version_parts() -> Option<(u32, u32, u32)> {
    use windows_sys::Win32::System::SystemInformation::OSVERSIONINFOW;

    unsafe {
        let mut info: OSVERSIONINFOW = std::mem::zeroed();
        info.dwOSVersionInfoSize = std::mem::size_of::<OSVERSIONINFOW>() as u32;
        if ntdll::RtlGetVersion(&mut info as *mut _) == 0 {
            Some((info.dwMajorVersion, info.dwMinorVersion, info.dwBuildNumber))
        } else {
            None
        }
    }
}

#[cfg(windows)]
fn os_version_string() -> String {
    os_version_parts()
        .map(|(major, minor, build)| format!("{major}.{minor}.{build}"))
        .unwrap_or_else(|| "unknown".into())
}

/// macOS：`sw_vers -productVersion`（如 14.5），失败回退 "14.0"。
#[cfg(target_os = "macos")]
fn os_version_string() -> String {
    command_stdout("sw_vers", &["-productVersion"]).unwrap_or_else(|| "14.0".into())
}

/// Linux：`uname -r`（内核版本），失败回退 "unknown"。
#[cfg(target_os = "linux")]
fn os_version_string() -> String {
    command_stdout("uname", &["-r"]).unwrap_or_else(|| "unknown".into())
}

/// 执行命令取 stdout 首行（trim 后非空才接受），仅 macOS / Linux 的设备信息探测用。
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn command_stdout(cmd: &str, args: &[&str]) -> Option<String> {
    let out = std::process::Command::new(cmd).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

// ---- 设备型号 ----

/// Windows 11 从 build 22000 开始。
#[cfg(windows)]
fn device_model() -> String {
    match os_version_parts() {
        Some((_, _, build)) if build >= 22000 => "Windows 11".into(),
        Some(_) => "Windows 10".into(),
        None => "Windows".into(),
    }
}

#[cfg(target_os = "macos")]
fn device_model() -> String {
    "macOS".into()
}

#[cfg(target_os = "linux")]
fn device_model() -> String {
    "Linux".into()
}

/// 给请求附加 X-Msh-* 头。Platform 固定为 kimi_code_cli，Version 取应用版本。
pub fn apply_headers(
    builder: reqwest::RequestBuilder,
    identity: &DeviceIdentity,
) -> reqwest::RequestBuilder {
    builder
        .header("X-Msh-Platform", "kimi_code_cli")
        .header("X-Msh-Version", env!("CARGO_PKG_VERSION"))
        .header("X-Msh-Device-Id", &identity.device_id)
        .header("X-Msh-Device-Name", &identity.device_name)
        .header("X-Msh-Device-Model", &identity.device_model)
        .header("X-Msh-Os-Version", &identity.os_version)
}
