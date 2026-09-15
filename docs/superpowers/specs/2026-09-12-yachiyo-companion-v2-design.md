# 八千代桌宠 v2：番茄钟 · 养成 · AI Agent · 工作区对话（设计方案与实施计划）

日期：2026-09-12。状态：待用户确认第十一节关键决策后进入 Phase 7。
（fork 上 **Phase 7 已实现**，见第七节完成记录；第十一节的决策点仍未确认——Phase 7 是纯底座，不依赖其中任何一条。）
前置：[最小实现设计](2026-09-10-yachiyo-mvp-design.md)、[MVP 计划](../plans/2026-09-10-yachiyo-mvp.md)、[气泡菜单计划](../plans/2026-09-11-bubble-menu.md)。

---

## 一、目标

在已验证的桌宠底座上增加四件事，并且让它们相互咬合，而不是四个孤立功能：

1. **番茄钟**：Rust 侧计时，隐藏/最小化/睡眠都不中断；托盘与桌宠都能控制。
2. **养成系统**：专注产生经验，经验带来等级与心情；养成反过来改变桌宠的表现（表情、语气、提示密度）。
3. **AI Agent**：不只是聊天，而是能调用工具的 Agent——能读写工作区文件，也能操作桌宠自身（切表情、开始番茄钟、查成长数据）。
4. **工作区与对话**：对话绑定到一个明确的本地目录，Agent 在该目录边界内工作；对话与会话持久化、可回看、可中断。

一句话概括 v2：把八千代从「会做表情的桌宠」变成「陪你专注、能帮你干活、并且记得你」的桌面伙伴。

**与 v1 的关系**：MVP 设计的非目标里明确写了「不实现聊天、语音、长期记忆」。本方案正是那份文档中预留的成长空间（「后续聊天模块可以调用现有表情入口」「对话上下文单独存储，不塞入 Live2D 渲染模块」）的正式展开。

---

## 二、现状盘点

### 已经能直接复用的

| 能力 | 现有实现 | v2 里怎么用 |
|---|---|---|
| 双向通信模式 | `app/src/api/pet.js` 集中封装 invoke/listen | 新增 `pomodoro.js`、`growth.js`、`workspace.js`、`agent.js` 沿用同一格式 |
| Rust 单一事实源 | `settings.rs` 的 `Mutex<PetSettings>` + `revision` | 番茄钟、养成、会话全部遵循「Rust 端提交后广播快照」 |
| 快照版本比较 | `petSettings.js` 的 `latestSettings`，防旧事件覆盖新值 | 所有新增快照沿用同一比较逻辑 |
| 单例窗口 | `settings_window.rs` 的 `Mutex<()>` 创建守卫 | 抽成 `window_manager.rs` 给对话窗口复用 |
| 统一错误上报 | `desktop.rs` 的 `report_error` + `pet-desktop-error` 事件 | Agent/工作区的错误也走这条通道 |
| 测试方式 | `node --test` + `cargo test`，纯函数优先 | 状态机、结算、裁剪、路径校验都写成纯函数 |
| 渲染暂停策略 | `renderPolicy.js` + Rust 侧 ticker 控制 | 番茄钟必须**绕开**它，见 4.1 |
| 权限白名单 | `capabilities/*.json` 只放开模型目录的只读 | 工作区访问不走前端 fs，见 4.5 |

### 必须先补的缺口（它们决定实现顺序）

| 编号 | 缺口 | 现状 | 为什么阻塞新功能 |
|---|---|---|---|
| G1 | **没有持久化** | `PetSettings` 只存在内存，Task 4 明确写「重启恢复默认值」 | 养成的等级、进行中的番茄钟、对话记录都必须跨重启 |
| G2 | **时间与渲染耦合** | 模型更新由 Pixi ticker 驱动，隐藏/最小化会停 | 番茄钟一旦挂到 ticker 上，隐藏窗口就会停表——这是最容易踩的坑 |
| G3 | **没有网络与凭据层** | 项目零网络依赖 | Agent 需要 HTTP 客户端、SSE 解析、密钥安全存储 |
| G4 | **没有目录访问边界** | fs 权限只覆盖模型资源 | 工作区需要「用户显式授权 + Rust 侧路径校验」的模型 |
| G5 | **主窗口不适合对话** | 340x440、`resizable: false`、透明无边框、置顶 | 聊天 UI 必须另开窗口，否则会把桌宠的形状约束撑坏 |

**结论**：Phase 7 先把 G1、G2 解决掉，这是纯投资、没有可见功能，但不做就会在 Phase 8 返工。

---

## 三、总体架构

```
┌─ Vue 窗口层 ───────────────────────────────────────────────┐
│ main 桌宠 340x440        settings 360x360+      chat 720x560 │
│ 表情·气泡菜单·倒计时徽标   设置/成长/专注配置     工作区·对话·工具卡片 │
└──────────────────┬─────────────────────────────────────────┘
                   │ invoke / listen（薄封装，命令名集中在 src/api/）
┌──────────────────┴─ Rust 服务层 ────────────────────────────┐
│ store.rs     原子持久化 + schemaVersion + 损坏回退            │
│ clock.rs     可注入时钟（测试用）                             │
│ pomodoro.rs  纯状态机 + 独立后台任务                          │
│ growth.rs    经验/等级/心情结算（纯函数 + 惰性衰减）           │
│ workspace.rs 工作区注册 + 路径边界校验                        │
│ fs_tools.rs  受限文件工具（给 Agent 用，不暴露给前端）          │
│ provider/    OpenAI 兼容 · Anthropic · Fake（测试）           │
│ agent.rs     Agent 循环 + 工具注册表 + 流式转发                │
│ secret.rs    keyring 封装（密钥不出 Rust）                    │
│ db.rs        rusqlite：会话与消息                             │
│ broadcast.rs 统一事件广播                                     │
└─────────────────────────────────────────────────────────────┘
```

### 五条贯穿全局的原则

1. **Rust 是唯一事实源**：任何状态变更都先在 Rust 完成并落盘，再广播事件；前端只做视图与乐观更新，不持有唯一副本。
2. **定时用绝对时间戳**：存 `deadline`，不存 `remaining`；不累加计数，避免漂移与睡眠后错乱。
3. **网络与文件 IO 只在 Rust 侧**：前端拿不到 API key，也不开放 fs scope。
4. **纯逻辑写成纯函数**：状态迁移与数值结算不依赖 Tauri、不依赖系统时间，方便单测。
5. **不引入新框架**：继续用 composables，不引入 Pinia / Vue Router / UI 组件库（与 v1 约束一致）。状态确实膨胀时再单独评估。

---

## 四、关键设计决策

### 4.1 时钟：绝对时间戳 + 独立后台任务

这是整个 v2 里**最重要的一个决策**。

```rust
// pomodoro.rs —— 只存 deadline，不存 remaining
pub struct PomodoroState {
    pub phase: Phase,            // idle | focus | shortBreak | longBreak
    pub running: bool,
    pub started_at: Option<u64>, // Unix 毫秒
    pub deadline: Option<u64>,   // Unix 毫秒；恢复与睡眠后据此重算
    pub paused_ms: u64,          // 累计暂停时长，暂停不计入专注
    pub completed_focus: u32,
    pub revision: u64,
}
```

- 后台任务用 `tauri::async_runtime::spawn`，每 1 秒检查一次是否越过 `deadline`。
- **不挂在 Pixi ticker 上**，也**不用依赖窗口可见性的 `setInterval`**。窗口隐藏、最小化、页面不可见时番茄钟必须继续走。
- 睡眠/休眠唤醒后：立刻用 `SystemTime::now()` 与 `deadline` 比较，一次性补齐状态（例如已经过去 40 分钟 → 专注早已完成），而不是「醒了才开始继续数」。
- 事件分两类：`pomodoro-changed`（状态变更，低频，带 `revision`）与 `pomodoro-tick`（每秒一次，只为驱动 UI 刷新）。前端本地插值显示，避免高频 DOM 更新。
- `clock.rs` 提供 `now_ms()`，测试时注入固定值，这样状态机测试不需要 `sleep`。

**明确禁止**：把番茄钟写进 `live2d/controller.js` 的 ticker，或在 Vue 里用 `setInterval` 作为唯一计时源。

### 4.2 持久化：原子写 + 版本号 + 损坏回退

- 位置：Tauri 的 `app_data_dir`（不写程序目录，避免权限与更新冲突）。
- 文件划分：
  - `settings.json`：现有桌宠设置（顺带修掉「重启恢复默认」）
  - `profile.json`：养成数据
  - `pomodoro.json`：进行中的会话快照（崩溃恢复用）
  - `sessions.db`：对话与会话（SQLite）
- 原子写：写 `*.tmp` → 落盘 → `rename` 覆盖；覆盖前保留一份 `*.bak`。
- 读取顺序：主文件 → 解析失败则读 `.bak` → 都失败则用默认值并 `report_error`，**不让程序起不来**。
- 每个文件带 `schemaVersion`，迁移写在单一函数里，便于以后加字段。
- **为什么对话单独用 SQLite**：单条会话可能累积上万条消息，需要按会话分页查询和事务；JSON 全量重写在这个量级不可接受。用 `rusqlite`（`bundled` feature，避免依赖系统 sqlite），数据库只被 Rust 访问，前端通过命令分页取。

### 4.3 番茄钟

状态机做成纯函数，`transition(state, event, now_ms) -> state`：

| 事件 | 说明 |
|---|---|
| `start` | 从 idle 进入 focus；记录 `started_at` 与 `deadline` |
| `pause` / `resume` | 累计 `paused_ms`，`deadline` 顺延 |
| `cancel` | 回到 idle，静默记录，**不扣分** |
| `complete` | 专注结束 → 结算经验 → 进入 shortBreak/longBreak |
| `tick` | 只做「是否越过 deadline」的判定 |

默认值（可在设置窗口改）：专注 25 分钟 / 短休 5 分钟 / 长休 15 分钟 / 每 4 个专注换长休。
其他规则：暂停期间不计入专注时长；取消给 0 经验但保留已完成计数；同一会话的经验结算用 `sessionId` 做幂等键，防止重复入账。

**崩溃恢复**：`pomodoro.json` 记录进行中的会话，重启后若发现未结束的会话，提示「上次专注还剩 12 分钟，继续 / 结算 / 放弃」，不静默丢数据。

**入口**：
- 托盘新增「番茄钟」子菜单：开始专注 / 暂停 / 继续 / 放弃；tooltip 显示剩余时间。
- 设置窗口新增「专注」分区：时长、长短休、是否自动开始下一个、是否联动表情。
- 桌宠侧：角色旁一个 HTML 倒计时徽标（不新增气泡菜单球位）。

**桌宠联动（受限于现有资源）**：模型只有 `smile / squint / tears / teardrop` 四个表情，**没有 `.motion3.json` 动作文件**，所以不要承诺「角色做出专注动画」。可做的只有：开始 → `squint`，完成 → `smile`，进行中每 5 分钟一次轻微提示（可关闭）。如果以后想加真实动作，需要补动作资源，属于本方案之外。

### 4.4 养成系统

**数据模型（`profile.json`）**：

```json
{
  "schemaVersion": 1,
  "level": 3,
  "exp": 240,
  "mood": 68,
  "totalFocusMinutes": 615,
  "streakDays": 5,
  "expToday": 125,
  "dayKey": "2026-09-12",
  "lastDecayAt": 1757654321000,
  "unlocked": ["pet:expression-smile", "bubble:gold"]
}
```

**经验规则（v1 建议值，均可配）**：

| 行为 | 经验 |
|---|---|
| 完成专注，按实际专注分钟 | 每分钟 1 |
| 完成短休 | 2 |
| 完成长休 | 5 |
| 同日连续第 n 个专注 | `min(2n, 10)` 加成 |
| 每日上限 | 300（防刷） |
| 提前放弃 | 0 |

**等级曲线**：升到 L(n+1) 需要 `100 + 50 * (n - 1)` 累计经验。

**心情机制**：
- 区间 `[0, 100]`，初始 70。
- 完成专注 +8（封顶 100）。
- 离线衰减：`floor(离线小时数) * 1`，单次最多 -24。
- **下限 20**：不做「生病 / 离家出走 / 死亡」。低心情只表现为话少、待机表情安静、提示频率下降。这是有意为之的产品取向——桌宠不该因用户忙而惩罚用户。
- 衰减**惰性结算**：只在读取时按 `lastDecayAt` 补算，不跑常驻循环。

**跨天**：`dayKey` 变化时重置 `expToday`；当天至少完成 1 个专注则 `streakDays + 1`，否则归 1。

**展示**：设置窗口「成长」分区（等级进度条、心情、累计专注、连续天数、解锁列表）；气泡菜单的专注面板里放一份摘要。

### 4.5 工作区

**定义**：工作区 = 用户显式选择的本地目录 + 一组能力开关 + 该目录下的对话历史。

- 创建：用 `tauri-plugin-dialog` 调系统目录选择器 → `canonicalize` → 落盘记录。用户没有显式选择过的目录，一律不可访问。
- **权限模型（关键）**：**不放宽前端 fs scope**。所有文件访问都经自定义 Rust 命令，命令内每次校验：

```rust
let real = std::fs::canonicalize(requested)?;           // 先解掉符号链接与 ..
if !real.starts_with(&workspace.root_canonical) {       // 再比较真实路径前缀
    return Err("路径超出工作区范围".into());
}
```

  这样即使前端被注入，也无法越界；而 `capabilities/*.json` 继续保持最小白名单。
- 能力开关：读目录/读文件默认开；**写文件默认关**，开启后仍需逐次确认（见 4.6 的确认机制）。执行命令不在 v2 范围内。
- 默认忽略：`.git`、`node_modules`、`target`、`dist`，以及 `.gitignore` 中的条目，减少上下文噪音。
- 不做 RAG / 向量库：先用 `grep` + 用户显式指定文件。等真的遇到「找不到」的问题再评估，避免过度设计。

### 4.6 AI Agent

**供应商抽象**：

```rust
// provider/mod.rs —— 新增供应商只需实现这个 trait
pub trait ChatProvider {
    fn stream(
        &self,
        messages: Vec<Message>,
        tools: Vec<ToolSchema>,
        cancel: CancelToken,
    ) -> Result<ChatStream, ProviderError>;
}
```

先实现 **OpenAI 兼容**（`/v1/chat/completions` + SSE 流式），这一条同时覆盖 OpenAI、多数国内兼容服务与本地 Ollama；保留 Anthropic Messages API 的适配位。

**凭据**：用 `keyring` 写入 Windows 凭据管理器。API key 只在 Rust 侧读取，**不回传前端**（前端只能问 `has_api_key()`）。日志、导出、错误信息一律脱敏。

**网络**：`reqwest` + `rustls`（不依赖系统 OpenSSL），带连接/整体超时、仅对幂等请求重试、支持取消。

**Agent 循环（在 Rust 侧，不在前端）**：

1. 组装消息：系统提示 + 工作区摘要 + 最近上下文 + 用户输入
2. 调模型（流式）
3. 若有工具调用 → 在 Rust 执行工具 → 结果回填 → 回到步骤 2（**最大轮数上限，例如 8**，防止死循环）
4. 结束 → 落库 → 广播结束事件

放在 Rust 的好处：前端刷新/切窗口不中断；取消逻辑只有一处；工具不会拿到不该拿的权限。

**工具注册表**（单一注册点，每个工具声明名字、描述、JSON schema、是否只读、是否需要用户确认）：

| 工具 | 归属 | 说明 |
|---|---|---|
| `set_expression` | 桌宠 | 复用现有表情白名单校验 |
| `get_pomodoro_status` | 番茄钟 | 只读 |
| `start_pomodoro` | 番茄钟 | 需确认 |
| `get_growth_summary` | 养成 | 只读 |
| `list_files` | 工作区 | 只读，受深度与条数限制 |
| `read_file` | 工作区 | 只读，单文件上限 |
| `grep` | 工作区 | 只读，优先用搜索代替整目录读取 |
| `write_file` | 工作区 | **需确认**；工作区未开写权限时直接拒绝 |

**流式协议（事件）**：

| 事件 | 载荷 |
|---|---|
| `agent-stream-started` | `{ sessionId, messageId }` |
| `agent-stream-delta` | `{ sessionId, messageId, text }` |
| `agent-tool-call` | `{ sessionId, messageId, name, args }` |
| `agent-tool-result` | `{ sessionId, messageId, name, ok, summary }` |
| `agent-stream-finished` | `{ sessionId, messageId, usage }` |
| `agent-stream-failed` | `{ sessionId, messageId, error }` |

前端按 `messageId` 聚合，并按约 50ms 节流刷新 DOM（token 级事件直接驱动渲染会明显掉帧，也会干扰 Live2D 的 30 FPS 预算）。

**上下文预算**：token 估算保守上取整（中文按字符数 /1.5，英文按 /4），预算取模型窗口的 70%。超出时按「系统提示 > 最近 3 轮 > 摘要 > 更早历史」的顺序裁剪；更早的部分折叠为摘要（v1 先做截断 + 标注，摘要可后置）。

**失败与降级**：无 key / 无网络 → 明确报错，不静默失败；超时 → 可重试；工具执行失败 → 把错误文本回填给模型让它自行调整，而不是直接崩掉整个会话。

### 4.7 对话窗口

- 第三个窗口 `chat`：单例，复用 `settings_window.rs` 的创建守卫（建议抽成 `window_manager.rs`）。
- 尺寸 720x560，可缩放、有边框、不透明；**不加载 Live2D Core**，保持启动开销与现有性能约束。
- 布局：左侧工作区/会话列表，右侧消息流 + 输入框 + 工具调用卡片（显示「正在读取 pomodoro.rs」这类过程）。
- 持久化（SQLite）：
  - `sessions(id, workspace_id, title, created_at, updated_at, provider, model)`
  - `messages(id, session_id, role, content, tool_calls_json, created_at, token_usage)`
- 多窗口同步：chat 窗口是唯一编辑者；main / settings 只读展示摘要，通过事件广播更新。
- 交互：流式中可中断（Stop → `cancel_stream`）；失败消息可重试；消息可复制。

### 4.8 气泡菜单的容纳问题（必须先定的冲突点）

现有菜单是**固定 5 个球位**、宽度常量 `COMPACT_WIDTH = 340` / `EXPANDED_WIDTH = 480`、主窗口 `resizable: false`。新增入口会直接撞上这个约束。三个方案：

| 方案 | 做法 | 代价 |
|---|---|---|
| **A（推荐）** | 一级扩到 6 个球：表情 / 专注 / 成长 / 对话 / 设置 / 更多（更多里收纳 置顶、隐藏、退出） | 需重算 `menu_layout.rs` 的几何与紧凑排布，重写相关测试 |
| B | 保持 5 个球，所有新功能入口都进对话窗口；桌宠侧只留倒计时徽标 | 发现性差，桌宠「不像有这些功能」 |
| C | 一级菜单分页 | 交互复杂，与「单击勿误触」的既有约定冲突 |

推荐 A：菜单仍在既有的展开/紧凑两套排布里，几何变化是可控的数学改动，而 B 会让新功能藏得太深。

---

## 五、新增 IPC 协议清单

| 命令 | 参数 | 返回 | 备注 |
|---|---|---|---|
| `get_pomodoro_state` | — | `PomodoroState` | 含 `revision` |
| `start_pomodoro` | `{ phase?, minutes? }` | `PomodoroState` | |
| `pause_pomodoro` / `resume_pomodoro` | — | `PomodoroState` | |
| `cancel_pomodoro` | — | `PomodoroState` | 不扣分 |
| `update_pomodoro_settings` | `{...}` | `PomodoroState` | 时长与联动开关 |
| `get_growth_summary` | — | `GrowthSummary` | 惰性结算衰减 |
| `list_workspaces` / `add_workspace` / `remove_workspace` | `{ id? }` | `Workspace[]` | 添加走系统目录选择器 |
| `set_workspace_permissions` | `{ id, allowWrite }` | `Workspace` | |
| `list_providers` | — | `ProviderInfo[]` | 含是否有 key |
| `set_provider_config` | `{ provider, model, baseUrl }` | `ProviderConfig` | 不含密钥 |
| `set_api_key` | `{ provider, key }` | `{ ok }` | 写入 keyring，不回传 |
| `has_api_key` | `{ provider }` | `bool` | |
| `create_session` / `list_sessions` | `{ workspaceId }` | `Session[]` | |
| `get_messages` | `{ sessionId, before?, limit }` | `Message[]` | 分页 |
| `send_message` | `{ sessionId, text }` | `{ messageId }` | 内容靠流式事件 |
| `cancel_stream` | `{ sessionId }` | `{ ok }` | |

事件：`pomodoro-changed`、`pomodoro-tick`、`growth-changed`、`workspace-changed`、`agent-stream-*`（见 4.6）。

沿用现有约定：命令返回**新快照 + revision**；事件名与命令名集中在 `src/api/*.js`；错误暂时仍是字符串，后续可升级为结构化错误码。

---

## 六、文件与目录变更

```text
app/src/
├─ api/            pet.js（保持）、pomodoro.js、growth.js、workspace.js、agent.js
├─ composables/    usePomodoro.js、useGrowth.js、useWorkspace.js、useChat.js、useAgentStream.js
├─ components/     PomodoroBadge.vue、PomodoroPanel.vue、GrowthPanel.vue
│                  ChatWindow.vue、MessageList.vue、ToolCallCard.vue、WorkspacePicker.vue
└─ main.js         按 ?view=settings / ?view=chat 分流入口

app/src-tauri/src/
├─ store.rs         原子读写 + schemaVersion + 损坏回退
├─ clock.rs         可注入时钟
├─ broadcast.rs     统一事件广播
├─ pomodoro.rs      纯状态机 + 后台任务
├─ growth.rs        经验/等级/心情结算
├─ workspace.rs     工作区注册 + 路径边界校验
├─ fs_tools.rs      受限文件工具
├─ provider/        mod.rs、openai.rs、anthropic.rs、fake.rs
├─ agent.rs         Agent 循环 + 工具注册表 + 流式转发
├─ secret.rs        keyring 封装
├─ db.rs            rusqlite：连接、迁移、会话与消息 CRUD
└─ window_manager.rs  单例窗口抽象（settings 与 chat 共用）
```

**注意**：`app/package.json` 的 `test` 脚本目前是硬编码目录列表（`src/live2d/*.test.js src/composables/*.test.js src/interactions/*.test.js`），新增测试目录时必须同步修改，否则新测试不会被跑到。

新增 Rust 依赖（建议值，实际以锁文件为准）：`reqwest`（`rustls-tls`, `stream`）、`futures-util`、`rusqlite`（`bundled`）、`keyring`、`tauri-plugin-dialog`、`tokio-util`（取消令牌）、`thiserror`。

---

## 七、实施阶段

每个阶段都要求：可运行、有测试、有验收，且**一次只推进一个阶段**。

### Phase 7：底座（持久化 + 时钟 + 广播）

**目标**：没有新功能，但把 G1、G2 解决掉。

**文件**：新增 `store.rs`、`clock.rs`、`broadcast.rs`；修改 `settings.rs`、`desktop.rs`、`lib.rs`。

- [x] `store.rs`：`load<T: DeserializeOwned + Default>` / `save<T: Serialize>`，原子写 + `.bak` + 解析失败回退默认值，并 `report_error`。
- [x] 把 `PetSettings` 接入持久化：启动读取，修改后写入。**验证「重启后设置保留」**（这是 v1 遗留的已知缺口）。
- [x] 为损坏文件写测试：主文件内容非法 → 回退 `.bak`；两者都非法 → 回退默认值且不 panic。
- [x] `clock.rs`：`now_ms()` 可注入，测试里替换为固定值。
- [x] `broadcast.rs`：把「发往 main / settings」的重复代码收成一处，新窗口只需注册 label。
- [x] 验收：改帧率 → 退出 → 重启，帧率保持；手工把 `settings.json` 改坏，程序仍能启动并给出错误提示。
- [ ] 学习练习：用户手动编辑 `settings.json` 里的 `maxFps`，观察重启后的行为与校验。

> **Phase 7 完成记录（2026-09-15，实现于 fork）**
>
> 改动：新增 `store.rs` / `clock.rs` / `broadcast.rs`，修改 `settings.rs` / `desktop.rs` / `commands.rs` / `lib.rs` / `Cargo.toml`（`serde_json` 由可选依赖改为常规依赖）。
> 测试：`cargo test` 27 项全过（新增 15 项），`cargo clippy --all-targets -- -D warnings` 与 `cargo fmt --check` 干净。
>
> **实现时定的三件事**（都偏离或细化了原文，供复核）：
>
> 1. **落盘的文件用信封格式**：`{"schemaVersion": 1, "data": {...}}`。`schemaVersion` 不进业务结构体，前端快照不会多出无关字段；将来 `profile.json` / `pomodoro.json` 共用同一套读写与版本判断。
> 2. **`revision` 与 `visible` 不落盘**。`revision` 是本次运行内的次序标记；`visible` 是会话状态——启动一律可见，否则托盘创建失败时窗口既不在屏幕上也没法找回。因此文件里只有 `maxFps` 与 `alwaysOnTop` 两项，正好是「跨重启有意义」的那部分。
> 3. **文件损坏或值非法时保留现场**：回退到默认值并上报，但**不**覆盖用户改过的那一份。只有「文件不存在」才写默认值（让文件可被发现、可被手工编辑）。`maxFps=60` 这类非法值在读取时重新校验，回退默认值并说明原因。
>
> **两处补充**（原文没写，实现时认为必要）：
>
> - 读取时剥掉 UTF-8 BOM：这个文件是给人改的，部分编辑器保存时带 BOM，不处理就会「只改了一个数字却看到设置被重置」。
> - 启动期（页面挂载前）的提示先进 `StartupNotices` 队列，由 `Builder::on_page_load` 在主窗口加载完成后补发。`setup` 里直接 emit 的事件没有监听者，会被丢掉。
>
> **未端到端验证**：`.bak` 回退只有单元测试覆盖。`.bak` 只在设置真正变更时才产生（需要托盘或 UI 操作），本轮用「改文件 + 重启」无法触发它。

### Phase 8：番茄钟

**目标**：一个隐藏窗口也不会停的番茄钟。

**文件**：新增 `pomodoro.rs`、`src/api/pomodoro.js`、`src/composables/usePomodoro.js`、`src/components/PomodoroBadge.vue`、`PomodoroPanel.vue`；修改 `tray.rs`、`SettingsWindow.vue`、`menu_layout.rs`（若选方案 A）、`style.css`。

- [ ] 先写状态机测试（不依赖真实时间，用 `clock::now_ms` 注入）：正常走完一轮、暂停/继续后截止时间正确顺延、取消回到 idle、跨 deadline 的 `tick` 只完成一次、睡眠 40 分钟后一次性补齐。
- [ ] 实现状态机与后台任务；确认任务与 Pixi ticker 完全无关。
- [ ] `pomodoro-changed` + `pomodoro-tick` 事件；前端倒计时徽标。
- [ ] 托盘子菜单：开始 / 暂停 / 继续 / 放弃，tooltip 显示剩余时间。
- [ ] 设置窗口「专注」分区。
- [ ] 表情联动：开始 → `squint`，完成 → `smile`（可关闭）。
- [ ] 崩溃恢复：`pomodoro.json` 未结束会话的重启提示。
- [ ] 验收：开始 25 分钟专注 → 隐藏窗口 → 等待 → 托盘恢复，剩余时间正确；笔记本合盖 5 分钟再唤醒，剩余时间正确；完成后桌宠切 `smile`。
- [ ] 学习练习：用户改一次专注时长默认值，并解释为什么存 `deadline` 而不是存 `remaining`。

### Phase 9：养成系统

**文件**：新增 `growth.rs`、`src/api/growth.js`、`src/composables/useGrowth.js`、`src/components/GrowthPanel.vue`；修改 `pomodoro.rs`（完成时结算）、`SettingsWindow.vue`。

- [ ] 先写纯函数测试：经验计算、每日上限、同日连续加成、幂等（同一 `sessionId` 重复结算不叠加）、心情衰减（含封顶 -24 与下限 20）、跨天重置与 streak。
- [ ] 实现结算与落盘；`growth-changed` 事件。
- [ ] 设置窗口「成长」分区：等级、经验进度、心情、累计专注、连续天数、解锁列表。
- [ ] 低心情影响表现（提示密度 / 表情选择倾向），**不做惩罚性表现**。
- [ ] 验收：完成一个 25 分钟专注 → 经验按分钟入账并显示；把 `lastDecayAt` 手工改到 30 小时前 → 重载后心情按 -24 结算；把系统日期调后一天 → `expToday` 重置。
- [ ] 学习练习：用户调整经验系数，观察等级进度变化。

### Phase 10：工作区

**文件**：新增 `workspace.rs`、`fs_tools.rs`、`src/api/workspace.js`、`src/composables/useWorkspace.js`、`src/components/WorkspacePicker.vue`；修改 `Cargo.toml`（加 dialog 插件）、`capabilities/*.json`、`lib.rs`。

- [ ] 工作区注册：系统目录选择器 → `canonicalize` → 落盘；重复添加同一目录时复用记录。
- [ ] **路径边界测试（安全关键）**：`../` 逃逸被拒、指向工作区外的符号链接被拒、大小写与分隔符变体被正确规范化、合法子路径通过。
- [ ] 受限文件工具：`list_files`（限深度与条数）、`read_file`（限单文件大小）、`grep`；统一忽略规则。
- [ ] 确认前端 fs 权限**没有**被放宽（检查 `capabilities/*.json` 的 diff）。
- [ ] 验收：添加一个真实目录 → 列出文件；尝试读取工作区外的文件 → 明确报错且不泄露路径内容。
- [ ] 学习练习：用户在工作区内新建一个文件，确认工具能读到。

### Phase 11：AI Agent

**文件**：新增 `provider/mod.rs`、`provider/openai.rs`、`provider/fake.rs`、`agent.rs`、`secret.rs`、`src/api/agent.js`、`src/composables/useAgentStream.js`；修改 `lib.rs`、`Cargo.toml`。

- [ ] `ChatProvider` trait + OpenAI 兼容实现（SSE 增量解析）+ `FakeProvider`（测试用，可脚本化返回固定流与工具调用）。
- [ ] `secret.rs`：keyring 读写；确认 key **不出现在**任何命令返回值与日志里。
- [ ] 工具注册表 + 7 个工具（见 4.6）；工具 schema 校验参数。
- [ ] Agent 循环：含最大轮数上限、取消令牌、工具失败回填。
- [ ] 流式事件转发 + 前端节流聚合。
- [ ] 测试（全部走 `FakeProvider`，不打真实网络）：单轮无工具、多轮工具调用、达到最大轮数、流中途取消、供应商错误、无 key 报错。
- [ ] 验收：配置一个真实供应商 → 问「帮我看看 pomodoro.rs 里 deadline 怎么算的」→ 能触发 `read_file` 并基于内容回答；对 `write_file` 请求弹出确认。
- [ ] 学习练习：用户新增一个只读工具（比如读某个固定文件），理解工具注册与描述对模型行为的影响。

### Phase 12：对话窗口与整合

**文件**：新增 `db.rs`、`window_manager.rs`、`src/components/ChatWindow.vue`、`MessageList.vue`、`ToolCallCard.vue`；修改 `settings_window.rs`（改用 window_manager）、`main.js`、`tray.rs`。

- [ ] 抽取 `window_manager.rs`，`settings` 与 `chat` 共用单例窗口逻辑。
- [ ] SQLite：建表、迁移、会话与消息 CRUD、分页查询。
- [ ] 对话窗口 UI：工作区/会话列表、消息流、工具调用卡片、Stop 按钮、失败重试、复制。
- [ ] 多窗口同步：会话变更广播；main / settings 显示摘要。
- [ ] 与桌宠联动：Agent 调用 `set_expression` / `start_pomodoro` 时，桌宠有可见反馈。
- [ ] 打包与验收：安装版下验证番茄钟、养成、工作区、对话四件事在重启后都保持。
- [ ] 性能回归：用 `app/scripts/measure-performance.ps1` 对比引入前后的 30/15 FPS 与隐藏停更，确认**没有**因为后台任务或流式事件破坏既有性能结论。
- [ ] 学习练习：用户从托盘 → 聊天窗口 → 工具调用卡片，完整走一遍并说明消息如何落库。

---

## 八、测试与验收策略

**纯函数优先**（不引入 Vitest，沿用 `node --test` 与 `cargo test`）：

| 对象 | 测试点 |
|---|---|
| 番茄钟状态机 | 正常流程、暂停顺延、取消、跨 deadline 只完成一次、睡眠后补齐 |
| 养成结算 | 经验与上限、连续加成、幂等、衰减边界、跨天 |
| 上下文裁剪 | token 估算、超预算时的裁剪顺序、系统提示永不被裁掉 |
| 路径校验 | `..` 逃逸、符号链接、大小写与分隔符变体 |
| 持久化 | 主文件损坏回退、`.bak` 回退、默认值兜底 |
| Agent 循环 | 用 `FakeProvider`：无工具、多轮工具、轮数上限、取消、错误 |

**不写** UI 快照测试，UI 走人工验收清单（与既有项目风格一致）。

**性能必须回归**：v1 已有 30/15 FPS、隐藏停更的 Release 实测数据。v2 引入后台任务与流式事件后必须重新测量，特别是：番茄钟的 1 秒 tick 是否影响渲染帧率、流式 token 事件是否干扰 Live2D 的 30 FPS 预算、对话窗口是否复用了同一 WebGL 资源（不应复用）。

---

## 九、风险与对策

| 风险 | 影响 | 对策 |
|---|---|---|
| 番茄钟挂到渲染 ticker / 依赖窗口可见性 | 隐藏窗口就停表，功能性地错 | 独立后台任务 + 绝对时间戳；Phase 8 验收明确覆盖隐藏与睡眠 |
| 系统睡眠后时间错乱 | 剩余时间错误、重复结算 | 只用 `deadline` 判定，唤醒后一次性补齐；不累加计数 |
| API key 泄露 | 安全问题，后果严重 | 只存 keyring，不回传前端；日志脱敏；`set_api_key` 无返回值回显 |
| 前端被注入后越界读文件 | 读到工作区外的敏感文件 | 不放宽前端 fs scope；Rust 侧 `canonicalize` + 前缀校验；专项测试 |
| 流式 token 洪泛 | 页面卡顿、拖累 Live2D 30 FPS | 事件按 `messageId` 聚合，前端约 50ms 节流刷新 |
| 上下文爆炸 / 费用失控 | 请求失败或成本超预期 | 预算裁剪 + 最大轮数 + 工具输出截断 + 单次会话 token 上限提示 |
| 养成数值可刷 / 作弊 | 数值失去意义 | 经验按实际专注分钟、每日上限、`sessionId` 幂等 |
| 持久化文件损坏 | 起不来或丢数据 | 原子写 + `.bak` + 解析失败回退默认值，绝不 panic |
| 窗口数量膨胀 | 资源占用上升 | 对话窗口单例、按需创建；加载时**不**引入第二份 Live2D Core |
| CSP 目前是 `null` | 接入网络后攻击面变大 | 网络请求全在 Rust 侧，前端无外联；Phase 12 评估收紧 CSP，注意 Live2D 需要 `asset:` 与 wasm 相关来源（需实测） |
| 菜单几何改动 | 打乱既有气泡菜单体验 | 方案 A 需重算 `menu_layout.rs` 常量并重写几何测试，同时人工验收多屏/DPI |
| 范围蔓延（语音、RAG、命令执行…） | 交付周期失控 | 见第十节非目标；需求变更先改本文件再动代码 |

---

## 十、非目标（v2 明确不做）

- 不做云端同步与账号体系；数据只在本机。
- 不做语音唤醒 / TTS（预留接入点，不在本期）。
- 不做 RAG / 向量库 / 嵌入检索。
- 不做命令行执行工具（`run_command`）——风险高，且 v2 的工作区读写已够用。
- 不做自动更新、开机启动（v1 遗留非目标继续保留）。
- 不新增 Live2D 动作资源；养成表现限于表情与现有参数（模型无 `.motion3.json`）。
- 不做多角色 / 多模型管理。
- 不做公网发布与多用户。

---

## 十一、需要用户确认的决策点

1. **AI 供应商**：先用哪家？（OpenAI 兼容 / Anthropic / 本地 Ollama）——直接决定 Phase 11 的首个实现与默认模型。
2. **工作区写权限**：默认关闭、逐次确认（本方案建议），还是允许用户永久开启写权限？
3. **养成深度**：采用本方案的「等级 + 心情 + 解锁」三件套，还是先只做等级与经验？（直接影响 Phase 9 的工作量）
4. **气泡菜单方案**：A（扩到 6 个球）/ B（入口都进对话窗口）/ C（分页）？本方案推荐 A。
5. **对话窗口**：确认新增第三个窗口（本方案建议），还是希望复用设置窗口？
6. **番茄钟默认值**：25/5/15、每 4 个长休是否符合你的使用习惯？是否需要「不联动表情」作为默认？

---

## 十二、参考入口

- 现有 IPC 与事件：`app/src/api/pet.js`
- 现有状态与版本号模式：`app/src-tauri/src/settings.rs`、`app/src/composables/petSettings.js`
- 现有单例窗口模式：`app/src-tauri/src/settings_window.rs`
- 现有后台与错误上报：`app/src-tauri/src/desktop.rs`
- 现有点击/拖动判定：`app/src/interactions/petGesture.js`、`docs/learning/07-bubble-menu.md`
- 渲染暂停策略：`app/src/live2d/renderPolicy.js`、`docs/learning/05-performance.md`
- 性能测量工具：`app/scripts/measure-performance.ps1`、`docs/performance/2026-09-10/README.md`
- [Tauri 2：前端调用 Rust](https://v2.tauri.app/develop/calling-rust/)
- [Tauri 2：状态管理](https://v2.tauri.app/develop/state-management/)
- [Tauri 2：窗口定制](https://v2.tauri.app/learn/window-customization/)
