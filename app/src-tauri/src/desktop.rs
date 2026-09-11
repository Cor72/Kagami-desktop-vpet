use crate::menu_layout;
use crate::settings::{PetSettings, PetState, SettingsChange};
use crate::tray;
use tauri::{AppHandle, Emitter, Manager, Window, WindowEvent};

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
    let snapshot = {
        let state = app.state::<PetState>();
        let mut settings = state.lock().map_err(|error| error.to_string())?;
        settings.update(change, || {
            let result = match change {
                SettingsChange::Visible(true) => window.unminimize().and_then(|_| window.show()),
                SettingsChange::Visible(false) => window.hide(),
                SettingsChange::AlwaysOnTop(enabled) => window.set_always_on_top(enabled),
                SettingsChange::MaxFps(_) => Ok(()),
            };
            result.map_err(|error| error.to_string())
        })?
    }; // 离开作用域就释放 Mutex 锁，再通知 Vue 和托盘。

    // 隐藏或恢复完成后再排队重置，避免“先重置、后到达的打开请求、再隐藏”的竞态。
    if matches!(change, SettingsChange::Visible(_)) {
        menu_layout::reset_nonblocking(app);
    }

    // 系统操作已经成功；通知失败记录错误，不伪装成系统操作失败。
    if let Err(error) = app.emit_to("main", "pet-settings-changed", &snapshot) {
        report_error(app, &format!("设置通知失败：{error}"));
    }
    if let Err(error) = app.emit_to("settings", "pet-settings-changed", &snapshot) {
        report_error(app, &format!("设置窗口通知失败：{error}"));
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

pub fn report_error(app: &AppHandle, message: &str) {
    eprintln!("[Rust] {message}");
    let _ = app.emit_to("main", "pet-desktop-error", message);
}

pub fn on_window_event(window: &Window, event: &WindowEvent) {
    if window.label() != "main" {
        return;
    }
    // Windows 最小化/还原会触发尺寸事件；这里只报告真实最小化状态。
    // 不读取 PetState 的锁，避免与正在执行的窗口操作互相等待。
    if let WindowEvent::Resized(_) = event {
        match window.is_minimized() {
            Ok(minimized) => {
                let _ = window.emit("pet-window-minimized", minimized);
                if minimized {
                    menu_layout::reset_nonblocking(window.app_handle());
                }
            }
            Err(error) => {
                report_error(window.app_handle(), &format!("读取最小化状态失败：{error}"))
            }
        }
    }
    if matches!(event, WindowEvent::Focused(false)) {
        menu_layout::reset_nonblocking(window.app_handle());
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
