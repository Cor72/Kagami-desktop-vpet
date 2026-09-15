//! 主动互动：规则引擎（纯函数，可单测）+ 低频调度 + 内置文案表。
//!
//! 三条触发（计划 §8.3、设计文档 §6.4）：
//!
//! | 触发 | 类型 | 说明 |
//! |---|---|---|
//! | 空闲 ≥ 5 分钟 | 状态型 | 用户离开了 |
//! | 从空闲回到活跃 | 状态型 | 用户回来了 |
//! | 前台窗口切换到某个已登记的程序 | 事件型 | 打开编辑器 / 浏览器 / 音乐 / 游戏 |
//!
//! 三条**都不调模型**：文案全是内置模板，所以没有 API Key 也能工作，零成本、零延迟、可测试。
//!
//! 防线的分工（计划 §8.4）：
//!
//! - **额度与冷却**决定最坏情况：事件型同类 5 分钟，状态型同类 20 分钟，每天合计 ≤ 30 条；
//! - **静默规则**决定什么时候绝对不说：静默时段、全屏（状态型）、用户正在输入（延后）、
//!   桌宠隐藏或最小化（连采样都不做，见 [`start`]）；
//! - **连续被忽略 3 次**之后同类冷却翻倍——这条才是核心，它让这个功能自己收敛到安静。

// TODO: 八千代的角色设定稿确定后，替换这里的文案与语气。
// 相关系统提示词同样待补（见 ai/persona.rs）。

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::broadcast;
use crate::context::{self, Sample};
use crate::{clock, desktop};

/// 采样间隔：低频采样，只在信号变化时才判断（计划 §8.1）。
pub const SAMPLE_INTERVAL_MS: u64 = 5_000;
/// 气泡显示时长，与 `PetSpeechBubble.vue` 的默认值一致。
const TTL_MS: u64 = 5_000;
/// 空闲多久算「离开了」。计划给的范围是 3–10 分钟、默认 5 分钟。
pub const IDLE_THRESHOLD_MS: u64 = 5 * 60_000;
/// 状态型触发（闲置问候 / 回到活跃）的同类冷却。
const STATE_COOLDOWN_MS: u64 = 20 * 60_000;
/// 事件型触发（程序切换）的同类冷却。
const APP_COOLDOWN_MS: u64 = 5 * 60_000;
/// 每日总量上限（兜底，不是主力约束）。
const DAILY_LIMIT: u32 = 30;
/// 静默时段：23:00–08:00。
const QUIET_FROM_HOUR: u32 = 23;
const QUIET_UNTIL_HOUR: u32 = 8;
/// 「最近有键盘/鼠标输入」的判定窗口：这期间不说，等用户停手（计划 §8.4）。
const RECENT_INPUT_MS: u64 = 30_000;
/// 「等停下来再说」的那一句最多等这么久，过期就作废——三分钟前的场景已经不成立了。
const PENDING_TTL_MS: u64 = 3 * 60_000;
/// 连续被忽略几次之后同类冷却翻倍。**不可关闭的机制**（计划 §8.4）。
const IGNORED_STREAK_LIMIT: u32 = 3;
/// 编辑器标题里提取出来的文件名最多留这么长，免得撑爆气泡。
const DETAIL_MAX_CHARS: usize = 24;

// ---------- 文案表 ----------

/// 应用分类。分类决定用哪一组文案，也决定「同类冷却」怎么算。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    Editor,
    Browser,
    Music,
    Game,
}

impl Category {
    fn key(self) -> &'static str {
        match self {
            Category::Editor => "editor",
            Category::Browser => "browser",
            Category::Music => "music",
            Category::Game => "game",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Category::Editor => "代码编辑器",
            Category::Browser => "浏览器",
            Category::Music => "音乐",
            Category::Game => "游戏",
        }
    }
}

/// 窗口标题怎么处理。**这是隐私分级的关键**（计划 §8.2）：
///
/// - [`TitleUse::EditorFile`]：已知编辑器，只从标题里提取**文件名**；
/// - [`TitleUse::Discard`]：其余应用（浏览器、聊天软件……）**整条丢弃**。
///
/// 于是知道你在玩什么游戏，但**不知道**你在跟谁聊天：
/// 「招商银行 - Chrome」「张三 - 微信」这类标题一条都不会被用上。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TitleUse {
    EditorFile,
    Discard,
}

/// 一条「进程名 → 文案」的规则。
///
/// 映射表要做成**数据**（就是下面这张表），不要散在 `if` 分支里——以后加分类只改这里。
#[derive(Debug)]
pub struct AppRule {
    /// 进程名，比较时不区分大小写。
    pub process: &'static str,
    pub category: Category,
    pub title: TitleUse,
    /// 备选文案，轮换使用。含 `{detail}` 的那几条只在拿到了文件名时才用。
    pub lines: &'static [&'static str],
}

/// 内置映射表的起步清单（计划 §8.5）。文案规则：陈述句、不评价、不催促、不给建议。
pub const APP_RULES: &[AppRule] = &[
    AppRule {
        process: "Code.exe",
        category: Category::Editor,
        title: TitleUse::EditorFile,
        lines: &[
            "编辑器打开了：{detail}",
            "屏幕上是 {detail}",
            "又打开了一个文件：{detail}",
            "编辑器亮起来了",
            "又回到代码这边",
        ],
    },
    AppRule {
        process: "idea64.exe",
        category: Category::Editor,
        title: TitleUse::EditorFile,
        lines: &[
            "编辑器打开了：{detail}",
            "又打开了一个文件：{detail}",
            "编辑器亮起来了",
        ],
    },
    AppRule {
        process: "sublime_text.exe",
        category: Category::Editor,
        title: TitleUse::EditorFile,
        lines: &[
            "编辑器打开了：{detail}",
            "又打开了一个文件：{detail}",
            "编辑器亮起来了",
        ],
    },
    AppRule {
        process: "gvim.exe",
        category: Category::Editor,
        title: TitleUse::EditorFile,
        lines: &[
            "编辑器打开了：{detail}",
            "又打开了一个文件：{detail}",
            "编辑器亮起来了",
        ],
    },
    AppRule {
        process: "chrome.exe",
        category: Category::Browser,
        title: TitleUse::Discard,
        lines: &["浏览器打开了", "又见浏览器", "打开了浏览器"],
    },
    AppRule {
        process: "msedge.exe",
        category: Category::Browser,
        title: TitleUse::Discard,
        lines: &["浏览器打开了", "又见浏览器", "打开了浏览器"],
    },
    AppRule {
        process: "firefox.exe",
        category: Category::Browser,
        title: TitleUse::Discard,
        lines: &["浏览器打开了", "又见浏览器", "打开了浏览器"],
    },
    AppRule {
        process: "cloudmusic.exe",
        category: Category::Music,
        title: TitleUse::Discard,
        lines: &["音乐响起来了", "开始听歌了", "音乐软件打开了"],
    },
    AppRule {
        process: "QQMusic.exe",
        category: Category::Music,
        title: TitleUse::Discard,
        lines: &["音乐响起来了", "开始听歌了", "音乐软件打开了"],
    },
    AppRule {
        process: "Spotify.exe",
        category: Category::Music,
        title: TitleUse::Discard,
        lines: &["音乐响起来了", "开始听歌了", "音乐软件打开了"],
    },
    AppRule {
        process: "GenshinImpact.exe",
        category: Category::Game,
        title: TitleUse::Discard,
        lines: &["游戏启动了", "打开了游戏", "开始打游戏了"],
    },
    AppRule {
        process: "Yuanshen.exe",
        category: Category::Game,
        title: TitleUse::Discard,
        lines: &["游戏启动了", "打开了游戏", "开始打游戏了"],
    },
];

/// 闲置问候：用户离开了。陈述句，不问「还在吗」——问句会制造回应的义务。
const IDLE_LINES: &[&str] = &["这儿安静下来了", "先在这儿待着", "我自己待一会儿"];
/// 回到活跃：用户回来了。
const BACK_LINES: &[&str] = &["回来啦", "你回来了", "欢迎回来"];

// ---------- 输入 ----------

/// 规则引擎的输入：一次采样 + 本地时间。**不依赖系统时间，也不依赖 Tauri**。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Signal {
    pub process_name: Option<String>,
    /// 原始窗口标题：只在本地用，只有已知编辑器会从中提取文件名。
    pub window_title: Option<String>,
    pub idle_ms: u64,
    pub fullscreen: bool,
    /// 本地日期（`20260915`），用于每日额度跨天归零。
    pub local_day: u32,
    /// 本地小时，用于静默时段。
    pub local_hour: u32,
}

impl From<Sample> for Signal {
    fn from(sample: Sample) -> Self {
        Self {
            process_name: sample.process_name,
            window_title: sample.window_title,
            idle_ms: sample.idle_ms,
            fullscreen: sample.fullscreen,
            local_day: sample.local_day,
            local_hour: sample.local_hour,
        }
    }
}

/// 归类结果：命中的规则，以及（只有编辑器才有）从标题里取出来的文件名。
#[derive(Clone, Debug)]
pub struct AppMatch {
    pub rule: &'static AppRule,
    pub detail: Option<String>,
}

impl AppMatch {
    fn label(&self) -> &'static str {
        self.rule.category.label()
    }
}

/// 把一次采样归类。未登记的应用返回 `None`——**只记录，不说话**。
pub fn classify(signal: &Signal) -> Option<AppMatch> {
    let rule = find_rule(signal.process_name.as_deref()?)?;
    let detail = match rule.title {
        // 只有编辑器才看标题，而且只看第一段（文件名）。
        TitleUse::EditorFile => signal
            .window_title
            .as_deref()
            .and_then(extract_file_name_from_title),
        // 其余应用：标题整条丢弃，连传都不往下传。
        TitleUse::Discard => None,
    };
    Some(AppMatch { rule, detail })
}

fn find_rule(process_name: &str) -> Option<&'static AppRule> {
    APP_RULES
        .iter()
        .find(|rule| rule.process.eq_ignore_ascii_case(process_name))
}

/// 从编辑器窗口标题的第一段取文件名。
///
/// - VS Code：`pomodoro.rs - 项目名 - Visual Studio Code`
/// - JetBrains / Sublime：`pomodoro.rs – 项目名 – IntelliJ IDEA`
///
/// 取不到第一段分隔符（说明标题里没有文件名）、或者第一段长得不像文件名时返回 `None`，
/// 此时退回不带文件名的那几句文案——宁可少说，也不要把整条标题当文件名念出来。
pub fn extract_file_name_from_title(title: &str) -> Option<String> {
    const SEPARATORS: [&str; 3] = [" - ", " – ", " — "];
    let index = SEPARATORS
        .iter()
        .filter_map(|separator| title.find(separator))
        .min()?;
    let head = title[..index].trim();
    if head.is_empty() || head.contains(['\\', '/', '\u{0}']) {
        return None;
    }
    Some(head.chars().take(DETAIL_MAX_CHARS).collect())
}

// ---------- 规则引擎 ----------

/// 触发类型。冷却与文案轮换都按它分组。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Idle,
    Back,
    App,
}

/// 一句要说的话。
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Utterance {
    /// 这句话的编号：前端关闭气泡时带回来，用于「连续被忽略」计数。
    pub id: String,
    pub text: String,
    /// 气泡显示多久（毫秒）。
    pub ttl_ms: u64,
}

/// 一次「要说的话」的全部上下文。攒在一起是为了让 [`try_speak`] 只收一个参数。
struct LineSet {
    kind: Kind,
    /// 冷却键：事件型按分类分开算（编辑器 / 浏览器 / 音乐 / 游戏 各自 5 分钟）。
    cooldown_key: String,
    /// 轮换键：同一个程序有自己的轮换（计划 §8.5）。
    rotation_key: String,
    lines: &'static [&'static str],
    detail: Option<String>,
}

/// 「等停下来再说」的那一句。计划 §8.4 写的是**延后**，不是不说。
#[derive(Clone, Debug)]
struct Pending {
    utterance: Utterance,
    kind: Kind,
    cooldown_key: String,
    /// 只对程序切换有意义：换了前台程序，那一刻就过去了，这句作废。
    process: Option<String>,
    created_ms: u64,
}

/// 引擎的记忆。**只活在内存里**：重启后从零开始比继承一堆旧冷却更合理。
#[derive(Debug)]
pub struct ProactiveState {
    /// 空闲阈值。默认 5 分钟；开发期可以用环境变量临时改小（见 [`debug_overrides`]）。
    idle_threshold_ms: u64,
    /// 见过第一帧没有。第一帧只用来建立基线，不发言——否则一启动就会「报告」你正在做什么。
    started: bool,
    last_process: Option<String>,
    was_idle: bool,
    /// 连续被忽略的次数（用户没点那条气泡）。
    ignored_streak: u32,
    /// 今天已经说了几条。`day` 变了就归零。
    spoken_today: u32,
    day: u32,
    /// 冷却：键 → 下次最早可以说话的毫秒时间戳。
    cooldowns: HashMap<String, u64>,
    /// 文案轮换：键 → 下一句的序号。
    rotation: HashMap<String, usize>,
    pending: Option<Pending>,
    /// 最近说出去的那一句的编号。用来判断「被忽略」的回报是不是针对这一句。
    last_spoken: Option<String>,
    next_id: u64,
}

impl Default for ProactiveState {
    fn default() -> Self {
        Self {
            idle_threshold_ms: IDLE_THRESHOLD_MS,
            started: false,
            last_process: None,
            was_idle: false,
            ignored_streak: 0,
            spoken_today: 0,
            day: 0,
            cooldowns: HashMap::new(),
            rotation: HashMap::new(),
            pending: None,
            last_spoken: None,
            next_id: 0,
        }
    }
}

/// 判断这一刻要不要说话，要说就把那句话生成出来。
///
/// 纯函数：不读系统时钟、不碰 Tauri、不调模型，所以可以直接 `cargo test` 覆盖。
/// `state` 要可变：冷却、额度、轮换位置、上一次的前台程序都在里面。
///
/// **一次采样最多说一句。** 优先级：回到活跃 > 等停下来再说 > 闲置问候 > 程序切换。
/// 回到活跃排在最前是因为它「现在不说就永远不说」。
pub fn should_speak(signal: &Signal, state: &mut ProactiveState, now_ms: u64) -> Option<Utterance> {
    let idle_now = signal.idle_ms >= state.idle_threshold_ms;
    let was_idle = state.was_idle;
    let process_changed = signal.process_name != state.last_process;

    // 先记下本次采样：无论说不说都要记，否则同一段空闲会被反复当成新事件。
    state.last_process = signal.process_name.clone();
    state.was_idle = idle_now;
    reset_daily_budget(state, signal.local_day);

    if !state.started {
        // 第一帧没有「上一次」可比，只建立基线。
        state.started = true;
        return None;
    }

    if was_idle && !idle_now {
        // 回到活跃：定义就是「用户刚恢复输入」，所以不受「最近有输入」的延后。
        let set = LineSet {
            kind: Kind::Back,
            cooldown_key: "back".into(),
            rotation_key: "back".into(),
            lines: BACK_LINES,
            detail: None,
        };
        if let Some(utterance) = try_speak(state, set, signal, now_ms) {
            return Some(utterance);
        }
    }

    if let Some(utterance) = take_ready_pending(state, signal, now_ms) {
        return Some(utterance);
    }

    if !was_idle && idle_now {
        // 一次离开只说一次：看的是「刚进入空闲」这个跳变，不是「一直空闲」。
        let set = LineSet {
            kind: Kind::Idle,
            cooldown_key: "idle".into(),
            rotation_key: "idle".into(),
            lines: IDLE_LINES,
            detail: None,
        };
        if let Some(utterance) = try_speak(state, set, signal, now_ms) {
            return Some(utterance);
        }
    }

    if process_changed {
        if let Some(app) = classify(signal) {
            let set = LineSet {
                kind: Kind::App,
                cooldown_key: format!("app:{}", app.rule.category.key()),
                // 轮换按分类走：同一个分类下的几个程序（chrome / edge / firefox）
                // 共用一组文案，才不会每换一个程序都从头念第一句。
                rotation_key: app.rule.category.key().to_string(),
                lines: app.rule.lines,
                detail: app.detail,
            };
            if let Some(utterance) = try_speak(state, set, signal, now_ms) {
                return Some(utterance);
            }
        }
    }

    None
}

/// 回报一句气泡的结局。`acknowledged` = 用户点了它。
///
/// 没点（自己淡出、被菜单挤掉）就算「被忽略」一次；连续 [`IGNORED_STREAK_LIMIT`] 次
/// 之后同类冷却翻倍。返回 `false` 说明这个编号不是当前那句（重复回报、过期回报）。
pub fn dismiss(state: &mut ProactiveState, id: &str, acknowledged: bool) -> bool {
    if state.last_spoken.as_deref() != Some(id) {
        return false;
    }
    if acknowledged {
        state.ignored_streak = 0;
    } else {
        state.ignored_streak = state.ignored_streak.saturating_add(1);
    }
    state.last_spoken = None;
    true
}

fn try_speak(
    state: &mut ProactiveState,
    set: LineSet,
    signal: &Signal,
    now_ms: u64,
) -> Option<Utterance> {
    // 静默时段对所有触发一视同仁。
    if is_quiet_hour(signal.local_hour) || state.spoken_today >= DAILY_LIMIT {
        return None;
    }
    // 全屏静默只管状态型：§6.4 与阶段 B 的验收都要求「打开游戏时冒一句」，
    // 而游戏恰恰是最典型的全屏应用——所以事件型照说。
    if signal.fullscreen && matches!(set.kind, Kind::Idle | Kind::Back) {
        return None;
    }
    if !cooldown_passed(state, &set.cooldown_key, now_ms) {
        return None;
    }

    let text = next_line(state, &set.rotation_key, set.lines, set.detail.as_deref())?;
    state.next_id += 1;
    let utterance = Utterance {
        id: format!("{}-{}", set.cooldown_key, state.next_id),
        text,
        ttl_ms: TTL_MS,
    };

    if defers_for_recent_input(set.kind, signal) {
        // 「等停下来再说」：先存着，等下一次采样里用户停手了再发。
        state.pending = Some(Pending {
            utterance,
            kind: set.kind,
            cooldown_key: set.cooldown_key,
            process: signal.process_name.clone(),
            created_ms: now_ms,
        });
        return None;
    }

    mark_spoken(state, set.kind, &set.cooldown_key, &utterance, now_ms);
    Some(utterance)
}

fn take_ready_pending(
    state: &mut ProactiveState,
    signal: &Signal,
    now_ms: u64,
) -> Option<Utterance> {
    let pending = state.pending.clone()?;

    if now_ms.saturating_sub(pending.created_ms) > PENDING_TTL_MS {
        // 等太久了，那句话的场合已经过去。
        state.pending = None;
        return None;
    }
    if pending.process != signal.process_name {
        // 用户已经换到别的程序去了。
        state.pending = None;
        return None;
    }
    // 用户停手了，或者已经等过了一个采样周期（默认 5 秒）：补上那一句。
    // 只等一个周期是刻意的，理由见 `defers_for_recent_input`。
    let paused = signal.idle_ms >= RECENT_INPUT_MS;
    let waited = now_ms >= pending.created_ms.saturating_add(SAMPLE_INTERVAL_MS);
    if !paused && !waited {
        return None;
    }

    state.pending = None;
    if is_quiet_hour(signal.local_hour) || state.spoken_today >= DAILY_LIMIT {
        return None;
    }
    mark_spoken(
        state,
        pending.kind,
        &pending.cooldown_key,
        &pending.utterance,
        now_ms,
    );
    Some(pending.utterance)
}

/// 「最近 30 秒内有输入 → 延后」的实现（计划 §8.4）。
///
/// 计划写的是**延后**而不是「不说」，所以这里真的把它做成延后：条件满足时先把这句话存进
/// `pending`，稍后再补上；中途换了程序就作废。
///
/// **唯独「回到活跃」不延后。** 它的定义就是「用户刚刚恢复输入」，对它延后等于永远不说
/// 话——阶段 B 的验收项会直接失效。
///
/// 补发的时机见 [`take_ready_pending`]：用户停手（空闲 ≥ 30 秒）就立刻补上，
/// 但**最迟只等一个采样周期**。只等一个周期是刻意的：如果把「延后」理解成「等用户
/// 停手为止」，一个正盯着桌宠看的用户只要动鼠标就永远等不到那句话——设计文档 §6.5
/// 自己说过「看不到的功能等于不存在」。所以这里的延后是「不打断你这一下」，
/// 而不是「你不动我就不说」。
fn defers_for_recent_input(kind: Kind, signal: &Signal) -> bool {
    kind != Kind::Back && signal.idle_ms < RECENT_INPUT_MS
}

fn is_quiet_hour(hour: u32) -> bool {
    // 静默是「23:00 到次日 8:00」，也就是清醒时段（8:00–22:59）之外。
    !(QUIET_UNTIL_HOUR..QUIET_FROM_HOUR).contains(&hour)
}

fn cooldown_passed(state: &ProactiveState, key: &str, now_ms: u64) -> bool {
    match state.cooldowns.get(key) {
        Some(until) => now_ms >= *until,
        None => true,
    }
}

fn mark_spoken(
    state: &mut ProactiveState,
    kind: Kind,
    cooldown_key: &str,
    utterance: &Utterance,
    now_ms: u64,
) {
    // 连续被忽略 3 次 → 同类冷却翻倍。这是「会自己学会闭嘴」的那一条。
    let multiplier = if state.ignored_streak >= IGNORED_STREAK_LIMIT {
        2
    } else {
        1
    };
    let cooldown = if kind == Kind::App {
        APP_COOLDOWN_MS
    } else {
        STATE_COOLDOWN_MS
    };
    state
        .cooldowns
        .insert(cooldown_key.to_string(), now_ms + cooldown * multiplier);
    state.spoken_today = state.spoken_today.saturating_add(1);
    state.last_spoken = Some(utterance.id.clone());
}

fn reset_daily_budget(state: &mut ProactiveState, local_day: u32) {
    if state.day != local_day {
        state.day = local_day;
        state.spoken_today = 0;
    }
}

/// 挑下一句并推进轮换。含 `{detail}` 的文案在没有文件名时会被跳过，
/// 所以「同一张表里既有带文件名的、也有不带的」是允许的。
fn next_line(
    state: &mut ProactiveState,
    rotation_key: &str,
    lines: &'static [&'static str],
    detail: Option<&str>,
) -> Option<String> {
    let eligible: Vec<&'static str> = lines
        .iter()
        .copied()
        .filter(|line| !line.contains("{detail}") || detail.is_some())
        .collect();
    if eligible.is_empty() {
        return None;
    }
    let slot = state
        .rotation
        .entry(rotation_key.to_string())
        .or_insert(0usize);
    let line = eligible[*slot % eligible.len()];
    *slot = (*slot + 1) % eligible.len();
    Some(match detail {
        Some(detail) => line.replace("{detail}", detail),
        None => line.to_string(),
    })
}

// ---------- 调度 ----------

/// 引擎的运行态。**必须是独立的 newtype**：Rust 的 type 别名是透明的，
/// 裸别名会和别的窗口守卫撞成同一个类型，Tauri 的 `manage()` 第二次注册就在启动时 panic
/// （M1 交付时踩过一次，详见 `chat_window.rs`）。
#[derive(Default)]
pub struct ProactiveStore(pub Mutex<ProactiveState>);

/// 启动采样循环。
///
/// 线程里做的事很少：读开关 → 读可见性 → 采一次样 → 交给规则引擎。
/// **关掉开关或桌宠不可见时连采样都不做**（计划 §3.2、§8.4），所以关掉之后
/// 不会读前台窗口、不会有任何事件、更不会有任何模型调用——这一阶段本来就不调模型。
pub fn start(app: &AppHandle) {
    let handle = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(SAMPLE_INTERVAL_MS));
        if let Err(error) = tick(&handle) {
            eprintln!("[Rust] 主动互动采样失败：{error}");
        }
    });

    debug_overrides(app);
}

/// 开发期把空闲阈值临时改小，省得验一条「空闲触发」要真等 5 分钟。
///
/// 只在调试构建里读：`$env:YACHIYO_PROACTIVE_IDLE_MS = 10000` 后 `pnpm tauri dev`。
/// 发布版没有这个入口，阈值恒为 [`IDLE_THRESHOLD_MS`]。
fn debug_overrides(app: &AppHandle) {
    #[cfg(not(debug_assertions))]
    let _ = app;

    #[cfg(debug_assertions)]
    {
        let Some(value) = std::env::var("YACHIYO_PROACTIVE_IDLE_MS")
            .ok()
            .and_then(|value| value.trim().parse::<u64>().ok())
        else {
            return;
        };
        let value = value.max(1_000);
        let store = app.state::<ProactiveStore>();
        // 末尾的分号不能省：不加的话 match 的临时值会活到块尾，比 `store` 还晚析构。
        match store.0.lock() {
            Ok(mut state) => {
                state.idle_threshold_ms = value;
                println!("[Rust] 主动互动：空闲阈值被环境变量改成 {value} 毫秒（仅开发版）");
            }
            Err(error) => eprintln!("[Rust] 主动互动：改空闲阈值失败：{error}"),
        };
    }
}

fn tick(app: &AppHandle) -> Result<(), String> {
    let settings = desktop::get_settings(app)?;
    if !settings.proactive_enabled || !pet_is_visible(app) {
        return Ok(());
    }

    let signal = Signal::from(context::sample());
    #[cfg(debug_assertions)]
    log_process_change(&signal);

    let now_ms = clock::now_ms();
    // 锁只在这一小段里拿着：规则判断是纯计算，不该和别的线程抢太久。
    let store = app.state::<ProactiveStore>();
    let mut state = store.0.lock().map_err(|error| error.to_string())?;
    let utterance = should_speak(&signal, &mut state, now_ms);
    drop(state);

    let Some(utterance) = utterance else {
        return Ok(());
    };

    #[cfg(debug_assertions)]
    println!(
        "[Rust] 主动互动：说「{}」（{}）",
        utterance.text, utterance.id
    );
    broadcast::emit_if_open(app, broadcast::MAIN, "proactive-speak", &utterance)
}

/// 桌宠不可见时不做任何判断。隐藏与最小化都算。
fn pet_is_visible(app: &AppHandle) -> bool {
    let Some(window) = app.get_webview_window(broadcast::MAIN) else {
        return false;
    };
    window.is_visible().unwrap_or(true) && !window.is_minimized().unwrap_or(false)
}

/// 开发版的诊断日志：前台程序换了就记一笔，顺便说明它有没有被登记。
///
/// 「为什么没冒气泡」十有八九是因为那个程序没在映射表里，这句日志能直接看出来。
/// 打印的是**进程名**，不是窗口标题。
#[cfg(debug_assertions)]
fn log_process_change(signal: &Signal) {
    use std::sync::{Mutex, OnceLock};

    static LAST: OnceLock<Mutex<Option<String>>> = OnceLock::new();
    let Ok(mut last) = LAST.get_or_init(|| Mutex::new(None)).lock() else {
        return;
    };
    if *last == signal.process_name {
        return;
    }
    *last = signal.process_name.clone();

    let name = signal.process_name.as_deref().unwrap_or("（读不到）");
    match classify(signal) {
        Some(app) => println!(
            "[Rust] 前台切到 {name}（{}）{}",
            app.label(),
            app.detail
                .map(|detail| format!("，标题里的文件名：{detail}"))
                .unwrap_or_default()
        ),
        None => println!("[Rust] 前台切到 {name}（未登记，不说话）"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_700_000_000_000;

    fn signal(process: Option<&str>, idle_ms: u64) -> Signal {
        Signal {
            process_name: process.map(str::to_string),
            window_title: None,
            idle_ms,
            fullscreen: false,
            local_day: 20_260_915,
            local_hour: 14,
        }
    }

    /// 已经过了「第一帧只建立基线」那一步的状态。
    ///
    /// `process` 是**当前**前台的程序名：测试里最常验的就是「从它切到别的程序」。
    fn ready_state(process: &str) -> ProactiveState {
        let mut state = ProactiveState::default();
        state.started = true;
        state.last_process = Some(process.to_string());
        state
    }

    /// 用户已经停手、前台停在某个程序上：事件型触发在真实世界里的常见形态。
    ///
    /// 用 40 秒（而不是 0）是因为刚点完任务栏的那一瞬间属于「正在输入」，
    /// 那一句会被延后一个采样周期——那条规则另有测试。
    fn settled(process: &str) -> Signal {
        signal(Some(process), 40_000)
    }

    #[test]
    fn the_first_sample_only_builds_a_baseline() {
        let mut state = ProactiveState::default();
        // 启动时用户就在 VS Code 里：不该立刻来一句「编辑器打开了」。
        assert_eq!(
            should_speak(&signal(Some("Code.exe"), 0), &mut state, NOW),
            None
        );
        assert_eq!(state.last_process.as_deref(), Some("Code.exe"));
        // 基线建立之后，换程序才算事件。
        let utterance = should_speak(&settled("chrome.exe"), &mut state, NOW + 10_000);
        assert!(utterance.is_some(), "建立基线后换程序应该说话");
    }

    #[test]
    fn idle_greeting_fires_once_per_absence() {
        let mut state = ready_state("Code.exe");
        assert_eq!(
            should_speak(&signal(Some("Code.exe"), 4 * 60_000), &mut state, NOW),
            None,
            "不到 5 分钟不算离开"
        );
        let utterance = should_speak(
            &signal(Some("Code.exe"), 6 * 60_000),
            &mut state,
            NOW + 60_000,
        )
        .expect("离开 6 分钟应该问一句");
        assert!(IDLE_LINES.contains(&utterance.text.as_str()));
        assert_eq!(utterance.ttl_ms, TTL_MS);

        assert_eq!(
            should_speak(
                &signal(Some("Code.exe"), 7 * 60_000),
                &mut state,
                NOW + 70_000
            ),
            None,
            "一次离开只说一次"
        );
    }

    #[test]
    fn coming_back_says_hello_and_then_respects_the_cooldown() {
        let mut state = ready_state("Code.exe");
        should_speak(&signal(Some("Code.exe"), 6 * 60_000), &mut state, NOW).expect("闲置问候");

        let back = should_speak(&signal(Some("Code.exe"), 0), &mut state, NOW + 10_000)
            .expect("回到活跃应该说一句");
        assert!(BACK_LINES.contains(&back.text.as_str()));

        // 十分钟后又离开：状态型同类冷却 20 分钟，这一次不该说。
        let later = NOW + 10 * 60_000;
        assert_eq!(
            should_speak(&signal(Some("Code.exe"), 6 * 60_000), &mut state, later),
            None,
            "闲置问候还在冷却里"
        );
        assert_eq!(
            should_speak(&signal(Some("Code.exe"), 0), &mut state, later + 5_000),
            None,
            "回到活跃也还在冷却里"
        );
        // 冷却过后恢复。
        let after_cooldown = NOW + 21 * 60_000;
        assert!(
            should_speak(
                &signal(Some("Code.exe"), 6 * 60_000),
                &mut state,
                after_cooldown
            )
            .is_some(),
            "冷却结束后应该恢复"
        );
    }

    #[test]
    fn switching_to_an_unregistered_process_stays_silent() {
        let mut state = ready_state("Code.exe");
        assert_eq!(
            should_speak(&signal(Some("explorer.exe"), 0), &mut state, NOW),
            None,
            "未登记的应用只记录、不说话"
        );
        assert_eq!(state.last_process.as_deref(), Some("explorer.exe"));
    }

    #[test]
    fn switching_to_a_known_app_says_a_line_from_its_category() {
        let mut state = ready_state("explorer.exe");
        let utterance =
            should_speak(&settled("msedge.exe"), &mut state, NOW).expect("打开浏览器应该说话");
        let browser = find_rule("chrome.exe").expect("表里有浏览器");
        assert!(browser.lines.contains(&utterance.text.as_str()));
    }

    #[test]
    fn same_category_has_a_five_minute_cooldown() {
        let mut state = ready_state("explorer.exe");

        assert!(
            should_speak(&settled("Code.exe"), &mut state, NOW).is_some(),
            "第一次打开编辑器"
        );
        // 换个分类不受影响。
        assert!(
            should_speak(&settled("chrome.exe"), &mut state, NOW + 1_000).is_some(),
            "浏览器是另一个分类，各算各的冷却"
        );
        // 同一分类（编辑器）四分钟后再回来：冷却还没过。
        assert_eq!(
            should_speak(&settled("idea64.exe"), &mut state, NOW + 4 * 60_000),
            None,
            "同类事件 5 分钟内只说一次"
        );
        // 冷却过后可以再说。
        assert!(
            should_speak(
                &settled("sublime_text.exe"),
                &mut state,
                NOW + 5 * 60_000 + 1
            )
            .is_some(),
            "冷却结束后可以再说"
        );
    }

    #[test]
    fn quiet_hours_and_the_daily_budget_silence_everything() {
        let mut state = ready_state("explorer.exe");
        // 每次都换一个已登记的程序，确保被挡下来的是静默规则，而不是「程序没变」。
        let night_processes = [
            "Code.exe",
            "chrome.exe",
            "cloudmusic.exe",
            "GenshinImpact.exe",
        ];
        for (index, process) in night_processes.iter().enumerate() {
            let mut night = settled(process);
            night.local_hour = if index % 2 == 0 { 23 } else { 3 };
            assert_eq!(
                should_speak(&night, &mut state, NOW + index as u64 * 1_000),
                None,
                "{process} 在静默时段不该说话"
            );
        }

        // 8 点整已经可以说话。
        let mut morning = settled("Code.exe");
        morning.local_hour = 8;
        assert!(
            should_speak(&morning, &mut state, NOW + 10_000).is_some(),
            "8 点整已经可以说话"
        );

        // 额度用完就不说了：换分类绕开冷却，确保挡它的是额度。
        state.spoken_today = DAILY_LIMIT;
        let mut evening = settled("chrome.exe");
        evening.local_hour = 20;
        assert_eq!(
            should_speak(&evening, &mut state, NOW + 20_000),
            None,
            "今天的额度用完了"
        );
        // 第二天归零。
        let mut tomorrow = settled("cloudmusic.exe");
        tomorrow.local_hour = 20;
        tomorrow.local_day = 20_260_916;
        assert!(
            should_speak(&tomorrow, &mut state, NOW + 30_000).is_some(),
            "跨天之后额度恢复"
        );
        assert_eq!(state.spoken_today, 1);
    }

    #[test]
    fn ignored_streak_doubles_the_cooldown_and_clicking_resets_it() {
        let mut state = ready_state("explorer.exe");
        let first = should_speak(&settled("Code.exe"), &mut state, NOW).expect("第一句");

        // 气泡自己淡掉了：算被忽略一次。
        assert!(dismiss(&mut state, &first.id, false));
        assert_eq!(state.ignored_streak, 1);
        // 重复回报同一个编号不算数。
        assert!(!dismiss(&mut state, &first.id, false));
        assert_eq!(state.ignored_streak, 1);

        // 攒够 3 次之后，冷却从 5 分钟变成 10 分钟。
        state.ignored_streak = IGNORED_STREAK_LIMIT;
        let second = should_speak(&settled("idea64.exe"), &mut state, NOW + 6 * 60_000)
            .expect("冷却过了就能再说");
        assert_eq!(
            state.cooldowns.get("app:editor").copied(),
            Some(NOW + 6 * 60_000 + APP_COOLDOWN_MS * 2),
            "连续被忽略之后冷却翻倍"
        );
        // 用户点了一次：连续被忽略中断。
        assert!(dismiss(&mut state, &second.id, true));
        assert_eq!(state.ignored_streak, 0);
    }

    #[test]
    fn fullscreen_silences_the_state_triggers_but_not_the_app_event() {
        let mut state = ready_state("Code.exe");

        let mut fullscreen = signal(Some("Code.exe"), 6 * 60_000);
        fullscreen.fullscreen = true;
        assert_eq!(
            should_speak(&fullscreen, &mut state, NOW),
            None,
            "全屏时不说状态型的话"
        );

        // 但「打开了某个全屏程序」这件事是要说的：验收项就是「打开游戏冒一句」。
        let mut game = settled("GenshinImpact.exe");
        game.fullscreen = true;
        assert!(
            should_speak(&game, &mut state, NOW + 1_000).is_some(),
            "打开全屏游戏仍然应该冒一句"
        );
    }

    #[test]
    fn recent_input_delays_the_app_line_instead_of_dropping_it() {
        let mut state = ready_state("explorer.exe");

        // 用户刚点了任务栏切到编辑器：此刻还在动，先别说。
        assert_eq!(
            should_speak(&signal(Some("Code.exe"), 1_000), &mut state, NOW),
            None
        );
        assert!(state.pending.is_some(), "这一句应该被存下来等着");
        assert!(
            state.cooldowns.get("app:editor").is_none(),
            "延后不等于已说：不该占用冷却"
        );

        // 下一次采样就补上：不能拖到「用户停手为止」，
        // 否则一直动鼠标的人永远看不到这句话。
        let utterance = should_speak(
            &signal(Some("Code.exe"), 6_000),
            &mut state,
            NOW + SAMPLE_INTERVAL_MS,
        )
        .expect("下一个采样周期应该补上那一句");
        assert!(utterance.text.contains("编辑器") || utterance.text.contains("代码"));
        assert!(state.pending.is_none());
    }

    #[test]
    fn a_deferred_line_comes_earlier_if_the_user_pauses() {
        let mut state = ready_state("explorer.exe");
        should_speak(&signal(Some("Code.exe"), 1_000), &mut state, NOW);
        assert!(state.pending.is_some());

        // 才过 2 秒，但用户已经停手 40 秒 —— 立刻补上那一句。
        let utterance = should_speak(&signal(Some("Code.exe"), 40_000), &mut state, NOW + 2_000)
            .expect("用户停手就补上");
        assert!(utterance.text.contains("编辑器") || utterance.text.contains("代码"));
    }

    #[test]
    fn a_deferred_line_is_dropped_if_the_user_switches_away_first() {
        let mut state = ready_state("explorer.exe");
        should_speak(&signal(Some("Code.exe"), 1_000), &mut state, NOW);
        assert!(state.pending.is_some());

        // 下一帧前台已经换成别的程序：那一刻的「打开编辑器」已经过去了。
        assert_eq!(
            should_speak(
                &signal(Some("explorer.exe"), 40_000),
                &mut state,
                NOW + 5_000
            ),
            None,
            "换了程序就不再补发旧的那一句"
        );
        assert!(state.pending.is_none(), "过期的待发句子要清掉");
    }

    #[test]
    fn back_to_active_is_never_deferred() {
        let mut state = ready_state("Code.exe");
        state.was_idle = true;
        // 空闲时长刚掉到 2 秒（用户刚动了一下鼠标）——回到活跃照样说。
        let utterance = should_speak(&signal(Some("Code.exe"), 2_000), &mut state, NOW)
            .expect("回到活跃不受「最近有输入」的延后");
        assert!(BACK_LINES.contains(&utterance.text.as_str()));
    }

    #[test]
    fn lines_rotate_so_the_same_line_does_not_repeat() {
        let mut state = ready_state("explorer.exe");

        // 每轮都先切到未登记的程序、再切回编辑器：这样每次都是新事件，
        // 中间隔开 6 分钟绕开同类冷却（冷却本身另有测试）。
        let mut seen: Vec<String> = Vec::new();
        for step in 0..3u64 {
            let now = NOW + step * 6 * 60_000;
            should_speak(&settled("explorer.exe"), &mut state, now);
            seen.push(
                should_speak(&settled("Code.exe"), &mut state, now + 1_000)
                    .expect("说话")
                    .text,
            );
        }
        assert_ne!(seen[0], seen[1], "相邻两次不该说同一句");
        assert_eq!(seen[0], seen[2], "轮换到头以后回到第一条");
    }

    #[test]
    fn every_template_obeys_the_writing_rules() {
        let mut tables: Vec<(&str, &[&str])> = vec![("闲置", IDLE_LINES), ("回来", BACK_LINES)];
        for rule in APP_RULES {
            tables.push((rule.process, rule.lines));
        }
        for (name, lines) in tables {
            assert!(lines.len() >= 3, "{name} 至少要有 3 条轮换（计划 §8.5）");
            for line in lines {
                assert!(
                    !line.contains('?') && !line.contains('？'),
                    "{name} 的文案必须是陈述句：{line}"
                );
                let width = line.replace("{detail}", "").chars().count();
                assert!(
                    width <= 20,
                    "{name} 的文案太长（不含文件名应 ≤ 20 字，实际 {width}）：{line}"
                );
            }
        }
    }

    #[test]
    fn the_table_has_no_duplicate_processes() {
        for (index, rule) in APP_RULES.iter().enumerate() {
            for other in APP_RULES.iter().skip(index + 1) {
                assert!(
                    !rule.process.eq_ignore_ascii_case(other.process),
                    "进程名重复：{}",
                    rule.process
                );
            }
        }
    }

    #[test]
    fn window_titles_are_only_used_for_editors() {
        // 编辑器：从标题里取文件名。
        let editor = classify(&Signal {
            process_name: Some("Code.exe".into()),
            window_title: Some("pomodoro.rs - yachiyo-desktop - Visual Studio Code".into()),
            ..signal(None, 0)
        });
        assert_eq!(
            editor.and_then(|matched| matched.detail).as_deref(),
            Some("pomodoro.rs")
        );

        // 浏览器：标题整条丢弃。「招商银行 - Chrome」里没有任何东西能流出去。
        let browser = classify(&Signal {
            process_name: Some("chrome.exe".into()),
            window_title: Some("招商银行 - Google Chrome".into()),
            ..signal(None, 0)
        });
        assert!(browser.is_some());
        assert_eq!(browser.and_then(|matched| matched.detail), None);

        // 聊天软件根本不在表里，连归类都没有。
        assert!(classify(&Signal {
            process_name: Some("WeChat.exe".into()),
            window_title: Some("张三 - 微信".into()),
            ..signal(None, 0)
        })
        .is_none());
    }

    #[test]
    fn file_names_are_only_taken_from_the_first_segment() {
        assert_eq!(
            extract_file_name_from_title("main.rs – 我的项目 – IntelliJ IDEA"),
            Some("main.rs".into())
        );
        // 太长的文件名会被截断，免得撑爆气泡。
        assert_eq!(
            extract_file_name_from_title(&format!("{}.rs - 项目 - Code", "a".repeat(40))),
            Some("a".repeat(DETAIL_MAX_CHARS))
        );
        assert_eq!(
            extract_file_name_from_title("Visual Studio Code"),
            None,
            "标题里没有分隔符就不知道文件是什么，退回不带文件名的那几句"
        );
        assert_eq!(extract_file_name_from_title(""), None);
    }
}
