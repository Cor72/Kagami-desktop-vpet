//! 写入确认：Rust 生成 diff → 发事件 → 前端弹卡片 → 用户点「应用」才真的写。
//!
//! **没有 diff 的确认等于盲签**（实施计划 §9 阶段 C）。用户点「应用」之前，
//! 磁盘上不会发生任何变化；点「拒绝」之后也不会——`write_file` 只是把改动放进
//! [`PendingWrites`] 里等一个答复。
//!
//! 这个模块不碰 Tauri 的事件之外的东西：diff 生成与原子写都是纯函数，能直接单测。

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use serde::Serialize;
use similar::TextDiff;
use tauri::async_runtime::{channel, Receiver, Sender};
use tauri::{AppHandle, Manager};

use crate::workspace::{self, Access};

/// 用户对一次写入确认的回答。
pub enum Decision {
    /// 应用：写入已经执行完，`Result` 就是它的结果。
    Applied(Result<String, String>),
    /// 拒绝：文件一个字都没动。
    Rejected,
}

/// 等确认的一次写入。
pub struct PendingWrite {
    /// 模型给的原始路径（再次校验时用它，而不是用已经解析好的结果）。
    pub raw_path: String,
    pub content: String,
    sender: Sender<Decision>,
}

/// 等着用户按按钮的写入请求。
///
/// **必须是独立的 newtype**（原因见 `chat_window.rs` 里 `ChatWindowStore` 的注释）。
#[derive(Default)]
pub struct PendingWrites(pub Mutex<HashMap<String, PendingWrite>>);

/// 回给前端的确认结果。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteOutcome {
    pub ok: bool,
    pub message: String,
}

fn new_id() -> String {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    format!("wr{}-{seq}", crate::clock::now_ms())
}

/// 生成红绿 diff（unified 格式，前后各留 3 行上下文）。
pub fn unified_diff(before: &str, after: &str) -> String {
    TextDiff::from_lines(before, after)
        .unified_diff()
        .context_radius(3)
        .header("修改前", "修改后")
        .to_string()
}

/// diff 里到底有没有「改了东西」。
///
/// 只有 `+` / `-` 开头的行才算改动：`@@`、文件头、空行都不算。
/// 模型经常把文件原样写一遍，那种情况下弹卡片只会浪费用户一次点击。
pub fn has_changes(diff: &str) -> bool {
    diff.lines().any(|line| {
        (line.starts_with('+') || line.starts_with('-'))
            && !line.starts_with("+++")
            && !line.starts_with("---")
    })
}

/// 原子写：先写临时文件，再改名覆盖。
///
/// 直接 `fs::write` 在断电或磁盘满的时候会留下半截文件——而它覆盖的是用户的
/// 真实代码。`rename` 在同一个目录内是原子的，所以要么是旧的，要么是新的。
pub fn write_atomic(path: &Path, content: &str) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("这个路径没有上级目录：{}", path.display()))?;
    let temp = parent.join(format!(
        ".{}.yachiyo.tmp",
        path.file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| "write".into())
    ));
    std::fs::write(&temp, content)
        .map_err(|error| format!("写入 {} 失败：{error}", temp.display()))?;
    std::fs::rename(&temp, path).map_err(|error| {
        let _ = std::fs::remove_file(&temp);
        format!("替换 {} 失败：{error}", path.display())
    })
}

/// 登记一次写入请求，返回「等回答」的接收端。
pub fn register(
    app: &AppHandle,
    raw_path: &str,
    content: String,
) -> Result<(String, Receiver<Decision>), String> {
    // 一次性应答通道：用户按按钮时把结果送回等在那里的 Agent 循环。
    let (sender, receiver) = channel(1);
    let id = new_id();
    let pending = PendingWrite {
        raw_path: raw_path.to_string(),
        content,
        sender,
    };
    let state = app.state::<PendingWrites>();
    let mut writes = state.0.lock().map_err(|error| error.to_string())?;
    writes.insert(id.clone(), pending);
    Ok((id, receiver))
}

/// 这一步作废了（用户点了停止、或答案已经送到）。
pub fn forget(app: &AppHandle, id: &str) {
    let state = app.state::<PendingWrites>();
    let cleared = state.0.lock().map(|mut writes| writes.remove(id).is_some());
    if let Err(error) = cleared {
        eprintln!("[Rust] 清理写入确认失败：{error}");
    }
}

fn take(app: &AppHandle, id: &str) -> Result<PendingWrite, String> {
    let state = app.state::<PendingWrites>();
    let mut writes = state.0.lock().map_err(|error| error.to_string())?;
    writes.remove(id).ok_or_else(|| {
        "这次确认已经作废了（可能是刚才点过，或者这一步已经取消），文件没有变化。".to_string()
    })
}

/// 用户点了「应用」：**这时才碰磁盘**。
///
/// 路径在这里重新走一遍校验（计划 §7：每次都要重新校验）。用户可能在等确认的
/// 这段时间里把那条工作区条目删掉了，那种情况下要拒绝，而不是照着旧结论写下去。
pub fn apply(app: &AppHandle, request_id: &str) -> Result<WriteOutcome, String> {
    let pending = take(app, request_id)?;
    let entries = workspace::get(app)?;
    let result: Result<String, String> =
        match workspace::resolve(&entries, &pending.raw_path, Access::Write) {
            Err(error) => Err(error),
            Ok(path) => {
                write_atomic(&path, &pending.content).map(|()| format!("已写入 {}", path.display()))
            }
        };
    // 回答先送出去再返回：Agent 循环那边要拿它当工具结果。
    // 对方可能已经不等了（用户点了停止）：那时这次写入不会生效，但也不该报错。
    let _ = pending.sender.try_send(Decision::Applied(result.clone()));
    Ok(match result {
        Ok(message) => WriteOutcome { ok: true, message },
        Err(message) => WriteOutcome { ok: false, message },
    })
}

/// 用户点了「拒绝」：什么都不发生。
pub fn reject(app: &AppHandle, request_id: &str) -> Result<WriteOutcome, String> {
    let pending = take(app, request_id)?;
    let _ = pending.sender.try_send(Decision::Rejected);
    Ok(WriteOutcome {
        ok: true,
        message: "这次改动没有应用，文件保持原样。".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::test_support::TempDir;
    use std::fs;

    #[test]
    fn diff_marks_added_and_removed_lines() {
        let diff = unified_diff("a\nb\nc\n", "a\nB\nc\nd\n");
        assert!(diff.contains("-b"), "{diff}");
        assert!(diff.contains("+B"), "{diff}");
        assert!(diff.contains("+d"), "{diff}");
        assert!(diff.contains("@@"), "要有 hunk 头：{diff}");
    }

    #[test]
    fn an_unchanged_file_produces_no_changes() {
        let diff = unified_diff("一样的内容\n", "一样的内容\n");
        assert!(!has_changes(&diff), "{diff}");
        assert!(has_changes(&unified_diff("a\n", "b\n")));
    }

    #[test]
    fn the_diff_carries_around_three_lines_of_context() {
        let before = (1..=20)
            .map(|n| format!("第 {n} 行"))
            .collect::<Vec<_>>()
            .join("\n");
        let after = before.replace("第 10 行", "改过的第 10 行");
        let diff = unified_diff(&before, &after);
        assert!(diff.contains("第 7 行"), "前面应当有上下文：{diff}");
        assert!(diff.contains("第 13 行"), "后面应当有上下文：{diff}");
        assert!(!diff.contains("第 1 行"), "不该把二十行全抄一遍：{diff}");
    }

    #[test]
    fn atomic_write_creates_and_replaces_files() {
        let dir = TempDir::new("write-atomic");
        let path = dir.path.join("notes.txt");
        write_atomic(&path, "第一版").expect("写新文件");
        assert_eq!(fs::read_to_string(&path).expect("读"), "第一版");
        write_atomic(&path, "第二版").expect("覆盖");
        assert_eq!(fs::read_to_string(&path).expect("读"), "第二版");
        // 临时文件不能留在用户的目录里。
        let left: Vec<String> = fs::read_dir(&dir.path)
            .expect("列目录")
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .filter(|name| name.contains("yachiyo"))
            .collect();
        assert!(left.is_empty(), "不该留下临时文件：{left:?}");
    }
}
