use crate::{
    commands, desktop,
    settings::{PetSettings, SettingsChange},
};
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};

pub const TRAY_ID: &str = "pet-tray";

// 保存菜单句柄，设置变化时直接更新勾选状态。
struct TrayMenu {
    visible: CheckMenuItem<tauri::Wry>,
    always_on_top: CheckMenuItem<tauri::Wry>,
    fps_30: CheckMenuItem<tauri::Wry>,
    fps_15: CheckMenuItem<tauri::Wry>,
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let visible = CheckMenuItem::with_id(app, "visible", "显示桌宠", true, true, None::<&str>)?;
    let always_on_top =
        CheckMenuItem::with_id(app, "always-on-top", "保持置顶", true, true, None::<&str>)?;
    let smile = MenuItem::with_id(app, "smile", "微笑", true, None::<&str>)?;
    let squint = MenuItem::with_id(app, "squint", "眯眼", true, None::<&str>)?;
    let tears = MenuItem::with_id(app, "tears", "泪眼", true, None::<&str>)?;
    let teardrop = MenuItem::with_id(app, "teardrop", "泪滴", true, None::<&str>)?;
    let expressions =
        Submenu::with_items(app, "表情", true, &[&smile, &squint, &tears, &teardrop])?;
    let fps_30 = CheckMenuItem::with_id(app, "fps-30", "30 FPS", true, true, None::<&str>)?;
    let fps_15 = CheckMenuItem::with_id(app, "fps-15", "15 FPS", true, false, None::<&str>)?;
    let frame_rate = Submenu::with_items(app, "帧率", true, &[&fps_30, &fps_15])?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "退出八千代", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &visible,
            &always_on_top,
            &expressions,
            &frame_rate,
            &separator,
            &quit,
        ],
    )?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("八千代桌宠 · 左键显示 / 右键菜单")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            if let Err(error) = handle_menu(app, event.id.as_ref()) {
                // 系统复选菜单会自行切换勾选；操作失败时恢复实际状态。
                if let Ok(settings) = desktop::get_settings(app) {
                    let _ = sync_menu(app, &settings);
                }
                desktop::report_error(app, &error);
            }
        })
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                if let Err(error) =
                    desktop::update_settings(tray.app_handle(), SettingsChange::Visible(true))
                {
                    desktop::report_error(tray.app_handle(), &error);
                }
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    app.manage(TrayMenu {
        visible,
        always_on_top,
        fps_30,
        fps_15,
    });
    Ok(())
}

fn handle_menu(app: &AppHandle, id: &str) -> Result<(), String> {
    match id {
        "visible" => {
            let visible = desktop::get_settings(app)?.visible;
            desktop::update_settings(app, SettingsChange::Visible(!visible))?;
        }
        "always-on-top" => {
            let enabled = desktop::get_settings(app)?.always_on_top;
            desktop::update_settings(app, SettingsChange::AlwaysOnTop(!enabled))?;
        }
        "fps-30" => {
            desktop::update_settings(app, SettingsChange::MaxFps(30))?;
        }
        "fps-15" => {
            desktop::update_settings(app, SettingsChange::MaxFps(15))?;
        }
        "smile" | "squint" | "tears" | "teardrop" => {
            commands::request_expression(app.clone(), id.into())?
        }
        "quit" => app.exit(0),
        _ => {}
    }
    Ok(())
}

pub fn sync_menu(app: &AppHandle, settings: &PetSettings) -> tauri::Result<()> {
    if let Some(menu) = app.try_state::<TrayMenu>() {
        menu.visible.set_checked(settings.visible)?;
        menu.always_on_top.set_checked(settings.always_on_top)?;
        menu.fps_30.set_checked(settings.max_fps == 30)?;
        menu.fps_15.set_checked(settings.max_fps == 15)?;
    }
    Ok(())
}
