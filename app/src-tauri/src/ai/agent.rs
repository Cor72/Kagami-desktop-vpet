//! Agent 循环：带工具的一轮对话，最多 8 轮。
//!
//! 与聊天模式的唯一区别是**给不给工具**。循环本身很朴素：
//!
//! 1. 带工具表发一次请求，把文字增量照常广播给界面；
//! 2. 流结束时如果这一轮有工具调用，就执行它们，把结果作为 `tool` 消息追加进历史；
//! 3. 回到第 1 步，直到模型不再要工具（正常说完），或者轮数用完。
//!
//! 取消（用户点「停止」）在每一层都会立刻生效：收流时听令牌、执行工具时听令牌、
//! 执行下一个工具之前再听一次。任何一步被取消，磁盘上都不会留下半截改动——
//! `write_file` 只在用户点过「应用」之后才写。

use futures_util::StreamExt;
use serde_json::Value;
use tauri::AppHandle;
use tokio_util::sync::CancellationToken;

use super::config::AiConfig;
use super::provider::{
    self, ByteStream, ChatMessage, FunctionCall, OpenAiCompatible, ToolCallWire,
};
use super::stream::{self, SseDecoder, ToolCallDelta};
use super::tools::{self, Budget, ToolCall};
use crate::chat;

/// 一轮对话里最多让模型调用几次工具。
///
/// 8 轮足够「列目录 → 搜 → 读 → 改」这样的流程；再多基本是模型在绕圈子，
/// 而每一轮都是一次真实的网络请求。
pub const MAX_ROUNDS: usize = 8;

/// 一次 Agent 回答的结果。
pub struct AgentOutcome {
    /// 已经广播出去的正文（被打断时也有半截）。
    pub text: String,
    /// 出错原因（正常说完是 `None`）。
    pub error: Option<String>,
    /// 是不是被用户点了「停止」。
    pub cancelled: bool,
}

impl AgentOutcome {
    fn done(text: String) -> Self {
        Self {
            text,
            error: None,
            cancelled: false,
        }
    }

    fn failed(text: String, error: String) -> Self {
        Self {
            text,
            error: Some(error),
            cancelled: false,
        }
    }

    fn cancelled(text: String) -> Self {
        Self {
            text,
            error: None,
            cancelled: true,
        }
    }
}

/// 一次流读完的结果。
enum Round {
    /// 说完了。可能带着「要调用的工具」，也可能没有。
    Done(Vec<ToolCall>),
    Cancelled,
    Failed(String),
}

/// 工具调用分片的累积器（按 `index` 拼）。
#[derive(Default)]
struct Pending {
    id: String,
    name: String,
    arguments: String,
}

fn merge_calls(slots: &mut Vec<Pending>, deltas: &[ToolCallDelta]) {
    for delta in deltas {
        let index = delta.index as usize;
        while slots.len() <= index {
            slots.push(Pending::default());
        }
        let slot = &mut slots[index];
        if let Some(id) = &delta.id {
            slot.id = id.clone();
        }
        if let Some(name) = &delta.name {
            slot.name = name.clone();
        }
        slot.arguments.push_str(&delta.arguments);
    }
}

/// 累积结果 → 可执行的调用。没拼出函数名的（半截分片）直接丢掉。
fn finish_calls(slots: Vec<Pending>) -> Vec<ToolCall> {
    slots
        .into_iter()
        .enumerate()
        .filter(|(_, slot)| !slot.name.trim().is_empty())
        .map(|(index, slot)| ToolCall {
            id: if slot.id.is_empty() {
                // 有的兼容服务在第一片里不给 id，补一个稳定的编号——
                // `tool_call_id` 必须能和助手消息里的那条对上。
                format!("call_{index}")
            } else {
                slot.id
            },
            name: slot.name,
            arguments: slot.arguments,
        })
        .collect()
}

/// 跑完一次 Agent 回答。
pub async fn run(
    app: AppHandle,
    session_id: String,
    message_id: String,
    config: AiConfig,
    key: String,
    mut messages: Vec<ChatMessage>,
    token: CancellationToken,
) -> AgentOutcome {
    let client = match OpenAiCompatible::from_config(&config, &key) {
        Ok(client) => client,
        Err(error) => return AgentOutcome::failed(String::new(), error),
    };
    let specs = tools::specs();
    let mut text = String::new();

    for round in 0..MAX_ROUNDS {
        let stream = match provider::open_stream(&client, &config.model, &messages, &specs).await {
            Ok(stream) => stream,
            Err(error) => {
                // 第一轮就连不上属于「这次回答没成」；中间轮失败时前面说过的话还在，
                // 两种都按带原因的失败收尾，由上层如实记进历史。
                return AgentOutcome::failed(text, error);
            }
        };

        let calls = match pump(&app, stream, &token, &session_id, &message_id, &mut text).await {
            Round::Done(calls) => calls,
            Round::Cancelled => return AgentOutcome::cancelled(text),
            Round::Failed(error) => return AgentOutcome::failed(text, error),
        };

        if calls.is_empty() {
            return AgentOutcome::done(text);
        }

        // 把「助手要求调用工具」这一步也记进消息历史，否则下一轮的
        // tool 结果会是一串没有出处的孤儿消息，模型看不懂。
        messages.push(ChatMessage::assistant_tool_calls(
            "",
            calls
                .iter()
                .map(|call| ToolCallWire {
                    id: call.id.clone(),
                    kind: "function".into(),
                    function: FunctionCall {
                        name: call.name.clone(),
                        arguments: call.arguments.clone(),
                    },
                })
                .collect(),
        ));

        // 预算按轮重置：每一轮的工具输出各自最多吃掉 25%。
        let mut budget = Budget::new();
        for call in &calls {
            if token.is_cancelled() {
                return AgentOutcome::cancelled(text);
            }
            chat::emit_chat(
                &app,
                chat::EVENT_TOOL_CALL,
                &chat::ToolCallEvent {
                    session_id: session_id.clone(),
                    message_id: message_id.clone(),
                    call_id: call.id.clone(),
                    name: call.name.clone(),
                    label: tools::label(call),
                    args: serde_json::from_str(&call.arguments)
                        .unwrap_or_else(|_| Value::String(call.arguments.clone())),
                },
            );

            let result =
                tools::execute(&app, &session_id, &message_id, call, &mut budget, &token).await;

            chat::emit_chat(
                &app,
                chat::EVENT_TOOL_RESULT,
                &chat::ToolResultEvent {
                    session_id: session_id.clone(),
                    message_id: message_id.clone(),
                    call_id: call.id.clone(),
                    name: call.name.clone(),
                    ok: result.ok,
                    summary: result.summary.clone(),
                },
            );
            messages.push(ChatMessage::tool_result(
                call.id.clone(),
                result.output.clone(),
            ));

            // 取消发生在工具执行期间（比如用户在看 diff 时点了停止）。
            if token.is_cancelled() {
                return AgentOutcome::cancelled(text);
            }
        }

        #[cfg(debug_assertions)]
        println!(
            "[Rust] Agent 第 {} 轮：执行了 {} 个工具，继续问模型",
            round + 1,
            calls.len()
        );
    }

    // 轮数用完：把话说明白，而不是假装正常结束。
    if text.trim().is_empty() {
        text.push_str("想做的事有点多，这一轮先到这儿——要我接着做吗？");
    }
    AgentOutcome::done(text)
}

/// 收一轮的流：文字增量照常广播，工具调用分片攒起来。
async fn pump(
    app: &AppHandle,
    mut stream: ByteStream,
    token: &CancellationToken,
    session_id: &str,
    message_id: &str,
    text: &mut String,
) -> Round {
    let mut decoder = SseDecoder::new();
    let mut slots: Vec<Pending> = Vec::new();

    loop {
        // 取消与收数据谁先来就听谁的（与聊天模式同一个理由：模型卡住时
        // 「停止」也要立刻生效）。
        let next = Box::pin(stream.next());
        let cancelled = Box::pin(token.cancelled());
        let chunk = match futures_util::future::select(next, cancelled).await {
            futures_util::future::Either::Left((chunk, _)) => chunk,
            futures_util::future::Either::Right(((), _)) => return Round::Cancelled,
        };

        let events = match chunk {
            Some(Ok(bytes)) => decoder.push(&bytes),
            Some(Err(error)) => return Round::Failed(error),
            None => {
                let tail = decoder.finish();
                match absorb(app, &tail, session_id, message_id, text, &mut slots) {
                    Ok(true) => return Round::Done(finish_calls(slots)),
                    Ok(false) => return Round::Done(finish_calls(slots)),
                    Err(error) => return Round::Failed(error),
                }
            }
        };

        match absorb(app, &events, session_id, message_id, text, &mut slots) {
            Ok(true) => return Round::Done(finish_calls(slots)),
            Ok(false) => {}
            Err(error) => return Round::Failed(error),
        }
    }
}

/// 处理一批 SSE 事件：文本广播出去，工具调用分片攒起来。
///
/// 返回 `Ok(true)` 表示收到 `[DONE]`。
fn absorb(
    app: &AppHandle,
    events: &[String],
    session_id: &str,
    message_id: &str,
    text: &mut String,
    slots: &mut Vec<Pending>,
) -> Result<bool, String> {
    for event in events {
        if event.trim() == stream::DONE {
            return Ok(true);
        }
        let Some(delta) = stream::parse_delta(event)? else {
            continue;
        };
        merge_calls(slots, &delta.tool_calls);
        if delta.content.is_empty() {
            continue;
        }
        text.push_str(&delta.content);
        chat::emit_chat(
            app,
            chat::EVENT_DELTA,
            &chat::StreamDelta {
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
    use crate::ai::stream::ToolCallDelta;

    fn delta(index: u64, id: Option<&str>, name: Option<&str>, arguments: &str) -> ToolCallDelta {
        ToolCallDelta {
            index,
            id: id.map(str::to_string),
            name: name.map(str::to_string),
            arguments: arguments.to_string(),
        }
    }

    #[test]
    fn fragments_are_joined_in_index_order() {
        let mut slots = Vec::new();
        merge_calls(
            &mut slots,
            &[delta(0, Some("call_a"), Some("read_file"), "{\"pa")],
        );
        merge_calls(&mut slots, &[delta(0, None, None, "th\":\"a.rs\"}")]);
        let calls = finish_calls(slots);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "read_file");
        assert_eq!(calls[0].id, "call_a");
        assert_eq!(calls[0].arguments, "{\"path\":\"a.rs\"}");
    }

    #[test]
    fn two_tool_calls_in_one_round_do_not_get_mixed_up() {
        let mut slots = Vec::new();
        merge_calls(
            &mut slots,
            &[
                delta(0, Some("a"), Some("list_files"), "{}"),
                delta(1, Some("b"), Some("read_file"), "{\"path\":\"b"),
            ],
        );
        merge_calls(&mut slots, &[delta(1, None, None, ".rs\"}")]);
        let calls = finish_calls(slots);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].name, "list_files");
        assert_eq!(calls[1].arguments, "{\"path\":\"b.rs\"}");
    }

    #[test]
    fn calls_without_a_name_are_dropped() {
        let mut slots = Vec::new();
        merge_calls(&mut slots, &[delta(0, Some("a"), None, "{}")]);
        assert!(finish_calls(slots).is_empty());
    }

    #[test]
    fn a_missing_id_gets_a_stable_placeholder() {
        let mut slots = Vec::new();
        merge_calls(&mut slots, &[delta(2, None, Some("grep"), "{}")]);
        let calls = finish_calls(slots);
        assert_eq!(
            calls[0].id, "call_2",
            "id 要和下标对上，tool 消息才接得回来"
        );
    }
}
