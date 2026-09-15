//! 对话历史的落盘。
//!
//! 一个会话一个文件：`<app_data_dir>/sessions/<sessionId>.json`，
//! 复用 Phase 7 的 [`Store`]（原子写 + `.bak` 备份 + 损坏回退）。
//! 用 JSON 而不是 SQLite 的理由见实施计划 §4：单用户本地、第一版消息量小、
//! 少一个要编译的依赖、容易写单测。
//!
//! **这里只碰文件，不碰 Tauri 的网络与事件**，所以整个模块都能用临时目录测。

use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use crate::store::Store;

/// 会话文件所在的子目录名。
pub const SESSIONS_DIR: &str = "sessions";
/// 单条用户消息的长度上限（字符）。挡住「把整份文件粘进输入框」。
pub const MAX_MESSAGE_CHARS: usize = 8_000;
/// 会话标题的长度上限（字符）。
pub const MAX_TITLE_CHARS: usize = 20;

/// 消息作者。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
}

impl Role {
    /// 发给模型的角色名。与线上格式一致，但目前两者字符串相同，留这个函数是为了
    /// 以后角色名变化时只有一个地方要改。
    pub fn as_wire(self) -> &'static str {
        match self {
            Role::User => "user",
            Role::Assistant => "assistant",
        }
    }
}

/// 一条消息。`error` 有值表示这条回答没正常说完（被打断或报错），
/// 历史里要看得出来——不能把「说了一半」伪装成完整回答。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: String,
    pub role: Role,
    pub content: String,
    pub created_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// 一个会话。
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    pub title: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub messages: Vec<Message>,
}

/// 会话目录。
pub fn store(app: &tauri::AppHandle) -> Result<Store, String> {
    Ok(Store::new(
        crate::settings::data_dir(app)?.join(SESSIONS_DIR),
    ))
}

/// 单调递增的序号：同一毫秒内连续创建也不会撞 id。
fn next_seq() -> u64 {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    SEQ.fetch_add(1, Ordering::Relaxed)
}

/// id 只允许 ASCII 字母数字与 `-` `_`。
///
/// 这不是洁癖：id 会拼进文件名，`../` 之类的东西必须在这里挡住。
pub fn is_safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        })
}

pub fn new_session_id(now_ms: u64) -> String {
    format!("s{now_ms}-{}", next_seq())
}

fn new_message_id(now_ms: u64) -> String {
    format!("m{now_ms}-{}", next_seq())
}

/// 从第一条用户消息里取标题：第一行、压掉多余空白、超长截断。
pub fn title_from(text: &str) -> String {
    let first_line = text.lines().next().unwrap_or_default().trim();
    let collapsed: String = first_line.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return "新的对话".into();
    }
    if collapsed.chars().count() > MAX_TITLE_CHARS {
        let head: String = collapsed.chars().take(MAX_TITLE_CHARS).collect();
        format!("{head}…")
    } else {
        collapsed
    }
}

fn path_name(id: &str) -> Result<String, String> {
    if !is_safe_id(id) {
        return Err(format!("会话 id 不合法：{id}"));
    }
    Ok(format!("{id}.json"))
}

/// 新建会话并立刻落盘：这样窗口关掉也不会留下「内存里有、文件里没有」的会话。
pub fn create(store: &Store, now_ms: u64) -> Result<Session, String> {
    let session = Session {
        id: new_session_id(now_ms),
        title: "新的对话".into(),
        created_at: now_ms,
        updated_at: now_ms,
        messages: Vec::new(),
    };
    save(store, &session)?;
    Ok(session)
}

pub fn save(store: &Store, session: &Session) -> Result<(), String> {
    let name = path_name(&session.id)?;
    store.save(&name, session)
}

/// 读一个会话。文件不存在或坏掉都返回可读的中文错误。
pub fn load(store: &Store, id: &str) -> Result<Session, String> {
    let name = path_name(id)?;
    if !store.path(&name).exists() && !store.path(&format!("{name}.bak")).exists() {
        return Err("这条对话已经不存在了，重新开一个吧".into());
    }
    let loaded = store.load::<Session>(&name);
    match loaded.source {
        crate::store::LoadSource::Primary | crate::store::LoadSource::Backup => Ok(loaded.value),
        _ => Err(format!(
            "这条对话读不出来了：{}",
            loaded.note.unwrap_or_else(|| "文件损坏".into())
        )),
    }
}

/// 所有会话，最近更新的排前面。
///
/// 坏掉的单个文件只跳过，不影响其它会话——用户不该因为一个文件坏了就看不到全部历史。
pub fn list(store: &Store) -> Vec<Session> {
    let entries = match std::fs::read_dir(store.path("")) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };
    let mut sessions = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        // 只认 `<id>.json`：`.bak` / `.tmp` 都不是会话本体。
        let Some(id) = name.strip_suffix(".json") else {
            continue;
        };
        if !is_safe_id(id) {
            continue;
        }
        let loaded = store.load::<Session>(name);
        if loaded.source == crate::store::LoadSource::Fallback {
            eprintln!(
                "[Rust] 跳过读不出来的对话 {name}：{}",
                loaded.note.unwrap_or_default()
            );
            continue;
        }
        sessions.push(loaded.value);
    }
    sessions.sort_by_key(|session| std::cmp::Reverse(session.updated_at));
    sessions
}

/// 删除一个会话。
///
/// 还没有对应的界面入口（阶段 A 只保证「历史还在」），先留着并被测试覆盖：
/// 阶段 C 的会话管理会直接用它。
#[allow(dead_code)]
pub fn delete(store: &Store, id: &str) -> Result<(), String> {
    let name = path_name(id)?;
    let path = store.path(&name);
    if path.exists() {
        std::fs::remove_file(&path).map_err(|error| format!("删除对话失败：{error}"))?;
    }
    Ok(())
}

/// 追加一条用户消息。第一条用户消息决定会话标题。
pub fn append_user(session: &mut Session, text: &str, now_ms: u64) -> Message {
    let message = Message {
        id: new_message_id(now_ms),
        role: Role::User,
        content: text.trim().to_string(),
        created_at: now_ms,
        error: None,
    };
    if session.title == "新的对话" {
        session.title = title_from(text);
    }
    session.messages.push(message.clone());
    session.updated_at = now_ms;
    message
}

/// 先放一条空的助手消息占位。
///
/// 为什么要占位：流式回答是一段一段来的，前端需要一个固定的 `messageId` 挂这些增量；
/// 万一程序在回答中途被杀掉，历史里也能看出「这里本来有一条回答」。
pub fn begin_assistant(session: &mut Session, now_ms: u64) -> Message {
    let message = Message {
        id: new_message_id(now_ms),
        role: Role::Assistant,
        content: String::new(),
        created_at: now_ms,
        error: None,
    };
    session.messages.push(message.clone());
    message
}

/// 收尾：写入最终文本，以及失败原因（正常说完了就传 `None`）。
///
/// 找不到那条消息时什么都不做——状态已经不一致了，报错也没有用。
pub fn finish_assistant(
    session: &mut Session,
    message_id: &str,
    content: &str,
    error: Option<String>,
    now_ms: u64,
) {
    if let Some(message) = session
        .messages
        .iter_mut()
        .find(|message| message.id == message_id)
    {
        message.content = content.to_string();
        message.error = error;
        session.updated_at = now_ms;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::test_support::TempDir;

    fn store(tag: &str) -> (TempDir, Store) {
        let dir = TempDir::new(tag);
        let store = dir.store();
        (dir, store)
    }

    #[test]
    fn a_conversation_survives_a_restart() {
        // 「重启」= 用同一个目录重新打开一次 Store。
        let (_dir, store) = store("session-restart");
        let mut session = create(&store, 1_000).expect("新建会话");
        append_user(&mut session, "你好呀", 1_100);
        let reply = begin_assistant(&mut session, 1_200);
        finish_assistant(&mut session, &reply.id, "在的，怎么了", None, 1_300);
        save(&store, &session).expect("写入");

        let restored = load(&store, &session.id).expect("重新读出来");
        assert_eq!(restored.messages.len(), 2);
        assert_eq!(restored.messages[0].role, Role::User);
        assert_eq!(restored.messages[0].content, "你好呀");
        assert_eq!(restored.messages[1].content, "在的，怎么了");
        assert_eq!(restored.title, "你好呀");
        assert_eq!(restored.updated_at, 1_300);
    }

    #[test]
    fn list_returns_the_most_recent_conversation_first() {
        let (_dir, store) = store("session-list");
        let older = create(&store, 1_000).expect("新建");
        let newer = create(&store, 2_000).expect("新建");
        let sessions = list(&store);
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].id, newer.id);
        assert_eq!(sessions[1].id, older.id);
    }

    #[test]
    fn titles_come_from_the_first_line_and_are_truncated() {
        assert_eq!(title_from("帮我看看这个 bug"), "帮我看看这个 bug");
        assert_eq!(title_from("第一行\n第二行"), "第一行");
        assert_eq!(title_from("   "), "新的对话");
        let long = "啊".repeat(MAX_TITLE_CHARS + 5);
        let title = title_from(&long);
        assert_eq!(title.chars().count(), MAX_TITLE_CHARS + 1);
        assert!(title.ends_with('…'));
    }

    #[test]
    fn unsafe_ids_are_rejected_before_touching_the_filesystem() {
        let (_dir, store) = store("session-unsafe");
        assert!(load(&store, "../../settings").is_err());
        assert!(delete(&store, "../evil").is_err());
        assert!(save(
            &store,
            &Session {
                id: "a/b".into(),
                title: "x".into(),
                created_at: 0,
                updated_at: 0,
                messages: Vec::new(),
            }
        )
        .is_err());
        assert!(!is_safe_id(""));
        assert!(is_safe_id("s1234-1"));
    }

    #[test]
    fn loading_a_missing_conversation_explains_itself() {
        let (_dir, store) = store("session-missing");
        let error = load(&store, "s1-1").expect_err("不存在的会话应当报错");
        assert!(error.contains("不存在"), "{error}");
    }

    #[test]
    fn delete_removes_the_file_and_tolerates_missing_ones() {
        let (_dir, store) = store("session-delete");
        let session = create(&store, 1_000).expect("新建");
        delete(&store, &session.id).expect("删除");
        assert!(list(&store).is_empty());
        delete(&store, &session.id).expect("再删一次也不报错");
    }

    #[test]
    fn partial_answers_keep_their_text_and_the_reason() {
        let (_dir, store) = store("session-cancel");
        let mut session = create(&store, 1_000).expect("新建");
        let reply = begin_assistant(&mut session, 1_100);
        finish_assistant(
            &mut session,
            &reply.id,
            "说到一半",
            Some("已停止".into()),
            1_200,
        );
        save(&store, &session).expect("写入");

        let restored = load(&store, &session.id).expect("重新读出来");
        assert_eq!(restored.messages[0].content, "说到一半");
        assert_eq!(restored.messages[0].error.as_deref(), Some("已停止"));
    }

    #[test]
    fn broken_files_are_skipped_so_the_rest_of_the_history_still_shows() {
        let (_dir, store) = store("session-broken");
        let good = create(&store, 1_000).expect("新建");
        std::fs::write(store.path("s999-1.json"), "{ 这不是 JSON").expect("写坏文件");
        let sessions = list(&store);
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, good.id);
    }
}
