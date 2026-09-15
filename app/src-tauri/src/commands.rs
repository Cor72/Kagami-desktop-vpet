use crate::{
    broadcast, desktop,
    expression::validate_expression,
    settings::{PetSettings, SettingsChange},
    settings_window,
};
use serde::Serialize;

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
    // 主窗口必须收到：真的换表情的是它，收不到就是故障，直接返回错误。
    broadcast::emit(&app, broadcast::MAIN, "pet-expression-requested", &payload)?;
    // 其余窗口只是「顺带同步一下显示」，没开就跳过。
    // 用「除主窗口外全部」而不是写死 settings：以后加对话窗口会自动拿到这份同步。
    for failure in
        broadcast::emit_all_except(&app, broadcast::MAIN, "pet-expression-observed", &payload)
    {
        desktop::report_error(&app, &format!("表情反馈通知失败：{failure}"));
    }
    Ok(())
}

#[tauri::command]
pub async fn open_pet_settings(app: tauri::AppHandle) -> Result<(), String> {
    settings_window::open(app).await
}

#[tauri::command]
pub async fn reload_pet_model(app: tauri::AppHandle) -> Result<(), String> {
    #[cfg(debug_assertions)]
    broadcast::emit(&app, broadcast::MAIN, "pet-model-reload", &())?;

    // 发布版不注册重载入口；保留函数以便两个构建共用同一份命令注册表。
    #[cfg(not(debug_assertions))]
    {
        let _ = app;
        return Err("模型重载仅在开发版本中可用".into());
    }

    #[cfg(debug_assertions)]
    Ok(())
}
