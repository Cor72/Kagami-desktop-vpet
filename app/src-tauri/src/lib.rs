mod ai;
#[cfg(feature = "perf-audit")]
mod audit;
mod broadcast;
mod chat;
mod chat_store;
mod chat_window;
mod clock;
mod commands;
mod context;
mod desktop;
mod expression;
mod proactive;
mod settings;
mod settings_window;
mod store;
mod tray;

use tauri::Manager;

pub fn run() {
    // 注册命令后，前端才可以通过 invoke 调用它。
    tauri::Builder::default()
        .plugin(tauri_plugin_fs::init())
        // 先用默认值占位，`setup` 里再换成磁盘上的值。
        // 这样命令表在注册之后立刻就能拿到状态，前端不存在「读不到状态」的窗口期。
        .manage(settings::PetState::default())
        .manage(settings_window::SettingsWindowStore::default())
        .manage(chat_window::ChatWindowStore::default())
        .manage(desktop::StartupNotices::default())
        .manage(ai::AiState::default())
        .manage(chat::ChatState::default())
        .manage(proactive::ProactiveStore::default())
        .setup(|app| {
            if let Err(error) = tray::create(app.handle()) {
                eprintln!("[Rust] 托盘创建失败，恢复普通窗口：{error}");
                // 失败时保留原生关闭按钮与任务栏入口，避免无法退出或找回窗口。
                if let Some(window) = app.get_webview_window("main") {
                    window.set_decorations(true)?;
                    window.set_skip_taskbar(false)?;
                    window.show()?;
                }
            }
            // 托盘就绪之后再恢复设置：恢复的最后一步要同步托盘勾选状态。
            desktop::restore_settings(app.handle());
            // AI 配置与设置同源：先恢复内存状态，命令表才能读到磁盘上的值。
            ai::restore(app.handle());
            // 主动互动的低频采样循环。开关与可见性由它自己每一轮先看一遍，
            // 所以关掉之后不采样、不判断、不发言。
            proactive::start(app.handle());
            #[cfg(feature = "perf-audit")]
            audit::start(app.handle());
            Ok(())
        })
        // 主窗口页面加载完成后，把启动期攒下的提示补发出去。
        .on_page_load(desktop::on_page_load)
        .on_window_event(desktop::on_window_event)
        .invoke_handler(tauri::generate_handler![
            commands::request_expression,
            commands::reload_pet_model,
            commands::get_pet_settings,
            commands::get_pet_cursor_position,
            commands::open_pet_settings,
            commands::open_chat_window,
            commands::set_pet_visible,
            commands::set_pet_max_fps,
            commands::set_pet_always_on_top,
            commands::quit_pet,
            commands::get_ai_config,
            commands::set_ai_config,
            commands::set_api_key,
            commands::clear_api_key,
            commands::test_ai_connection,
            commands::create_session,
            commands::list_sessions,
            commands::get_messages,
            commands::send_message,
            commands::cancel_stream,
            commands::get_proactive_state,
            commands::set_proactive_enabled,
            commands::set_browser_title_enabled,
            commands::mute_proactive_today,
            commands::clear_proactive_mute,
            commands::proactive_dismiss,
        ])
        .run(tauri::generate_context!())
        .expect("启动八千代桌宠失败");
}

#[cfg(test)]
mod tests {
    /// 回归测试：`manage()` 注册的每个状态类型都必须**互不相同**。
    ///
    /// M1 交付时程序启动即 panic，原因就是两个窗口守卫都写成了 `Mutex<()>` 的裸
    /// type 别名——Rust 的别名是透明的，Tauri 的 `manage()` 按 `TypeId` 存状态，
    /// 第二次注册同一个类型就崩。这个 panic 只在真跑程序时出现，`cargo test` 与
    /// `cargo build` 都抓不到，所以在这里用类型层面把它们钉住。
    #[test]
    fn managed_states_are_distinct_types() {
        let types: Vec<(&str, std::any::TypeId)> = vec![
            (
                "settings::PetState",
                std::any::TypeId::of::<crate::settings::PetState>(),
            ),
            (
                "settings_window::SettingsWindowStore",
                std::any::TypeId::of::<crate::settings_window::SettingsWindowStore>(),
            ),
            (
                "chat_window::ChatWindowStore",
                std::any::TypeId::of::<crate::chat_window::ChatWindowStore>(),
            ),
            (
                "desktop::StartupNotices",
                std::any::TypeId::of::<crate::desktop::StartupNotices>(),
            ),
            ("ai::AiState", std::any::TypeId::of::<crate::ai::AiState>()),
            (
                "chat::ChatState",
                std::any::TypeId::of::<crate::chat::ChatState>(),
            ),
            (
                "proactive::ProactiveStore",
                std::any::TypeId::of::<crate::proactive::ProactiveStore>(),
            ),
        ];
        for (index, (left_name, left)) in types.iter().enumerate() {
            for (right_name, right) in types.iter().skip(index + 1) {
                assert_ne!(
                    left, right,
                    "{left_name} 与 {right_name} 是同一个 Rust 类型，manage() 会在启动时 panic"
                );
            }
        }
    }
}
