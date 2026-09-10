use crate::expression::validate_expression;
use serde::Serialize;
use tauri::Emitter;

// Serialize 让 Tauri 可以把这个 Rust 结构体转换成事件中的 JSON 对象。
#[derive(Clone, Serialize)]
pub struct ExpressionRequested {
    pub name: String,
}

#[tauri::command]
pub fn request_expression(app: tauri::AppHandle, name: String) -> Result<(), String> {
    #[cfg(debug_assertions)]
    println!("[Rust] 收到表情请求: {name}");

    // 校验失败时，? 会提前返回 Err，下面的事件不会发送。
    validate_expression(&name)?;

    app.emit_to(
        "main",
        "pet-expression-requested",
        ExpressionRequested { name },
    )
    .map_err(|error| error.to_string())
}
