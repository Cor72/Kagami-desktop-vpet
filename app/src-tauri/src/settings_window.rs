use std::sync::Mutex;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

pub const SETTINGS_LABEL: &str = "settings";

/// 单例创建守卫。
///
/// 用 newtype 而不是 `type SettingsWindowStore = Mutex<()>;`：Rust 的 type 别名是透明的，
/// 裸别名会和别的窗口守卫撞成同一个类型，导致 Tauri `manage()` 在启动时 panic。
/// 完整说明见 `chat_window.rs` 里 `ChatWindowStore` 的注释。
#[derive(Default)]
pub struct SettingsWindowStore(pub Mutex<()>);

fn open_sync(app: &AppHandle) -> Result<(), String> {
    let store = app.state::<SettingsWindowStore>();
    let _creation_guard = store.0.lock().map_err(|error| error.to_string())?;

    if let Some(window) = app.get_webview_window(SETTINGS_LABEL) {
        window.unminimize().map_err(|error| error.to_string())?;
        window.show().map_err(|error| error.to_string())?;
        window.set_focus().map_err(|error| error.to_string())?;
        return Ok(());
    }

    WebviewWindowBuilder::new(
        app,
        SETTINGS_LABEL,
        WebviewUrl::App("index.html?view=settings".into()),
    )
    .title("八千代 · 设置")
    .inner_size(360.0, 360.0)
    .min_inner_size(300.0, 300.0)
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
