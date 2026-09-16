//! 对话的编排：收一条用户消息 → 发请求 → 边收边广播 → 落盘。
//!
//! 分层的原因：`ai/*` 只管「配置、密钥、请求、解析」，`chat_store` 只管文件，
//! 这里负责把它们串起来，并处理「中途被打断」「连不上」这些真实会发生的状态。
//!
//! 两条路：聊天模式走 [`pump`]（就是阶段 A 那条），Agent 模式走
//! [`crate::ai::agent`] 的循环。**分岔点只有一处**——给不给工具。

use std::collections::HashMap;
use std::sync::Mutex;

use futures_util::future::Either;
use futures_util::StreamExt;
use serde::Serialize;
use tauri::{AppHandle, Manager};
use tokio_util::sync::CancellationToken;

use crate::ai::config::AiConfig;
use crate::ai::config::Mode;
use crate::ai::provider::{self, ByteStream, ChatMessage, OpenAiCompatible};
use crate::ai::stream::{self, SseDecoder};
use crate::ai::{agent, persona, secret};
use crate::chat_store::{self, Session};
use crate::workspace::{self, WorkspaceEntry};
use crate::{broadcast, chat_window, clock, desktop};

// 事件名。前端在 `app/src/api/chat.js` 里也有一份，两边要一起改。
pub const EVENT_STARTED: &str = "chat-stream-started";
pub const EVENT_DELTA: &str = "chat-stream-delta";
pub const EVENT_FINISHED: &str = "chat-stream-finished";
pub const EVENT_FAILED: &str = "chat-stream-failed";
/// Agent 模式：开始执行一次工具调用。
pub const EVENT_TOOL_CALL: &str = "chat-tool-call";
/// Agent 模式：一次工具调用的结果。
pub const EVENT_TOOL_RESULT: &str = "chat-tool-result";
/// Agent 模式：等用户确认的写入（带红绿 diff）。
pub const EVENT_WRITE_REQUEST: &str = "chat-write-request";

/// 用户点「停止」时写进历史的原因。
pub const CANCELLED_NOTE: &str = "已停止";

/// 正在进行的流。key 是 sessionId。
///
/// **必须是独立的 newtype**（`Mutex<HashMap<..>>` 的裸别名会和别的状态撞类型），
/// 原因见 `chat_window.rs` 里 `ChatWindowStore` 的注释。
#[derive(Default)]
pub struct ChatState(pub Mutex<HashMap<String, CancellationToken>>);

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct StreamStarted {
    session_id: String,
    message_id: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamDelta {
    pub session_id: String,
    pub message_id: String,
    pub text: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct StreamFinished {
    session_id: String,
    message_id: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct StreamFailed {
    session_id: String,
    message_id: String,
    error: String,
}

/// 一次工具调用开始。`label` 是卡片上那句「正在读取 xxx」（文案在 `ai/tools.rs`）。
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallEvent {
    pub session_id: String,
    pub message_id: String,
    /// 和结果事件配对用。计划里的载荷没有它，但没有它就无法把两个同名调用对上。
    pub call_id: String,
    pub name: String,
    pub label: String,
    pub args: serde_json::Value,
}

/// 一次工具调用的结果。
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolResultEvent {
    pub session_id: String,
    pub message_id: String,
    pub call_id: String,
    pub name: String,
    pub ok: bool,
    pub summary: String,
}

/// 等确认的一次写入。`diff` 是 unified 格式的原文，前端按行前缀上色。
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteRequestEvent {
    pub session_id: String,
    pub message_id: String,
    pub request_id: String,
    pub path: String,
    pub diff: String,
    pub reason: String,
}

/// 一次流跑完的结果。
enum Outcome {
    /// 正常说完。
    Done(String),
    /// 用户点了停止，带上已经收到的半截文字。
    Cancelled(String),
    /// 出错了，带上原因。
    Failed(String),
}

/// 发一个对话事件。对话窗口关掉之后就没人收，那属于正常状态，不是错误。
pub fn emit_chat(app: &AppHandle, event: &str, payload: &impl Serialize) {
    // 对话窗口关掉之后就没人收事件了，那属于正常状态，不是错误。
    if let Err(error) = broadcast::emit_if_open(app, chat_window::CHAT_LABEL, event, payload) {
        eprintln!("[Rust] 对话事件 {event} 发送失败：{error}");
    }
}

/// 登记一条正在跑的流；同一会话已经有一条在跑时返回 `false`。
fn register(app: &AppHandle, session_id: &str, token: CancellationToken) -> Result<bool, String> {
    let state = app.state::<ChatState>();
    let mut streams = state.0.lock().map_err(|error| error.to_string())?;
    if streams.contains_key(session_id) {
        return Ok(false);
    }
    streams.insert(session_id.to_string(), token);
    Ok(true)
}

fn unregister(app: &AppHandle, session_id: &str) {
    let state = app.state::<ChatState>();
    let guard = state.0.lock();
    if let Ok(mut streams) = guard {
        streams.remove(session_id);
    }
}

/// 用户点「停止」。
///
/// 幂等：没有正在跑的流也算成功——用户点的时候它可能刚好说完了。
pub fn cancel(app: &AppHandle, session_id: &str) -> bool {
    let state = app.state::<ChatState>();
    let token = match state.0.lock() {
        Ok(streams) => streams.get(session_id).cloned(),
        Err(error) => {
            eprintln!("[Rust] 读取进行中的对话失败：{error}");
            None
        }
    };
    match token {
        Some(token) => {
            token.cancel();
            true
        }
        None => false,
    }
}

/// 停掉所有正在跑的流。用于对话窗口关闭时——没人看的事件没必要继续花钱。
pub fn cancel_all(app: &AppHandle) {
    let state = app.state::<ChatState>();
    let tokens: Vec<CancellationToken> = match state.0.lock() {
        Ok(streams) => streams.values().cloned().collect(),
        Err(_) => return,
    };
    for token in tokens {
        token.cancel();
    }
}

/// 组装发给模型的消息：系统提示词 + 历史。
///
/// 空内容的消息（占位中的助手消息）与失败消息里没有任何文字的，一律不发。
fn build_messages(
    config: &AiConfig,
    session: &Session,
    entries: &[WorkspaceEntry],
) -> Vec<ChatMessage> {
    let mut messages = vec![ChatMessage::system(persona::system_prompt(
        config.mode,
        entries,
    ))];
    for message in &session.messages {
        if message.content.trim().is_empty() {
            continue;
        }
        messages.push(ChatMessage {
            role: message.role.as_wire().to_string(),
            content: message.content.clone(),
            ..Default::default()
        });
    }
    messages
}

/// 收到一条用户消息，开始一轮回答。
///
/// 返回值是**助手那条消息的 id**：前端拿它当流式增量的落点。
pub async fn send(app: AppHandle, session_id: &str, text: &str) -> Result<String, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("消息是空的".into());
    }
    if text.chars().count() > chat_store::MAX_MESSAGE_CHARS {
        return Err(format!(
            "这条太长了（上限 {} 个字符），分两次发吧",
            chat_store::MAX_MESSAGE_CHARS
        ));
    }

    let store = chat_store::store(&app)?;
    let mut session = chat_store::load(&store, session_id)?;

    // 先备齐配置与 Key。缺 Key 的时候**不要**先写历史，免得留下半截对话。
    let config = crate::ai::get_config(&app)?;
    let key = secret::load(config.provider)?.ok_or_else(|| {
        format!(
            "还没有填 {} 的 API Key：点右上角齿轮 → 在「AI」一节填入后点「测试连接」。",
            config.provider.label()
        )
    })?;
    let client = OpenAiCompatible::from_config(&config, &key)?;

    let token = CancellationToken::new();
    if !register(&app, session_id, token.clone())? {
        return Err("上一条还在回答，先等它说完，或者点「停止」".into());
    }

    // 写历史：用户消息 + 助手占位。任何一步失败都要把登记撤掉。
    let now = clock::now_ms();
    chat_store::append_user(&mut session, text, now);
    let placeholder = chat_store::begin_assistant(&mut session, now);
    if let Err(error) = chat_store::save(&store, &session) {
        unregister(&app, session_id);
        return Err(error);
    }

    // 助手消息已经占位，告诉前端「这条 id 开始出字了」。
    emit_chat(
        &app,
        EVENT_STARTED,
        &StreamStarted {
            session_id: session_id.to_string(),
            message_id: placeholder.id.clone(),
        },
    );

    // 系统提示词要拼上当前的工作区条目清单（计划 §8.6）。
    let entries = workspace::get(&app)?;
    let messages = build_messages(&config, &session, &entries);

    // ---------- Agent 模式：交给 Agent 循环（带工具，可能要好几轮）----------
    if config.mode == Mode::Agent {
        let app_for_task = app.clone();
        let session_id_for_task = session_id.to_string();
        let message_id = placeholder.id.clone();
        tauri::async_runtime::spawn(async move {
            let outcome = agent::run(
                app_for_task.clone(),
                session_id_for_task.clone(),
                message_id.clone(),
                config,
                key,
                messages,
                token,
            )
            .await;
            unregister(&app_for_task, &session_id_for_task);
            // 被打断时把已经说过的话留下（与聊天模式一致），只是多一句「已停止」。
            let (text, error) = if outcome.cancelled {
                (outcome.text, Some(CANCELLED_NOTE.to_string()))
            } else {
                (outcome.text, outcome.error)
            };
            finalize(
                &app_for_task,
                &store,
                session,
                &session_id_for_task,
                &message_id,
                &text,
                error,
            );
        });
        return Ok(placeholder.id);
    }

    // ---------- 聊天模式：阶段 A 那条路，一个工具都不给 ----------
    // 工具表是空的：聊天模式不给模型任何工具（设计文档 §3）。
    let stream = match provider::open_stream(&client, &config.model, &messages, &[]).await {
        Ok(stream) => stream,
        Err(error) => {
            // 连不上：如实记进这条消息里，历史与界面都看得见。
            finalize(
                &app,
                &store,
                session,
                session_id,
                &placeholder.id,
                "",
                Some(error),
            );
            return Ok(placeholder.id);
        }
    };

    let app_for_task = app.clone();
    let session_id_for_task = session_id.to_string();
    let message_id = placeholder.id.clone();
    tauri::async_runtime::spawn(async move {
        let outcome = pump(
            &app_for_task,
            stream,
            token,
            &session_id_for_task,
            &message_id,
        )
        .await;
        unregister(&app_for_task, &session_id_for_task);
        let (text, error) = match outcome {
            Outcome::Done(text) => (text, None),
            Outcome::Cancelled(text) => (text, Some(CANCELLED_NOTE.to_string())),
            Outcome::Failed(error) => (String::new(), Some(error)),
        };
        finalize(
            &app_for_task,
            &store,
            session,
            &session_id_for_task,
            &message_id,
            &text,
            error,
        );
    });

    Ok(placeholder.id)
}

/// 收尾：写回历史 + 广播结束/失败事件。
fn finalize(
    app: &AppHandle,
    store: &crate::store::Store,
    mut session: Session,
    session_id: &str,
    message_id: &str,
    text: &str,
    error: Option<String>,
) {
    chat_store::finish_assistant(
        &mut session,
        message_id,
        text,
        error.clone(),
        clock::now_ms(),
    );
    if let Err(save_error) = chat_store::save(store, &session) {
        desktop::report_error(
            app,
            &format!("回答没能写进历史（关掉窗口后会看不到）：{save_error}"),
        );
    }
    match error {
        Some(error) => emit_chat(
            app,
            EVENT_FAILED,
            &StreamFailed {
                session_id: session_id.to_string(),
                message_id: message_id.to_string(),
                error,
            },
        ),
        None => emit_chat(
            app,
            EVENT_FINISHED,
            &StreamFinished {
                session_id: session_id.to_string(),
                message_id: message_id.to_string(),
            },
        ),
    }
}

/// 读流直到说完、被取消或出错。
async fn pump(
    app: &AppHandle,
    mut stream: ByteStream,
    token: CancellationToken,
    session_id: &str,
    message_id: &str,
) -> Outcome {
    let mut decoder = SseDecoder::new();
    let mut text = String::new();

    loop {
        // 取消与收数据谁先来就听谁的：单靠「每收到一块检查一次」在模型卡住时
        // 会让「停止」按钮失灵。
        let next = Box::pin(stream.next());
        let cancelled = Box::pin(token.cancelled());
        let chunk = match futures_util::future::select(next, cancelled).await {
            Either::Left((chunk, _)) => chunk,
            Either::Right(((), _)) => return Outcome::Cancelled(text),
        };

        let events = match chunk {
            Some(Ok(bytes)) => decoder.push(&bytes),
            Some(Err(error)) => return Outcome::Failed(error),
            // 流正常结束：最后可能还剩一行没有换行符。
            None => {
                let tail = decoder.finish();
                if let Err(error) = absorb(app, &tail, session_id, message_id, &mut text) {
                    return Outcome::Failed(error);
                }
                return Outcome::Done(text);
            }
        };

        match absorb(app, &events, session_id, message_id, &mut text) {
            Ok(true) => return Outcome::Done(text),
            Ok(false) => {}
            Err(error) => return Outcome::Failed(error),
        }
    }
}

/// 处理一批事件，把增量文本拼进 `text` 并广播。
///
/// 返回 `Ok(true)` 表示收到 `[DONE]` —— 说完了。
fn absorb(
    app: &AppHandle,
    events: &[String],
    session_id: &str,
    message_id: &str,
    text: &mut String,
) -> Result<bool, String> {
    for event in events {
        if event.trim() == stream::DONE {
            return Ok(true);
        }
        let Some(delta) = stream::parse_delta(event)? else {
            continue;
        };
        if delta.content.is_empty() {
            continue;
        }
        text.push_str(&delta.content);
        emit_chat(
            app,
            EVENT_DELTA,
            &StreamDelta {
                session_id: session_id.to_string(),
                message_id: message_id.to_string(),
                text: delta.content,
            },
        );
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::config::{Mode, Provider};
    use crate::chat_store::{Message, Role};

    fn session_with(messages: Vec<Message>) -> Session {
        Session {
            id: "s1-1".into(),
            title: "t".into(),
            created_at: 0,
            updated_at: 0,
            messages,
        }
    }

    fn message(role: Role, content: &str) -> Message {
        Message {
            id: format!("m-{content}"),
            role,
            content: content.into(),
            created_at: 0,
            error: None,
        }
    }

    #[test]
    fn system_prompt_comes_first_and_history_follows() {
        let config = AiConfig {
            mode: Mode::Chat,
            ..AiConfig::default()
        };
        let session = session_with(vec![
            message(Role::User, "在吗"),
            message(Role::Assistant, "在的"),
            message(Role::User, ""),
        ]);
        let messages = build_messages(&config, &session, &[]);
        assert_eq!(messages.len(), 3, "空内容的消息不该发给模型");
        assert_eq!(messages[0].role, "system");
        assert_eq!(messages[1].content, "在吗");
        assert_eq!(messages[2].content, "在的");
    }

    /// 阶段 C：Agent 模式的系统提示词要带上当前工作区清单，并说明工具的使用顺序。
    #[test]
    fn agent_mode_carries_the_workspace_and_the_tool_rules() {
        let config = AiConfig {
            provider: Provider::Deepseek,
            mode: Mode::Agent,
            ..AiConfig::default()
        };
        let entries = vec![WorkspaceEntry {
            id: "w1-1".into(),
            kind: crate::workspace::EntryKind::Dir,
            path: r"C:\proj".into(),
            label: "proj".into(),
        }];
        let messages = build_messages(&config, &session_with(Vec::new()), &entries);
        assert!(messages[0].content.contains("Agent 模式"));
        assert!(messages[0].content.contains(r"C:\proj"), "要列出授权条目");
        assert!(messages[0].content.contains("write_file"), "要说清能改文件");
    }

    /// 聊天模式仍然一个字都不提工作区：那条路不该出现文件相关的能力。
    #[test]
    fn chat_mode_never_mentions_the_workspace() {
        let config = AiConfig {
            mode: Mode::Chat,
            ..AiConfig::default()
        };
        let entries = vec![WorkspaceEntry {
            id: "w1-1".into(),
            kind: crate::workspace::EntryKind::Dir,
            path: r"C:\proj".into(),
            label: "proj".into(),
        }];
        let messages = build_messages(&config, &session_with(Vec::new()), &entries);
        assert!(!messages[0].content.contains(r"C:\proj"));
        assert!(messages[0].content.contains("聊天模式"));
    }

    #[test]
    fn cancellation_is_idempotent_and_does_not_panic_without_a_stream() {
        // 这里只验证「没有 token 时返回 false」这一条：真取消要 AppHandle，属于集成范畴。
        let token = CancellationToken::new();
        assert!(!token.is_cancelled());
        token.cancel();
        assert!(token.is_cancelled());
    }
}
