//! 受限制的文件工具：`list_files` / `read_file` / `grep`。
//!
//! 每一个函数都先经过 [`crate::workspace::resolve`]——**每次都重新校验路径**，
//! 不缓存、不信任上一步的结果（实施计划 §7）。参数里的路径可以是绝对路径，
//! 也可以是相对某个条目的相对路径，解析规则在 `workspace::resolve` 里。
//!
//! 这里的硬上限（实施计划 §5.2）不是优化，是安全边界：工具返回值会**全部进入
//! 上下文**，一个 5000 行的文件读进来，对话历史就被挤出去了。
//!
//! 本模块只用 `std`，可以用临时目录直接单测。

use std::path::{Path, PathBuf};

use crate::workspace::{self, Access, WorkspaceEntry};

/// `list_files` 的深度上限。再深基本是依赖目录，没有阅读价值。
pub const MAX_LIST_DEPTH: usize = 3;
/// `list_files` 的条数上限。
pub const MAX_LIST_ENTRIES: usize = 200;
/// `grep` 的命中上限。
pub const MAX_GREP_MATCHES: usize = 50;
/// `grep` 单条命中最多留多少个字符。
pub const MAX_GREP_LINE_CHARS: usize = 200;
/// 单文件读取上限。
pub const MAX_READ_BYTES: usize = 64 * 1024;
/// `grep` 会跳过的、看起来不是源码的目录。
pub const IGNORED_DIRS: &[&str] = &[
    "node_modules",
    ".git",
    "target",
    "dist",
    ".venv",
    "__pycache__",
    ".idea",
    ".vscode",
    ".next",
    ".cache",
];
/// `grep` 扫描的文件数上限：挡住「把工作区指向整个 C 盘」那种输入。
pub const MAX_GREP_FILES: usize = 2_000;
/// `grep` 单文件的体积上限。
pub const MAX_GREP_FILE_BYTES: u64 = 1024 * 1024;

fn ignored_dir(name: &str) -> bool {
    IGNORED_DIRS
        .iter()
        .any(|ignored| name.eq_ignore_ascii_case(ignored))
}

/// 截断一行，附省略号。
fn clip(text: &str, limit: usize) -> String {
    let trimmed = text.trim_end();
    if trimmed.chars().count() <= limit {
        return trimmed.to_string();
    }
    trimmed.chars().take(limit).collect::<String>() + "…"
}

/// 按「目录在前、再按名字」排序，保证同样的工作区每次输出一样（测试才好写）。
fn sorted_children(dir: &Path) -> Vec<PathBuf> {
    let Ok(reader) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut children: Vec<PathBuf> = reader.flatten().map(|entry| entry.path()).collect();
    children.sort_by_key(|path| {
        let is_dir = path.is_dir();
        (
            !is_dir,
            path.file_name()
                .map(|name| name.to_string_lossy().to_lowercase()),
        )
    });
    children
}

/// 相对一个根排版的路径名（只看最后一段，够用且短）。
fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

/// 这次要列 / 要搜的根。
///
/// 路径为空 = 所有条目根（设计文档 §5.4：路径本身就是范围，不为工具新增 `scope` 参数）。
fn roots(entries: &[WorkspaceEntry], path: Option<&str>) -> Result<Vec<PathBuf>, String> {
    match path.map(str::trim).filter(|value| !value.is_empty()) {
        Some(raw) => Ok(vec![workspace::resolve(entries, raw, Access::Read)?]),
        None => {
            if entries.is_empty() {
                return Err(
                    "用户还没有授权任何文件或文件夹，先在对话窗口的 Agent 模式里加一个。".into(),
                );
            }
            Ok(entries
                .iter()
                .map(|entry| PathBuf::from(&entry.path))
                .collect())
        }
    }
}

/// 目录树。`depth` 从 0（根本身）开始。
fn walk_tree(path: &Path, depth: usize, out: &mut Vec<String>, truncated: &mut bool) {
    if *truncated {
        return;
    }
    let indent = "  ".repeat(depth + 1);
    for child in sorted_children(path) {
        if *truncated {
            return;
        }
        let name = name_of(&child);
        let is_dir = child.is_dir();
        if is_dir && ignored_dir(&name) {
            out.push(format!("{indent}{name}/（已跳过）"));
            continue;
        }
        if out.len() >= MAX_LIST_ENTRIES {
            *truncated = true;
            return;
        }
        if is_dir {
            out.push(format!("{indent}{name}/"));
            if depth + 1 < MAX_LIST_DEPTH {
                walk_tree(&child, depth + 1, out, truncated);
            } else {
                out.push(format!("{indent}  …（更深的内容没列出来）"));
            }
        } else {
            let size = std::fs::metadata(&child)
                .map(|meta| meta.len())
                .unwrap_or(0);
            out.push(format!("{indent}{name}（{size} 字节）"));
        }
    }
}

/// 列出工作区结构。
pub fn list_files(entries: &[WorkspaceEntry], path: Option<&str>) -> Result<String, String> {
    let roots = roots(entries, path)?;
    let mut lines = Vec::new();
    let mut truncated = false;

    for root in &roots {
        if lines.len() >= MAX_LIST_ENTRIES {
            truncated = true;
            break;
        }
        if root.is_dir() {
            lines.push(format!("{}/", root.display()));
            walk_tree(root, 0, &mut lines, &mut truncated);
        } else {
            lines.push(format!(
                "{}（{} 字节，文件条目）",
                root.display(),
                std::fs::metadata(root).map(|meta| meta.len()).unwrap_or(0)
            ));
        }
    }

    let mut text = format!(
        "工作区结构（深度 ≤ {MAX_LIST_DEPTH}，最多 {MAX_LIST_ENTRIES} 条）：\n{}",
        lines.join("\n")
    );
    if truncated {
        text.push_str(&format!(
            "\n…已截断，只列到第 {MAX_LIST_ENTRIES} 条。要看更多，先用 grep 定位。"
        ));
    }
    Ok(text)
}

/// 把文件最后修改时间转成 UTC 字符串。
///
/// 手写而不是引一个新依赖：这里只需要「哪一天哪一刻」这一种格式，
/// 而本地时区在 Windows 上要走系统 API，多一个依赖不划算。
pub fn format_utc(seconds_since_epoch: u64) -> String {
    let days = (seconds_since_epoch / 86_400) as i64;
    let rest = seconds_since_epoch % 86_400;
    let (hour, minute, second) = (rest / 3_600, (rest % 3_600) / 60, rest % 60);

    // Howard Hinnant 的 civil_from_days：把「1970-01-01 起的天数」还原成公历日期。
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = (z - era * 146_097) as u64;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era as i64 + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let mp = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    if month <= 2 {
        year += 1;
    }
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02} UTC")
}

/// 读一个文件。返回值里带**最后修改时间**（计划 §7）：文件条目记的是路径而不是副本，
/// 内容随时可能变，模型至少要知道自己看的是哪个时刻的版本。
pub fn read_file(entries: &[WorkspaceEntry], path: &str) -> Result<String, String> {
    let resolved = workspace::resolve(entries, path, Access::Read)?;
    let metadata = std::fs::metadata(&resolved)
        .map_err(|error| format!("读不到这个文件（{error}）：{}", resolved.display()))?;
    if metadata.is_dir() {
        return Err(format!(
            "{} 是一个文件夹。用 list_files 看里面有什么。",
            resolved.display()
        ));
    }

    let bytes = std::fs::read(&resolved)
        .map_err(|error| format!("读不到这个文件（{error}）：{}", resolved.display()))?;
    let truncated = bytes.len() > MAX_READ_BYTES;
    let slice = if truncated {
        &bytes[..MAX_READ_BYTES]
    } else {
        &bytes[..]
    };
    if slice.contains(&0) {
        return Err(format!(
            "{} 看起来是二进制文件，读不出文字。",
            resolved.display()
        ));
    }

    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| format_utc(duration.as_secs()))
        .unwrap_or_else(|| "读不到".into());

    let mut text = format!(
        "文件：{}\n最后修改：{modified}\n大小：{} 字节\n----\n{}",
        resolved.display(),
        metadata.len(),
        String::from_utf8_lossy(slice)
    );
    if truncated {
        text.push_str(&format!(
            "\n…（文件超过 {} KB，只给了前 {} KB。要找具体位置，先用 grep。）",
            MAX_READ_BYTES / 1024,
            MAX_READ_BYTES / 1024
        ));
    }
    Ok(text)
}

/// 收集根目录下的所有可读文件（同一个忽略规则与深度上限）。
fn collect_files(root: &Path, out: &mut Vec<PathBuf>) {
    if root.is_file() {
        if out.len() < MAX_GREP_FILES {
            out.push(root.to_path_buf());
        }
        return;
    }
    collect_into(root, 0, out);
}

fn collect_into(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth >= MAX_LIST_DEPTH || out.len() >= MAX_GREP_FILES {
        return;
    }
    for child in sorted_children(dir) {
        if out.len() >= MAX_GREP_FILES {
            return;
        }
        let name = name_of(&child);
        if child.is_dir() {
            if ignored_dir(&name) {
                continue;
            }
            collect_into(&child, depth + 1, out);
        } else {
            out.push(child);
        }
    }
}

/// 在工作区里搜文本。
///
/// 匹配用**大小写不敏感的子串**，不是正则：模型最常找的是「这个函数名 / 这个字段」，
/// 而正则引擎会把每个文件都编译一遍，得不偿失（也少一个要编译的依赖）。
pub fn grep(
    entries: &[WorkspaceEntry],
    pattern: &str,
    path: Option<&str>,
) -> Result<String, String> {
    let pattern = pattern.trim();
    if pattern.is_empty() {
        return Err("要找什么？pattern 是空的。".into());
    }
    let needle = pattern.to_lowercase();
    let roots = roots(entries, path)?;

    let mut files = Vec::new();
    for root in &roots {
        collect_files(root, &mut files);
    }

    let mut hits: Vec<String> = Vec::new();
    let mut skipped_big = 0usize;
    for file in &files {
        if hits.len() >= MAX_GREP_MATCHES {
            break;
        }
        let Ok(metadata) = std::fs::metadata(file) else {
            continue;
        };
        if metadata.len() > MAX_GREP_FILE_BYTES {
            skipped_big += 1;
            continue;
        }
        let Ok(bytes) = std::fs::read(file) else {
            continue;
        };
        if bytes.contains(&0) {
            continue;
        }
        let text = String::from_utf8_lossy(&bytes);
        for (index, line) in text.lines().enumerate() {
            if hits.len() >= MAX_GREP_MATCHES {
                break;
            }
            if line.to_lowercase().contains(&needle) {
                hits.push(format!(
                    "{}:{}: {}",
                    file.display(),
                    index + 1,
                    clip(line, MAX_GREP_LINE_CHARS)
                ));
            }
        }
    }

    if hits.is_empty() {
        return Ok(format!("没有找到「{pattern}」。"));
    }
    let mut text = format!("找到 {} 处：\n{}", hits.len(), hits.join("\n"));
    if hits.len() >= MAX_GREP_MATCHES {
        text.push_str(&format!("\n…最多只给 {MAX_GREP_MATCHES} 处，剩下的没列。"));
    }
    if skipped_big > 0 {
        text.push_str(&format!("\n（跳过了 {skipped_big} 个超过 1 MB 的文件）"));
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::test_support::TempDir;
    use crate::workspace::EntryKind;
    use std::fs;

    struct Fixture {
        _dir: TempDir,
        root: PathBuf,
        entries: Vec<WorkspaceEntry>,
    }

    fn fixture(tag: &str) -> Fixture {
        let dir = TempDir::new(tag);
        let root = dir.path.join("proj");
        fs::create_dir_all(root.join("src").join("deep")).expect("建目录");
        fs::create_dir_all(root.join("node_modules").join("left-pad")).expect("建目录");
        fs::write(root.join("notes.txt"), "记得买松饼\n松饼要加黄油\n").expect("写文件");
        fs::write(
            root.join("src").join("main.rs"),
            "fn main() {\n    println!(\"yachiyo\");\n}\n",
        )
        .expect("写文件");
        fs::write(
            root.join("node_modules").join("left-pad").join("index.js"),
            "module.exports = 1\n",
        )
        .expect("写文件");
        let entries = vec![WorkspaceEntry {
            id: "w1-1".into(),
            kind: EntryKind::Dir,
            path: root.to_string_lossy().to_string(),
            label: "proj".into(),
        }];
        Fixture {
            _dir: dir,
            root,
            entries,
        }
    }

    #[test]
    fn listing_shows_the_tree_and_skips_dependency_directories() {
        let fixture = fixture("list");
        let text = list_files(&fixture.entries, None).expect("列目录");
        assert!(text.contains("notes.txt"), "{text}");
        assert!(text.contains("main.rs"), "{text}");
        assert!(text.contains("已跳过"), "依赖目录应当被跳过：{text}");
        assert!(!text.contains("left-pad"), "跳过就不该再列出来：{text}");
    }

    #[test]
    fn listing_a_subdirectory_stays_inside_it() {
        let fixture = fixture("list-sub");
        let text = list_files(&fixture.entries, Some("src")).expect("列子目录");
        assert!(text.contains("main.rs"), "{text}");
        assert!(
            !text.contains("notes.txt"),
            "不该列出子目录外面的东西：{text}"
        );
    }

    #[test]
    fn listing_outside_the_workspace_is_refused() {
        let fixture = fixture("list-escape");
        assert!(list_files(&fixture.entries, Some("../")).is_err());
        assert!(list_files(&fixture.entries, Some("C:\\Windows")).is_err());
    }

    #[test]
    fn reading_returns_the_text_and_the_modification_time() {
        let fixture = fixture("read");
        let text = read_file(&fixture.entries, "notes.txt").expect("读文件");
        assert!(text.contains("记得买松饼"), "{text}");
        assert!(text.contains("最后修改："), "要带上修改时间：{text}");
        assert!(text.contains("UTC"), "{text}");
    }

    #[test]
    fn reading_a_directory_says_so_instead_of_failing_obscurely() {
        let fixture = fixture("read-dir");
        let error = read_file(&fixture.entries, "src").expect_err("目录不是文件");
        assert!(error.contains("文件夹"), "{error}");
    }

    #[test]
    fn big_files_are_truncated_with_a_hint() {
        let fixture = fixture("read-big");
        let big = "x".repeat(MAX_READ_BYTES + 100);
        fs::write(fixture.root.join("big.txt"), &big).expect("写大文件");
        let text = read_file(&fixture.entries, "big.txt").expect("读大文件");
        assert!(text.contains("只给了前"), "{text}");
        assert!(
            text.len() < MAX_READ_BYTES + 1_000,
            "截断之后不该把整个文件带回来：{}",
            text.len()
        );
    }

    #[test]
    fn binary_files_are_refused_instead_of_turned_into_garbage() {
        let fixture = fixture("read-binary");
        fs::write(fixture.root.join("blob.bin"), [0u8, 1, 2, 3]).expect("写二进制");
        let error = read_file(&fixture.entries, "blob.bin").expect_err("二进制应被拒");
        assert!(error.contains("二进制"), "{error}");
    }

    #[test]
    fn grep_finds_matches_with_file_and_line() {
        let fixture = fixture("grep");
        let text = grep(&fixture.entries, "松饼", None).expect("搜");
        assert!(text.contains("notes.txt:1"), "{text}");
        assert!(text.contains("notes.txt:2"), "{text}");
        assert!(text.contains("2 处"), "{text}");
    }

    #[test]
    fn grep_is_case_insensitive_and_reports_nothing_found() {
        let fixture = fixture("grep-case");
        let text = grep(&fixture.entries, "PRINTLN", None).expect("搜");
        assert!(text.contains("main.rs"), "{text}");
        let text = grep(&fixture.entries, "不存在的东西", None).expect("搜");
        assert!(text.contains("没有找到"), "{text}");
    }

    #[test]
    fn grep_does_not_walk_into_ignored_directories() {
        let fixture = fixture("grep-ignore");
        let text = grep(&fixture.entries, "module.exports", None).expect("搜");
        assert!(text.contains("没有找到"), "node_modules 不该被搜：{text}");
    }

    #[test]
    fn grep_without_a_pattern_explains_itself() {
        let fixture = fixture("grep-empty");
        assert!(grep(&fixture.entries, "   ", None).is_err());
    }

    #[test]
    fn grep_caps_the_number_of_hits() {
        let fixture = fixture("grep-cap");
        let many = "命中\n".repeat(MAX_GREP_MATCHES + 20);
        fs::write(fixture.root.join("many.txt"), many).expect("写文件");
        let text = grep(&fixture.entries, "命中", None).expect("搜");
        assert!(text.contains("最多只给"), "{text}");
    }

    #[test]
    fn utc_formatting_matches_known_timestamps() {
        assert_eq!(format_utc(0), "1970-01-01 00:00:00 UTC");
        assert_eq!(format_utc(1_600_000_000), "2020-09-13 12:26:40 UTC");
        // 闰年 2 月 29 日，最容易算错的一天。
        assert_eq!(format_utc(1_582_934_400), "2020-02-29 00:00:00 UTC");
    }

    #[test]
    fn long_lines_are_clipped() {
        assert_eq!(clip("abc", 10), "abc");
        assert_eq!(clip("abcdef", 3), "abc…");
    }
}
