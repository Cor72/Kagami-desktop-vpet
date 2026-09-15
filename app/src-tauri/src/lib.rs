#[cfg(feature = "perf-audit")]
mod audit;
mod broadcast;
mod clock;
mod commands;
mod desktop;
mod expression;
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
        .manage(desktop::StartupNotices::default())
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
            commands::set_pet_visible,
            commands::set_pet_max_fps,
            commands::set_pet_always_on_top,
            commands::quit_pet,
        ])
        .run(tauri::generate_context!())
        .expect("启动八千代桌宠失败");
}
