use crate::{
    ai::{
        self,
        config::{AiConfigPatch, AiConfigView, Provider},
        secret,
        writes::{self, WriteOutcome},
    },
    broadcast, chat, chat_store, chat_window, clock, context, desktop,
    expression::validate_expression,
    proactive,
    settings::{PetSettings, SettingsChange},
    settings_window,
    workspace::{self, EntryKind, WorkspaceEntry},
};
use serde::Serialize;
use tauri::Manager;

// Serialize 让 Tauri 可以把这个 Rust 结构体转换成事件中的 JSON 对象。
#[derive(Clone, Serialize)]
pub struct ExpressionRequested {
    pub name: String,
}

#[tauri::command]
pub fn get_pet_settings(app: tauri::AppHandle) -> Result<PetSettings, String> {
    desktop::get_settings(&app)
}

#[tauri::command]
pub async fn get_pet_cursor_position(
    window: tauri::WebviewWindow,
) -> Result<desktop::CursorPosition, String> {
    desktop::get_cursor_position(&window)
}

#[tauri::command]
pub async fn set_pet_visible(app: tauri::AppHandle, visible: bool) -> Result<PetSettings, String> {
    desktop::update_settings(&app, SettingsChange::Visible(visible))
}

#[tauri::command]
pub fn set_pet_max_fps(app: tauri::AppHandle, max_fps: u32) -> Result<PetSettings, String> {
    desktop::update_settings(&app, SettingsChange::MaxFps(max_fps))
}

#[tauri::command]
pub fn set_pet_always_on_top(app: tauri::AppHandle, enabled: bool) -> Result<PetSettings, String> {
    desktop::update_settings(&app, SettingsChange::AlwaysOnTop(enabled))
}

#[tauri::command]
pub fn quit_pet(app: tauri::AppHandle) {
    app.exit(0);
}

#[tauri::command]
pub fn request_expression(app: tauri::AppHandle, name: String) -> Result<(), String> {
    #[cfg(debug_assertions)]
    println!("[Rust] 收到表情请求: {name}");

    // 校验失败时，? 会提前返回 Err，下面的事件不会发送。
    validate_expression(&name)?;

    let payload = ExpressionRequested { name };
    // 主窗口必须收到：真的换表情的是它，收不到就是故障，直接返回错误。
    broadcast::emit(&app, broadcast::MAIN, "pet-expression-requested", &payload)?;
    // 其余窗口只是「顺带同步一下显示」，没开就跳过。
    // 用「除主窗口外全部」而不是写死 settings：以后加对话窗口会自动拿到这份同步。
    for failure in
        broadcast::emit_all_except(&app, broadcast::MAIN, "pet-expression-observed", &payload)
    {
        desktop::report_error(&app, &format!("表情反馈通知失败：{failure}"));
    }
    Ok(())
}

#[tauri::command]
pub async fn open_pet_settings(app: tauri::AppHandle) -> Result<(), String> {
    settings_window::open(app).await
}

#[tauri::command]
pub async fn open_chat_window(app: tauri::AppHandle) -> Result<(), String> {
    chat_window::open(app).await
}

#[tauri::command]
pub async fn reload_pet_model(app: tauri::AppHandle) -> Result<(), String> {
    #[cfg(debug_assertions)]
    broadcast::emit(&app, broadcast::MAIN, "pet-model-reload", &())?;

    // 发布版不注册重载入口；保留函数以便两个构建共用同一份命令注册表。
    #[cfg(not(debug_assertions))]
    {
        let _ = app;
        return Err("模型重载仅在开发版本中可用".into());
    }

    #[cfg(debug_assertions)]
    Ok(())
}

// ---------- AI 配置与 API Key ----------

/// 前端的 AI 设置快照：配置 + `hasKey` + 掩码。**没有 Key 本身。**
#[tauri::command]
pub fn get_ai_config(app: tauri::AppHandle) -> Result<AiConfigView, String> {
    let config = ai::get_config(&app)?;
    ai::view(&config)
}

#[tauri::command]
pub fn set_ai_config(app: tauri::AppHandle, patch: AiConfigPatch) -> Result<AiConfigView, String> {
    let config = ai::update_config(&app, patch)?;
    ai::view(&config)
}

/// 保存 API Key。**不返回 Key，也不回显**——只回新的掩码。
#[tauri::command]
pub fn set_api_key(
    app: tauri::AppHandle,
    provider: Provider,
    key: String,
) -> Result<AiConfigView, String> {
    secret::save(provider, &key)?;
    publish_ai_config(&app)
}

#[tauri::command]
pub fn clear_api_key(app: tauri::AppHandle, provider: Provider) -> Result<AiConfigView, String> {
    secret::clear(provider)?;
    publish_ai_config(&app)
}

/// Key 变了，掩码也就变了：广播一份新快照给设置窗口与对话窗口。
fn publish_ai_config(app: &tauri::AppHandle) -> Result<AiConfigView, String> {
    let config = ai::get_config(app)?;
    ai::broadcast_config(app, &config);
    ai::view(&config)
}

#[tauri::command]
pub async fn test_ai_connection(app: tauri::AppHandle) -> Result<ai::TestOutcome, String> {
    Ok(ai::test_connection(&app).await)
}

// ---------- 对话与会话 ----------

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SentMessage {
    /// 助手那条消息的 id：前端把它当流式增量的落点。
    pub message_id: String,
}

#[derive(Clone, Serialize)]
pub struct CancelResult {
    pub ok: bool,
}

#[tauri::command]
pub fn create_session(app: tauri::AppHandle) -> Result<chat_store::Session, String> {
    let store = chat_store::store(&app)?;
    chat_store::create(&store, clock::now_ms())
}

#[tauri::command]
pub fn list_sessions(app: tauri::AppHandle) -> Result<Vec<chat_store::Session>, String> {
    let store = chat_store::store(&app)?;
    Ok(chat_store::list(&store))
}

#[tauri::command]
pub fn get_messages(
    app: tauri::AppHandle,
    session_id: String,
) -> Result<Vec<chat_store::Message>, String> {
    let store = chat_store::store(&app)?;
    Ok(chat_store::load(&store, &session_id)?.messages)
}

/// 发一条消息。**立刻返回**助手消息的 id，回答通过 `chat-stream-*` 事件陆续到达。
#[tauri::command]
pub async fn send_message(
    app: tauri::AppHandle,
    session_id: String,
    text: String,
) -> Result<SentMessage, String> {
    let message_id = chat::send(app, &session_id, &text).await?;
    Ok(SentMessage { message_id })
}

#[tauri::command]
pub fn cancel_stream(app: tauri::AppHandle, session_id: String) -> Result<CancelResult, String> {
    Ok(CancelResult {
        ok: chat::cancel(&app, &session_id),
    })
}

// ---------- 工作区 ----------

/// 当前的工作区条目。对话窗口打开时读一次，之后靠 `workspace-changed` 同步。
#[tauri::command]
pub fn list_workspace_entries(app: tauri::AppHandle) -> Result<Vec<WorkspaceEntry>, String> {
    workspace::get(&app)
}

/// 「添加文件夹」：弹出系统目录选择器，选中的目录作为**可读写**条目。
///
/// 选择器在 Rust 侧调用（`tauri-plugin-dialog`），前端只发一个命令。这样前端
/// 不需要任何 fs 或对话框权限——能力边界留在 Rust 这边（计划 §10 第 1 条）。
#[tauri::command]
pub async fn add_workspace_dir(app: tauri::AppHandle) -> Result<Vec<WorkspaceEntry>, String> {
    let selected = {
        let app = app.clone();
        tauri::async_runtime::spawn_blocking(move || {
            use tauri_plugin_dialog::DialogExt;
            app.dialog()
                .file()
                .set_title("选一个文件夹给八千代看（可以读写，改动前会给 diff）")
                .blocking_pick_folder()
        })
        .await
        .map_err(|error| format!("打开目录选择器失败：{error}"))?
    };
    let Some(folder) = selected else {
        // 用户点了取消：不是错误，原样返回当前清单。
        return workspace::get(&app);
    };
    let path = folder
        .into_path()
        .map_err(|error| format!("这个选择读不出路径：{error}"))?;
    add_workspace_paths(&app, EntryKind::Dir, &[path.to_string_lossy().to_string()])
}

/// 拖拽投放：**一律作为只读文件条目**。
///
/// 拖进来的是引用而不是副本：原文件更新后读到的就是最新的（设计文档 §4.2）。
/// 目录不从这里进来——那样会绕开「加文件夹」这个明确的授权动作。
#[tauri::command]
pub fn add_workspace_files(
    app: tauri::AppHandle,
    paths: Vec<String>,
) -> Result<Vec<WorkspaceEntry>, String> {
    if paths.is_empty() {
        return Err("没有拖进来任何东西".into());
    }
    add_workspace_paths(&app, EntryKind::File, &paths)
}

/// 加条目：校验 → 写回内存 → 落盘 → 广播。
fn add_workspace_paths(
    app: &tauri::AppHandle,
    kind: EntryKind,
    paths: &[String],
) -> Result<Vec<WorkspaceEntry>, String> {
    let mut entries = workspace::get(app)?;
    workspace::add(&mut entries, kind, paths, clock::now_ms())?;
    workspace::replace(app, entries)
}

#[tauri::command]
pub fn remove_workspace_entry(
    app: tauri::AppHandle,
    id: String,
) -> Result<Vec<WorkspaceEntry>, String> {
    let mut entries = workspace::get(&app)?;
    workspace::remove(&mut entries, &id)?;
    workspace::replace(&app, entries)
}

// ---------- 写入确认 ----------

/// 用户点了「应用」：**这时才真的写**（路径会在这里重新校验一遍）。
#[tauri::command]
pub fn apply_pending_write(
    app: tauri::AppHandle,
    request_id: String,
) -> Result<WriteOutcome, String> {
    writes::apply(&app, &request_id)
}

/// 用户点了「拒绝」：文件一个字都不动。
#[tauri::command]
pub fn reject_pending_write(
    app: tauri::AppHandle,
    request_id: String,
) -> Result<WriteOutcome, String> {
    writes::reject(&app, &request_id)
}

// ---------- 主动互动 ----------

/// 前端的主动互动快照。开关本身存在 `settings.json` 的 `proactive.enabled` 里，
/// 所以它跟着 `pet-settings-changed` 一起走。
#[derive(Clone, Serialize)]
pub struct ProactiveStateView {
    pub enabled: bool,
    /// 今天是否处在「别烦我」状态。跨天自动变回 `false`。
    pub muted_today: bool,
}

/// 组装主动互动的状态快照。
///
/// **先读设置、再锁主动互动**，固定这个顺序，避免和别处的加锁顺序反过来造成死锁。
fn proactive_state_view(app: &tauri::AppHandle) -> Result<ProactiveStateView, String> {
    let enabled = desktop::get_settings(app)?.proactive_enabled;
    let muted_today = {
        let store = app.state::<proactive::ProactiveStore>();
        let state = store.0.lock().map_err(|error| error.to_string())?;
        proactive::is_muted_today(&state, context::local_day())
    };
    Ok(ProactiveStateView {
        enabled,
        muted_today,
    })
}

#[tauri::command]
pub fn get_proactive_state(app: tauri::AppHandle) -> Result<ProactiveStateView, String> {
    proactive_state_view(&app)
}

/// 「今天别烦我」：今天剩下的时间一句都不说。
///
/// 这是**手动静默**的入口——早先那版会按时段（23:00–08:00）自动闭嘴，
/// 那等于替用户定作息，而且半夜写代码的人只会觉得功能坏了。静默该由用户按下。
#[tauri::command]
pub fn mute_proactive_today(app: tauri::AppHandle) -> Result<ProactiveStateView, String> {
    set_proactive_mute(&app, true)?;
    proactive_state_view(&app)
}

/// 解除「今天别烦我」。
#[tauri::command]
pub fn clear_proactive_mute(app: tauri::AppHandle) -> Result<ProactiveStateView, String> {
    set_proactive_mute(&app, false)?;
    proactive_state_view(&app)
}

/// 把「今天别烦我」设成指定状态，返回设置之后的状态。
/// 命令与托盘菜单共用这一个入口，保证两条路径的行为一致。
pub fn set_proactive_mute(app: &tauri::AppHandle, muted: bool) -> Result<bool, String> {
    let day = context::local_day();
    let store = app.state::<proactive::ProactiveStore>();
    let mut state = store.0.lock().map_err(|error| error.to_string())?;
    if muted {
        proactive::mute_today(&mut state, day)
    } else {
        proactive::unmute(&mut state)
    };
    Ok(muted)
}

/// 一键开关。走 `desktop::update_settings`，于是落盘、广播、托盘三件事都有。
///
/// 返回的是**完整的设置快照**（比 `{ enabled }` 多几项）：前端的设置页拿它直接更新界面，
/// 不必再回读一次，也就不会出现「点了开关、界面慢半拍」。
#[tauri::command]
pub fn set_proactive_enabled(app: tauri::AppHandle, enabled: bool) -> Result<PetSettings, String> {
    // 重新打开开关 = 「我想让它说话」，顺手解除「今天别烦我」。
    // 否则用户会撞上「开关明明是开的，它却一整天不理我」。
    if enabled {
        let _ = set_proactive_mute(&app, false);
    }
    desktop::update_settings(&app, SettingsChange::ProactiveEnabled(enabled))
}

/// 浏览器标题开关。
///
/// 跟主动互动总开关一样走 `desktop::update_settings`，于是落盘、广播、托盘同步三件事都有。
/// 单独一个命令而不是复用总开关：用户可能希望桌宠照常陪着，但不希望它读浏览器标题。
#[tauri::command]
pub fn set_browser_title_enabled(
    app: tauri::AppHandle,
    enabled: bool,
) -> Result<PetSettings, String> {
    desktop::update_settings(&app, SettingsChange::BrowserTitleEnabled(enabled))
}

/// 气泡消失时回报一句：`acknowledged` = 用户**主动点掉了**它。
///
/// **只有主动点掉才计入降频。** 自己淡出不计分：桌宠窗口可能在别的窗口后面、
/// 或者在屏幕角落，用户根本没看见——把「没看见」当成「不想看」，
/// 会让这个功能因为一件用户没做过的事把自己静音。
/// 连续三次被点掉之后同类冷却翻倍（计划 §8.4）。
/// 回报的编号对不上当前那句时返回 `ok: false`——重复回报不该重复计数。
#[tauri::command]
pub fn proactive_dismiss(
    app: tauri::AppHandle,
    id: String,
    acknowledged: bool,
) -> Result<CancelResult, String> {
    let store = app.state::<proactive::ProactiveStore>();
    let mut state = store.0.lock().map_err(|error| error.to_string())?;
    let ok = proactive::dismiss(&mut state, &id, acknowledged);
    drop(state);

    #[cfg(debug_assertions)]
    println!(
        "[Rust] 主动互动：气泡回报（{id}，{}）→ {}",
        if acknowledged {
            "被点掉"
        } else {
            "自己淡出，不计分"
        },
        if ok {
            "已记录"
        } else {
            "不是当前那句，忽略"
        }
    );
    Ok(CancelResult { ok })
}
