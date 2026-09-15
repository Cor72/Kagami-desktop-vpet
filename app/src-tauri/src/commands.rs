use crate::{
    ai::{
        self,
        config::{AiConfigPatch, AiConfigView, Provider},
        secret,
    },
    broadcast, chat, chat_store, chat_window, clock, desktop,
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
pub async fn open_chat_window(app: tauri::AppHandle) -> Result<(), String> {
    chat_window::open(app).await
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

// ---------- AI 配置与 API Key ----------

/// 前端的 AI 设置快照：配置 + `hasKey` + 掩码。**没有 Key 本身。**
#[tauri::command]
pub fn get_ai_config(app: tauri::AppHandle) -> Result<AiConfigView, String> {
    let config = ai::get_config(&app)?;
    ai::view(&config)
}

#[tauri::command]
pub fn set_ai_config(app: tauri::AppHandle, patch: AiConfigPatch) -> Result<AiConfigView, String> {
    let config = ai::update_config(&app, patch)?;
    ai::view(&config)
}

/// 保存 API Key。**不返回 Key，也不回显**——只回新的掩码。
#[tauri::command]
pub fn set_api_key(
    app: tauri::AppHandle,
    provider: Provider,
    key: String,
) -> Result<AiConfigView, String> {
    secret::save(provider, &key)?;
    publish_ai_config(&app)
}

#[tauri::command]
pub fn clear_api_key(app: tauri::AppHandle, provider: Provider) -> Result<AiConfigView, String> {
    secret::clear(provider)?;
    publish_ai_config(&app)
}

/// Key 变了，掩码也就变了：广播一份新快照给设置窗口与对话窗口。
fn publish_ai_config(app: &tauri::AppHandle) -> Result<AiConfigView, String> {
    let config = ai::get_config(app)?;
    ai::broadcast_config(app, &config);
    ai::view(&config)
}

#[tauri::command]
pub async fn test_ai_connection(app: tauri::AppHandle) -> Result<ai::TestOutcome, String> {
    Ok(ai::test_connection(&app).await)
}

// ---------- 对话与会话 ----------

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SentMessage {
    /// 助手那条消息的 id：前端把它当流式增量的落点。
    pub message_id: String,
}

#[derive(Clone, Serialize)]
pub struct CancelResult {
    pub ok: bool,
}

#[tauri::command]
pub fn create_session(app: tauri::AppHandle) -> Result<chat_store::Session, String> {
    let store = chat_store::store(&app)?;
    chat_store::create(&store, clock::now_ms())
}

#[tauri::command]
pub fn list_sessions(app: tauri::AppHandle) -> Result<Vec<chat_store::Session>, String> {
    let store = chat_store::store(&app)?;
    Ok(chat_store::list(&store))
}

#[tauri::command]
pub fn get_messages(
    app: tauri::AppHandle,
    session_id: String,
) -> Result<Vec<chat_store::Message>, String> {
    let store = chat_store::store(&app)?;
    Ok(chat_store::load(&store, &session_id)?.messages)
}

/// 发一条消息。**立刻返回**助手消息的 id，回答通过 `chat-stream-*` 事件陆续到达。
#[tauri::command]
pub async fn send_message(
    app: tauri::AppHandle,
    session_id: String,
    text: String,
) -> Result<SentMessage, String> {
    let message_id = chat::send(app, &session_id, &text).await?;
    Ok(SentMessage { message_id })
}

#[tauri::command]
pub fn cancel_stream(app: tauri::AppHandle, session_id: String) -> Result<CancelResult, String> {
    Ok(CancelResult {
        ok: chat::cancel(&app, &session_id),
    })
}
