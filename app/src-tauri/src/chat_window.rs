//! 对话窗口的单例创建守卫。
//!
//! 结构与 `settings_window.rs` 完全一致：`Mutex<()>` 保证不会有两次创建撞在一起，
//! 窗口已经存在时只做「还原 + 显示 + 聚焦」，不重复创建。
//! 窗口本身的尺寸、标题等几何参数写在这里，前端只负责渲染页面。

use std::sync::Mutex;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

pub const CHAT_LABEL: &str = "chat";
pub type ChatWindowStore = Mutex<()>;

fn open_sync(app: &AppHandle) -> Result<(), String> {
    let store = app.state::<ChatWindowStore>();
    let _creation_guard = store.lock().map_err(|error| error.to_string())?;

    if let Some(window) = app.get_webview_window(CHAT_LABEL) {
        window.unminimize().map_err(|error| error.to_string())?;
        window.show().map_err(|error| error.to_string())?;
        window.set_focus().map_err(|error| error.to_string())?;
        return Ok(());
    }

    WebviewWindowBuilder::new(
        app,
        CHAT_LABEL,
        WebviewUrl::App("index.html?view=chat".into()),
    )
    .title("八千代 · 对话")
    .inner_size(720.0, 560.0)
    .min_inner_size(480.0, 400.0)
    .resizable(true)
    .decorations(true)
    .transparent(false)
    .always_on_top(false)
    .build()
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub async fn open(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || open_sync(&app))
        .await
        .map_err(|error| error.to_string())?
}
