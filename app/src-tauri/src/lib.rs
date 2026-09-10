pub fn run() {
    // 从 tauri.conf.json 读取配置，创建窗口并启动事件循环。
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("启动八千代桌宠失败");
}
