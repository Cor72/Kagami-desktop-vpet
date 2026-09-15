# 八千代桌宠：AI 对话 + Agent 模式 + 主动互动（实施计划）

日期：2026-09-15。状态：**待实现**。读者：执行实现的 agent。
设计依据：[Agent 模式与情境感知设计](../../specs/2026-09-14-agent-mode-and-context-design.md)。

---

## 〇、这份计划是什么

**交付物是一个 PR，内容是成品。** 用户拿到手之后，这个桌宠会多出三件能亲眼看见的事：

1. 气泡菜单多一个「对话」球，点开是一个聊天窗口；
2. 聊天窗口里能切「聊天模式 / Agent 模式」，Agent 模式能读用户授权的文件、能切桌宠表情、能改文件（改前给 diff 让用户确认）；
3. 桌宠在用户没问它的时候会自己冒文字气泡说话（空闲问候、打开某个程序时的一句）。

**这不是替作者完成 v2 的 Phase 7–12。** 那些阶段的目标、顺序、验收标准与本计划无关，不要顺手去实现番茄钟或养成系统。

**分支**：在 `feat/phase7-persistence` 上继续。Phase 7 的持久化代码（`store.rs` / `clock.rs` / `broadcast.rs` + `settings.rs` 改造）**保留在同一个 PR 里**——新功能要用它存配置。不要把它拆出去单独提 PR。

---

## 一、成品验收（做完这些算完成）

- [ ] 右键桌宠 → 气泡菜单多出「对话」，点它打开聊天窗口
- [ ] 聊天窗口能切换「聊天模式 / Agent 模式」，切换状态有可见变化
- [ ] 不填 API Key 时，窗口里有明确提示告诉用户去哪儿填
- [ ] 设置窗口能填 API Key，保存后只显示掩码（如 `sk-****1234`），能测试连接，能删除
- [ ] 聊天模式：输入文字 → 收到的回复**逐字流式出现**（不是等半天一次出现）
- [ ] 关掉窗口再打开，历史对话还在
- [ ] Agent 模式：能添加一个文件夹作为工作区，能拖文件进去（只读）
- [ ] Agent 模式：让它「看看工作区里有什么文件」，它会真的调用工具去读，并在对话流里显示「正在读取 xxx」
- [ ] Agent 模式：让它改一个文件，弹出 diff（红绿行），点「应用」才真写，点「拒绝」不写
- [ ] 主动互动：闲置 5 分钟后回来，桌宠冒一个文字气泡
- [ ] 主动互动：打开 VS Code / 浏览器 / 游戏，桌宠各冒一句对应的气泡
- [ ] 设置里能一键关掉主动互动
- [ ] 桌宠隐藏或最小化时，主动互动停止、不产生任何模型调用

---

## 二、范围

### 做

| # | 内容 | 用户可见 |
|---|---|---|
| 1 | 气泡菜单第 6 个球「对话」 | 是 |
| 2 | 独立聊天窗口（720×560） | 是 |
| 3 | 聊天模式 / Agent 模式切换 | 是 |
| 4 | 设置窗口的 API Key 一节 | 是 |
| 5 | 工作区：选文件夹 + 拖文件 | 是 |
| 6 | 6 个工具（含 `write_file` + diff 确认） | 是 |
| 7 | 主动互动文字气泡 | 是 |

### 不做

- 不做番茄钟、养成系统（那是作者的 Phase 8 / 9，与本计划无关）
- 不做语音、TTS、RAG、向量库（v2 第十节已列为非目标）
- 不做定时截图 / 录屏（设计文档 §6.1 有理由）
- 不引入 LangChain / LangGraph / Pinia / Vue Router / UI 组件库
- 不放宽 `app/src-tauri/capabilities/*.json` 里前端的 fs 权限
- **不要碰仓库根下那个叫 `.gitignore` 的文件夹**（误提交的 BongoCat 源码，用户决定暂时不管）

---

## 三、界面规格

### 3.1 气泡菜单加第 6 个球

**文件**：`app/src/composables/petMenu.js`

`rootItems` 目前是 5 项。在「表情」之后插入一项：

```js
{ id: 'chat', label: '对话', icon: 'message-circle' },
```

图标库是 `@lucide/vue`，`message-circle` 已在该库中。

**同时要改的**：

- `app/src/App.vue` 的动作分发里加 `chat` 分支 → 调用 `api.openChatWindow()`
- **`app/src/components/PetBubbleMenu.vue` 的图标映射表必须同步**。该文件第 3 行按需 import，
  第 15 行是一个 `icons` 对象，键用 kebab-case：

  ```js
  import { ..., MessageCircle } from '@lucide/vue'
  const icons = { ..., 'message-circle': MessageCircle }
  ```

  漏了这一步，第 6 个球会渲染成空白。
- `app/src/composables/petMenu.test.js` 加断言：`getMenuItems('root')` 长度由 5 变 6，且含 `chat`

**几何风险**：菜单竖直贴在画布右侧、**不改变窗口尺寸**（这是 v1 的硬约束）。主窗口高 440 逻辑像素，球是 52×52。6 个球 + 间距可能接近或超过可用高度。**实现时先实测**，如果放不下，按顺序尝试：缩小间距 → 缩小球径 → 菜单整体上移。**不要改窗口尺寸。**

### 3.2 文字气泡组件（新）

**新文件**：`app/src/components/PetSpeechBubble.vue`

规格：

- 绝对定位在桌宠窗口内，位于模型上方
- **不改变窗口尺寸**（与气泡菜单同一条约束）
- 默认 5 秒后淡出；点击立即关闭
- 同时只显示一条，新消息直接替换旧的
- 文本上限约 40 字，超出折行，最多 3 行
- 透明区域**不能挡住模型**：组件根节点默认 `pointer-events: none`，只有气泡本体 `pointer-events: auto`
- 与气泡菜单**互斥**：菜单打开时不显示气泡，反之亦然

**触发来源**：监听 Rust 事件 `proactive-speak`，载荷 `{ id, text, ttlMs }`。

### 3.3 对话窗口（新）

**Rust 侧**：新建 `app/src-tauri/src/chat_window.rs`，**直接照抄 `settings_window.rs` 的写法**（单例守卫 + `WebviewWindowBuilder`），只改这几处：

| 项 | 值 |
|---|---|
| label | `"chat"` |
| URL | `index.html?view=chat` |
| title | `"八千代 · 对话"` |
| inner_size | `720.0, 560.0` |
| min_inner_size | `480.0, 400.0` |
| resizable | `true` |
| decorations | `true` |
| transparent | `false` |
| always_on_top | `false` |

在 `lib.rs` 里 `manage` 一个对应的 `Mutex<()>` 守卫，并注册命令 `open_chat_window`。

**前端入口**：`app/src/main.js` 现在按 `?view=settings` 分流，加一个分支：

```js
const view = new URLSearchParams(location.search).get('view')
if (view === 'settings') { /* 现有逻辑 */ }
if (view === 'chat') { /* 挂载 ChatWindow.vue，不加载 Live2D Core */ }
```

**关键约束**：对话窗口**不加载 Live2D Core**（`main.js` 里现在只有主窗口会注入 `live2dcubismcore.min.js`）。保持这个结构。

**新文件**：`app/src/ChatWindow.vue`

布局（照这个做，不要自由发挥）：

```
┌────────────────────────────────────────────────────┐
│  八千代                        [聊天] [Agent]   ⚙  │  ← 顶栏：标题 + 模式切换
├────────────────────────────────────────────────────┤
│  ┌── 工作区栏（仅 Agent 模式显示）──────────────┐  │
│  │ 📁 我的项目（可读写）  📄 report.pdf（只读）  │  │
│  │ [+ 添加文件夹]   把文件拖到这里               │  │
│  └────────────────────────────────────────────┘  │
│                                                    │
│   消息流（可滚动）                                  │
│     · 用户气泡（右）                                │
│     · 八千代气泡（左，流式逐字）                     │
│     · 工具调用卡片：「正在读取 pomodoro.rs」         │
│     · 写文件确认卡片：diff + [应用] [拒绝]           │
│                                                    │
├────────────────────────────────────────────────────┤
│  [ 输入框                          ] [发送] [停止]  │
└────────────────────────────────────────────────────┘
```

**组件拆分**（都放 `app/src/components/chat/`）：

| 文件 | 职责 |
|---|---|
| `MessageList.vue` | 消息流渲染、自动滚到底 |
| `MessageBubble.vue` | 单条消息 |
| `ToolCallCard.vue` | 工具调用过程卡片 |
| `WriteConfirmCard.vue` | diff 展示 + 应用/拒绝 |
| `WorkspaceBar.vue` | 工作区条目 + 添加 + 拖拽区 |
| `ChatComposer.vue` | 输入框 + 发送 + 停止 |

**composable**：`app/src/composables/useChat.js`（会话状态、消息列表、发送、取消）。

### 3.4 设置窗口加 API 一节

**文件**：`app/src/SettingsWindow.vue`

现有窗口是 360×360，**放不下**。做法：

1. 窗口内层改成可滚动（`overflow-y: auto`）
2. `chat` 无关的既有设置项位置不动，只在下面追加一节

新增一节「AI」：

| 控件 | 说明 |
|---|---|
| 服务商 | 下拉：DeepSeek（默认）/ OpenAI / 自定义 |
| API Key | 密码输入框；已保存时显示掩码 `sk-****1234`，旁边有「删除」 |
| Base URL | 仅「自定义」时显示 |
| 模型名 | 文本输入，默认随服务商变化 |
| 测试连接 | 按钮，返回成功/失败与原因 |

**API Key 绝不回传前端全文**。前端只能拿到 `hasKey: boolean` 和掩码。

---

## 四、数据与存储

全部落在 Tauri 的 `app_data_dir`（`store.rs` 已有这套能力，直接复用）。

| 文件 | 内容 |
|---|---|
| `settings.json` | 已有。追加 `proactive: { enabled: bool }` |
| `ai.json` | `{ schemaVersion, provider, model, baseUrl, mode }` |
| `workspace.json` | `{ schemaVersion, entries: [{ id, kind: "dir"｜"file", path }] }` |
| `sessions/<sessionId>.json` | `{ schemaVersion, id, title, workspaceId, createdAt, updatedAt, messages: [...] }` |

**API Key 不进任何 json 文件**，存 Windows 凭据管理器（`keyring` crate）：
- service = `"yachiyo-desktop"`
- account = provider 名（`"deepseek"` / `"openai"` / `"custom"`）

> **与 v2 设计的一处偏离**：v2 §4.2 要求对话历史用 SQLite（`rusqlite`）。本计划改用 JSON 文件，理由是单用户本地、第一版消息量小、少一个需要编译的依赖、更容易写单测。**若作者坚持 SQLite，再切换**——切换只影响 `chat_store.rs`，其余模块不受影响。

---

## 五、Rust 模块

```
app/src-tauri/src/
├── store.rs          已有（Phase 7）
├── clock.rs          已有
├── broadcast.rs      已有
├── settings.rs       已有，追加 proactive 字段
├── chat_window.rs    新：对话窗口单例（抄 settings_window.rs）
├── ai/
│   ├── mod.rs          模块出口
│   ├── config.rs       AiConfig 读写（ai.json）
│   ├── secret.rs       keyring 封装
│   ├── provider.rs     ChatProvider trait + OpenAI 兼容实现
│   ├── stream.rs       SSE 增量解析（含 tool_calls 分片拼接）
│   ├── agent.rs        Agent 循环（最多 8 轮）+ 取消令牌
│   └── tools.rs        工具注册表 + JSON schema
├── workspace.rs       工作区条目注册 + 路径校验
├── fs_tools.rs        受限文件工具实现
├── chat_store.rs      会话与消息落盘
├── context.rs         情境信号采集（windows crate）
└── proactive.rs       主动行为规则引擎（纯函数）+ 调度
```

**新增依赖**（`app/src-tauri/Cargo.toml`）：

```toml
reqwest       = { version = "0.12", default-features = false, features = ["rustls-tls", "json", "stream"] }
futures-util  = "0.3"
tokio-util    = "0.7"          # CancellationToken
keyring       = "3"
similar       = "2"            # 生成 diff
tauri-plugin-dialog = "2"
windows       = { version = "0.58", features = ["Win32_Foundation", "Win32_UI_WindowsAndMessaging", "Win32_System_Threading", "Win32_System_SystemInformation"] }
```

`keyring` / `windows` 的版本号以 `cargo add` 实际解析到的为准。

---

## 六、IPC 协议

命令名与事件名集中写在 `app/src/api/chat.js`（新建）。**组件里不出现裸字符串。**

### 命令

| 命令 | 参数 | 返回 |
|---|---|---|
| `open_chat_window` | — | — |
| `get_ai_config` | — | `{ provider, model, baseUrl, mode, hasKey, keyMask }` |
| `set_ai_config` | `{ provider?, model?, baseUrl?, mode? }` | 新配置 |
| `set_api_key` | `{ provider, key }` | `{ ok }`（不回显） |
| `clear_api_key` | `{ provider }` | `{ ok }` |
| `test_ai_connection` | — | `{ ok, message }` |
| `create_session` | — | `Session` |
| `list_sessions` | — | `Session[]` |
| `delete_session` | `{ sessionId }` | `{ ok }` |
| `get_messages` | `{ sessionId, limit?, before? }` | `Message[]` |
| `send_message` | `{ sessionId, text }` | `{ messageId }` |
| `cancel_stream` | `{ sessionId }` | `{ ok }` |
| `list_workspace_entries` | — | `WorkspaceEntry[]` |
| `add_workspace_dir` | — | `WorkspaceEntry`（内部调目录选择器） |
| `add_workspace_files` | `{ paths }` | `WorkspaceEntry[]`（拖拽用） |
| `remove_workspace_entry` | `{ id }` | `WorkspaceEntry[]` |
| `apply_pending_write` | `{ requestId }` | `{ ok }` |
| `reject_pending_write` | `{ requestId }` | `{ ok }` |
| `get_proactive_state` | — | `{ enabled }` |
| `set_proactive_enabled` | `{ enabled }` | `{ enabled }` |
| `proactive_dismiss` | `{ id }` | `{ ok }`（用于「连续被忽略」计数） |

### 事件

| 事件 | 载荷 | 消费方 |
|---|---|---|
| `chat-stream-started` | `{ sessionId, messageId }` | chat 窗口 |
| `chat-stream-delta` | `{ sessionId, messageId, text }` | chat 窗口 |
| `chat-tool-call` | `{ sessionId, messageId, name, args }` | chat 窗口 |
| `chat-tool-result` | `{ sessionId, messageId, name, ok, summary }` | chat 窗口 |
| `chat-write-request` | `{ sessionId, requestId, path, diff }` | chat 窗口 |
| `chat-stream-finished` | `{ sessionId, messageId }` | chat 窗口 |
| `chat-stream-failed` | `{ sessionId, messageId, error }` | chat 窗口 |
| `ai-config-changed` | 配置快照（含 `revision`） | settings + chat |
| `workspace-changed` | `WorkspaceEntry[]` | chat |
| `proactive-speak` | `{ id, text, ttlMs }` | **主窗口** |

**流式事件的节流是硬要求**：delta 事件到达频率很高，前端要按约 **50ms** 节流刷新 DOM。理由见设计文档——token 级事件直接驱动渲染会干扰 Live2D 的 30 FPS 预算。

---

## 七、工具集（Agent 模式一次给全）

| 工具 | 参数 | 权限 | 说明 |
|---|---|---|---|
| `get_workspace_info` | — | 只读 | 返回已授权条目清单。**条目清单同时也要写进系统提示词**，这个工具只是兜底 |
| `list_files` | `{ path? }` | 只读 | 路径为空 = 所有条目根。深度 ≤ 3，条数 ≤ 200，超出返回「已截断，共 N 条」 |
| `grep` | `{ pattern, path? }` | 只读 | 命中 ≤ 50 条，单条 ≤ 200 字符 |
| `read_file` | `{ path }` | 只读 | 单文件 ≤ 64 KB；返回值带**文件最后修改时间** |
| `set_expression` | `{ name }` | 桌宠 | 白名单：`smile` / `squint` / `tears` / `teardrop` |
| `write_file` | `{ path, content, reason }` | **需确认** | 只能写**目录条目**内的路径；文件条目一律拒绝 |

**硬约束**：

- 单轮所有工具返回值合计 ≤ 上下文的 25%
- `write_file` 的确认流程：Rust 生成 diff → 发 `chat-write-request` → 前端弹卡片 → 用户点「应用」才写
- **文件条目永远只读**，不存在例外
- 每个工具每次调用都要**重新校验路径**（`canonicalize` 后比对白名单），不能缓存校验结果

**系统提示词里要写死工具使用顺序**：先 `list_files` 看结构 → 再 `grep` 定位 → 最后才 `read_file` 读片段。模型默认行为是急着读整份文件。

---

## 八、主动互动

### 8.1 信号采集（`context.rs`）

用 `windows` crate 读三样，全部本地、低成本：

| 信号 | API |
|---|---|
| 前台窗口标题 | `GetForegroundWindow` + `GetWindowTextW` |
| 前台进程名 | `GetWindowThreadProcessId` + 查进程 |
| 空闲时长 | `GetLastInputInfo` |

采集方式：**低频采样（默认 5 秒一次）**，只在信号发生变化时才执行规则判断。**绝不定时调用模型。**

### 8.2 隐私分级（重要）

| 信息 | 处理 |
|---|---|
| **应用名** | 直接可用，映射成中文（`GenshinImpact.exe` → 原神、`Code.exe` → 代码编辑器） |
| **窗口标题** | **只对已知编辑器**（VS Code / JetBrains / Sublime / Vim）提取文件名；其余应用**整条丢弃**，只留应用类型 |

也就是说：知道你在玩什么游戏，**不知道**你在跟谁聊天。这条要写进代码注释和设置页说明。

### 8.3 规则引擎（`proactive.rs`）

纯函数，可 `cargo test` 覆盖，不依赖真实时间（用 Phase 7 的 `clock.rs`）：

```rust
fn should_speak(signal: &Signal, state: &ProactiveState, now_ms: u64) -> Option<Utterance>
```

**第一版只放三条触发**：

| 触发 | 文案来源 |
|---|---|
| 空闲 ≥ 5 分钟（可配 3–10） | 内置模板表 |
| 回到活跃 | 内置模板表 |
| 检测到某程序启动 | 「进程名 → 文案」映射表 |

番茄钟相关的触发**不做**（本计划不含番茄钟）。

### 8.4 频率与静默

| 防线 | 规则 |
|---|---|
| 事件型（程序启动） | 同类每次至多 1 句，同类冷却 ≥ 5 分钟 |
| 状态型（闲置、回来） | **一次离开只说一次**，同类冷却 ≥ 20 分钟 |
| 每日总量 | ≤ 30 条 |
| 连续被忽略 3 次 | 冷却翻倍（**不可关闭的机制**） |
| 静默：全屏应用 | 不说 |
| 静默：23:00–08:00 | 不说 |
| 静默：最近 30 秒内有键盘输入 | 延后 |
| 静默：桌宠隐藏 / 最小化 | 停止采集，不产生任何模型调用 |

### 8.5 文案

第一版**只用内置模板**，不调模型。这样：

- 不需要 API Key 也能工作
- 零成本、零延迟
- 可测试

**八千代的人设尚未定稿**（用户明确说下一阶段再定），所以文案先写成中性、简短的几句话，并集中放在一个文件里（建议 `app/src-tauri/src/proactive.rs` 顶部的常量表，或单独的 `proactive_lines.rs`），顶部标注：

```
// TODO: 八千代的角色设定稿确定后，替换这里的文案与语气。
// 相关系统提示词同样待补（见 ai/persona.rs）。
```

### 8.6 聊天用的系统提示词

`ai/agent.rs` 组装消息时需要一个系统提示词。第一版写最小占位：

```
你叫八千代，是一只常驻用户 Windows 桌面的伙伴。
回答简短、口语化，不要长篇大论，不要用 Markdown 列表。
你不知道的事情就说不知道。
```

**单独放一个常量或 `ai/persona.rs`**，顶部同样标注待人设定稿后替换。系统提示词里另外要拼上：当前工作区条目清单、当前模式、以及（若开启）情境摘要。

---

## 九、实现顺序（六个里程碑，一个 PR）

每个里程碑结束时都必须：**能运行 + 有测试 + 用户能看到变化**。不要一次写完全部再跑。

### M1 对话窗口骨架

- 气泡菜单加第 6 个球 + 动作分发 + 测试
- 新建 `chat_window.rs`，注册 `open_chat_window`
- `main.js` 加 `?view=chat` 分流
- `ChatWindow.vue` 只放空壳（顶栏 + 空消息区 + 输入框）

**验证**：右键桌宠 → 点「对话」→ 窗口打开；关掉再点能重新打开（单例不重复创建）。

### M2 API Key 与配置

- `ai/config.rs` + `ai/secret.rs` + 相关命令
- 设置窗口加「AI」一节（含测试连接）
- `ChatWindow.vue` 在没有 key 时显示引导提示

**验证**：在设置里填入 DeepSeek key → 保存 → 显示掩码 → 测试连接成功 → 重启应用后 key 仍在。

### M3 聊天模式跑通（第一个可见成果）

- `ai/provider.rs` + `ai/stream.rs`（SSE 解析）
- `chat_store.rs`（会话与消息落盘）
- `send_message` / `cancel_stream` + 六个流式事件
- `MessageList.vue` / `MessageBubble.vue` / `ChatComposer.vue`

**验证**：打一句话 → 回复逐字出现 → 停止按钮能中断 → 关窗口重开历史还在。
**这一步做完，最核心的东西就能用了。**

### M4 Agent 模式：工作区 + 只读工具

- `workspace.rs`（条目注册 + `canonicalize` 白名单校验）
- `fs_tools.rs`（`list_files` / `read_file` / `grep`）
- `ai/tools.rs`（工具注册表）+ `ai/agent.rs`（循环，最多 8 轮）
- `WorkspaceBar.vue`（添加文件夹 + 拖拽投放）
- `ToolCallCard.vue`

**验证**：加一个文件夹 → 问「工作区里有什么」→ 看到工具调用卡片并得到正确回答；拖入一个文件 → 它能读到；试图读工作区外的文件 → 明确报错。

### M5 write_file + diff 确认

- `write_file` 工具 + `similar` 生成 diff
- pending 状态（`Mutex<HashMap<String, PendingWrite>>`）
- `chat-write-request` 事件 + `WriteConfirmCard.vue`
- `apply_pending_write` / `reject_pending_write`

**验证**：让它改一个文件 → 出现红绿 diff → 点「拒绝」文件不变 → 再改一次点「应用」文件真的变了。

### M6 主动互动

- `context.rs`（三个系统 API）
- `proactive.rs`（规则引擎 + 调度 + 文案表）
- `PetSpeechBubble.vue` + `proactive-speak` 事件
- 设置里加主动互动开关

**验证**：闲置 5 分钟回来看到气泡；打开 VS Code 看到气泡；关掉开关后不再出现；隐藏桌宠时不再产生任何调用。

---

## 十、坑与硬约束

1. **不要放宽 `app/src-tauri/capabilities/*.json` 的前端 fs 权限。** 文件访问一律走自定义 Rust 命令，命令内每次校验。
2. **对话窗口不加载 Live2D Core**，也不要引入第二份渲染器。
3. **流式事件按 50ms 节流**，否则抢走 Live2D 的帧预算。
4. **气泡与菜单都不能改变桌宠窗口尺寸**（340×440 是硬约束，改尺寸要同时改 `tauri.conf.json`、`style.css` 里 `.pet-anchor` 与 `.pet-view` 三处）。
5. **API Key 绝不回显、绝不进日志、绝不进 json**。
6. `app/package.json` 的 `test` 脚本是**硬编码的文件列表**，新增测试目录（如 `src/components/chat/*.test.js`）必须同步修改，否则新测试根本不会跑。
7. `app/patches/easy-live2d@0.4.4.patch`、`app/pnpm-workspace.yaml`、锁文件**不能删**。
8. `tauri.conf.json` 里 `bundle.active` 是 `false`，打安装包要显式传 `--bundles nsis`。
9. **测功能要用安装版**：直接跑 `target/release/yachiyo-desktop.exe` 会因模型资源没释放到旁边而报「找不到模型」。
10. 新窗口要确认已加进 `capabilities`，否则 `invoke` 会被 Tauri 拒绝。
11. git 全局配置里的代理（`127.0.0.1:7897`）平时没开，联网命令要用 `git -c http.proxy= -c https.proxy=` 绕过。

---

## 十一、与既有设计文档的关系

- 本计划是 [Agent 模式与情境感知设计](../../specs/2026-09-14-agent-mode-and-context-design.md) 的落地。
- 设计文档 §6.4 里的「番茄钟完成」「长时间刷非工作应用」两条触发**不在本计划范围**（前者依赖 Phase 8，后者最容易变成说教，建议最后做）。
- 设计文档 §5.1 原本建议「第一版不做 `write_file`」。**本计划按用户要求改为一次做全**，但为此必须先实现 diff 确认卡片（M5）——没有它，写入确认等于盲签。
- 若作者对任何一处设计提出异议，**先改设计文档再改代码**（这是作者定的规矩）。

---

## 十二、交付方式

- 分支：`feat/phase7-persistence` 上继续（Phase 7 底座留在同一个 PR）
- 建议**一个里程碑一个提交**，提交信息用中文，格式可参考仓库既有历史
- PR 描述里要写清：这个 PR 包含 Phase 7 底座（原因：新功能依赖持久化）
- 推送需要联网，本执行环境可能连不上 GitHub，**若失败就让用户在自己终端推**（Clash 打开着的时候）

---

## 数据来源与口径

| 数据 | 来源 |
|---|---|
| 窗口尺寸 340×440、菜单不改变窗口尺寸 | `README.md` 已知问题、`app/src/style.css` |
| 30 FPS / 15 FPS 与隐藏停更 | `README.md` 性能小节 |
| 工具输出的上下文占比 25% | 本文自拟建议值，需实测调整 |
| 主动互动的频率与冷却数值 | 设计文档 §6.5，均为建议初值 |
