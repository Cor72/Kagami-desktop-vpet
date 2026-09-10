// 发布版启动时不额外弹出控制台窗口。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    yachiyo_desktop_lib::run();
}
