//! 对话窗口的单例创建守卫。
//!
//! 结构与 `settings_window.rs` 一致：守卫保证不会有两次创建撞在一起，
//! 窗口已经存在时只做「还原 + 显示 + 聚焦」，不重复创建。
//! 窗口本身的尺寸、标题等几何参数写在这里，前端只负责渲染页面。

use std::sync::Mutex;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

pub const CHAT_LABEL: &str = "chat";

/// 单例创建守卫。
///
/// **必须是独立类型，不能写成 `type ChatWindowStore = Mutex<()>;`。**
/// Rust 的 type 别名是透明的——两个别名都等于 `Mutex<()>`，而 Tauri 的 `manage()`
/// 按 `TypeId` 存状态，同类型注册第二次会在**启动时** panic：
///
/// ```text
/// state for type 'Mutex<()>' is already being managed
/// ```
///
/// 这个 panic 只在真正运行程序时出现，`cargo test` 与 `cargo build` 都抓不到。
/// 所以设置窗口与对话窗口的守卫都用 newtype 隔开，不要改回裸别名。
#[derive(Default)]
pub struct ChatWindowStore(pub Mutex<()>);

fn open_sync(app: &AppHandle) -> Result<(), String> {
    let store = app.state::<ChatWindowStore>();
    let _creation_guard = store.0.lock().map_err(|error| error.to_string())?;

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

#[cfg(test)]
mod tests {
    use super::*;

    /// 回归测试：各窗口的创建守卫必须是**不同**的 Rust 类型。
    ///
    /// 这两个守卫曾经都是 `Mutex<()>` 的裸 type 别名，于是被 Tauri 当成同一个状态类型，
    /// `manage()` 第二次调用会 panic，程序启动即崩——而那个问题测试抓不到，
    /// 因为 `Mutex<()>` 的别名在编译期完全合法。这里直接在类型层面把它钉住。
    #[test]
    fn window_guards_are_distinct_types() {
        assert_ne!(
            std::any::TypeId::of::<ChatWindowStore>(),
            std::any::TypeId::of::<crate::settings_window::SettingsWindowStore>(),
        );
    }
}
