//! AI 对话的 Rust 侧。
//!
//! 分层（每一层只干一件事，便于单测）：
//!
//! | 模块 | 职责 |
//! |---|---|
//! | [`config`] | `ai.json` 的读写与校验（服务商 / 模型 / Base URL / 模式） |
//! | [`secret`] | API Key 的存取（Windows 凭据管理器），只进内存，不进任何 json |
//! | [`provider`] | 怎么发请求：`ChatProvider` trait + OpenAI 兼容实现 |
//! | [`stream`] | 怎么把 SSE 分片拼回文本与工具调用（纯函数，可单测） |
//! | [`tools`] | Agent 模式的工具表与执行（含输出预算） |
//! | [`agent`] | Agent 循环：带工具的一轮，最多 8 轮 |
//! | [`writes`] | 写入确认：diff → 等用户点「应用」→ 才写 |
//! | [`persona`] | 系统提示词，待人设稿定稿后替换 |
//!
//! 「读配置 → 发请求 → 落盘 → 广播事件」的编排在 [`crate::chat`] 里。

pub mod agent;
pub mod config;
pub mod persona;
pub mod provider;
pub mod secret;
pub mod stream;
pub mod tools;
pub mod writes;

use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

use crate::broadcast;
use config::{AiConfig, AiConfigPatch, AiConfigView};

/// AI 配置的运行态。**必须是独立的 newtype**，不能写成 `type AiState = Mutex<AiConfig>;`：
/// Rust 的 type 别名是透明的，两个别名会变成同一个类型，Tauri 的 `manage()`
/// 第二次注册就在启动时 panic（M1 交付时踩过一次，详见 `chat_window.rs`）。
#[derive(Default)]
pub struct AiState(pub Mutex<AiConfig>);

/// 读取运行态配置。
pub fn get_config(app: &AppHandle) -> Result<AiConfig, String> {
    let state = app.state::<AiState>();
    let config = state.0.lock().map_err(|error| error.to_string())?;
    Ok(config.clone())
}

/// 改配置：校验 → 落盘 → 写回内存 → 广播。顺序与 `desktop::update_settings` 一致。
///
/// 落盘失败不推翻本次修改（设置已经生效，重启后会丢），但要如实上报。
pub fn update_config(app: &AppHandle, patch: AiConfigPatch) -> Result<AiConfig, String> {
    let snapshot = {
        let state = app.state::<AiState>();
        let mut current = state.0.lock().map_err(|error| error.to_string())?;
        let snapshot = current.apply(patch)?;
        if let Err(error) = config::save(app, &snapshot) {
            crate::desktop::report_error(
                app,
                &format!("AI 设置已生效，但写入磁盘失败（重启后会丢失）：{error}"),
            );
        }
        snapshot
    };

    broadcast_config(app, &snapshot);
    #[cfg(debug_assertions)]
    println!("[Rust] AI 配置：{snapshot:?}");
    Ok(snapshot)
}

/// 把配置快照（含掩码，不含 Key 本身）广播给所有打开的窗口。
///
/// 掩码是现读的：用户可能在设置窗口保存了 Key，也可能刚点了「删除」。
pub fn broadcast_config(app: &AppHandle, config: &AiConfig) {
    match view(config) {
        Ok(view) => {
            for failure in broadcast::emit_all(app, "ai-config-changed", &view) {
                eprintln!("[Rust] AI 配置通知失败：{failure}");
            }
        }
        Err(error) => eprintln!("[Rust] 生成 AI 配置快照失败：{error}"),
    }
}

/// 给前端的快照：配置 + `hasKey` + 掩码。**Key 本身永远不出现在这里。**
pub fn view(config: &AiConfig) -> Result<AiConfigView, String> {
    let key = secret::load(config.provider)?;
    Ok(AiConfigView {
        revision: config.revision,
        provider: config.provider,
        model: config.model.clone(),
        base_url: config.base_url.clone(),
        mode: config.mode,
        has_key: key.is_some(),
        key_mask: key.as_deref().map(secret::mask).unwrap_or_default(),
    })
}

/// 启动时把磁盘上的 AI 配置恢复进内存。
///
/// 与 `desktop::restore_settings` 一样：读失败不 panic，回退默认值并上报原因。
pub fn restore(app: &AppHandle) {
    let (restored, note) = match config::store(app) {
        Ok(store) => config::load_from(&store),
        Err(error) => (AiConfig::default(), Some(error)),
    };
    if let Some(note) = note {
        crate::desktop::report_startup_error(app, &format!("读取 AI 配置时发生回退：{note}"));
    }
    match app.state::<AiState>().0.lock() {
        Ok(mut current) => *current = restored.clone(),
        Err(error) => {
            crate::desktop::report_startup_error(app, &format!("恢复 AI 配置失败：{error}"))
        }
    }
    #[cfg(debug_assertions)]
    println!("[Rust] 启动时恢复 AI 配置：{restored:?}");
}

/// 「测试连接」的结果。
///
/// 失败也走 `Ok`：它是一次正常的探测，不是程序错误——用户要看到的是原因，
/// 而不是一个红框异常。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestOutcome {
    pub ok: bool,
    pub message: String,
}

impl TestOutcome {
    fn failed(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            message: message.into(),
        }
    }
}

/// 真的发一次最小请求，确认地址、Key、模型三者都对得上。
///
/// 只检查 Key 的格式是不够的——用户点这个按钮就是想确认「到底通不通」。
pub async fn test_connection(app: &AppHandle) -> TestOutcome {
    let config = match get_config(app) {
        Ok(config) => config,
        Err(error) => return TestOutcome::failed(error),
    };
    let key = match secret::load(config.provider) {
        Ok(Some(key)) => key,
        Ok(None) => {
            return TestOutcome::failed(format!("还没有填 {} 的 API Key", config.provider.label()))
        }
        Err(error) => return TestOutcome::failed(error),
    };
    let model = match config.request_target() {
        Ok((_, model)) => model.to_string(),
        Err(error) => return TestOutcome::failed(error),
    };
    let client = match provider::OpenAiCompatible::from_config(&config, &key) {
        Ok(client) => client,
        Err(error) => return TestOutcome::failed(error),
    };

    match provider::ping(&client, &model).await {
        Ok(reply) => TestOutcome {
            ok: true,
            message: format!("{}：{reply}", config.provider.label()),
        },
        Err(error) => TestOutcome::failed(error),
    }
}
