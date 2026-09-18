use crate::settings::{self, PetSettings, PetState, SettingsChange};
use crate::{broadcast, tray};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, WebviewWindow, Window, WindowEvent};

const COMPACT_WINDOW_WIDTH: f64 = 340.0;
const WIDE_WINDOW_WIDTH: f64 = 520.0;
const PET_WINDOW_HEIGHT: f64 = 480.0;
const WIDE_MONITOR_MIN_WIDTH: u32 = 2560;

#[derive(Debug, PartialEq, serde::Serialize)]
pub struct CursorPosition {
    x: f64,
    y: f64,
}

fn logical_cursor_position(
    cursor: tauri::PhysicalPosition<f64>,
    origin: tauri::PhysicalPosition<i32>,
    scale: f64,
) -> CursorPosition {
    CursorPosition {
        x: (cursor.x - f64::from(origin.x)) / scale,
        y: (cursor.y - f64::from(origin.y)) / scale,
    }
}

fn pet_window_width(monitor_physical_width: u32) -> f64 {
    if monitor_physical_width >= WIDE_MONITOR_MIN_WIDTH {
        WIDE_WINDOW_WIDTH
    } else {
        COMPACT_WINDOW_WIDTH
    }
}

pub fn sync_pet_window_size(window: &WebviewWindow) -> Result<(), String> {
    let monitor = window
        .current_monitor()
        .map_err(|error| error.to_string())?
        .or(window
            .primary_monitor()
            .map_err(|error| error.to_string())?);
    let Some(monitor) = monitor else {
        return Ok(());
    };

    let target_width = pet_window_width(monitor.size().width);
    let scale = window.scale_factor().map_err(|error| error.to_string())?;
    let current = window.inner_size().map_err(|error| error.to_string())?;
    let current_logical_width = f64::from(current.width) / scale;
    if (current_logical_width - target_width).abs() < 0.5 {
        return Ok(());
    }

    window
        .set_size(tauri::LogicalSize::new(target_width, PET_WINDOW_HEIGHT))
        .map_err(|error| error.to_string())
}

pub fn get_cursor_position(window: &tauri::WebviewWindow) -> Result<CursorPosition, String> {
    // 三个值都从原生窗口读取，跨屏和拖动后不沿用旧的窗口位置/DPI。
    // 屏幕物理坐标先减客户区原点，再除缩放率，得到可与 DOMRect 对齐的坐标。
    let cursor = window
        .cursor_position()
        .map_err(|error| error.to_string())?;
    let origin = window.inner_position().map_err(|error| error.to_string())?;
    let scale = window.scale_factor().map_err(|error| error.to_string())?;
    Ok(logical_cursor_position(cursor, origin, scale))
}

pub fn get_settings(app: &AppHandle) -> Result<PetSettings, String> {
    let state = app.state::<PetState>();
    let settings = state.lock().map_err(|error| error.to_string())?;
    Ok(settings.clone())
}

// JS 命令和 Rust 托盘都调用这里，因此窗口行为与状态只有一套。
pub fn update_settings(app: &AppHandle, change: SettingsChange) -> Result<PetSettings, String> {
    let window = app.get_webview_window("main").ok_or("桌宠窗口不存在")?;
    if matches!(change, SettingsChange::Visible(false)) && app.tray_by_id(tray::TRAY_ID).is_none() {
        return Err("托盘未就绪，暂时不能隐藏桌宠；可以直接退出".into());
    }
    let (snapshot, persist_error) = {
        let state = app.state::<PetState>();
        let mut current = state.lock().map_err(|error| error.to_string())?;
        let stored_before = current.to_stored();
        let snapshot = current.update(change, || {
            let result = match change {
                SettingsChange::Visible(true) => window.unminimize().and_then(|_| window.show()),
                SettingsChange::Visible(false) => window.hide(),
                SettingsChange::AlwaysOnTop(enabled) => window.set_always_on_top(enabled),
                // 帧率与主动互动都不需要动窗口，只改状态与落盘。
                SettingsChange::MaxFps(_)
                | SettingsChange::ProactiveEnabled(_)
                | SettingsChange::BrowserTitleEnabled(_) => Ok(()),
            };
            result.map_err(|error| error.to_string())
        })?;
        // 落盘放在锁内：两次并发修改（托盘一次、设置窗口一次）不会写岔顺序。
        // 另外只有影响磁盘内容的改动才写——显示/隐藏属于会话状态，不进文件。
        let persist_error = if snapshot.to_stored() == stored_before {
            None
        } else {
            settings::save(app, &snapshot).err()
        };
        (snapshot, persist_error)
    }; // 离开作用域就释放 Mutex 锁，再通知 Vue 和托盘。

    // 写入失败不推翻本次修改：窗口操作已经发生，状态也已经是新的，
    // 唯一真实的后果是「重启后会丢」，所以如实说清楚。
    if let Some(error) = persist_error {
        report_error(
            app,
            &format!("设置已生效，但写入磁盘失败（重启后会丢失）：{error}"),
        );
    }
    // 系统操作已经成功；通知失败记录错误，不伪装成系统操作失败。
    for failure in broadcast::emit_all(app, "pet-settings-changed", &snapshot) {
        report_error(app, &format!("设置通知失败：{failure}"));
    }
    if let Err(error) = tray::sync_menu(app, &snapshot) {
        report_error(app, &format!("托盘状态更新失败：{error}"));
    }
    if matches!(change, SettingsChange::Visible(true)) {
        if let Err(error) = window.set_focus() {
            eprintln!("[Rust] 窗口已显示，但获取焦点失败：{error}");
        }
    }
    #[cfg(debug_assertions)]
    println!("[Rust] 桌宠设置：{snapshot:?}");
    Ok(snapshot)
}

/// 启动时把磁盘上的设置恢复到内存、窗口与托盘。
///
/// 三件事都要做，漏掉任何一件都会出现「设置文件是对的，但程序行为不对」：
///
/// 1. 把值写进 `PetState`，之后前端读到、后续修改都基于它；
/// 2. 应用到窗口——`tauri.conf.json` 里 `alwaysOnTop` 是 `true`，
///    用户上次关掉的话必须在这一步改回来；
/// 3. 同步托盘勾选，否则托盘会显示一份与内存不一致的勾选状态。
pub fn restore_settings(app: &AppHandle) {
    let (restored, note) = settings::load(app);
    if let Some(note) = note {
        report_startup_error(app, &format!("读取设置时发生回退：{note}"));
    }

    match app.state::<PetState>().lock() {
        Ok(mut current) => *current = restored.clone(),
        Err(error) => report_startup_error(app, &format!("恢复设置失败：{error}")),
    }
    if let Some(window) = app.get_webview_window("main") {
        if let Err(error) = window.set_always_on_top(restored.always_on_top) {
            report_startup_error(app, &format!("恢复置顶状态失败：{error}"));
        }
    }
    if let Err(error) = tray::sync_menu(app, &restored) {
        report_startup_error(app, &format!("恢复托盘勾选状态失败：{error}"));
    }
    #[cfg(debug_assertions)]
    println!("[Rust] 启动时恢复设置：{restored:?}");
}

pub fn report_error(app: &AppHandle, message: &str) {
    eprintln!("[Rust] {message}");
    let _ = broadcast::emit_if_open(app, broadcast::MAIN, "pet-desktop-error", &message);
}

/// 启动期（页面还没挂载）的提示先攒在这里，等页面加载完成再补发。
///
/// 为什么需要它：`setup` 跑在页面加载**之前**，那一瞬间发出的事件没有任何监听者，
/// 会被直接丢掉。结果是「设置文件坏了，程序却看起来一切正常」——而这正是
/// Phase 7 要防的情况。
#[derive(Default)]
pub struct StartupNotices(Mutex<Vec<String>>);

impl StartupNotices {
    fn push(&self, message: &str) {
        match self.0.lock() {
            Ok(mut pending) => pending.push(message.to_string()),
            Err(error) => eprintln!("[Rust] 记录启动提示失败：{error}"),
        }
    }

    fn drain(&self) -> Vec<String> {
        match self.0.lock() {
            Ok(mut pending) => std::mem::take(&mut *pending),
            Err(_) => Vec::new(),
        }
    }
}

/// 记录一条启动期提示：打印到控制台，同时排队等页面就绪后补发。
pub fn report_startup_error(app: &AppHandle, message: &str) {
    eprintln!("[Rust] {message}");
    app.state::<StartupNotices>().push(message);
}

/// 把启动期攒下的提示补发给主窗口。由 [`on_page_load`] 在页面加载完成时调用。
pub fn flush_startup_notices(app: &AppHandle) {
    for message in app.state::<StartupNotices>().drain() {
        let _ = broadcast::emit_if_open(app, broadcast::MAIN, "pet-desktop-error", &message);
    }
}

/// `Builder::on_page_load` 的回调：主窗口加载完成时补发启动提示。
pub fn on_page_load(
    webview: &tauri::Webview<tauri::Wry>,
    payload: &tauri::webview::PageLoadPayload<'_>,
) {
    if webview.label() != broadcast::MAIN {
        return;
    }
    if matches!(payload.event(), tauri::webview::PageLoadEvent::Finished) {
        if let Some(window) = webview.app_handle().get_webview_window("main") {
            if let Err(error) = sync_pet_window_size(&window) {
                report_startup_error(
                    webview.app_handle(),
                    &format!("按显示器调整桌宠窗口失败：{error}"),
                );
            }
        }
        #[cfg(debug_assertions)]
        println!("[Rust] 主窗口页面加载完成");
        flush_startup_notices(webview.app_handle());
    }
}

pub fn on_window_event(window: &Window, event: &WindowEvent) {
    // 对话窗口关闭：没人收流式事件了，正在跑的请求也一起停掉。
    // 既省 token，也避免一条回答只在历史里写下一半却没人知道原因。
    if window.label() == crate::chat_window::CHAT_LABEL {
        if matches!(event, WindowEvent::Destroyed) {
            crate::chat::cancel_all(window.app_handle());
        }
        return;
    }
    if window.label() != "main" {
        return;
    }
    if matches!(
        event,
        WindowEvent::Moved(_) | WindowEvent::ScaleFactorChanged { .. }
    ) {
        if let Some(webview_window) = window.app_handle().get_webview_window("main") {
            if let Err(error) = sync_pet_window_size(&webview_window) {
                report_error(
                    window.app_handle(),
                    &format!("按显示器调整桌宠窗口失败：{error}"),
                );
            }
        }
    }
    // Windows 最小化/还原会触发尺寸事件；这里只报告真实最小化状态。
    // 不读取 PetState 的锁，避免与正在执行的窗口操作互相等待。
    if let WindowEvent::Resized(_) = event {
        match window.is_minimized() {
            Ok(minimized) => {
                let _ = window.emit("pet-window-minimized", minimized);
            }
            Err(error) => {
                report_error(window.app_handle(), &format!("读取最小化状态失败：{error}"))
            }
        }
    }
    if let WindowEvent::CloseRequested { api, .. } = event {
        // 只有托盘创建成功，才允许关闭改为隐藏；显式退出不经过这里。
        if window.app_handle().tray_by_id(tray::TRAY_ID).is_some() {
            api.prevent_close();
            if let Err(error) = update_settings(window.app_handle(), SettingsChange::Visible(false))
            {
                report_error(window.app_handle(), &error);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pet_window_width_uses_monitor_physical_resolution() {
        assert_eq!(pet_window_width(1920), 340.0);
        assert_eq!(pet_window_width(2559), 340.0);
        assert_eq!(pet_window_width(2560), 520.0);
        assert_eq!(pet_window_width(3840), 520.0);
    }

    #[test]
    fn cursor_uses_client_origin_and_monitor_scale() {
        assert_eq!(
            logical_cursor_position(
                tauri::PhysicalPosition::new(1440.0, 990.0),
                tauri::PhysicalPosition::new(1200, 600),
                1.5,
            ),
            CursorPosition { x: 160.0, y: 260.0 },
        );
    }

    #[test]
    fn cursor_supports_negative_screens_and_positions_outside_window() {
        assert_eq!(
            logical_cursor_position(
                tauri::PhysicalPosition::new(-1920.0, -200.0),
                tauri::PhysicalPosition::new(-1600, 100),
                2.0,
            ),
            CursorPosition {
                x: -160.0,
                y: -150.0
            },
        );
        assert_eq!(
            logical_cursor_position(
                tauri::PhysicalPosition::new(-1920.0, -200.0),
                tauri::PhysicalPosition::new(-2000, -400),
                1.0,
            ),
            CursorPosition { x: 80.0, y: 200.0 },
        );
    }
}
