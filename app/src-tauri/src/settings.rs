use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

use crate::store::{LoadSource, Store};

/// 设置文件名，位于 Tauri 的 `app_data_dir` 下
/// （Windows 上是 `%APPDATA%\com.yachiyo.desktop\settings.json`）。
pub const SETTINGS_FILE: &str = "settings.json";

pub type PetState = Mutex<PetSettings>;

/// 帧率只接受这两个值：渲染策略与性能实测数据都是按它们定的。
pub const SUPPORTED_FPS: [u32; 2] = [15, 30];

fn is_supported_fps(fps: u32) -> bool {
    SUPPORTED_FPS.contains(&fps)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PetSettings {
    pub revision: u64,
    pub visible: bool,
    pub max_fps: u32,
    pub always_on_top: bool,
    /// 主动互动总开关。关掉之后 Rust 侧连采样都不做（见 `proactive.rs`）。
    pub proactive_enabled: bool,
    /// 允许把浏览器标签页标题当内容用。
    ///
    /// 单独一个开关而不是跟着总开关走：这条通道会读到网银、公司内部系统、
    /// 网页版聊天这类页面，而且内容会**发给第三方模型服务**。
    /// 有个独立开关，才能一句话交代过去：「浏览器标题可以在设置里关掉」。
    pub browser_title_enabled: bool,
}

impl Default for PetSettings {
    fn default() -> Self {
        Self {
            revision: 0,
            visible: true,
            max_fps: 30,
            always_on_top: true,
            proactive_enabled: true,
            browser_title_enabled: true,
        }
    }
}

/// 真正落盘的部分：只有跨重启仍然有意义的偏好。
///
/// `revision` 与 `visible` 故意不在其中：
///
/// - `revision` 是本次运行内「哪个快照更新」的次序标记，重启后从 0 重新开始即可；
/// - `visible` 是会话状态。启动时一定可见，否则托盘一旦创建失败（见 `lib.rs` 里的
///   兜底分支），窗口就既不在屏幕上也没法从托盘找回，只能去杀进程。
///
/// 这也是这个文件可以被手工编辑的原因：里面只有两项，改错的影响范围一眼看得清。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StoredSettings {
    pub max_fps: u32,
    pub always_on_top: bool,
    pub proactive: StoredProactive,
}

/// 主动互动的持久化部分。做成一个小结构体，以后加「静默时段」之类的配置
/// 只要往这里添字段，不用再动 `StoredSettings` 的形状。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StoredProactive {
    pub enabled: bool,
    /// 是否允许用浏览器标签页标题（老文件里没有这一项，默认开着）。
    pub browser_title: bool,
}

impl Default for StoredProactive {
    fn default() -> Self {
        Self {
            enabled: PetSettings::default().proactive_enabled,
            browser_title: PetSettings::default().browser_title_enabled,
        }
    }
}

impl Default for StoredSettings {
    fn default() -> Self {
        let defaults = PetSettings::default();
        Self {
            max_fps: defaults.max_fps,
            always_on_top: defaults.always_on_top,
            proactive: StoredProactive::default(),
        }
    }
}

#[derive(Clone, Copy)]
pub enum SettingsChange {
    Visible(bool),
    MaxFps(u32),
    AlwaysOnTop(bool),
    ProactiveEnabled(bool),
    BrowserTitleEnabled(bool),
}

impl PetSettings {
    pub fn update(
        &mut self,
        change: SettingsChange,
        apply_window: impl FnOnce() -> Result<(), String>,
    ) -> Result<Self, String> {
        let mut next = self.clone();
        match change {
            SettingsChange::Visible(visible) => next.visible = visible,
            SettingsChange::AlwaysOnTop(enabled) => next.always_on_top = enabled,
            SettingsChange::ProactiveEnabled(enabled) => next.proactive_enabled = enabled,
            SettingsChange::BrowserTitleEnabled(enabled) => next.browser_title_enabled = enabled,
            SettingsChange::MaxFps(fps) => {
                if !is_supported_fps(fps) {
                    return Err("帧率只能是 15 或 30".into());
                }
                next.max_fps = fps;
            }
        }
        // 先完成系统操作，失败时保留原状态和版本号。
        apply_window()?;
        next.revision += 1;
        *self = next.clone();
        Ok(next)
    }

    /// 由磁盘上的值构造运行状态。
    ///
    /// 文件是可以被手工编辑的，所以这里要重新校验一遍：非法值不静默采纳，
    /// 而是回退默认值并把原因交给调用方上报。返回值的第二项就是那段说明。
    pub fn from_stored(stored: StoredSettings) -> (Self, Option<String>) {
        let fallback_fps = Self::default().max_fps;
        if !is_supported_fps(stored.max_fps) {
            return (
                Self {
                    max_fps: fallback_fps,
                    always_on_top: stored.always_on_top,
                    proactive_enabled: stored.proactive.enabled,
                    browser_title_enabled: stored.proactive.browser_title,
                    ..Self::default()
                },
                Some(format!(
                    "{} 里的 maxFps={} 不合法（只能是 {} 或 {}），已改用 {}",
                    SETTINGS_FILE, stored.max_fps, SUPPORTED_FPS[0], SUPPORTED_FPS[1], fallback_fps
                )),
            );
        }
        (
            Self {
                max_fps: stored.max_fps,
                always_on_top: stored.always_on_top,
                proactive_enabled: stored.proactive.enabled,
                browser_title_enabled: stored.proactive.browser_title,
                ..Self::default()
            },
            None,
        )
    }

    /// 提取需要落盘的部分。
    pub fn to_stored(&self) -> StoredSettings {
        StoredSettings {
            max_fps: self.max_fps,
            always_on_top: self.always_on_top,
            proactive: StoredProactive {
                enabled: self.proactive_enabled,
                browser_title: self.browser_title_enabled,
            },
        }
    }
}

/// 应用数据目录：交给 Tauri 按 `tauri.conf.json` 的 identifier 解析，
/// 不写程序目录，避免权限问题与更新时被清掉。
///
/// 设置、AI 配置、对话历史都落在这里，只是文件名不同。
pub fn data_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    app.path()
        .app_data_dir()
        .map_err(|error| format!("读取应用数据目录失败：{error}"))
}

pub fn store(app: &AppHandle) -> Result<Store, String> {
    Ok(Store::new(data_dir(app)?))
}

/// 启动时读取设置。
///
/// 纯函数版本：只依赖一个 [`Store`]，因此测试里可以指向临时目录。
/// 返回设置本身，以及一段需要上报给用户的说明（正常情况下是 `None`）。
pub fn load_from(store: &Store) -> (PetSettings, Option<String>) {
    let loaded = store.load::<StoredSettings>(SETTINGS_FILE);
    let (settings, invalid_note) = PetSettings::from_stored(loaded.value);

    let mut notes: Vec<String> = loaded.note.into_iter().collect();
    notes.extend(invalid_note);

    // 首次运行：把默认值写出来一份，用户才有文件可以看、可以改。
    // 文件损坏时刻意不写——保留现场比覆盖它更有用。
    if loaded.source == LoadSource::Missing {
        if let Err(error) = store.save(SETTINGS_FILE, &settings.to_stored()) {
            notes.push(format!("写入默认设置失败：{error}"));
        }
    }

    (
        settings,
        if notes.is_empty() {
            None
        } else {
            Some(notes.join("；"))
        },
    )
}

/// 写入设置。纯函数版本。
pub fn save_to(store: &Store, settings: &PetSettings) -> Result<(), String> {
    store.save(SETTINGS_FILE, &settings.to_stored())
}

/// 启动时读取：读出磁盘上的值，并给出需要上报的说明。
pub fn load(app: &AppHandle) -> (PetSettings, Option<String>) {
    match store(app) {
        Ok(store) => load_from(&store),
        Err(error) => (PetSettings::default(), Some(error)),
    }
}

/// 写入设置。
pub fn save(app: &AppHandle, settings: &PetSettings) -> Result<(), String> {
    save_to(&store(app)?, settings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::test_support::TempDir;
    use std::fs;

    #[test]
    fn failed_window_change_does_not_publish_new_state() {
        let mut settings = PetSettings::default();
        let result = settings.update(
            SettingsChange::Visible(false),
            || Err("窗口隐藏失败".into()),
        );
        assert!(result.is_err());
        assert!(settings.visible);
        assert_eq!(settings.revision, 0);
    }

    #[test]
    fn unsupported_fps_is_rejected_before_applying_changes() {
        let mut settings = PetSettings::default();
        for fps in [0, 14, 16, 60] {
            assert!(settings
                .update(SettingsChange::MaxFps(fps), || panic!("不应操作窗口"))
                .is_err());
            assert_eq!(settings.max_fps, 30);
            assert_eq!(settings.revision, 0);
        }
    }

    #[test]
    fn successful_changes_preserve_other_settings_and_advance_revision() {
        let mut settings = PetSettings::default();
        settings
            .update(SettingsChange::MaxFps(15), || Ok(()))
            .unwrap();
        settings
            .update(SettingsChange::Visible(false), || Ok(()))
            .unwrap();
        let result = settings
            .update(SettingsChange::AlwaysOnTop(false), || Ok(()))
            .unwrap();
        assert_eq!(
            result,
            PetSettings {
                revision: 3,
                visible: false,
                max_fps: 15,
                always_on_top: false,
                proactive_enabled: true,
                browser_title_enabled: true,
            }
        );
        assert_eq!(settings, result);
    }

    /// 主动互动的开关要和别的偏好一样跨重启保留，而且不跟着「显示/隐藏」写文件。
    #[test]
    fn proactive_switch_survives_a_restart_and_is_written_under_its_own_key() {
        let dir = TempDir::new("proactive-restart");
        let store = dir.store();

        let mut settings = PetSettings::default();
        settings
            .update(SettingsChange::ProactiveEnabled(false), || Ok(()))
            .unwrap();
        // 隐藏只改会话状态，但也必须把主动互动的选择一起带走，不能覆盖掉。
        settings
            .update(SettingsChange::Visible(false), || Ok(()))
            .unwrap();
        save_to(&store, &settings).expect("写入设置");

        let text = fs::read_to_string(store.path(SETTINGS_FILE)).expect("读取");
        let json: serde_json::Value = serde_json::from_str(&text).expect("解析");
        assert_eq!(json["data"]["proactive"]["enabled"], false, "{text}");

        let (restored, note) = load_from(&store);
        assert!(note.is_none(), "正常读取不应产生说明：{note:?}");
        assert!(!restored.proactive_enabled, "重启后开关应保持关闭");
    }

    /// 老版本写出来的 `settings.json` 里没有 `proactive` 一节：默认必须是**开着**，
    /// 否则升级上来的人会发现新功能「怎么没反应」。
    #[test]
    fn a_file_without_the_proactive_section_keeps_the_feature_on() {
        let dir = TempDir::new("no-proactive-key");
        let store = dir.store();
        fs::write(
            store.path(SETTINGS_FILE),
            r#"{"schemaVersion": 1, "data": {"maxFps": 15, "alwaysOnTop": false}}"#,
        )
        .expect("写入旧版文件");

        let (settings, note) = load_from(&store);
        assert!(note.is_none(), "{note:?}");
        assert!(settings.proactive_enabled, "默认开着");
        assert_eq!(settings.max_fps, 15);
    }

    /// Phase 7 的核心验收：写下去的设置，下一次启动读到的是同一份值。
    /// 用同一个临时目录模拟「退出再启动」。
    #[test]
    fn settings_survive_a_restart() {
        let dir = TempDir::new("restart");
        let store = dir.store();

        let mut first_run = PetSettings::default();
        first_run
            .update(SettingsChange::MaxFps(15), || Ok(()))
            .unwrap();
        first_run
            .update(SettingsChange::AlwaysOnTop(false), || Ok(()))
            .unwrap();
        save_to(&store, &first_run).expect("写入设置");

        let (restored, note) = load_from(&store);
        assert!(note.is_none(), "正常读取不应产生说明：{note:?}");
        assert_eq!(restored.max_fps, 15, "重启后帧率应保持 15");
        assert!(!restored.always_on_top, "重启后置顶开关应保持关闭");
        assert_eq!(restored.revision, 0, "revision 是本次运行内的标记");
    }

    #[test]
    fn first_run_writes_a_default_file_users_can_edit() {
        let dir = TempDir::new("first-run");
        let store = dir.store();
        assert!(!store.path(SETTINGS_FILE).exists());

        let (settings, note) = load_from(&store);
        assert_eq!(settings, PetSettings::default());
        assert!(note.is_none(), "首次运行不是错误：{note:?}");

        let text = fs::read_to_string(store.path(SETTINGS_FILE)).expect("默认文件应被写出");
        let json: serde_json::Value = serde_json::from_str(&text).expect("解析");
        assert_eq!(json["data"]["maxFps"], 30);
        assert_eq!(json["data"]["alwaysOnTop"], true);
        assert!(
            json["data"].get("visible").is_none() && json["data"].get("revision").is_none(),
            "会话状态不应落盘：{text}"
        );
    }

    /// 手工把 maxFps 改成 60：程序要能启动、要改用默认值、要说清原因，
    /// 并且**不**偷偷把用户改过的文件覆盖掉。
    #[test]
    fn invalid_fps_from_hand_edited_file_is_reported_and_not_silently_rewritten() {
        let dir = TempDir::new("invalid-fps");
        let store = dir.store();
        fs::write(
            store.path(SETTINGS_FILE),
            r#"{"schemaVersion": 1, "data": {"maxFps": 60, "alwaysOnTop": false}}"#,
        )
        .expect("写入手工编辑过的文件");

        let (settings, note) = load_from(&store);
        assert_eq!(settings.max_fps, 30, "非法帧率回退默认值");
        assert!(!settings.always_on_top, "其余字段仍然生效");
        let note = note.expect("应上报原因");
        assert!(
            note.contains("maxFps=60"),
            "说明里应包含用户写进去的值：{note}"
        );

        let text = fs::read_to_string(store.path(SETTINGS_FILE)).expect("读取");
        assert!(text.contains("60"), "用户改过的文件应保留现场：{text}");
    }

    #[test]
    fn corrupt_file_falls_back_to_defaults_and_leaves_the_file_alone() {
        let dir = TempDir::new("corrupt");
        let store = dir.store();
        fs::write(store.path(SETTINGS_FILE), "这不是 JSON").expect("写坏文件");

        let (settings, note) = load_from(&store);
        assert_eq!(settings, PetSettings::default());
        let note = note.expect("应上报原因");
        assert!(note.contains("解析失败"), "说明：{note}");
        assert_eq!(
            fs::read_to_string(store.path(SETTINGS_FILE)).unwrap(),
            "这不是 JSON",
            "损坏的文件应留给用户排查，不被覆盖"
        );
    }

    #[test]
    fn hidden_state_is_not_persisted_so_startup_is_always_visible() {
        let dir = TempDir::new("hidden");
        let store = dir.store();
        let mut settings = PetSettings::default();
        settings
            .update(SettingsChange::Visible(false), || Ok(()))
            .unwrap();
        save_to(&store, &settings).expect("写入");

        let (restored, _) = load_from(&store);
        assert!(restored.visible, "启动时必须可见，否则窗口可能找不回来");
    }
}
