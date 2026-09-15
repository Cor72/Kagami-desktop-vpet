//! 服务商适配：怎么发请求。
//!
//! 三家（DeepSeek / OpenAI / 自定义）都是 OpenAI 兼容格式，所以只有一份实现
//! [`OpenAiCompatible`]，差别只在 Base URL、模型名与 Key。要做成 trait 是为了
//! 以后接别家（或测试替身）时不必改上层编排。
//!
//! 请求体与地址拼接都是纯函数，能在没有网络的情况下断言（见文件末尾的测试）。

use std::pin::Pin;
use std::time::Duration;

use futures_util::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::config::AiConfig;

/// 建连超时：连不上要快点告诉用户，别让人对着转圈等。
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// 读取超时：**只要还在出字就不算超时**，30 秒以上没有新字节才判定卡死。
/// 不用总超时的原因是一次回答可能很长，总超时会把正常的回答掐断。
pub const READ_TIMEOUT: Duration = Duration::from_secs(45);
/// 错误信息里附带的响应正文上限。
const ERROR_BODY_CHARS: usize = 240;

/// 发给模型的一条消息（OpenAI 兼容线上格式）。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".into(),
            content: content.into(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: content.into(),
        }
    }
}

/// 一家的接口形状。要加别家时实现这个 trait 即可，编排层不用动。
pub trait ChatProvider: Send + Sync {
    /// 完整请求地址。
    fn endpoint(&self) -> String;
    /// `Authorization` 头的值。
    fn authorization(&self) -> String;
    /// 请求体。抽出来是为了能单测格式。
    fn body(&self, model: &str, messages: &[ChatMessage]) -> Value;
}

/// OpenAI 兼容实现（DeepSeek 与 OpenAI 官方接口都是这个形状）。
pub struct OpenAiCompatible {
    base_url: String,
    api_key: String,
}

impl OpenAiCompatible {
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            api_key: api_key.into(),
        }
    }

    /// 从运行态配置构造。地址/模型没填全时给出可操作的中文错误。
    pub fn from_config(config: &AiConfig, api_key: &str) -> Result<Self, String> {
        let (base_url, _) = config.request_target()?;
        Ok(Self::new(base_url, api_key))
    }
}

impl ChatProvider for OpenAiCompatible {
    fn endpoint(&self) -> String {
        chat_completions_url(&self.base_url)
    }

    fn authorization(&self) -> String {
        // 只在这里拼一次：Key 不进日志、不进错误信息。
        format!("Bearer {}", self.api_key)
    }

    fn body(&self, model: &str, messages: &[ChatMessage]) -> Value {
        json!({
            "model": model,
            "messages": messages,
            "stream": true,
        })
    }
}

/// 拼接 `/chat/completions` 地址，容忍结尾多写的 `/`。
pub fn chat_completions_url(base_url: &str) -> String {
    format!("{}/chat/completions", base_url.trim().trim_end_matches('/'))
}

/// 流式响应的字节流。每个元素是一小段 SSE 原文，交给 [`super::stream::SseDecoder`]。
pub type ByteStream = Pin<Box<dyn Stream<Item = Result<Vec<u8>, String>> + Send>>;

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .read_timeout(READ_TIMEOUT)
        .build()
        .map_err(|error| format!("创建 HTTP 客户端失败：{error}"))
}

/// 发起流式请求。**这里的失败是「连不上/被拒绝」**，还没开始出字；
/// 出字之后的失败由调用方按流中断处理。
pub async fn open_stream(
    provider: &dyn ChatProvider,
    model: &str,
    messages: &[ChatMessage],
) -> Result<ByteStream, String> {
    let response = client()?
        .post(provider.endpoint())
        .header(reqwest::header::AUTHORIZATION, provider.authorization())
        .header(reqwest::header::ACCEPT, "text/event-stream")
        .json(&provider.body(model, messages))
        .send()
        .await
        .map_err(describe_transport_error)?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(describe_http_error(status.as_u16(), &body));
    }

    Ok(Box::pin(response.bytes_stream().map(|item| {
        item.map(|bytes| bytes.to_vec())
            .map_err(|error| format!("读取模型返回的数据时中断：{error}"))
    })))
}

/// 「测试连接」用的最小请求：非流式、只要 8 个 token。
///
/// 用真的请求而不是只检查 Key 格式：用户点这个按钮就是想知道「到底通不通」。
pub async fn ping(provider: &dyn ChatProvider, model: &str) -> Result<String, String> {
    let mut body = provider.body(model, &[ChatMessage::user("你好")]);
    body["stream"] = json!(false);
    body["max_tokens"] = json!(8);

    let response = client()?
        .post(provider.endpoint())
        .header(reqwest::header::AUTHORIZATION, provider.authorization())
        .json(&body)
        .send()
        .await
        .map_err(describe_transport_error)?;

    let status = response.status();
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        return Err(describe_http_error(status.as_u16(), &text));
    }

    let value: Value = response
        .json()
        .await
        .map_err(|error| format!("服务端返回的不是预期的 JSON：{error}"))?;
    let reply = value
        .get("choices")
        .and_then(|choices| choices.get(0))
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(|content| content.as_str())
        .unwrap_or_default()
        .trim()
        .to_string();
    Ok(if reply.is_empty() {
        "连接成功（模型没有返回文字）".into()
    } else {
        format!("连接成功：{reply}")
    })
}

/// 网络层错误 → 中文说明。带上原始原因，方便用户自己判断是不是代理的问题。
pub fn describe_transport_error(error: reqwest::Error) -> String {
    if error.is_timeout() {
        format!("请求超时：{error}")
    } else if error.is_connect() {
        format!("连不上服务器（{error}）。检查网络，或看看有没有开代理")
    } else {
        format!("请求失败：{error}")
    }
}

/// HTTP 状态码 → 用户能看懂的原因。附上服务端正文的片段（截断过）。
pub fn describe_http_error(status: u16, body: &str) -> String {
    let reason = match status {
        401 => "API Key 不对或已失效",
        402 => "账户余额不足",
        403 => "这个 Key 没有访问该模型的权限",
        404 => "地址或模型名不对（404）",
        429 => "请求太频繁，等一会儿再试",
        500..=599 => "服务端出错了，稍后再试",
        _ => "请求被拒绝",
    };
    let detail = excerpt(body, ERROR_BODY_CHARS);
    if detail.is_empty() {
        format!("{reason}（HTTP {status}）")
    } else {
        format!("{reason}（HTTP {status}）：{detail}")
    }
}

/// 截断长正文：错误信息要短到能读，又不能把整页 HTML 灌进界面。
fn excerpt(body: &str, limit: usize) -> String {
    let mut text = body.trim().replace('\n', " ");
    if text.chars().count() > limit {
        text = text.chars().take(limit).collect::<String>() + "…";
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::config::AiConfig;

    fn provider() -> OpenAiCompatible {
        OpenAiCompatible::new("https://api.deepseek.com/v1", "sk-test-key")
    }

    #[test]
    fn endpoint_ignores_trailing_slash() {
        assert_eq!(
            chat_completions_url("https://api.deepseek.com/v1"),
            "https://api.deepseek.com/v1/chat/completions"
        );
        assert_eq!(
            chat_completions_url("https://api.deepseek.com/v1/"),
            "https://api.deepseek.com/v1/chat/completions"
        );
    }

    #[test]
    fn body_is_openai_compatible_and_streaming() {
        let messages = vec![ChatMessage::system("你是八千代"), ChatMessage::user("在吗")];
        let body = provider().body("deepseek-chat", &messages);
        assert_eq!(body["model"], "deepseek-chat");
        assert_eq!(body["stream"], true);
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][1]["content"], "在吗");
    }

    #[test]
    fn authorization_uses_bearer_scheme() {
        assert_eq!(provider().authorization(), "Bearer sk-test-key");
    }

    #[test]
    fn provider_from_incomplete_config_fails_before_sending() {
        let config = AiConfig {
            provider: crate::ai::config::Provider::Custom,
            model: "".into(),
            base_url: "".into(),
            ..AiConfig::default()
        };
        let error = OpenAiCompatible::from_config(&config, "sk-test-key")
            .err()
            .unwrap();
        assert!(error.contains("Base URL"), "错误要说清缺什么：{error}");
    }

    #[test]
    fn http_errors_map_to_actionable_chinese() {
        assert!(describe_http_error(401, "").contains("API Key"));
        assert!(describe_http_error(402, "").contains("余额"));
        assert!(describe_http_error(429, "").contains("频繁"));
        assert!(describe_http_error(503, "").contains("服务端"));
        let with_body = describe_http_error(400, "{\"error\":{\"message\":\"bad model\"}}");
        assert!(
            with_body.contains("bad model"),
            "要带上服务端的说明：{with_body}"
        );
    }

    #[test]
    fn long_error_bodies_are_truncated() {
        let long = "x".repeat(1000);
        let text = describe_http_error(500, &long);
        assert!(
            text.chars().count() < 300,
            "错误信息不该太长：{}",
            text.len()
        );
        assert!(text.ends_with('…'));
    }

    // ---------- 走真实 HTTP 的链路 ----------
    //
    // 上面测的是纯函数。这一组起一个本地「假模型服务」，把「发请求 → 读分片 →
    // 拼 delta」整条链路跑一遍——不需要联网，也不需要真的 API Key。
    // 最容易写错的 SSE 解析（分片、[DONE]、错误响应）都在这里被钉住。

    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;
    use std::time::Duration;

    /// 起一个只吐固定 SSE 分片的假服务，返回它的 Base URL。
    fn spawn_sse_server(chunks: Vec<&'static str>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("绑定本地端口");
        let address = listener.local_addr().expect("读取端口");
        thread::spawn(move || {
            let Ok((mut socket, _)) = listener.accept() else {
                return;
            };
            // 读掉请求头。内容不重要——这里只关心响应怎么被解析。
            let mut buffer = [0u8; 4096];
            let _ = socket.read(&mut buffer);
            let _ = socket.write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n",
            );
            for chunk in chunks {
                let frame = format!("{:x}\r\n{chunk}\r\n", chunk.len());
                if socket.write_all(frame.as_bytes()).is_err() {
                    return;
                }
                let _ = socket.flush();
                // 一小段间隔：让每个分片真的分多次到达，而不是被合成一次读取。
                thread::sleep(Duration::from_millis(10));
            }
            let _ = socket.write_all(b"0\r\n\r\n");
            let _ = socket.flush();
        });
        format!("http://{address}/v1")
    }

    /// 起一个固定返回某段 HTTP 响应的假服务。
    fn spawn_http_server(response: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("绑定本地端口");
        let address = listener.local_addr().expect("读取端口");
        thread::spawn(move || {
            let Ok((mut socket, _)) = listener.accept() else {
                return;
            };
            let mut buffer = [0u8; 4096];
            let _ = socket.read(&mut buffer);
            let _ = socket.write_all(response.as_bytes());
            let _ = socket.flush();
        });
        format!("http://{address}/v1")
    }

    fn collect(stream: ByteStream) -> String {
        use crate::ai::stream::{parse_delta, SseDecoder, DONE};
        tauri::async_runtime::block_on(async move {
            let mut stream = stream;
            let mut decoder = SseDecoder::new();
            let mut text = String::new();
            let absorb = |events: Vec<String>, text: &mut String| -> bool {
                for event in events {
                    if event.trim() == DONE {
                        return true;
                    }
                    if let Some(delta) = parse_delta(&event).expect("解析 delta") {
                        text.push_str(&delta.content);
                    }
                }
                false
            };
            while let Some(chunk) = stream.next().await {
                let bytes = chunk.expect("读取分片");
                if absorb(decoder.push(&bytes), &mut text) {
                    return text;
                }
            }
            let tail = decoder.finish();
            absorb(tail, &mut text);
            text
        })
    }

    #[test]
    fn streams_text_from_a_real_sse_endpoint() {
        let base_url = spawn_sse_server(vec![
            // 故意把一条事件切成两半，第二段还横跨到下一条事件。
            "data: {\"choices\":[{\"delta\":{\"con",
            "tent\":\"你\"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"好",
            "呀\"}}]}\n\ndata: [DONE]\n\n",
        ]);
        let provider = OpenAiCompatible::new(base_url, "sk-test-key");
        let stream = tauri::async_runtime::block_on(open_stream(
            &provider,
            "test-model",
            &[ChatMessage::user("在吗")],
        ))
        .expect("打开流");
        assert_eq!(collect(stream), "你好呀");
    }

    #[test]
    fn http_401_from_the_server_becomes_a_readable_error() {
        let body = "{\"error\":{\"message\":\"Authentication Fails\"}}";
        let response = format!(
            "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        let base_url = spawn_http_server(Box::leak(response.into_boxed_str()));
        let provider = OpenAiCompatible::new(base_url, "sk-wrong-key");
        let error = tauri::async_runtime::block_on(open_stream(
            &provider,
            "test-model",
            &[ChatMessage::user("在吗")],
        ))
        .err()
        .expect("401 应当失败");
        assert!(error.contains("API Key"), "{error}");
        assert!(error.contains("Authentication Fails"), "{error}");
    }

    #[test]
    fn ping_reports_success_with_the_model_reply() {
        let body = "{\"choices\":[{\"message\":{\"content\":\"你好\"}}]}";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        let base_url = spawn_http_server(Box::leak(response.into_boxed_str()));
        let provider = OpenAiCompatible::new(base_url, "sk-test-key");
        let message =
            tauri::async_runtime::block_on(ping(&provider, "test-model")).expect("探活成功");
        assert!(message.contains("连接成功"), "{message}");
        assert!(message.contains("你好"), "{message}");
    }
}
