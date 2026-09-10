mod commands;
mod expression;

pub fn run() {
    // 注册命令后，前端才可以通过 invoke 调用它。
    tauri::Builder::default()
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![commands::request_expression])
        .run(tauri::generate_context!())
        .expect("启动八千代桌宠失败");
}
