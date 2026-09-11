#[cfg(feature = "perf-audit")]
mod audit;
mod commands;
mod desktop;
mod expression;
mod settings;
mod tray;

use tauri::Manager;

pub fn run() {
    // 注册命令后，前端才可以通过 invoke 调用它。
    tauri::Builder::default()
        .plugin(tauri_plugin_fs::init())
        .manage(settings::PetState::default())
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
            #[cfg(feature = "perf-audit")]
            audit::start(app.handle());
            Ok(())
        })
        .on_window_event(desktop::on_window_event)
        .invoke_handler(tauri::generate_handler![
            commands::request_expression,
            commands::get_pet_settings,
            commands::get_pet_cursor_position,
            commands::set_pet_visible,
            commands::set_pet_max_fps,
            commands::set_pet_always_on_top,
            commands::quit_pet,
        ])
        .run(tauri::generate_context!())
        .expect("启动八千代桌宠失败");
}
