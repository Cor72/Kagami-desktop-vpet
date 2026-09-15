//! 工作区：用户显式授权的条目，以及**每次调用都要重做一遍**的路径校验。
//!
//! 条目分两种（实施计划 §4.1）：
//!
//! | | 目录条目 | 文件条目 |
//! |---|---|---|
//! | 怎么加进来 | 系统目录选择器 | 拖拽投放 |
//! | 权限 | 读写（写入仍要过 diff 确认） | **只读，物理上没有写入通道** |
//! | 用户的心智模型 | 「这是我的项目」 | 「这是我给你的资料」 |
//!
//! 校验只有一套：`canonicalize` 之后比对白名单，按条目类型决定这次访问允不允许。
//! **校验结果绝不缓存**——符号链接可以在两次工具调用之间被换掉，缓存等于把这条
//! 防线拆了。所以 [`resolve`] 每次调用都重新 `canonicalize`。
//!
//! 本模块不碰 Tauri 的网络与事件之外的东西，主体可以用临时目录直接单测。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::broadcast;
use crate::store::{LoadSource, Store};

/// 工作区文件，位于 Tauri 的 `app_data_dir` 下（与 `settings.json` 同一目录）。
pub const WORKSPACE_FILE: &str = "workspace.json";

/// 条目上限。超过这个数，用户自己也管不过来（设计文档 §4.4）。
pub const MAX_ENTRIES: usize = 32;

/// 条目类型。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryKind {
    /// 目录条目：可读，写要走 diff 确认。
    Dir,
    /// 文件条目：**永远只读**，不存在例外。
    File,
}

impl EntryKind {
    /// 写入通道只对目录条目开放。
    pub fn can_write(self) -> bool {
        matches!(self, EntryKind::Dir)
    }

    /// 给界面与模型看的中文名。
    pub fn label(self) -> &'static str {
        match self {
            EntryKind::Dir => "目录",
            EntryKind::File => "只读文件",
        }
    }
}

/// 一条授权条目。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceEntry {
    pub id: String,
    pub kind: EntryKind,
    /// `canonicalize` 之后的绝对路径（Windows 的 `\\?\` 前缀已经去掉）。
    /// 存规范化路径而不是用户拖进来的字面路径，比较时才不会因为符号链接或大小写跑偏。
    pub path: String,
    /// 最后一段名字，界面与系统提示词都显示它。
    pub label: String,
}

/// 落盘的部分。
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StoredWorkspace {
    pub entries: Vec<WorkspaceEntry>,
}

/// 运行态。**必须是独立的 newtype**（裸 `Mutex<Vec<..>>` 别名会和别的状态撞类型，
/// 见 `chat_window.rs` 里 `ChatWindowStore` 的注释）。
#[derive(Default)]
pub struct WorkspaceState(pub Mutex<Vec<WorkspaceEntry>>);

/// 这次访问想干什么。写要比读多一道「条目是不是目录」的检查。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    Read,
    Write,
}

// ---------- 路径规范化 ----------

/// 去掉 Windows `canonicalize` 给出的 verbatim 前缀。
///
/// `std::fs::canonicalize("C:\\a")` 在 Windows 上返回 `\\?\C:\a`。这个前缀对
/// `std::fs` 是合法的，但写进 JSON、显示给用户、塞进系统提示词都很难看，
/// 而且模型照着抄回来时容易抄错。两边的路径都过这一道，比较仍然一致。
pub fn simplify(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy().to_string();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    if let Some(rest) = text.strip_prefix(r"\\?\") {
        return PathBuf::from(rest);
    }
    path
}

/// 规范化一个**必须已经存在**的路径。
pub fn canonical(raw: &str) -> Result<PathBuf, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("路径是空的".into());
    }
    let path = PathBuf::from(trimmed);
    std::fs::canonicalize(&path)
        .map(simplify)
        .map_err(|error| format!("这个路径打不开（{error}）：{}", path.display()))
}

/// 路径比较：Windows 上大小写不敏感，统一按小写比。
fn name_eq(left: &str, right: &str) -> bool {
    left.to_lowercase() == right.to_lowercase()
}

/// `target` 是否落在 `root` 里（含相等）。
///
/// **按分量比较，不比字符串前缀**——比前缀会把 `C:\foobar` 误判成在 `C:\foo` 里。
pub fn is_within(root: &Path, target: &Path) -> bool {
    let mut root_parts = root.components();
    let mut target_parts = target.components();
    loop {
        match (root_parts.next(), target_parts.next()) {
            // root 走完了：target 还有剩下（子路径）或者刚好一样，都算「在里面」。
            (None, _) => return true,
            // target 比 root 短，且前面全一样 → 不可能在里面。
            (Some(_), None) => return false,
            (Some(root_part), Some(target_part)) => {
                if !name_eq(
                    &root_part.as_os_str().to_string_lossy(),
                    &target_part.as_os_str().to_string_lossy(),
                ) {
                    return false;
                }
            }
        }
    }
}

/// 这条已授权的路径（可能是文件，也可能是目录）覆盖了 `target` 吗。
fn entry_covers(entry: &WorkspaceEntry, target: &Path) -> bool {
    is_within(Path::new(&entry.path), target)
}

/// 两个规范化路径是不是同一个。
fn same_path(left: &Path, right: &Path) -> bool {
    is_within(left, right) && is_within(right, left)
}

// ---------- 路径校验 ----------

/// 把模型给的路径解析成真正要操作的绝对路径。
///
/// **每次工具调用都要走这里，结果不缓存。** 这是符号链接攻击唯一的防线。
pub fn resolve(entries: &[WorkspaceEntry], raw: &str, access: Access) -> Result<PathBuf, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("路径是空的".into());
    }
    if entries.is_empty() {
        return Err("用户还没有授权任何文件或文件夹，先在对话窗口的 Agent 模式里加一个。".into());
    }

    let mut denied: Option<PathBuf> = None;
    for candidate in candidates(trimmed, entries) {
        match std::fs::canonicalize(&candidate).map(simplify) {
            Ok(resolved) => match check(entries, &resolved, access) {
                Ok(()) => return Ok(resolved),
                Err(_) => denied = denied.or(Some(resolved)),
            },
            Err(_) => {
                // 文件还不存在（`write_file` 建新文件）时只能校验它所在的目录。
                if access != Access::Write {
                    continue;
                }
                if let Some(resolved) = resolve_missing(&candidate, entries) {
                    return Ok(resolved);
                }
            }
        }
    }

    Err(explain(entries, trimmed, access, denied.as_deref()))
}

/// 已经规范化好的路径是否被允许。
fn check(entries: &[WorkspaceEntry], resolved: &Path, access: Access) -> Result<(), String> {
    for entry in entries {
        if !entry_covers(entry, resolved) {
            continue;
        }
        // 文件条目永远只读：它是「我给你的资料」，不是「你可以改的东西」。
        if access == Access::Write && !entry.kind.can_write() {
            return Err(format!(
                "{} 是只读条目（用户拖进来的文件），不能改。要改文件，先把它所在的文件夹用「添加文件夹」加进来。",
                entry.label
            ));
        }
        return Ok(());
    }
    Err("不在授权范围内".into())
}

/// 目标文件不存在时，校验它**所在的目录**是否在某个目录条目里。
///
/// 这里刻意只认 `parent()`：`..` 之类的东西在 `canonicalize(parent)` 时就被解析掉了，
/// 最后拼出来的仍然是规范化的绝对路径。
fn resolve_missing(candidate: &Path, entries: &[WorkspaceEntry]) -> Option<PathBuf> {
    let file_name = candidate.file_name()?;
    if file_name == "." || file_name == ".." {
        return None;
    }
    let parent = std::fs::canonicalize(candidate.parent()?).ok()?;
    let parent = simplify(parent);
    let allowed = entries
        .iter()
        .any(|entry| entry.kind.can_write() && entry_covers(entry, &parent));
    if !allowed {
        return None;
    }
    Some(parent.join(file_name))
}

/// 模型给的路径可能长什么样。绝对路径直接用；相对路径挨个条目试着拼。
fn candidates(raw: &str, entries: &[WorkspaceEntry]) -> Vec<PathBuf> {
    let given = PathBuf::from(raw);
    if given.is_absolute() {
        return vec![given];
    }

    let mut out = Vec::new();
    let first = given
        .components()
        .next()
        .map(|part| part.as_os_str().to_string_lossy().to_string());

    for entry in entries {
        let root = PathBuf::from(&entry.path);
        // 模型直接写条目自己的名字（`report.pdf`、`我的项目`）。
        if name_eq(&entry.label, raw) {
            out.push(root.clone());
        }
        if entry.kind == EntryKind::Dir {
            // `src/main.rs`
            out.push(root.join(&given));
            // `我的项目/src/main.rs`：以条目名开头时去掉那一层，别再拼一遍。
            if let Some(first) = first.as_deref() {
                if name_eq(first, &entry.label) && given.components().count() > 1 {
                    let rest: PathBuf = given.components().skip(1).collect();
                    out.push(root.join(rest));
                }
            }
        }
    }
    out
}

/// 被拒绝时给模型（和日志）一段能照着做的话，而不是一句「权限不足」。
fn explain(entries: &[WorkspaceEntry], raw: &str, access: Access, denied: Option<&Path>) -> String {
    if let Some(resolved) = denied {
        for entry in entries {
            if entry_covers(entry, resolved) && access == Access::Write && !entry.kind.can_write() {
                return format!(
                    "{} 是只读条目（用户拖进来的文件），不能改。要改文件，先把它所在的文件夹用「添加文件夹」加进来。",
                    entry.label
                );
            }
        }
    }
    format!(
        "这个路径不在八千代被授权的范围里：{raw}\n现在能碰的只有：\n{}",
        prompt_listing(entries)
    )
}

/// 条目清单，一行一条。同时用于系统提示词与拒绝时的说明。
pub fn prompt_listing(entries: &[WorkspaceEntry]) -> String {
    if entries.is_empty() {
        return "（空：用户还没有授权任何文件或文件夹）".into();
    }
    entries
        .iter()
        .take(MAX_ENTRIES)
        .map(|entry| {
            let access = if entry.kind.can_write() {
                "可读，写要用户确认"
            } else {
                "只读"
            };
            format!("- [{}] {}（{access}）", entry.kind.label(), entry.path)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// ---------- 添加与删除 ----------

/// 单调递增的序号：同一毫秒内连续添加也不会撞 id。
fn next_seq() -> u64 {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    SEQ.fetch_add(1, Ordering::Relaxed)
}

fn new_id(now_ms: u64) -> String {
    format!("w{now_ms}-{}", next_seq())
}

fn label_of(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        // 盘根（`D:\`）没有最后一段，退回整条路径，免得界面上是一片空白。
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

/// 把若干路径加进条目列表。返回真正新增的条数。
///
/// 重复添加同一条目时**复用已有记录**（按规范化后的路径比），不会出现两条一样的。
pub fn add(
    entries: &mut Vec<WorkspaceEntry>,
    kind: EntryKind,
    paths: &[String],
    now_ms: u64,
) -> Result<usize, String> {
    let mut added = 0;
    for raw in paths {
        let resolved = canonical(raw)?;
        let metadata = std::fs::metadata(&resolved)
            .map_err(|error| format!("读不到 {}（{error}）", resolved.display()))?;
        if kind == EntryKind::Dir && !metadata.is_dir() {
            return Err(format!(
                "{} 不是一个文件夹。文件夹请用「添加文件夹」，单个文件请拖进来。",
                resolved.display()
            ));
        }
        if kind == EntryKind::File && !metadata.is_file() {
            return Err(format!(
                "{} 不是一个文件。文件夹请用「添加文件夹」按钮——通过拖拽加进来的目录会绕开写入确认。",
                resolved.display()
            ));
        }
        if entries
            .iter()
            .any(|entry| same_path(Path::new(&entry.path), &resolved))
        {
            continue;
        }
        if entries.len() >= MAX_ENTRIES {
            return Err(format!("工作区最多 {MAX_ENTRIES} 条，先删掉几个再加吧。"));
        }
        entries.push(WorkspaceEntry {
            id: new_id(now_ms),
            kind,
            label: label_of(&resolved),
            path: resolved.to_string_lossy().to_string(),
        });
        added += 1;
    }
    Ok(added)
}

/// 按 id 删掉一条。
pub fn remove(entries: &mut Vec<WorkspaceEntry>, id: &str) -> Result<(), String> {
    let before = entries.len();
    entries.retain(|entry| entry.id != id);
    if entries.len() == before {
        return Err("这条记录已经没有了".into());
    }
    Ok(())
}

// ---------- 落盘与运行态 ----------

pub fn store(app: &AppHandle) -> Result<Store, String> {
    Ok(Store::new(crate::settings::data_dir(app)?))
}

/// 读磁盘上的条目。纯函数版本，测试指向临时目录。
pub fn load_from(store: &Store) -> (Vec<WorkspaceEntry>, Option<String>) {
    let loaded = store.load::<StoredWorkspace>(WORKSPACE_FILE);
    let mut notes: Vec<String> = loaded.note.into_iter().collect();
    let mut entries = loaded.value.entries;

    // 手工编辑过的文件里可能塞进乱七八糟的东西：太长的、路径为空的，直接丢掉并说明。
    let before = entries.len();
    entries.retain(|entry| !entry.path.trim().is_empty() && !entry.id.trim().is_empty());
    if entries.len() != before {
        notes.push(format!(
            "{WORKSPACE_FILE} 里有 {} 条没有路径的记录，已忽略",
            before - entries.len()
        ));
    }
    if entries.len() > MAX_ENTRIES {
        entries.truncate(MAX_ENTRIES);
        notes.push(format!(
            "{WORKSPACE_FILE} 里的条目超过上限，只保留前 {MAX_ENTRIES} 条"
        ));
    }

    if loaded.source == LoadSource::Missing {
        if let Err(error) = store.save(
            WORKSPACE_FILE,
            &StoredWorkspace {
                entries: entries.clone(),
            },
        ) {
            notes.push(format!("写入默认工作区失败：{error}"));
        }
    }

    (
        entries,
        if notes.is_empty() {
            None
        } else {
            Some(notes.join("；"))
        },
    )
}

pub fn save(app: &AppHandle, entries: &[WorkspaceEntry]) -> Result<(), String> {
    store(app)?.save(
        WORKSPACE_FILE,
        &StoredWorkspace {
            entries: entries.to_vec(),
        },
    )
}

/// 读当前条目列表。
pub fn get(app: &AppHandle) -> Result<Vec<WorkspaceEntry>, String> {
    let state = app.state::<WorkspaceState>();
    let entries = state.0.lock().map_err(|error| error.to_string())?;
    Ok(entries.clone())
}

/// 写回内存 → 落盘 → 广播。顺序与 `desktop::update_settings` 一致：
/// 先让本次修改生效，落盘失败只上报、不推翻（重启后会丢，原因会告诉用户）。
pub fn replace(
    app: &AppHandle,
    entries: Vec<WorkspaceEntry>,
) -> Result<Vec<WorkspaceEntry>, String> {
    {
        let state = app.state::<WorkspaceState>();
        let mut current = state.0.lock().map_err(|error| error.to_string())?;
        *current = entries.clone();
    }
    if let Err(error) = save(app, &entries) {
        crate::desktop::report_error(
            app,
            &format!("工作区已生效，但写入磁盘失败（重启后会丢失）：{error}"),
        );
    }
    for failure in broadcast::emit_all(app, "workspace-changed", &entries) {
        eprintln!("[Rust] 工作区通知失败：{failure}");
    }
    Ok(entries)
}

/// 启动时把磁盘上的条目恢复进内存。
pub fn restore(app: &AppHandle) {
    let (entries, note) = match store(app) {
        Ok(store) => load_from(&store),
        Err(error) => (Vec::new(), Some(error)),
    };
    if let Some(note) = note {
        crate::desktop::report_startup_error(app, &format!("读取工作区时发生回退：{note}"));
    }
    match app.state::<WorkspaceState>().0.lock() {
        Ok(mut current) => *current = entries.clone(),
        Err(error) => {
            crate::desktop::report_startup_error(app, &format!("恢复工作区失败：{error}"))
        }
    }
    #[cfg(debug_assertions)]
    println!("[Rust] 启动时恢复工作区：{} 条条目", entries.len());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::test_support::TempDir;
    use std::fs;

    /// 建一个临时目录，装几个文件当工作区。
    fn workspace(tag: &str) -> (TempDir, PathBuf, PathBuf) {
        let dir = TempDir::new(tag);
        let root = dir.path.join("proj");
        fs::create_dir_all(root.join("src")).expect("建目录");
        fs::write(root.join("src").join("main.rs"), "fn main() {}").expect("写文件");
        fs::write(root.join("notes.txt"), "记得买松饼").expect("写文件");
        let file = root.join("notes.txt");
        (dir, root, file)
    }

    fn dir_entry(path: &Path) -> WorkspaceEntry {
        WorkspaceEntry {
            id: "w1-1".into(),
            kind: EntryKind::Dir,
            path: path.to_string_lossy().to_string(),
            label: "proj".into(),
        }
    }

    fn file_entry(path: &Path) -> WorkspaceEntry {
        WorkspaceEntry {
            id: "w1-2".into(),
            kind: EntryKind::File,
            path: path.to_string_lossy().to_string(),
            label: "notes.txt".into(),
        }
    }

    #[test]
    fn a_path_inside_the_authorized_directory_resolves() {
        let (_dir, root, _file) = workspace("inside");
        let entries = vec![dir_entry(&root)];
        let resolved = resolve(&entries, "src/main.rs", Access::Read).expect("应能解析");
        assert!(resolved.ends_with("main.rs"), "{}", resolved.display());
        let absolute = resolve(
            &entries,
            &root.join("src").join("main.rs").to_string_lossy(),
            Access::Read,
        )
        .expect("绝对路径也要能解析");
        assert_eq!(resolved, absolute);
    }

    #[test]
    fn escaping_the_directory_with_dot_dot_is_rejected() {
        let (_dir, root, _file) = workspace("escape");
        let entries = vec![dir_entry(&root)];
        let error = resolve(&entries, "../secret.txt", Access::Read).expect_err("越界应当被拒");
        assert!(error.contains("不在"), "{error}");
        let error = resolve(&entries, "src/../../secret.txt", Access::Read).expect_err("越界");
        assert!(error.contains("不在"), "{error}");
    }

    #[test]
    fn something_outside_the_whitelist_is_rejected_with_a_readable_reason() {
        let (dir, root, _file) = workspace("outside");
        let outside = dir.path.join("outside.txt");
        fs::write(&outside, "不该被读到").expect("写外部文件");
        let entries = vec![dir_entry(&root)];
        let error = resolve(&entries, &outside.to_string_lossy(), Access::Read).expect_err("越界");
        assert!(error.contains("不在八千代被授权的范围"), "{error}");
        assert!(error.contains("目录"), "要说清现在能碰什么：{error}");
    }

    #[test]
    fn file_entries_are_always_read_only() {
        let (_dir, _root, file) = workspace("readonly");
        let entries = vec![file_entry(&file)];
        assert!(resolve(&entries, &file.to_string_lossy(), Access::Read).is_ok());
        let error =
            resolve(&entries, &file.to_string_lossy(), Access::Write).expect_err("文件条目不能写");
        assert!(error.contains("只读"), "{error}");
        // 连它旁边的兄弟文件也碰不到——文件条目的范围就是它自己。
        let sibling = file.with_file_name("other.txt");
        fs::write(&sibling, "x").expect("写同目录文件");
        assert!(resolve(&entries, &sibling.to_string_lossy(), Access::Read).is_err());
    }

    #[test]
    fn write_can_create_a_file_that_does_not_exist_yet_but_only_inside_a_directory_entry() {
        let (_dir, root, _file) = workspace("create");
        let entries = vec![dir_entry(&root)];
        let target = root.join("src").join("new.rs");
        let resolved =
            resolve(&entries, &target.to_string_lossy(), Access::Write).expect("新建文件");
        assert_eq!(resolved, simplify(target));
        // 但要建在授权的目录里。
        assert!(resolve(&entries, "../new.rs", Access::Write).is_err());
    }

    #[test]
    fn relative_paths_that_include_the_entry_name_are_understood() {
        let (_dir, root, _file) = workspace("relative");
        let entries = vec![dir_entry(&root)];
        let target = root.join("src").join("main.rs");
        assert_eq!(
            resolve(&entries, "proj/src/main.rs", Access::Read).expect("带条目名的相对路径"),
            simplify(target)
        );
    }

    /// 两条防线里最要紧的一条：解析后的真实路径要重新过一遍白名单。
    /// 这里用一个指向工作区外的**软链接**来模拟攻击。
    #[cfg(windows)]
    #[test]
    fn a_symlink_pointing_outside_is_rejected_after_canonicalize() {
        let (dir, root, _file) = workspace("symlink");
        let secret_dir = dir.path.join("secret");
        fs::create_dir_all(&secret_dir).expect("建外部目录");
        let secret = secret_dir.join("secret.txt");
        fs::write(&secret, "机密").expect("写外部文件");
        let link = root.join("link.txt");
        if std::os::windows::fs::symlink_file(&secret, &link).is_err() {
            // 没有创建符号链接的权限（开发者模式未开）：这条用例跳过，不影响结论。
            eprintln!("跳过：当前环境不允许创建符号链接");
            return;
        }
        let entries = vec![dir_entry(&root)];
        let error = resolve(&entries, &link.to_string_lossy(), Access::Read)
            .expect_err("指向外部文件的软链接必须被拒");
        assert!(error.contains("不在"), "{error}");
    }

    #[test]
    fn adding_is_idempotent_and_capped() {
        let (_dir, root, file) = workspace("add");
        let mut entries = Vec::new();
        let added = add(
            &mut entries,
            EntryKind::Dir,
            &[root.to_string_lossy().to_string()],
            1,
        )
        .expect("加目录");
        assert_eq!(added, 1);
        // 同一路径再拖一次：复用已有记录，不出现第二条。
        let added = add(
            &mut entries,
            EntryKind::Dir,
            &[root.to_string_lossy().to_string()],
            2,
        )
        .expect("重复添加");
        assert_eq!(added, 0);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].label, "proj");

        add(
            &mut entries,
            EntryKind::File,
            &[file.to_string_lossy().to_string()],
            3,
        )
        .expect("加文件");
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn a_file_added_as_a_directory_is_rejected_with_a_hint() {
        let (_dir, root, file) = workspace("kind-mismatch");
        let mut entries = Vec::new();
        let error = add(
            &mut entries,
            EntryKind::Dir,
            &[file.to_string_lossy().to_string()],
            1,
        )
        .expect_err("文件不能当目录加");
        assert!(error.contains("不是一个文件夹"), "{error}");
        let error = add(
            &mut entries,
            EntryKind::File,
            &[root.to_string_lossy().to_string()],
            1,
        )
        .expect_err("目录不能当文件加");
        assert!(error.contains("不是一个文件"), "{error}");
    }

    #[test]
    fn the_listing_tells_the_model_what_it_may_touch() {
        let (_dir, root, file) = workspace("listing");
        let listing = prompt_listing(&[dir_entry(&root), file_entry(&file)]);
        assert!(listing.contains("[目录]"), "{listing}");
        assert!(listing.contains("[只读文件]"), "{listing}");
        assert!(listing.contains("只读"), "{listing}");
        assert_eq!(
            prompt_listing(&[]),
            "（空：用户还没有授权任何文件或文件夹）"
        );
    }

    #[test]
    fn entries_survive_a_restart() {
        let (_dir, root, file) = workspace("restart");
        let store = Store::new(_dir.path.join("data"));
        let entries = vec![dir_entry(&root), file_entry(&file)];
        store
            .save(
                WORKSPACE_FILE,
                &StoredWorkspace {
                    entries: entries.clone(),
                },
            )
            .expect("落盘");

        let (restored, note) = load_from(&store);
        assert!(note.is_none(), "{note:?}");
        assert_eq!(restored, entries);
    }

    #[test]
    fn records_without_a_path_are_dropped_with_a_note() {
        let (dir, _root, _file) = workspace("broken-record");
        let store = Store::new(dir.path.join("data"));
        fs::create_dir_all(store.path("")).expect("建目录");
        fs::write(
            store.path(WORKSPACE_FILE),
            r#"{"schemaVersion":1,"data":{"entries":[{"id":"w1","kind":"dir","path":"","label":""},{"id":"w2","kind":"file","path":"C:\\ok.txt","label":"ok.txt"}]}}"#,
        )
        .expect("写文件");

        let (entries, note) = load_from(&store);
        assert_eq!(entries.len(), 1);
        assert!(note.expect("应说明").contains("已忽略"));
    }

    #[test]
    fn removing_an_entry_by_id_reports_when_it_is_gone() {
        let (_dir, root, _file) = workspace("remove");
        let mut entries = vec![dir_entry(&root)];
        remove(&mut entries, "w1-1").expect("删除");
        assert!(entries.is_empty());
        assert!(remove(&mut entries, "w1-1").is_err());
    }

    #[test]
    fn prefix_similar_directories_are_not_confused() {
        // C:\foo 不该覆盖 C:\foobar —— 这是字符串前缀比较最容易踩的坑。
        assert!(is_within(Path::new(r"C:\foo"), Path::new(r"C:\foo\bar")));
        assert!(is_within(Path::new(r"C:\foo"), Path::new(r"C:\FOO\Bar")));
        assert!(!is_within(Path::new(r"C:\foo"), Path::new(r"C:\foobar")));
        assert!(!is_within(Path::new(r"C:\foo\bar"), Path::new(r"C:\foo")));
    }

    #[test]
    fn verbatim_prefixes_are_stripped() {
        assert_eq!(
            simplify(PathBuf::from(r"\\?\C:\a\b")),
            PathBuf::from(r"C:\a\b")
        );
        assert_eq!(
            simplify(PathBuf::from(r"\\?\UNC\server\share")),
            PathBuf::from(r"\\server\share")
        );
        assert_eq!(simplify(PathBuf::from(r"C:\a")), PathBuf::from(r"C:\a"));
    }
}
