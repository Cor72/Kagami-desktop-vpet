//! Agent 模式的工具集：注册表（JSON schema）+ 执行。
//!
//! 一次给全 6 个（实施计划 §7）。聊天模式一个都不给——**模式只决定给模型哪些
//! 工具，能碰哪些文件是另一层，由工作区授权决定**，开关无权放开（设计文档 §3）。
//!
//! 三条硬约束：
//!
//! 1. 每个工具每次调用都重新校验路径（在 `fs_tools` / `workspace` 里）；
//! 2. 单轮所有工具返回值合计不超过 [`ROUND_OUTPUT_BUDGET_CHARS`]；
//! 3. `write_file` 只生成 diff，等用户点「应用」才写（`ai::writes`）。
//!
//! 卡片上的文案按《八千代角色设定》写：她是八千年的主播，说话短、不用系统播报腔。
//! 所以这里写「正在读取 pomodoro.rs」，而不是「正在执行 read_file 工具」。

use futures_util::future::{select, Either};
use serde_json::{json, Value};
use tauri::AppHandle;
use tokio_util::sync::CancellationToken;

use super::writes::{self, Decision};
use crate::expression::validate_expression;
use crate::workspace::{self, Access, WorkspaceEntry};
use crate::{broadcast, chat, fs_tools};

/// 单轮所有工具返回值合计的长度上限（实施计划 §5.2）。
///
/// 25% 的上下文预算，按「中英混排大约一个字符半个到一个 token」折算成字符数。
/// 这是个**估算值**：真正兜底的是每个工具各自的上限（200 条 / 50 命中 / 64 KB），
/// 这一层只是保证「一次搜出十个大文件」也不会把对话历史挤掉。
pub const ROUND_OUTPUT_BUDGET_CHARS: usize = 24_000;

/// 超出预算时追加的说明。
const TRUNCATED_NOTE: &str = "\n…（工具输出总量到上限了，剩下的没给。）";

/// 一次工具调用。`arguments` 是模型给的 JSON 原文。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

/// 一次工具执行的结果。
#[derive(Clone, Debug)]
pub struct ToolResult {
    pub ok: bool,
    /// 还给模型的正文（会进入上下文，所以要走预算）。
    pub output: String,
    /// 卡片上给用户看的短句（她的口吻）。
    pub summary: String,
}

impl ToolResult {
    fn ok(output: impl Into<String>, summary: impl Into<String>) -> Self {
        Self {
            ok: true,
            output: output.into(),
            summary: summary.into(),
        }
    }

    fn failed(output: impl Into<String>, summary: impl Into<String>) -> Self {
        Self {
            ok: false,
            output: output.into(),
            summary: summary.into(),
        }
    }
}

/// 单轮的输出预算。
pub struct Budget {
    remaining: usize,
}

impl Default for Budget {
    fn default() -> Self {
        Self::new()
    }
}

impl Budget {
    pub fn new() -> Self {
        Self {
            remaining: ROUND_OUTPUT_BUDGET_CHARS,
        }
    }

    /// 扣一次预算，超了就截断并说明。每个工具各自的上限之外，这一层兜底。
    pub fn spend(&mut self, text: String) -> String {
        if text.chars().count() <= self.remaining {
            self.remaining -= text.chars().count();
            return text;
        }
        let kept: String = text.chars().take(self.remaining).collect();
        self.remaining = 0;
        format!("{kept}{TRUNCATED_NOTE}")
    }
}

/// 工具表。`tools` 参数为空数组时，请求体里不会出现 `tools` 字段。
pub fn specs() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "function": {
                "name": "get_workspace_info",
                "description": "列出用户已授权的条目。这份清单本来就在系统提示词里，只有当你怀疑它变了才用。",
                "parameters": { "type": "object", "properties": {} }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "list_files",
                "description": "看工作区的目录结构。深度最多 3 层、最多 200 条，依赖目录（node_modules、.git、target 等）会被跳过。看代码之前先用它，不要一上来就读整份文件。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "要看的目录。不填 = 所有已授权条目的根。" }
                    }
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "grep",
                "description": "在工作区里按**大小写不敏感的子串**搜文本（不是正则），返回文件名、行号与该行内容。找函数名、字段名、报错信息时优先用它。最多 50 处。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "pattern": { "type": "string", "description": "要找的文字" },
                        "path": { "type": "string", "description": "限定在哪个目录里找。不填 = 全部已授权条目。" }
                    },
                    "required": ["pattern"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "read_file",
                "description": "读一个文件的正文，返回值带它的最后修改时间。单个文件最多 64 KB，超出只给前 64 KB；二进制文件读不了。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "文件路径" }
                    },
                    "required": ["path"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "set_expression",
                "description": "换桌宠的表情。只在情绪真的对上时用，不要每句话都换。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "name": {
                            "type": "string",
                            "enum": ["smile", "squint", "tears", "teardrop"],
                            "description": "smile=微笑，squint=眯眼，tears=泪眼，teardrop=泪滴"
                        }
                    },
                    "required": ["name"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "write_file",
                "description": "改一个文件（整份覆盖）。**只有目录条目里的文件能改，拖进来的只读文件条目一律拒绝。** 调用后用户会看到红绿 diff，点了「应用」才会真的写；被拒绝时不要重试，先问用户想怎么改。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "要改的文件路径" },
                        "content": { "type": "string", "description": "改完之后文件的完整内容" },
                        "reason": { "type": "string", "description": "一句话说明为什么这么改，会显示在确认卡片上" }
                    },
                    "required": ["path", "content"]
                }
            }
        }),
    ]
}

/// 把参数解析成 JSON 对象。空参数按 `{}` 处理。
fn args_of(call: &ToolCall) -> Result<Value, String> {
    let raw = call.arguments.trim();
    if raw.is_empty() {
        return Ok(json!({}));
    }
    let value: Value = serde_json::from_str(raw)
        .map_err(|error| format!("参数不是合法的 JSON（{error}）：{raw}"))?;
    if !value.is_object() {
        return Err(format!("参数应当是一个对象，收到的是：{raw}"));
    }
    Ok(value)
}

fn required_str(args: &Value, key: &str) -> Result<String, String> {
    match args.get(key).and_then(|value| value.as_str()) {
        Some(value) if !value.trim().is_empty() => Ok(value.to_string()),
        _ => Err(format!("缺少参数 {key}")),
    }
}

fn optional_str(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
}

/// 路径的最后一段，用在卡片文案里。
fn short_name(path: &str) -> String {
    let trimmed = path.trim_end_matches(['\\', '/']);
    trimmed
        .rsplit(['\\', '/'])
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(trimmed)
        .to_string()
}

/// 卡片上「正在做什么」的那句话。人设要求：短、像人在说话。
pub fn label(call: &ToolCall) -> String {
    let args = serde_json::from_str::<Value>(&call.arguments).unwrap_or_else(|_| json!({}));
    match call.name.as_str() {
        "get_workspace_info" => "看看手头有什么".into(),
        "list_files" => match optional_str(&args, "path") {
            Some(path) => format!("看看 {} 里有什么", short_name(&path)),
            None => "看看工作区里有什么".into(),
        },
        "grep" => {
            let pattern = optional_str(&args, "pattern").unwrap_or_default();
            match optional_str(&args, "path") {
                Some(path) => format!("在 {} 里找「{pattern}」", short_name(&path)),
                None => format!("找找「{pattern}」"),
            }
        }
        "read_file" => match optional_str(&args, "path") {
            Some(path) => format!("正在读取 {}", short_name(&path)),
            None => "正在读一个文件".into(),
        },
        "set_expression" => match optional_str(&args, "name").as_deref() {
            Some("smile") => "换个微笑".into(),
            Some("squint") => "眯一下眼".into(),
            Some("tears") => "眼睛有点湿".into(),
            Some("teardrop") => "挂一滴眼泪".into(),
            _ => "换个表情".into(),
        },
        "write_file" => match optional_str(&args, "path") {
            Some(path) => format!("想把 {} 改成这样", short_name(&path)),
            None => "想改一个文件".into(),
        },
        other => format!("用了 {other}"),
    }
}

/// 执行一次工具调用。
///
/// 调用方负责发 `chat-tool-call`（用 [`label`]）与 `chat-tool-result`（用返回的
/// `summary`）；`write_file` 的确认卡片由这里发。
pub async fn execute(
    app: &AppHandle,
    session_id: &str,
    message_id: &str,
    call: &ToolCall,
    budget: &mut Budget,
    token: &CancellationToken,
) -> ToolResult {
    let args = match args_of(call) {
        Ok(args) => args,
        Err(error) => return ToolResult::failed(error, "没看懂这次调用"),
    };
    let entries = match workspace::get(app) {
        Ok(entries) => entries,
        Err(error) => return ToolResult::failed(format!("读工作区失败：{error}"), "没读成"),
    };

    let result = match call.name.as_str() {
        "get_workspace_info" => ToolResult::ok(
            format!("已授权条目：\n{}", workspace::prompt_listing(&entries)),
            "看过了",
        ),
        "list_files" => {
            match fs_tools::list_files(&entries, optional_str(&args, "path").as_deref()) {
                Ok(text) => ToolResult::ok(text, "看过目录了"),
                Err(error) => ToolResult::failed(error, "没看成"),
            }
        }
        "grep" => match required_str(&args, "pattern") {
            Err(error) => ToolResult::failed(error, "没搜成"),
            Ok(pattern) => {
                match fs_tools::grep(&entries, &pattern, optional_str(&args, "path").as_deref()) {
                    Ok(text) => ToolResult::ok(text, format!("搜过「{pattern}」了")),
                    Err(error) => ToolResult::failed(error, "没搜成"),
                }
            }
        },
        "read_file" => match required_str(&args, "path") {
            Err(error) => ToolResult::failed(error, "没读成"),
            Ok(path) => match fs_tools::read_file(&entries, &path) {
                Ok(text) => ToolResult::ok(text, format!("读过 {} 了", short_name(&path))),
                Err(error) => ToolResult::failed(error, format!("没读到 {}", short_name(&path))),
            },
        },
        "set_expression" => set_expression(app, &args),
        "write_file" => write_file(app, session_id, message_id, &entries, &args, token).await,
        other => ToolResult::failed(format!("没有叫 {other} 的工具"), "用错了工具"),
    };

    // 预算只作用于**还给模型的那部分**；卡片上的短句不受影响。
    ToolResult {
        output: budget.spend(result.output),
        ..result
    }
}

/// 换表情：复用现有的白名单校验与广播路径（与 `commands::request_expression` 同一套）。
fn set_expression(app: &AppHandle, args: &Value) -> ToolResult {
    let name = match required_str(args, "name") {
        Ok(name) => name,
        Err(error) => return ToolResult::failed(error, "没换成"),
    };
    if let Err(error) = validate_expression(&name) {
        return ToolResult::failed(
            format!("{error}。能用的只有 smile / squint / tears / teardrop。"),
            "没换成",
        );
    }
    let payload = crate::commands::ExpressionRequested { name: name.clone() };
    // 主窗口必须收到：真的换表情的是它。
    if let Err(error) = broadcast::emit(app, broadcast::MAIN, "pet-expression-requested", &payload)
    {
        return ToolResult::failed(format!("表情没能送出去：{error}"), "没换成");
    }
    for failure in
        broadcast::emit_all_except(app, broadcast::MAIN, "pet-expression-observed", &payload)
    {
        eprintln!("[Rust] 表情反馈通知失败：{failure}");
    }
    ToolResult::ok("表情换好了。", "换好表情了")
}

/// 写文件：**先生成 diff 等用户确认，再写**。
async fn write_file(
    app: &AppHandle,
    session_id: &str,
    message_id: &str,
    entries: &[WorkspaceEntry],
    args: &Value,
    token: &CancellationToken,
) -> ToolResult {
    let path = match required_str(args, "path") {
        Ok(path) => path,
        Err(error) => return ToolResult::failed(error, "没写成"),
    };
    let content = match args.get("content").and_then(|value| value.as_str()) {
        Some(content) => content.to_string(),
        None => return ToolResult::failed("缺少参数 content", "没写成"),
    };
    let reason = optional_str(args, "reason").unwrap_or_default();

    // 先校验：文件条目的写入要在这里就被拒掉，连卡片都不该弹（免得用户以为可以点）。
    let resolved = match workspace::resolve(entries, &path, Access::Write) {
        Ok(resolved) => resolved,
        Err(error) => return ToolResult::failed(error, "没写成"),
    };

    let before = match std::fs::read(&resolved) {
        Ok(bytes) if bytes.contains(&0) => {
            return ToolResult::failed(
                format!("{} 是二进制文件，没法这样改。", resolved.display()),
                "没写成",
            )
        }
        Ok(bytes) => String::from_utf8_lossy(&bytes).to_string(),
        // 文件还不存在 = 新建，旧内容按空串算。
        Err(_) => String::new(),
    };

    let diff = writes::unified_diff(&before, &content);
    if !writes::has_changes(&diff) {
        return ToolResult::ok(
            "文件已经是这个内容了，没有改动，也没有打扰用户。",
            "没有要改的",
        );
    }

    let (request_id, receiver) = match writes::register(app, &path, content) {
        Ok(pair) => pair,
        Err(error) => return ToolResult::failed(format!("登记这次修改失败：{error}"), "没写成"),
    };
    chat::emit_chat(
        app,
        chat::EVENT_WRITE_REQUEST,
        &chat::WriteRequestEvent {
            session_id: session_id.to_string(),
            message_id: message_id.to_string(),
            request_id: request_id.clone(),
            path: resolved.to_string_lossy().to_string(),
            diff,
            reason,
        },
    );

    // 等用户按按钮，同时留一只耳朵听「停止」。
    let mut receiver = receiver;
    let decision = match select(Box::pin(receiver.recv()), Box::pin(token.cancelled())).await {
        Either::Left((Some(decision), _)) => Some(decision),
        // 发送端没了：这次确认作废了。
        Either::Left((None, _)) => None,
        Either::Right(((), _)) => None,
    };

    match decision {
        Some(Decision::Applied(Ok(message))) => {
            ToolResult::ok(format!("{message}。用户已经确认过这次改动。"), "改好了")
        }
        Some(Decision::Applied(Err(error))) => {
            ToolResult::failed(format!("没能写进去：{error}"), "没写成")
        }
        Some(Decision::Rejected) => ToolResult::ok(
            "用户拒绝了这次修改，文件**没有**任何变化。不要再重试同一个改动，先问用户希望怎么改。",
            "没改，保持原样",
        ),
        None => {
            // 用户点了停止，或者程序要退出了：这一步作废，磁盘上什么都没有发生。
            writes::forget(app, &request_id);
            ToolResult::failed("这一步被取消了，文件没有变化。", "已取消")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(name: &str, arguments: &str) -> ToolCall {
        ToolCall {
            id: "call_1".into(),
            name: name.into(),
            arguments: arguments.into(),
        }
    }

    #[test]
    fn every_tool_has_a_schema_with_a_name_and_a_description() {
        let specs = specs();
        let names: Vec<String> = specs
            .iter()
            .map(|spec| spec["function"]["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(
            names,
            vec![
                "get_workspace_info",
                "list_files",
                "grep",
                "read_file",
                "set_expression",
                "write_file",
            ]
        );
        for spec in &specs {
            assert_eq!(spec["type"], "function");
            let description = spec["function"]["description"].as_str().unwrap();
            assert!(description.chars().count() > 10, "描述要能指导模型：{spec}");
            assert!(spec["function"]["parameters"]["type"] == "object");
        }
    }

    /// 写入工具的 schema 里必须写明「只读文件条目会被拒」，否则模型会反复撞墙。
    #[test]
    fn the_write_tool_documents_its_boundary() {
        let spec = specs()
            .into_iter()
            .find(|spec| spec["function"]["name"] == "write_file")
            .expect("有 write_file");
        let description = spec["function"]["description"].as_str().unwrap();
        assert!(description.contains("只读"), "{description}");
        assert!(description.contains("应用"), "{description}");
    }

    #[test]
    fn labels_sound_like_her_not_like_a_log_line() {
        assert_eq!(
            label(&call("read_file", r#"{"path":"C:\\proj\\pomodoro.rs"}"#)),
            "正在读取 pomodoro.rs"
        );
        assert_eq!(label(&call("list_files", "{}")), "看看工作区里有什么");
        assert_eq!(
            label(&call("grep", r#"{"pattern":"松饼"}"#)),
            "找找「松饼」"
        );
        assert_eq!(
            label(&call("set_expression", r#"{"name":"smile"}"#)),
            "换个微笑"
        );
        for text in [
            label(&call("read_file", r#"{"path":"a.rs"}"#)),
            label(&call("write_file", r#"{"path":"a.rs"}"#)),
        ] {
            assert!(!text.contains("工具"), "不要系统播报腔：{text}");
            assert!(!text.contains("检测"), "{text}");
        }
    }

    #[test]
    fn labels_never_panic_on_broken_arguments() {
        assert_eq!(label(&call("read_file", "不是 JSON")), "正在读一个文件");
        assert_eq!(label(&call("grep", "{}")), "找找「」");
        assert_eq!(label(&call("不存在", "{}")), "用了 不存在");
    }

    #[test]
    fn arguments_must_be_an_object() {
        assert!(args_of(&call("list_files", "")).is_ok());
        assert!(args_of(&call("list_files", "  ")).is_ok());
        assert!(args_of(&call("list_files", "[1,2]")).is_err());
        assert!(args_of(&call("list_files", "{坏")).is_err());
    }

    #[test]
    fn required_arguments_are_checked() {
        let args = json!({"pattern": "  "});
        assert!(required_str(&args, "pattern").is_err());
        assert!(required_str(&json!({}), "path").is_err());
        assert_eq!(optional_str(&json!({"path": " a "}), "path").unwrap(), "a");
        assert!(optional_str(&json!({"path": " "}), "path").is_none());
    }

    #[test]
    fn the_round_budget_truncates_instead_of_exploding() {
        let mut budget = Budget::new();
        let small = budget.spend("a".repeat(100));
        assert_eq!(small.chars().count(), 100);

        let mut tiny = Budget::new();
        tiny.remaining = 10;
        let text = tiny.spend("x".repeat(50));
        assert!(text.starts_with(&"x".repeat(10)), "{text}");
        assert!(text.contains("上限"), "{text}");
        // 预算用完后再花，什么都不给。
        let text = tiny.spend("y".repeat(50));
        assert!(!text.contains('y'), "{text}");
    }

    #[test]
    fn short_names_survive_trailing_separators() {
        assert_eq!(short_name(r"C:\proj\src\main.rs"), "main.rs");
        assert_eq!(short_name("proj\\"), "proj");
        assert_eq!(short_name("main.rs"), "main.rs");
    }
}
