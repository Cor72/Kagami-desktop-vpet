use std::sync::Mutex;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

pub const SETTINGS_LABEL: &str = "settings";
pub type SettingsWindowStore = Mutex<()>;

fn open_sync(app: &AppHandle) -> Result<(), String> {
    let store = app.state::<SettingsWindowStore>();
    let _creation_guard = store.lock().map_err(|error| error.to_string())?;

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
