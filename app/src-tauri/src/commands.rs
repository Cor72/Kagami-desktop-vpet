use crate::{
    desktop,
    expression::validate_expression,
    menu_layout::{self, MenuLayout},
    settings::{PetSettings, SettingsChange},
    settings_window,
};
use serde::Serialize;
use tauri::Emitter;

// Serialize 让 Tauri 可以把这个 Rust 结构体转换成事件中的 JSON 对象。
#[derive(Clone, Serialize)]
pub struct ExpressionRequested {
    pub name: String,
}

#[tauri::command]
pub fn get_pet_settings(app: tauri::AppHandle) -> Result<PetSettings, String> {
    desktop::get_settings(&app)
}

#[tauri::command]
pub async fn get_pet_cursor_position(
    window: tauri::WebviewWindow,
) -> Result<desktop::CursorPosition, String> {
    desktop::get_cursor_position(&window)
}

#[tauri::command]
pub async fn set_pet_visible(app: tauri::AppHandle, visible: bool) -> Result<PetSettings, String> {
    menu_layout::reset(app.clone()).await?;
    desktop::update_settings(&app, SettingsChange::Visible(visible))
}

#[tauri::command]
pub fn set_pet_max_fps(app: tauri::AppHandle, max_fps: u32) -> Result<PetSettings, String> {
    desktop::update_settings(&app, SettingsChange::MaxFps(max_fps))
}

#[tauri::command]
pub fn set_pet_always_on_top(app: tauri::AppHandle, enabled: bool) -> Result<PetSettings, String> {
    desktop::update_settings(&app, SettingsChange::AlwaysOnTop(enabled))
}

#[tauri::command]
pub fn quit_pet(app: tauri::AppHandle) {
    app.exit(0);
}

#[tauri::command]
pub fn request_expression(app: tauri::AppHandle, name: String) -> Result<(), String> {
    #[cfg(debug_assertions)]
    println!("[Rust] 收到表情请求: {name}");

    // 校验失败时，? 会提前返回 Err，下面的事件不会发送。
    validate_expression(&name)?;

    let payload = ExpressionRequested { name };
    app.emit_to("main", "pet-expression-requested", &payload)
        .map_err(|error| error.to_string())?;
    if let Err(error) = app.emit_to("settings", "pet-expression-observed", payload) {
        desktop::report_error(&app, &format!("表情反馈通知失败：{error}"));
    }
    Ok(())
}

#[tauri::command]
pub async fn set_pet_menu_open(app: tauri::AppHandle, open: bool) -> Result<MenuLayout, String> {
    menu_layout::set_open(app, open).await
}

#[tauri::command]
pub async fn open_pet_settings(app: tauri::AppHandle) -> Result<(), String> {
    settings_window::open(app).await
}

#[tauri::command]
pub fn reload_pet_model(app: tauri::AppHandle) -> Result<(), String> {
    #[cfg(debug_assertions)]
    {
        return app
            .emit_to("main", "pet-model-reload", ())
            .map_err(|error| error.to_string());
    }

    #[cfg(not(debug_assertions))]
    {
        let _ = app;
        Err("模型重载仅在开发版本中可用".into())
    }
}
