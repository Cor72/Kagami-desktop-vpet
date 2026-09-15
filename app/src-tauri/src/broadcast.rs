//! 统一事件广播。
//!
//! 之前「发往 main」「发往 settings」的逻辑散在 `desktop.rs` 与 `commands.rs` 里，
//! 加第三个窗口就要改好几处。这里把窗口清单收成一处：**新增窗口只需在
//! [`WINDOW_LABELS`] 里加一行**。
//!
//! 一个刻意的选择：窗口不存在时不当作错误，而是否当作错误由调用方决定——
//! 「设置窗口没开」是正常状态，「主窗口收不到表情指令」是真故障。

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

/// 主窗口标签：桌宠本体，始终存在。
pub const MAIN: &str = "main";

/// 接收 Rust 事件的窗口标签（对应设计文档 §3 的窗口层）。
/// 设置窗口的标签取自它自己的常量，避免同一个字符串写两遍。
pub const WINDOW_LABELS: &[&str] = &[MAIN, crate::settings_window::SETTINGS_LABEL];

/// 发给指定窗口；窗口不存在时返回 `Err`。
///
/// 用于「这个窗口必须收到」的场合，例如把表情指令交给主窗口渲染。
pub fn emit(
    app: &AppHandle,
    label: &str,
    event: &str,
    payload: &impl Serialize,
) -> Result<(), String> {
    if app.get_webview_window(label).is_none() {
        return Err(format!("窗口 {label} 不存在"));
    }
    app.emit_to(label, event, payload)
        .map_err(|error| error.to_string())
}

/// 发给指定窗口；窗口不存在时静默跳过。
///
/// 用于「有就更新，没有就算了」的场合，例如把表情已切换的消息同步给设置窗口。
pub fn emit_if_open(
    app: &AppHandle,
    label: &str,
    event: &str,
    payload: &impl Serialize,
) -> Result<(), String> {
    if app.get_webview_window(label).is_none() {
        return Ok(());
    }
    app.emit_to(label, event, payload)
        .map_err(|error| error.to_string())
}

/// 发给所有当前打开的窗口，返回失败明细（「没打开」不算失败）。
///
/// 调用方负责把每一项变成一条错误上报；这样一个窗口出问题不会连累其他窗口。
pub fn emit_all(app: &AppHandle, event: &str, payload: &impl Serialize) -> Vec<String> {
    let mut failures = Vec::new();
    for label in WINDOW_LABELS {
        if let Err(error) = emit(app, label, event, payload) {
            failures.push(error);
        }
    }
    failures
}

/// 除某个窗口外，再发给所有打开的窗口。
pub fn emit_all_except(
    app: &AppHandle,
    excluded: &str,
    event: &str,
    payload: &impl Serialize,
) -> Vec<String> {
    let mut failures = Vec::new();
    for label in WINDOW_LABELS {
        if *label == excluded {
            continue;
        }
        if let Err(error) = emit(app, label, event, payload) {
            failures.push(error);
        }
    }
    failures
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_labels_cover_every_named_window_once() {
        assert!(WINDOW_LABELS.contains(&MAIN));
        assert!(WINDOW_LABELS.contains(&crate::settings_window::SETTINGS_LABEL));
        let mut sorted = WINDOW_LABELS.to_vec();
        sorted.sort_unstable();
        let count = sorted.len();
        sorted.dedup();
        assert_eq!(sorted.len(), count, "窗口标签不应重复");
    }
}
