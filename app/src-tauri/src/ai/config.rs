//! AI 配置（`ai.json`）。
//!
//! 只存「跨重启仍然有意义」的偏好：服务商、模型名、Base URL、对话模式。
//! **API Key 不在这里**——它只进 Windows 凭据管理器，见 [`super::secret`]。
//!
//! 文件是可以被用户手工编辑的，所以读取路径要重新校验一遍：非法值不静默采纳，
//! 而是回退到该服务商的默认值，并把原因交给调用方上报（与 `settings.rs` 同一套口径）。

use serde::{Deserialize, Serialize};

use crate::store::{LoadSource, Store};

pub const AI_FILE: &str = "ai.json";

pub const DEEPSEEK_BASE_URL: &str = "https://api.deepseek.com/v1";
pub const DEEPSEEK_MODEL: &str = "deepseek-chat";
pub const OPENAI_BASE_URL: &str = "https://api.openai.com/v1";
pub const OPENAI_MODEL: &str = "gpt-4o-mini";

/// 模型名的长度上限。挡住「把整篇提示词粘进模型名」这种输入。
pub const MAX_MODEL_CHARS: usize = 80;
/// Base URL 的长度上限。
pub const MAX_BASE_URL_CHARS: usize = 200;

/// 服务商。默认 DeepSeek（国内直连，用户手上就是它的 Key）。
///
/// 三家的请求格式都是 OpenAI 兼容，区别只有默认地址、默认模型和 Key 的存放位置。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    #[default]
    Deepseek,
    Openai,
    Custom,
}

impl Provider {
    /// 稳定标识：keyring 的 account 名、日志里的名字都用它。
    pub fn id(self) -> &'static str {
        match self {
            Provider::Deepseek => "deepseek",
            Provider::Openai => "openai",
            Provider::Custom => "custom",
        }
    }

    pub fn default_base_url(self) -> &'static str {
        match self {
            Provider::Deepseek => DEEPSEEK_BASE_URL,
            Provider::Openai => OPENAI_BASE_URL,
            // 自定义服务商没有默认地址：留空，逼用户自己填。
            Provider::Custom => "",
        }
    }

    pub fn default_model(self) -> &'static str {
        match self {
            Provider::Deepseek => DEEPSEEK_MODEL,
            Provider::Openai => OPENAI_MODEL,
            Provider::Custom => "",
        }
    }

    /// 给用户看的名字，错误信息里用。
    pub fn label(self) -> &'static str {
        match self {
            Provider::Deepseek => "DeepSeek",
            Provider::Openai => "OpenAI",
            Provider::Custom => "自定义服务商",
        }
    }
}

/// 对话模式。阶段 A 只实现聊天模式，Agent 模式在阶段 C。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Chat,
    Agent,
}

/// 落盘的部分。`#[serde(default)]` 让新加的字段在旧文件里缺省可用。
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StoredAiConfig {
    pub provider: Provider,
    pub model: String,
    pub base_url: String,
    pub mode: Mode,
}

/// 运行态配置。`revision` 是本次运行内的次序标记，不进文件（与 `PetSettings` 同理）。
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiConfig {
    pub revision: u64,
    pub provider: Provider,
    pub model: String,
    pub base_url: String,
    pub mode: Mode,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            revision: 0,
            provider: Provider::default(),
            model: DEEPSEEK_MODEL.to_string(),
            base_url: DEEPSEEK_BASE_URL.to_string(),
            mode: Mode::default(),
        }
    }
}

/// 前端拿到的快照：配置 + 「有没有 Key」+ 掩码。**没有 Key 本身。**
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiConfigView {
    pub revision: u64,
    pub provider: Provider,
    pub model: String,
    pub base_url: String,
    pub mode: Mode,
    pub has_key: bool,
    /// 如 `sk-****1234`；没存 Key 时是空串。
    pub key_mask: String,
}

/// 一次修改要改哪些字段。缺省字段保持原值。
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AiConfigPatch {
    pub provider: Option<Provider>,
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub mode: Option<Mode>,
}

impl AiConfig {
    /// 应用一次修改。校验失败时**不改动**当前配置。
    pub fn apply(&mut self, patch: AiConfigPatch) -> Result<Self, String> {
        let mut next = self.clone();

        if let Some(provider) = patch.provider {
            if provider != next.provider {
                // 换服务商时把「还是上一家默认值」的模型与地址一起换掉；
                // 用户自己改过的值保持不动——他可能存了自建中转站的地址。
                let model_was_default =
                    next.model.trim().is_empty() || next.model == next.provider.default_model();
                let base_was_default = next.base_url.trim().is_empty()
                    || next.base_url == next.provider.default_base_url();
                if model_was_default {
                    next.model = provider.default_model().to_string();
                }
                if base_was_default {
                    next.base_url = provider.default_base_url().to_string();
                }
                next.provider = provider;
            }
        }

        if let Some(model) = patch.model {
            let model = model.trim();
            if model.chars().count() > MAX_MODEL_CHARS {
                return Err(format!("模型名太长了（上限 {MAX_MODEL_CHARS} 个字符）"));
            }
            next.model = model.to_string();
        }

        if let Some(base_url) = patch.base_url {
            // 空串是合法输入：自定义服务商可以先把地址留空，之后再填。
            next.base_url = if base_url.trim().is_empty() {
                String::new()
            } else {
                check_base_url(&base_url)?
            };
        }

        if let Some(mode) = patch.mode {
            next.mode = mode;
        }

        next.validate()?;
        next.revision = self.revision + 1;
        *self = next.clone();
        Ok(next)
    }

    /// 保存时的校验：只保证「填了的东西是合法的」。
    ///
    /// 自定义服务商允许地址为空（用户正在填），真正发请求前由
    /// [`Self::request_target`] 再拦一次。
    pub fn validate(&self) -> Result<(), String> {
        if self.model.chars().count() > MAX_MODEL_CHARS {
            return Err(format!("模型名太长了（上限 {MAX_MODEL_CHARS} 个字符）"));
        }
        if !self.base_url.trim().is_empty() {
            check_base_url(&self.base_url)?;
        }
        if self.base_url.trim().is_empty() && self.provider != Provider::Custom {
            return Err(format!("{} 的 Base URL 不能为空", self.provider.label()));
        }
        Ok(())
    }

    /// 发请求前拿地址与模型名：这里要求它们是完整的。
    pub fn request_target(&self) -> Result<(String, String), String> {
        if self.base_url.trim().is_empty() {
            return Err(format!(
                "{} 的 Base URL 还没填：去设置窗口的 AI 一节补上",
                self.provider.label()
            ));
        }
        if self.model.trim().is_empty() {
            return Err("模型名还没填：去设置窗口的 AI 一节补上".into());
        }
        Ok((
            check_base_url(&self.base_url)?,
            self.model.trim().to_string(),
        ))
    }

    /// 由磁盘上的值构造运行态，并给出需要上报的说明（正常时为 `None`）。
    pub fn from_stored(stored: StoredAiConfig) -> (Self, Option<String>) {
        let provider = stored.provider;
        let mut notes = Vec::new();

        let mut model = stored.model.trim().to_string();
        if model.is_empty() {
            model = provider.default_model().to_string();
        }
        if model.chars().count() > MAX_MODEL_CHARS {
            notes.push(format!(
                "{AI_FILE} 里的 model 太长，已改用 {}",
                provider.label()
            ));
            model = provider.default_model().to_string();
        }

        let mut base_url = stored.base_url.trim().to_string();
        if !base_url.is_empty() {
            match check_base_url(&base_url) {
                Ok(normalized) => base_url = normalized,
                Err(error) => {
                    notes.push(format!(
                        "{AI_FILE} 里的 baseUrl 不能用（{error}），已回退默认值"
                    ));
                    base_url = provider.default_base_url().to_string();
                }
            }
        } else if provider != Provider::Custom {
            base_url = provider.default_base_url().to_string();
        }

        (
            Self {
                revision: 0,
                provider,
                model,
                base_url,
                mode: stored.mode,
            },
            if notes.is_empty() {
                None
            } else {
                Some(notes.join("；"))
            },
        )
    }

    pub fn to_stored(&self) -> StoredAiConfig {
        StoredAiConfig {
            provider: self.provider,
            model: self.model.clone(),
            base_url: self.base_url.clone(),
            mode: self.mode,
        }
    }
}

/// 校验并规范化 Base URL。
///
/// 两件贴心的事：
/// - 用户把完整的 `.../chat/completions` 粘进来时自动截掉——那是最常见的填错方式；
/// - 去掉结尾的 `/`，免得拼出 `//chat/completions`。
pub fn check_base_url(raw: &str) -> Result<String, String> {
    let mut value = raw.trim().trim_end_matches('/').to_string();
    if let Some(stripped) = value.strip_suffix("/chat/completions") {
        value = stripped.trim_end_matches('/').to_string();
    }
    if value.is_empty() {
        return Err("地址是空的".into());
    }
    if value.chars().count() > MAX_BASE_URL_CHARS {
        return Err(format!("地址太长了（上限 {MAX_BASE_URL_CHARS} 个字符）"));
    }
    if !(value.starts_with("http://") || value.starts_with("https://")) {
        return Err(format!(
            "地址要以 http:// 或 https:// 开头，现在是「{value}」"
        ));
    }
    if value.contains(char::is_whitespace) {
        return Err("地址里不能有空格".into());
    }
    Ok(value)
}

/// 应用数据目录下的 `ai.json`。
pub fn store(app: &tauri::AppHandle) -> Result<Store, String> {
    Ok(Store::new(crate::settings::data_dir(app)?))
}

/// 读取配置。纯函数版本，测试里可以指向临时目录。
pub fn load_from(store: &Store) -> (AiConfig, Option<String>) {
    let loaded = store.load::<StoredAiConfig>(AI_FILE);
    let (config, invalid_note) = AiConfig::from_stored(loaded.value);

    let mut notes: Vec<String> = loaded.note.into_iter().collect();
    notes.extend(invalid_note);

    // 首次运行写一份默认值出来，用户才有文件可以看、可以改（与 settings.json 一致）。
    if loaded.source == LoadSource::Missing {
        if let Err(error) = store.save(AI_FILE, &config.to_stored()) {
            notes.push(format!("写入默认 AI 配置失败：{error}"));
        }
    }

    (
        config,
        if notes.is_empty() {
            None
        } else {
            Some(notes.join("；"))
        },
    )
}

pub fn save(app: &tauri::AppHandle, config: &AiConfig) -> Result<(), String> {
    store(app)?.save(AI_FILE, &config.to_stored())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::test_support::TempDir;
    use std::fs;

    #[test]
    fn default_provider_is_deepseek() {
        let config = AiConfig::default();
        assert_eq!(config.provider, Provider::Deepseek);
        assert_eq!(config.model, "deepseek-chat");
        assert_eq!(config.base_url, "https://api.deepseek.com/v1");
        assert_eq!(config.mode, Mode::Chat);
    }

    #[test]
    fn switching_provider_replaces_defaults_it_inherited() {
        let mut config = AiConfig::default();
        let next = config
            .apply(AiConfigPatch {
                provider: Some(Provider::Openai),
                ..Default::default()
            })
            .expect("切换服务商");
        assert_eq!(next.provider, Provider::Openai);
        assert_eq!(next.model, OPENAI_MODEL);
        assert_eq!(next.base_url, OPENAI_BASE_URL);
        assert_eq!(next.revision, 1);
    }

    #[test]
    fn switching_provider_keeps_values_the_user_typed() {
        let mut config = AiConfig::default();
        config
            .apply(AiConfigPatch {
                model: Some("my-local-model".into()),
                base_url: Some("http://127.0.0.1:8000/v1".into()),
                ..Default::default()
            })
            .expect("写自定义值");
        let next = config
            .apply(AiConfigPatch {
                provider: Some(Provider::Custom),
                ..Default::default()
            })
            .expect("切到自定义");
        assert_eq!(next.model, "my-local-model");
        assert_eq!(next.base_url, "http://127.0.0.1:8000/v1");
    }

    #[test]
    fn custom_provider_without_base_url_can_be_saved_but_cannot_be_used() {
        let mut config = AiConfig::default();
        let next = config
            .apply(AiConfigPatch {
                provider: Some(Provider::Custom),
                ..Default::default()
            })
            .expect("切到自定义：地址留空也应能保存");
        assert_eq!(next.base_url, "");
        assert!(next.request_target().is_err(), "发请求前必须拦住");
    }

    #[test]
    fn base_url_is_normalized() {
        assert_eq!(
            check_base_url("  https://api.deepseek.com/v1/  ").unwrap(),
            "https://api.deepseek.com/v1"
        );
        // 常见的粘错：把完整端点填进 Base URL。
        assert_eq!(
            check_base_url("https://api.deepseek.com/v1/chat/completions").unwrap(),
            "https://api.deepseek.com/v1"
        );
        assert!(check_base_url("api.deepseek.com").is_err());
        assert!(check_base_url("ftp://example.com").is_err());
        assert!(check_base_url("https://exa mple.com").is_err());
    }

    #[test]
    fn invalid_patch_is_rejected_without_touching_current_config() {
        let mut config = AiConfig::default();
        let error = config
            .apply(AiConfigPatch {
                base_url: Some("随便写的".into()),
                ..Default::default()
            })
            .expect_err("非法地址应被拒绝");
        assert!(error.contains("http://"), "错误信息要能指导用户：{error}");
        assert_eq!(config.base_url, DEEPSEEK_BASE_URL);
        assert_eq!(config.revision, 0, "失败的修改不该推进版本号");
    }

    #[test]
    fn overly_long_model_name_is_rejected() {
        let mut config = AiConfig::default();
        assert!(config
            .apply(AiConfigPatch {
                model: Some("x".repeat(MAX_MODEL_CHARS + 1)),
                ..Default::default()
            })
            .is_err());
    }

    #[test]
    fn stored_config_round_trips() {
        let dir = TempDir::new("ai-round-trip");
        let store = dir.store();
        let mut config = AiConfig::default();
        config
            .apply(AiConfigPatch {
                model: Some("deepseek-reasoner".into()),
                mode: Some(Mode::Agent),
                ..Default::default()
            })
            .unwrap();
        store.save(AI_FILE, &config.to_stored()).expect("写入");

        let (restored, note) = load_from(&store);
        assert!(note.is_none(), "正常读取不该有说明：{note:?}");
        assert_eq!(restored.model, "deepseek-reasoner");
        assert_eq!(restored.mode, Mode::Agent);
        assert_eq!(restored.revision, 0, "revision 是本次运行内的标记");
    }

    #[test]
    fn first_run_writes_a_file_without_key_fields() {
        let dir = TempDir::new("ai-first-run");
        let store = dir.store();
        let (config, note) = load_from(&store);
        assert_eq!(config, AiConfig::default());
        assert!(note.is_none(), "首次运行不是错误：{note:?}");

        let text = fs::read_to_string(store.path(AI_FILE)).expect("默认文件应被写出");
        let json: serde_json::Value = serde_json::from_str(&text).expect("解析");
        assert_eq!(json["data"]["provider"], "deepseek");
        assert_eq!(json["data"]["model"], "deepseek-chat");
        assert_eq!(json["data"]["mode"], "chat");
        assert!(
            serde_json::to_string(&json).unwrap().find("sk-").is_none(),
            "ai.json 里不该出现任何看起来像 Key 的东西：{text}"
        );
    }

    #[test]
    fn hand_edited_file_with_bad_url_falls_back_and_explains() {
        let dir = TempDir::new("ai-bad-url");
        let store = dir.store();
        fs::write(
            store.path(AI_FILE),
            r#"{"schemaVersion": 1, "data": {"provider": "deepseek", "model": "deepseek-chat", "baseUrl": "这里"}}"#,
        )
        .expect("写入手工编辑的文件");

        let (config, note) = load_from(&store);
        assert_eq!(config.base_url, DEEPSEEK_BASE_URL);
        assert!(note.expect("应上报原因").contains("baseUrl"));
    }

    #[test]
    fn unknown_provider_in_file_falls_back_to_default() {
        let dir = TempDir::new("ai-bad-provider");
        let store = dir.store();
        fs::write(
            store.path(AI_FILE),
            r#"{"schemaVersion": 1, "data": {"provider": "gemini", "model": "deepseek-chat", "baseUrl": ""}}"#,
        )
        .expect("写入手工编辑的文件");

        let (config, note) = load_from(&store);
        // provider 认不出来会先回退成整份默认值，再补上各家默认地址。
        assert_eq!(config.provider, Provider::Deepseek);
        assert_eq!(config.base_url, DEEPSEEK_BASE_URL);
        assert!(note.is_some(), "回退要留痕");
    }
}
