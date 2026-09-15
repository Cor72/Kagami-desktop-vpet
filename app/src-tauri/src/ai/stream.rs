//! SSE 增量解析。
//!
//! DeepSeek / OpenAI 的 `/chat/completions` 在 `stream: true` 时按行返回：
//!
//! ```text
//! data: {"choices":[{"delta":{"content":"你"}}]}
//!
//! data: {"choices":[{"delta":{"content":"好"}}]}
//!
//! data: [DONE]
//! ```
//!
//! 三个最容易写错的点，这里都处理了：
//!
//! 1. **分片**：一次网络读取可能只拿到半行，甚至只拿到一个 UTF-8 汉字的前两个字节。
//!    所以缓冲区按**字节**存，遇到 `\n` 才当成一整行解析——先转成 `String` 再拼会
//!    在分片处产生乱码。
//! 2. **事件由空行分隔**：一个事件可以有多条 `data:` 行，要用 `\n` 连接。
//! 3. **`[DONE]`**：是流结束标记，不是 JSON，不能丢给 `serde_json`。

/// 流结束标记。收到它就说明模型说完了。
pub const DONE: &str = "[DONE]";

/// 增量式的 SSE 解码器。喂字节进去，吐出一个个事件载荷。
#[derive(Default)]
pub struct SseDecoder {
    /// 还没凑成一整行的字节。
    pending: Vec<u8>,
    /// 当前事件攒下的 `data:` 行。
    data_lines: Vec<String>,
}

impl SseDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// 喂入一段字节，返回这次凑齐的事件载荷（`data:` 后面的内容）。
    pub fn push(&mut self, chunk: &[u8]) -> Vec<String> {
        self.pending.extend_from_slice(chunk);
        let mut events = Vec::new();
        while let Some(index) = self.pending.iter().position(|byte| *byte == b'\n') {
            let mut line: Vec<u8> = self.pending.drain(..=index).collect();
            line.pop(); // 去掉结尾的 '\n'
            let line = String::from_utf8_lossy(&line)
                .trim_end_matches('\r')
                .to_string();
            self.handle_line(&line, &mut events);
        }
        events
    }

    /// 流结束时调用：最后一行可能没有以换行符结尾。
    pub fn finish(&mut self) -> Vec<String> {
        let mut events = Vec::new();
        if !self.pending.is_empty() {
            let pending = std::mem::take(&mut self.pending);
            let line = String::from_utf8_lossy(&pending)
                .trim_end_matches('\r')
                .to_string();
            self.handle_line(&line, &mut events);
        }
        self.flush_event(&mut events);
        events
    }

    fn handle_line(&mut self, line: &str, events: &mut Vec<String>) {
        if line.is_empty() {
            // 空行 = 一个事件结束。
            self.flush_event(events);
            return;
        }
        if let Some(rest) = line.strip_prefix("data:") {
            // SSE 规定字段名后的第一个空格是分隔符，不属于内容。
            self.data_lines
                .push(rest.strip_prefix(' ').unwrap_or(rest).to_string());
        }
        // `event:` / `id:` / `retry:` 与以 ':' 开头的注释（心跳）本版用不到，忽略。
    }

    fn flush_event(&mut self, events: &mut Vec<String>) {
        if self.data_lines.is_empty() {
            return;
        }
        events.push(self.data_lines.join("\n"));
        self.data_lines.clear();
    }
}

/// 一条 delta 里我们关心的东西。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Delta {
    /// 本次增量文本。可能是空串（比如只带了 `finish_reason`）。
    pub content: String,
    /// 模型说完了的原因（`stop` / `length` …）。
    pub finish_reason: Option<String>,
    /// 本次增量里的工具调用分片（阶段 C）。聊天模式下永远是空的。
    pub tool_calls: Vec<ToolCallDelta>,
}

/// 工具调用的一次分片。
///
/// 线上格式把它切碎了发：第一片带 `id` 与 `function.name`，后面几片只带
/// `function.arguments` 的一段字符串。**按 `index` 拼接**，不能按到达顺序拼——
/// 同一个回答里可以同时有多个工具调用。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ToolCallDelta {
    pub index: u64,
    /// 只有第一片有。
    pub id: Option<String>,
    /// 只有第一片有。
    pub name: Option<String>,
    /// 参数 JSON 的一段。
    pub arguments: String,
}

/// 解析一条 `data:` 载荷。`[DONE]` 要由调用方先判断并跳过。
///
/// 返回 `Ok(None)` 表示这条帧里没有可用的 delta（有些兼容服务会先发一条只含 `usage` 的帧）。
pub fn parse_delta(payload: &str) -> Result<Option<Delta>, String> {
    let value: serde_json::Value =
        serde_json::from_str(payload).map_err(|error| format!("模型返回的数据看不懂：{error}"))?;

    // 服务端在流里插的错误对象：当成失败处理，不要静默忽略。
    if let Some(error) = value.get("error") {
        if !error.is_null() {
            return Err(error_message(error));
        }
    }

    let Some(choice) = value.get("choices").and_then(|choices| choices.get(0)) else {
        return Ok(None);
    };

    let content = choice
        .get("delta")
        .and_then(|delta| delta.get("content"))
        .and_then(|content| content.as_str())
        .unwrap_or_default()
        .to_string();
    let finish_reason = choice
        .get("finish_reason")
        .and_then(|reason| reason.as_str())
        .map(|reason| reason.to_string());

    let tool_calls = choice
        .get("delta")
        .and_then(|delta| delta.get("tool_calls"))
        .and_then(|calls| calls.as_array())
        .map(|calls| calls.iter().filter_map(parse_tool_call).collect())
        .unwrap_or_default();

    Ok(Some(Delta {
        content,
        finish_reason,
        tool_calls,
    }))
}

/// 解析一片工具调用。整条都不是对象时丢掉而不是报错：这一帧可能是别的东西。
fn parse_tool_call(value: &serde_json::Value) -> Option<ToolCallDelta> {
    if !value.is_object() {
        return None;
    }
    let function = value.get("function");
    Some(ToolCallDelta {
        index: value
            .get("index")
            .and_then(|index| index.as_u64())
            .unwrap_or(0),
        id: value
            .get("id")
            .and_then(|id| id.as_str())
            .filter(|id| !id.is_empty())
            .map(|id| id.to_string()),
        name: function
            .and_then(|function| function.get("name"))
            .and_then(|name| name.as_str())
            .filter(|name| !name.is_empty())
            .map(|name| name.to_string()),
        arguments: function
            .and_then(|function| function.get("arguments"))
            .and_then(|arguments| arguments.as_str())
            .unwrap_or_default()
            .to_string(),
    })
}

/// 从 `{"error": {...}}` 里取出给人看的原因。
pub fn error_message(error: &serde_json::Value) -> String {
    if let Some(message) = error.get("message").and_then(|message| message.as_str()) {
        return message.to_string();
    }
    if let Some(message) = error.as_str() {
        return message.to_string();
    }
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn content_of(events: &[String]) -> String {
        let mut text = String::new();
        for event in events {
            if event == DONE {
                continue;
            }
            if let Some(delta) = parse_delta(event).expect("解析 delta") {
                text.push_str(&delta.content);
            }
        }
        text
    }

    #[test]
    fn parses_two_events_and_the_done_marker() {
        let mut decoder = SseDecoder::new();
        let events = decoder.push(
            "data: {\"choices\":[{\"delta\":{\"content\":\"你\"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"好\"}}]}\n\ndata: [DONE]\n\n".as_bytes(),
        );
        assert_eq!(events.len(), 3);
        assert_eq!(events[2], DONE);
        assert_eq!(content_of(&events), "你好");
    }

    #[test]
    fn an_event_split_across_chunks_is_reassembled() {
        let mut decoder = SseDecoder::new();
        // 网络分片完全随机：这里把一条事件切成三段，最后一段还带上下一条的开头。
        assert!(decoder
            .push(b"data: {\"choices\":[{\"delta\":{\"con")
            .is_empty());
        assert!(
            decoder.push(b"tent\":\"\xe4\xbd").is_empty(),
            "半个汉字也不能当成错误"
        );
        // 剩下的那半个汉字字节（\xa0）加上后面一句完整的“好”。
        let events = decoder.push(b"\xa0\xe5\xa5\xbd\"}}]}\n\ndata: [DONE]\n\n");
        assert_eq!(events.len(), 2);
        assert_eq!(content_of(&events), "你好");
        assert_eq!(events[1], DONE);
    }

    #[test]
    fn crlf_and_absence_of_trailing_newline_are_handled() {
        let mut decoder = SseDecoder::new();
        let events = decoder.push(b"data: {\"choices\":[{\"delta\":{\"content\":\"a\"}}]}\r\n\r\n");
        assert_eq!(content_of(&events), "a");
        // 流在最后一行没有换行符时结束。
        decoder.push(b"data: {\"choices\":[{\"delta\":{\"content\":\"b\"}}]}");
        assert_eq!(content_of(&decoder.finish()), "b");
        assert!(decoder.finish().is_empty(), "finish 之后再调不应重复吐数据");
    }

    #[test]
    fn comments_and_other_fields_are_ignored() {
        let mut decoder = SseDecoder::new();
        let events = decoder.push(
            b": keep-alive\n\nevent: message\nid: 1\ndata: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\n",
        );
        assert_eq!(events.len(), 1);
        assert_eq!(content_of(&events), "hi");
    }

    #[test]
    fn multi_line_data_is_joined_with_newline() {
        let mut decoder = SseDecoder::new();
        let events =
            decoder.push(b"data: {\"choices\":[{\"delta\":{\"content\":\ndata: \"x\"}}]}\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(content_of(&events), "x");
    }

    #[test]
    fn frames_without_choices_are_skipped_not_failed() {
        let mut decoder = SseDecoder::new();
        let events = decoder.push(b"data: {\"usage\":{\"total_tokens\":3}}\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(parse_delta(&events[0]).unwrap(), None);
    }

    #[test]
    fn finish_reason_is_carried_through() {
        let delta = parse_delta("{\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}")
            .unwrap()
            .unwrap();
        assert_eq!(delta.content, "");
        assert_eq!(delta.finish_reason.as_deref(), Some("stop"));
    }

    #[test]
    fn null_content_is_an_empty_delta_not_a_crash() {
        let delta = parse_delta("{\"choices\":[{\"delta\":{\"content\":null}}]}")
            .unwrap()
            .unwrap();
        assert_eq!(delta.content, "");
    }

    #[test]
    fn error_frames_become_readable_errors() {
        let error = parse_delta("{\"error\":{\"message\":\"Insufficient Balance\",\"code\":402}}")
            .expect_err("错误帧应当失败");
        assert_eq!(error, "Insufficient Balance");
    }

    #[test]
    fn tool_call_fragments_are_parsed_with_their_index() {
        // 第一片：带 id 与函数名，参数只有一半。
        let first = parse_delta(
            "{\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"type\":\"function\",\"function\":{\"name\":\"read_file\",\"arguments\":\"{\\\"pa\"}}]}}]}",
        )
        .unwrap()
        .unwrap();
        assert_eq!(first.content, "");
        assert_eq!(first.tool_calls.len(), 1);
        assert_eq!(first.tool_calls[0].index, 0);
        assert_eq!(first.tool_calls[0].id.as_deref(), Some("call_1"));
        assert_eq!(first.tool_calls[0].name.as_deref(), Some("read_file"));
        assert_eq!(first.tool_calls[0].arguments, "{\"pa");

        // 后续片：只有参数字符串。
        let second = parse_delta(
            "{\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"th\\\":\\\"a.rs\\\"}\"}}]}}]}",
        )
        .unwrap()
        .unwrap();
        assert!(second.tool_calls[0].id.is_none());
        assert_eq!(second.tool_calls[0].arguments, "th\":\"a.rs\"}");
    }

    #[test]
    fn a_delta_can_carry_several_tool_calls_and_still_parses_content() {
        let delta = parse_delta(
            "{\"choices\":[{\"delta\":{\"content\":\"我看看\",\"tool_calls\":[{\"index\":0,\"id\":\"a\"},{\"index\":1,\"id\":\"b\"}]},\"finish_reason\":\"tool_calls\"}]}",
        )
        .unwrap()
        .unwrap();
        assert_eq!(delta.content, "我看看");
        assert_eq!(delta.finish_reason.as_deref(), Some("tool_calls"));
        assert_eq!(delta.tool_calls.len(), 2);
        assert_eq!(delta.tool_calls[1].index, 1);
    }

    #[test]
    fn frames_without_tool_calls_leave_the_field_empty() {
        let delta = parse_delta("{\"choices\":[{\"delta\":{\"content\":\"好\"}}]}")
            .unwrap()
            .unwrap();
        assert!(delta.tool_calls.is_empty());
    }
}
