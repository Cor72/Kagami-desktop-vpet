//! 通用文件存储：原子写 + `.bak` 备份 + 损坏回退。
//!
//! 本模块只依赖 `std` 与 `serde`，**不依赖 Tauri**，因此可以直接用临时目录做单元测试。
//! 落盘策略对应设计文档 §4.2：
//!
//! 1. 内容先写进 `*.tmp`，`sync_all` 之后再 `rename` 覆盖主文件，
//!    避免「写了一半就断电」留下半截 JSON 把下次启动弄坏。
//! 2. 覆盖之前把旧内容复制到 `*.bak`，主文件被外部改坏时还有一份可用的。
//! 3. 读取顺序是主文件 → `.bak` → 默认值。任何一步失败都不 panic，
//!    而是把原因交给调用方去 `report_error`。

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// 所有持久化文件共用的 schema 版本。
/// 加字段或改结构时在这里升版本，并在 [`read`] 里补一段迁移。
pub const SCHEMA_VERSION: u32 = 1;

/// 一次读取里，值实际上是从哪来的。
///
/// 调用方靠它区分「首次运行」和「文件坏了」：首次运行该写一份默认值出来给用户看，
/// 文件坏了则应当保留现场、只上报错误。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadSource {
    /// 主文件读取成功。
    Primary,
    /// 主文件不可用，值来自 `.bak`。
    Backup,
    /// 主文件与 `.bak` 都不存在，值来自默认值（首次运行）。
    Missing,
    /// 文件存在但无法使用，值来自默认值。
    Fallback,
}

/// 读取结果：值 + 来源 + 建议上报给用户的说明。
#[derive(Debug)]
pub struct Loaded<T> {
    pub value: T,
    pub source: LoadSource,
    /// 回退原因。正常读取时为 `None`。
    pub note: Option<String>,
}

/// 一个目录下的若干持久化文件。目录由调用方给出，测试里指向临时目录。
#[derive(Clone, Debug)]
pub struct Store {
    dir: PathBuf,
}

impl Store {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    fn backup_path(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{name}.bak"))
    }

    fn temp_path(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{name}.tmp"))
    }

    /// 读取：主文件 → `.bak` → 默认值。
    ///
    /// 这个函数不会返回 `Err`——读失败本身就是一种需要被优雅处理的状态，
    /// 而不是让程序起不来的原因。
    pub fn load<T: DeserializeOwned + Default>(&self, name: &str) -> Loaded<T> {
        let primary = self.path(name);
        let backup = self.backup_path(name);
        let mut notes = Vec::new();

        match read::<T>(&primary) {
            ReadOutcome::Ok(value) => {
                return Loaded {
                    value,
                    source: LoadSource::Primary,
                    note: None,
                }
            }
            // 首次运行，没有文件不是错误。
            ReadOutcome::Missing => {}
            ReadOutcome::Invalid(reason) => notes.push(reason),
        }

        match read::<T>(&backup) {
            ReadOutcome::Ok(value) => {
                notes.push(format!("已改用备份 {}", backup.display()));
                return Loaded {
                    value,
                    source: LoadSource::Backup,
                    note: Some(notes.join("；")),
                };
            }
            ReadOutcome::Missing => {}
            ReadOutcome::Invalid(reason) => notes.push(reason),
        }

        if notes.is_empty() {
            Loaded {
                value: T::default(),
                source: LoadSource::Missing,
                note: None,
            }
        } else {
            notes.push("已回退默认值".into());
            Loaded {
                value: T::default(),
                source: LoadSource::Fallback,
                note: Some(notes.join("；")),
            }
        }
    }

    /// 写入：`*.tmp` → `.bak` → `rename`。
    pub fn save<T: Serialize>(&self, name: &str, value: &T) -> Result<(), String> {
        let envelope = Envelope {
            schema_version: SCHEMA_VERSION,
            data: value,
        };
        let mut bytes = serde_json::to_vec_pretty(&envelope)
            .map_err(|error| format!("序列化 {name} 失败：{error}"))?;
        // 结尾补一个换行：手工编辑时编辑器不会提示「文件末尾没有换行」。
        bytes.push(b'\n');

        let path = self.path(name);
        let temp = self.temp_path(name);
        fs::create_dir_all(&self.dir)
            .map_err(|error| format!("创建目录 {} 失败：{error}", self.dir.display()))?;

        let written = (|| -> std::io::Result<()> {
            let mut file = fs::File::create(&temp)?;
            file.write_all(&bytes)?;
            // 先落盘再改名：改名本身是原子的，但内容还在系统缓存里就不是了。
            file.sync_all()?;
            drop(file);
            if path.exists() {
                fs::copy(&path, self.backup_path(name))?;
            }
            fs::rename(&temp, &path)
        })();

        if let Err(error) = written {
            // 失败时别留下半截临时文件，下一次写入会重新创建。
            let _ = fs::remove_file(&temp);
            return Err(format!("写入 {name} 失败：{error}"));
        }
        Ok(())
    }
}

/// 文件里真正的形状：版本号 + 内容。
///
/// 用信封而不是把 `schemaVersion` 塞进业务结构体，是为了让业务结构体保持干净：
/// 前端拿到的快照里不会多出一个与它无关的字段。
#[derive(Serialize, Deserialize)]
struct Envelope<T> {
    #[serde(rename = "schemaVersion")]
    schema_version: u32,
    data: T,
}

enum ReadOutcome<T> {
    Ok(T),
    Missing,
    Invalid(String),
}

fn read<T: DeserializeOwned>(path: &Path) -> ReadOutcome<T> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return ReadOutcome::Missing,
        Err(error) => return ReadOutcome::Invalid(format!("{} 读取失败：{error}", path.display())),
    };

    // 这些文件是给人编辑的，而有些编辑器保存 UTF-8 会带 BOM。
    // 不剥掉它的话，用户「只改了一个数字」就会看到设置被回退成默认值。
    match serde_json::from_str::<Envelope<T>>(text.strip_prefix('\u{feff}').unwrap_or(&text)) {
        Ok(envelope) if envelope.schema_version == SCHEMA_VERSION => ReadOutcome::Ok(envelope.data),
        Ok(envelope) => ReadOutcome::Invalid(format!(
            "{} 的 schemaVersion 是 {}，当前版本只认识 {SCHEMA_VERSION}",
            path.display(),
            envelope.schema_version
        )),
        Err(error) => ReadOutcome::Invalid(format!("{} 解析失败：{error}", path.display())),
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::Store;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// 测试用临时目录：名字里带进程号与序号，并行执行的测试不会互相踩。
    /// 用完自动删除，不需要 `tempfile` 之类的新依赖。
    pub(crate) struct TempDir {
        pub(crate) path: PathBuf,
    }

    impl TempDir {
        pub(crate) fn new(tag: &str) -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "yachiyo-store-{}-{tag}-{unique}",
                std::process::id()
            ));
            fs::create_dir_all(&path).expect("创建临时目录");
            Self { path }
        }

        pub(crate) fn store(&self) -> Store {
            Store::new(&self.path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::TempDir;
    use super::*;
    use serde::Deserialize;

    #[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", default)]
    struct Sample {
        max_fps: u32,
        always_on_top: bool,
    }

    const NAME: &str = "settings.json";
    const SAMPLE: Sample = Sample {
        max_fps: 15,
        always_on_top: false,
    };

    #[test]
    fn missing_file_reports_missing_and_uses_default() {
        let dir = TempDir::new("missing");
        let loaded = dir.store().load::<Sample>(NAME);
        assert_eq!(loaded.source, LoadSource::Missing);
        assert_eq!(loaded.value, Sample::default());
        assert!(loaded.note.is_none(), "首次运行不应被当成错误");
    }

    #[test]
    fn saved_value_round_trips() {
        let dir = TempDir::new("round-trip");
        let store = dir.store();
        store.save(NAME, &SAMPLE).expect("写入");

        let loaded = store.load::<Sample>(NAME);
        assert_eq!(loaded.source, LoadSource::Primary);
        assert_eq!(loaded.value, SAMPLE);
        assert!(loaded.note.is_none());
    }

    #[test]
    fn save_writes_envelope_with_schema_version_and_no_temp_left() {
        let dir = TempDir::new("envelope");
        let store = dir.store();
        store.save(NAME, &SAMPLE).expect("写入");

        let text = fs::read_to_string(store.path(NAME)).expect("读取主文件");
        let json: serde_json::Value = serde_json::from_str(&text).expect("解析 JSON");
        assert_eq!(json["schemaVersion"], SCHEMA_VERSION);
        assert_eq!(json["data"]["maxFps"], 15);
        assert!(!store.temp_path(NAME).exists(), "成功后不应留下 .tmp 文件");
        assert!(
            !store.backup_path(NAME).exists(),
            "第一次写入没有旧内容可备份"
        );
    }

    #[test]
    fn save_creates_missing_directory() {
        let dir = TempDir::new("create-dir");
        let nested = Store::new(dir.path.join("a").join("b"));
        nested.save(NAME, &SAMPLE).expect("目录不存在时应自动创建");
        assert_eq!(nested.load::<Sample>(NAME).value, SAMPLE);
    }

    #[test]
    fn overwriting_keeps_previous_content_as_backup() {
        let dir = TempDir::new("backup");
        let store = dir.store();
        store.save(NAME, &SAMPLE).expect("第一次写入");
        let next = Sample {
            max_fps: 30,
            always_on_top: true,
        };
        store.save(NAME, &next).expect("第二次写入");

        assert_eq!(store.load::<Sample>(NAME).value, next);
        let backup = fs::read_to_string(store.backup_path(NAME)).expect("读取备份");
        let json: serde_json::Value = serde_json::from_str(&backup).expect("解析备份");
        assert_eq!(json["data"]["maxFps"], 15, "备份里应当是上一次的内容");
    }

    #[test]
    fn corrupt_primary_falls_back_to_backup() {
        let dir = TempDir::new("corrupt-primary");
        let store = dir.store();
        store.save(NAME, &SAMPLE).expect("第一次写入");
        store.save(NAME, &Sample::default()).expect("第二次写入");
        // 主文件被外部改坏（比如此前写盘写了一半）。
        fs::write(store.path(NAME), "{ 这不是 JSON").expect("写坏主文件");

        let loaded = store.load::<Sample>(NAME);
        assert_eq!(loaded.source, LoadSource::Backup);
        assert_eq!(loaded.value, SAMPLE);
        let note = loaded.note.expect("应说明回退原因");
        assert!(note.contains("解析失败"), "实际说明：{note}");
        assert!(note.contains("备份"), "实际说明：{note}");
    }

    #[test]
    fn corrupt_primary_and_backup_fall_back_to_default_without_panicking() {
        let dir = TempDir::new("corrupt-both");
        let store = dir.store();
        store.save(NAME, &SAMPLE).expect("写入");
        store.save(NAME, &SAMPLE).expect("写入第二次，产生备份");
        fs::write(store.path(NAME), "坏掉的主文件").expect("写坏主文件");
        fs::write(store.backup_path(NAME), "坏掉的备份").expect("写坏备份");

        let loaded = store.load::<Sample>(NAME);
        assert_eq!(loaded.source, LoadSource::Fallback);
        assert_eq!(loaded.value, Sample::default());
        let note = loaded.note.expect("应说明两个文件都不可用");
        assert!(note.contains("已回退默认值"), "实际说明：{note}");
    }

    #[test]
    fn unknown_schema_version_is_rejected_instead_of_guessed() {
        let dir = TempDir::new("schema");
        let store = dir.store();
        store.save(NAME, &SAMPLE).expect("写入");
        fs::write(
            store.path(NAME),
            r#"{"schemaVersion": 99, "data": {"maxFps": 60, "alwaysOnTop": true}}"#,
        )
        .expect("写入未来版本的文件");

        let loaded = store.load::<Sample>(NAME);
        assert_eq!(loaded.source, LoadSource::Fallback);
        assert_eq!(loaded.value, Sample::default());
        assert!(loaded.note.unwrap().contains("schemaVersion"));
    }

    #[test]
    fn byte_order_mark_from_editors_is_tolerated() {
        let dir = TempDir::new("bom");
        let store = dir.store();
        let body = r#"{"schemaVersion": 1, "data": {"maxFps": 15, "alwaysOnTop": false}}"#;
        fs::write(store.path(NAME), format!("\u{feff}{body}")).expect("写入带 BOM 的文件");

        let loaded = store.load::<Sample>(NAME);
        assert_eq!(
            loaded.source,
            LoadSource::Primary,
            "编辑器加的 BOM 不该被当成文件损坏"
        );
        assert_eq!(loaded.value.max_fps, 15);
    }

    #[test]
    fn missing_fields_use_defaults_so_new_fields_can_be_added_later() {
        let dir = TempDir::new("partial");
        let store = dir.store();
        fs::write(
            store.path(NAME),
            r#"{"schemaVersion": 1, "data": {"maxFps": 15}}"#,
        )
        .expect("写入只含部分字段的文件");

        let loaded = store.load::<Sample>(NAME);
        assert_eq!(loaded.source, LoadSource::Primary);
        assert_eq!(loaded.value.max_fps, 15);
        assert!(!loaded.value.always_on_top, "缺失字段用默认值补齐");
    }
}
