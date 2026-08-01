//! KimiCodeBar Windows 版 —— Tauri 2 应用入口与全局状态。

pub mod archive;
pub mod commands;
pub mod config;
pub mod creds;
pub mod kimi;
pub mod local_usage;
pub mod opencode_go;
pub mod polling;
pub mod quota;
pub mod skills;
pub mod storage;
pub mod tray;
pub mod update;

use std::io;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use tauri::{App, AppHandle, Emitter, Manager, WebviewWindow, WindowEvent};
use tauri_plugin_opener::OpenerExt;
use tokio::sync::watch;

use crate::kimi::identity::DeviceIdentity;
use crate::local_usage::LocalUsageReport;
use crate::quota::QuotaInfo;
use crate::storage::{AppSettings, WidgetPosition};

/// 托盘图标（普通 / 警告），构建时内嵌。
/// 注：macOS 上彩色图标可用但不遵循系统明暗模板，后续可改用 template image
/// （`Image::from_bytes` + `tray.set_icon_as_template(true)`），Windows 无此概念。
pub const TRAY_ICON_NORMAL: &[u8] = include_bytes!("../icons/tray-normal.png");
pub const TRAY_ICON_WARN: &[u8] = include_bytes!("../icons/tray-warn.png");

/// 全局共享状态（tauri manage + State 传递）。
pub struct AppState {
    pub config_dir: PathBuf,
    pub kimi_home: PathBuf,
    pub http: reqwest::Client,
    pub device: DeviceIdentity,
    pub settings: RwLock<AppSettings>,
    pub quota: RwLock<Option<QuotaInfo>>,
    /// 当前是否处于低额告警状态（用于托盘图标切换去抖）
    pub low_quota: AtomicBool,
    pub tray: Mutex<Option<tauri::tray::TrayIcon<tauri::Wry>>>,
    /// 轮询任务重启信号（设置变更时发送）
    pub poll_restart: watch::Sender<()>,
    /// 进行中的 Device Flow 取消信号
    pub login_cancel: Mutex<Option<watch::Sender<bool>>>,
    pub local_usage: RwLock<Option<LocalUsageReport>>,
    pub local_usage_scan_at: Mutex<Option<Instant>>,
    pub opencode_go_exchange_rate: Arc<Mutex<opencode_go::ExchangeRateCache>>,
}

/// 显示面板窗口并定位到工作区右下角（窗口上方留任务栏空间）。
pub fn show_panel(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        position_bottom_right(&win);
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
}

/// 将窗口定位到工作区右下角（窗口上方留任务栏空间，右侧留 12px）。
fn position_bottom_right(win: &WebviewWindow) {
    let Ok(Some(monitor)) = win.primary_monitor() else {
        return;
    };
    let Ok(win_size) = win.outer_size() else {
        return;
    };
    let area = monitor.work_area();
    let x = area.position.x + area.size.width as i32 - win_size.width as i32 - 12;
    let y = area.position.y + area.size.height as i32 - win_size.height as i32 - 12;
    let _ = win.set_position(tauri::PhysicalPosition::new(x, y));
}

/// 小部件窗口置为不激活（Windows）：点击/显示不抢焦点，保持普通窗口层语义。
/// 注意：不用 tao 的 set_always_on_bottom（HWND_BOTTOM）——实测它会把窗口压到
/// 桌面壁纸层（WorkerW）之下导致不可见（Win+D/重启后不恢复、开窗口才出现）。
/// webview 异步初始化会重置宿主窗口样式，因此保活任务每轮幂等重设。
#[cfg(target_os = "windows")]
fn ensure_widget_noactivate(win: &WebviewWindow) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE,
    };
    let Ok(hwnd) = win.hwnd() else {
        return;
    };
    unsafe {
        let style = GetWindowLongPtrW(hwnd.0, GWL_EXSTYLE);
        if style & WS_EX_NOACTIVATE as isize == 0 {
            let _ = SetWindowLongPtrW(hwnd.0, GWL_EXSTYLE, style | WS_EX_NOACTIVATE as isize);
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn ensure_widget_noactivate(_win: &WebviewWindow) {}

/// 显示小部件但不激活（Windows）：tao 的 show() 用 SW_SHOW 会把窗口抬到普通层
/// 顶部（启动/开开关瞬间可能盖住已有窗口）。tao 的 is_visible() 读 WS_VISIBLE
/// 样式（util.rs IsWindowVisible）而非自跟踪标志，FFI ShowWindow(SW_SHOWNOACTIVATE)
/// 后状态仍一致；非 Windows 平台保持 win.show()。
#[cfg(target_os = "windows")]
fn show_widget_noactivate(win: &WebviewWindow) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_SHOWNOACTIVATE};
    let Ok(hwnd) = win.hwnd() else {
        let _ = win.show();
        return;
    };
    unsafe {
        let _ = ShowWindow(hwnd.0, SW_SHOWNOACTIVATE);
    }
}

#[cfg(not(target_os = "windows"))]
fn show_widget_noactivate(win: &WebviewWindow) {
    let _ = win.show();
}

/// 隐藏小部件（Windows）：与 show_widget_noactivate 对称的纯 FFI 隐藏。
/// 不能直接 win.hide()——tao 的 set_visible 靠 WindowFlags::VISIBLE 的 diff 驱动
/// SW_HIDE（window_state.rs apply_diff 空 diff 早退），而显示已走 FFI 未置位该标志，
/// diff 为空会导致隐藏不执行；非 Windows 平台保持 win.hide()。
#[cfg(target_os = "windows")]
fn hide_widget(win: &WebviewWindow) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};
    let Ok(hwnd) = win.hwnd() else {
        let _ = win.hide();
        return;
    };
    unsafe {
        let _ = ShowWindow(hwnd.0, SW_HIDE);
    }
}

#[cfg(not(target_os = "windows"))]
fn hide_widget(win: &WebviewWindow) {
    let _ = win.hide();
}

/// 恢复最小化的小部件（Windows）：纯 FFI SW_RESTORE，不经过 tao 的 apply_diff。
/// 不能直接 win.unminimize()——tao 的 apply_diff 在 MINIMIZED diff 之后有无条件
/// `if !new.contains(VISIBLE) { SW_HIDE }`（window_state.rs），而显示走 FFI 从未
/// 置位 VISIBLE 标志，恢复后窗口会被立刻隐藏且无路径再恢复。
/// SW_RESTORE 对未最小化的窗口无害（保活任务仅在 is_minimized 为 true 时调用）。
#[cfg(target_os = "windows")]
fn restore_widget(win: &WebviewWindow) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_RESTORE};
    let Ok(hwnd) = win.hwnd() else {
        let _ = win.unminimize();
        return;
    };
    unsafe {
        let _ = ShowWindow(hwnd.0, SW_RESTORE);
    }
}

#[cfg(not(target_os = "windows"))]
fn restore_widget(win: &WebviewWindow) {
    let _ = win.unminimize();
}

/// 确保小部件位于 Windows 桌面窗口正上方。
///
/// Win+D 不会最小化或隐藏带 `WS_EX_NOACTIVATE` 的小部件，而是把 `Progman`
/// 提到它上方。此时 `is_visible() == true` 且 `is_minimized() == false`，只检查
/// 显隐/最小化状态无法发现窗口已被桌面遮住。
///
/// 这里仅在小部件确实落到桌面层下方时调整一次：取 `Progman` 的前一个窗口作为
/// 插入锚点，把小部件放到二者之间。这样小部件重新位于桌面之上，同时仍低于所有
/// 普通应用窗口；不会像 `HWND_TOP` 那样盖住应用，也不会像 `HWND_BOTTOM` 那样掉到
/// 壁纸层下方。小部件本来就在桌面之上时不做任何操作，避免保活任务干扰拖拽。
#[cfg(target_os = "windows")]
fn ensure_widget_above_desktop(win: &WebviewWindow) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetShellWindow, GetWindow, SetWindowPos, GW_HWNDPREV, SWP_ASYNCWINDOWPOS, SWP_NOACTIVATE,
        SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW,
    };

    let Ok(hwnd) = win.hwnd() else {
        return;
    };
    unsafe {
        let desktop = GetShellWindow();
        if desktop.is_null() || hwnd.0 == desktop {
            return;
        }

        // 从桌面向 Z 序顶部遍历；能遇到小部件说明它已在桌面上方，无需重排。
        let mut current = GetWindow(desktop, GW_HWNDPREV);
        for _ in 0..2048 {
            if current.is_null() {
                break;
            }
            if current == hwnd.0 {
                return;
            }
            current = GetWindow(current, GW_HWNDPREV);
        }

        // 小部件在桌面下方：插入到桌面原前驱之后，即桌面正上方。
        // anchor 为空等价于 HWND_TOP（桌面已在普通层顶部的极端兜底）。
        let anchor = GetWindow(desktop, GW_HWNDPREV);
        let _ = SetWindowPos(
            hwnd.0,
            anchor,
            0,
            0,
            0,
            0,
            SWP_ASYNCWINDOWPOS | SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
        );
    }
}

#[cfg(not(target_os = "windows"))]
fn ensure_widget_above_desktop(_win: &WebviewWindow) {}

/// 按设置显示/隐藏桌面小部件；无记忆位置或记忆坐标已不在任何屏幕工作区内时，
/// 回退到工作区右下角定位。
pub fn apply_widget_visibility(app: &AppHandle, enabled: bool) {
    let Some(win) = app.get_webview_window("widget") else {
        return;
    };
    if enabled {
        let pos = app
            .state::<AppState>()
            .settings
            .read()
            .unwrap()
            .widget_position;
        match pos {
            Some(p) if widget_position_visible(&win, p) => {
                let _ = win.set_position(tauri::PhysicalPosition::new(p.x, p.y));
            }
            // 无记忆位置或记忆坐标已不在屏幕工作区内：右下角定位；
            // 窗口尺寸会在前端渲染后自适应收缩，延时重定位一次保持贴右下角
            Some(_) | None => {
                position_bottom_right(&win);
                let handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    if let Some(win) = handle.get_webview_window("widget") {
                        position_bottom_right(&win);
                    }
                });
            }
        }
        // 显示但不激活（SW_SHOWNOACTIVATE，Windows），避免瞬时抬升到普通层顶部
        show_widget_noactivate(&win);
        ensure_widget_noactivate(&win);
        ensure_widget_above_desktop(&win);
        // 注意：不再 set_always_on_bottom（HWND_BOTTOM 会把窗口压到桌面壁纸层
        // WorkerW 之下导致不可见）；窗口保持在普通层，由 WS_EX_NOACTIVATE 保证
        // 点击不激活不排顶，任何普通窗口打开时都会盖住它
    } else {
        hide_widget(&win);
    }
}

/// 记忆坐标是否仍可见：窗口需完整落在某显示器工作区内
/// （含不可见阴影边距的 outer_size 判定；获取失败时退回仅校验左上角）。
/// 否则漂移到屏缘的坐标（左上角在屏内、窗口大半在屏外）也能通过校验，
/// 导致重启后恢复到近乎不可见的位置。
fn widget_position_visible(win: &WebviewWindow, pos: WidgetPosition) -> bool {
    let Ok(Some(monitor)) = win.monitor_from_point(pos.x as f64, pos.y as f64) else {
        return false;
    };
    let area = monitor.work_area();
    let left = area.position.x;
    let top = area.position.y;
    let right = area.position.x + area.size.width as i32;
    let bottom = area.position.y + area.size.height as i32;
    let Ok(size) = win.outer_size() else {
        // 获取失败兜底：仅校验窗口左上角在工作区内
        return pos.x >= left && pos.y >= top && pos.x < right && pos.y < bottom;
    };
    pos.x >= left
        && pos.y >= top
        && pos.x + size.width as i32 <= right
        && pos.y + size.height as i32 <= bottom
}

/// 把内存中的设置原子写盘（内存为唯一权威）。调用方先更新内存（settings 写锁）、
/// 释放锁后再调用本函数；所有落盘统一走这里，杜绝"读盘旧值改回写"互相覆盖。
pub(crate) fn persist_settings(state: &AppState) -> io::Result<()> {
    let settings = state.settings.read().unwrap();
    storage::save_settings(&state.config_dir, &settings)
}

fn setup_tray(app: &App) -> Result<(), Box<dyn std::error::Error>> {
    use tauri::menu::{MenuBuilder, MenuItemBuilder};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    let console = MenuItemBuilder::with_id("console", "打开控制台").build(app)?;
    let refresh = MenuItemBuilder::with_id("refresh", "刷新").build(app)?;
    let settings = MenuItemBuilder::with_id("settings", "设置").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "退出").build(app)?;
    let menu = MenuBuilder::new(app)
        .items(&[&console, &refresh, &settings, &quit])
        .build()?;

    let icon = tauri::image::Image::from_bytes(TRAY_ICON_NORMAL)
        .map_err(|e| format!("托盘图标解码失败: {e}"))?;

    let tray = TrayIconBuilder::new()
        .icon(icon)
        .menu(&menu)
        .tooltip("KimiCodeBar")
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "console" => {
                let _ = app
                    .opener()
                    .open_url("https://kimi.com/code/console", None::<&str>);
            }
            "refresh" => {
                let handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = polling::poll_once(&handle).await;
                });
            }
            "settings" => {
                // 打开独立设置窗口：每次打开都居中显示
                if let Some(win) = app.get_webview_window("settings") {
                    let _ = win.center();
                    let _ = win.show();
                    let _ = win.unminimize();
                    let _ = win.set_focus();
                }
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            // 左键点击 → 弹出面板
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_panel(tray.app_handle());
            }
        })
        .build(app)?;

    *app.state::<AppState>().tray.lock().unwrap() = Some(tray);
    Ok(())
}

fn setup_windows(app: &App) {
    // 面板：失焦隐藏；关闭请求转为隐藏
    if let Some(win) = app.get_webview_window("main") {
        let w = win.clone();
        win.on_window_event(move |event| match event {
            WindowEvent::Focused(false) => {
                let _ = w.hide();
            }
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let _ = w.hide();
            }
            _ => {}
        });
    }

    // 设置窗口：关闭请求转为隐藏（保留状态，下次打开居中重显）
    if let Some(win) = app.get_webview_window("settings") {
        let w = win.clone();
        win.on_window_event(move |event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = w.hide();
            }
        });
    }

    // 桌面小部件：拖动记忆位置（防抖写盘）；关闭请求转为隐藏并关闭开关。
    // 注意：不加失焦隐藏——桌面小部件失焦后仍需可见。
    if let Some(win) = app.get_webview_window("widget") {
        // 点击不激活（Windows）：不抢焦点、不排到普通层顶部，
        // 任何普通窗口打开时都自然盖住小部件
        ensure_widget_noactivate(&win);
        let w = win.clone();
        let app_handle = app.handle().clone();
        let last_write = Arc::new(Mutex::new(Instant::now()));
        // 保活任务使用的句柄（在 move 闭包捕获前 clone）
        let keepalive_handle = app_handle.clone();
        win.on_window_event(move |event| match event {
            WindowEvent::Moved(position) => {
                let pos = WidgetPosition {
                    x: position.x,
                    y: position.y,
                };
                {
                    let state = app_handle.state::<AppState>();
                    state.settings.write().unwrap().widget_position = Some(pos);
                }
                // 防抖：距上次触发写盘超过 500ms 才再触发。写盘任务统一从内存
                // 取最新设置落盘（persist_settings），拖动过程中即使多次触发，
                // 最终也以最后一次写入获胜。
                let should_write = {
                    let mut last = last_write.lock().unwrap();
                    if last.elapsed() >= Duration::from_millis(500) {
                        *last = Instant::now();
                        true
                    } else {
                        false
                    }
                };
                if should_write {
                    let handle = app_handle.clone();
                    tauri::async_runtime::spawn(async move {
                        tokio::time::sleep(Duration::from_millis(500)).await;
                        let state = handle.state::<AppState>();
                        let _ = persist_settings(&state);
                    });
                }
            }
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                hide_widget(&w);
                // 窗口状态与设置一致：关闭小部件即关闭开关（先改内存、再统一落盘）
                let state = app_handle.state::<AppState>();
                state.settings.write().unwrap().widget_enabled = false;
                let _ = persist_settings(&state);
                let updated = state.settings.read().unwrap().clone();
                // 通知已打开的设置窗口即时刷新小部件开关
                let _ = app_handle.emit(crate::commands::EVENT_SETTINGS_CHANGED, &updated);
            }
            _ => {}
        });

        // 低频保活任务（每 2s）：恢复系统操作造成的最小化；启动初始化若在首次
        // show 后再次隐藏窗口，也会重新显示；Win+D 把 Progman 提到小部件上方时，
        // 只把小部件插回桌面正上方。不做 HWND_BOTTOM 持续压底，避免掉入壁纸层或
        // 干扰拖拽；普通应用窗口仍处于小部件上方。
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(2)).await;
                let Some(win) = keepalive_handle.get_webview_window("widget") else {
                    continue;
                };
                if !keepalive_handle
                    .state::<AppState>()
                    .settings
                    .read()
                    .unwrap()
                    .widget_enabled
                {
                    continue;
                }
                if win.is_minimized().unwrap_or(false) {
                    restore_widget(&win);
                }
                if !win.is_visible().unwrap_or(false) {
                    show_widget_noactivate(&win);
                }
                // webview 初始化会重置宿主窗口样式，每轮幂等重设不激活样式
                ensure_widget_noactivate(&win);
                ensure_widget_above_desktop(&win);
            }
        });
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let config_dir = config::config_dir();
    let _ = std::fs::create_dir_all(&config_dir);
    let kimi_home = config::kimi_code_home();
    let settings = storage::load_settings(&config_dir);
    let device = DeviceIdentity::load(&kimi_home);
    let http = crate::kimi::client::build_http_client();
    let (poll_restart, _) = watch::channel(());

    let state = AppState {
        config_dir,
        kimi_home,
        http,
        device,
        settings: RwLock::new(settings),
        quota: RwLock::new(None),
        low_quota: AtomicBool::new(false),
        tray: Mutex::new(None),
        poll_restart,
        login_cancel: Mutex::new(None),
        local_usage: RwLock::new(None),
        local_usage_scan_at: Mutex::new(None),
        opencode_go_exchange_rate: Arc::new(Mutex::new(opencode_go::ExchangeRateCache::default())),
    };

    tauri::Builder::default()
        // single-instance 必须最先注册：第二个实例唤起已有面板并退出
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_panel(app);
        }))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_opener::init())
        .manage(state)
        .setup(|app| {
            setup_tray(app)?;
            setup_windows(app);
            polling::start(app.handle().clone());
            archive::start_archive_timer(app.handle().clone());
            // 启动时按设置显示桌面小部件。setup 阶段 webview 尚在异步初始化，
            // 立即 show 可能被后续初始化再次隐藏，因此延迟到首帧之后应用一次；
            // 2s 保活任务仍会兜底重显。
            if app
                .state::<AppState>()
                .settings
                .read()
                .unwrap()
                .widget_enabled
            {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    if handle
                        .state::<AppState>()
                        .settings
                        .read()
                        .unwrap()
                        .widget_enabled
                    {
                        apply_widget_visibility(&handle, true);
                    }
                });
            }
            // 启动时后台查一次更新
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                update::background_update_check(&handle).await;
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::get_login_state,
            commands::start_device_login,
            commands::cancel_login,
            commands::logout,
            commands::set_api_key,
            commands::get_masked_api_key,
            commands::set_login_method,
            commands::set_web_token,
            commands::clear_web_token,
            commands::get_web_token_configured,
            commands::get_monthly,
            commands::set_opencode_go_credentials,
            commands::clear_opencode_go_credentials,
            commands::get_opencode_go_configured,
            commands::get_opencode_go_usage,
            commands::refresh_quota,
            commands::get_quota,
            commands::get_local_usage,
            commands::refresh_local_usage,
            commands::get_archive_overview,
            commands::archive_sessions,
            commands::unarchive_session,
            commands::run_auto_archive_now,
            commands::get_skills,
            commands::read_skill,
            commands::reveal_in_explorer,
            commands::check_cli_update,
            commands::check_app_update,
            commands::quit_app,
            commands::open_url,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
