# 八千代桌宠

一只常驻 Windows 桌面的 Live2D 伙伴。透明无边框窗口，角色会跟随鼠标，单击换表情，右键弹出菜单；托盘和独立设置窗口管理全部开关。

**技术栈**：Rust + Tauri 2 + Vue 3 + JavaScript + PixiJS 8 + easy-live2d（Live2D Cubism 3+）。
**当前版本**：0.1.0，Release 已实测（30/15 FPS、隐藏与最小化停更），NSIS 安装包已能打出。

## 快速开始

```powershell
cd <仓库目录>\app
pnpm install --frozen-lockfile --registry=https://registry.npmjs.org
pnpm tauri dev
```

开发模式下设置窗口底部会多出两个按钮：「开发联调」（展开 `DevPanel`，显示 Vue ↔ Rust 的通信过程）和「重新加载模型」。这两项在发布版中不存在。

## 功能

### 角色与表现

| 能力 | 说明 |
|---|---|
| 待机表现 | 眨眼、呼吸、物理摆动由运行库驱动 |
| 四个表情 | `smile` / `squint` / `tears` / `teardrop` |
| 鼠标跟随 | 头部、眼睛、身体平滑朝向鼠标。**鼠标移出桌宠窗口也继续跟随**（采样桌面全局坐标，再用窗口位置与屏幕缩放率换算） |
| 4K 运行贴图 | 保留原始 8K 资源，运行使用两张 4096×4096 RGBA 贴图 |

### 交互

- **单击角色**：随机切换表情（排除上一次，保证连续两次不重复）
- **按住拖动**：移动超过 6 个逻辑像素才开始拖动窗口，避免点击时误触
- **右键角色**：打开/收起气泡菜单
- **Esc**：逐层返回（二级 → 一级 → 收起）
- **失焦或点击空白处**：收起菜单

### 气泡菜单

菜单是**桌宠窗口内的一块普通 DOM**，竖直贴在画布右侧，**不改变窗口尺寸**。

- 一级六项：表情 / 对话 / 设置 / 置顶 / 隐藏 / 退出
- 二级五项：微笑 / 眯眼 / 泪眼 / 泪滴 / 返回
- 按钮只有图标（52×52 圆形），文字通过**悬浮提示**显示，提示框用 CSS 自绘（原生 `title` 的边框无法调细）
- 菜单与模型的距离由 `src/style.css` 的 `--menu-gap` 控制

### 设置窗口

独立单例窗口（360×360，可缩放、不透明），从气泡菜单或托盘打开：

设置项：

- **动画帧率**：30 FPS / 15 FPS
- **保持置顶**：让角色留在其他窗口上方

开发模式下窗口底部额外提供「开发联调」与「重新加载模型」两个按钮（发布版不含）。

### 对话窗口（AI 聊天）

从气泡菜单的「对话」打开，是与桌宠窗口分离的单例窗口（720×560，可缩放）。它**不加载 Live2D Core**，也不碰画布。

- **服务商**：默认 DeepSeek（`https://api.deepseek.com/v1` + `deepseek-chat`），另预设 OpenAI 与「自定义」。三家都是 OpenAI 兼容格式。
- **API Key 存在 Windows 凭据管理器**（service = `yachiyo-desktop`，account = 服务商名）。它**不写进任何 json、不进日志、不回传前端**；前端只能拿到 `hasKey` 与掩码（如 `sk-****1234`）。
- **回复逐字出现**：请求带 `stream: true`，Rust 解析 SSE 增量后广播 `chat-stream-delta`；前端按 **50ms** 合批刷新 DOM，避免抢走 Live2D 的帧预算。
- 发送前会真的探活一次最小请求（设置窗口的「测试连接」同一套逻辑）：401 / 402 / 404 / 429 都会被翻译成一句能照着改的中文。
- 没填 Key 时窗口不会假装能聊：顶部提示 + 「去设置里填 Key」按钮。
- 窗口关掉再打开，历史还在（对话落盘见下一节）；窗口关闭时会中断正在进行的回答。

### Agent 模式（读文件、改文件）

对话窗口顶部可以切「聊天 / Agent」。**模式只决定给模型哪些工具**；能碰哪些文件是另一回事，
由工作区授权决定——开关无权放开权限。

| | 聊天模式 | Agent 模式 |
|---|---|---|
| 给模型的工具 | **一个都不给** | 6 个（见下表） |
| 能碰文件吗 | 不能 | 只能碰工作区里你显式给的那几样 |
| 上下文里有什么 | 角色设定 + 对话历史 | 再加**工作区条目清单**（最多 32 条，一条一行） |

**工作区条目分两种**，用同一套 `canonicalize` 白名单校验，靠类型字段区分权限：

| | 目录条目 | 文件条目 |
|---|---|---|
| 怎么加进来 | 「添加文件夹」按钮（系统目录选择器） | **把文件拖进来** |
| 权限 | 可读，写入要走 diff 确认 | **只读**——物理上没有写入通道，不存在例外 |
| 心智模型 | 「这是我的项目」 | 「这是我给你的资料」 |

拖进来的是**引用而不是副本**：原文件更新后读到的就是最新的。
拖一个目录进来会被拒绝并提示改用「添加文件夹」——那条路才是明确的授权动作。

工具集（一次给全）：

| 工具 | 作用 | 硬上限 |
|---|---|---|
| `get_workspace_info` | 兜底：列出已授权条目 | 清单本来就在系统提示词里 |
| `list_files` | 看目录结构，跳过 `node_modules` / `.git` / `target` 等 | 深度 ≤ 3，条数 ≤ 200 |
| `grep` | 大小写不敏感的子串搜索（不是正则） | 命中 ≤ 50，单条 ≤ 200 字 |
| `read_file` | 读文件正文，返回值带**最后修改时间** | 单文件 ≤ 64 KB，二进制拒读 |
| `set_expression` | 换桌宠表情 | 复用现有表情白名单 |
| `write_file` | **改文件，需确认** | 只能写目录条目里的路径 |

几条写死的规矩：

- **每次工具调用都重新 `canonicalize` 再比对白名单**，校验结果不缓存——符号链接可以在两次
  调用之间被换掉，缓存等于把防线拆了。解析后的真实路径会重新过一遍白名单，指向工作区外的
  软链接会被拒。
- **单轮所有工具返回值合计不超过上下文的 25%**（估算值，落在 `ai/tools.rs` 的
  `ROUND_OUTPUT_BUDGET_CHARS`），超出就截断并说明。逐个工具自己的上限才是主力。
- **改文件必须过 diff 确认**：Rust 生成红绿 diff → 交给对话窗口 → 你点「应用」才写盘，
  点「拒绝」一个字节都不动。写入是原子的（先写 `.tmp` 再改名）。没有 diff 的确认等于盲签。
- Agent 循环**最多 8 轮**工具调用；点「停止」或关掉窗口都会立刻中断，磁盘上不会留下半截改动。
- 系统提示词里写明用工具的顺序：先 `list_files` 看结构 → 再 `grep` 定位 → 最后才 `read_file`
  读片段。模型默认行为是急着读整份文件。

对话流里会看到两类卡片：工具卡片（「正在读取 pomodoro.rs」→「读过 pomodoro.rs 了」）与
写入确认卡片（红绿 diff + 应用 / 拒绝）。文案按角色设定写，不是「正在执行 read_file 工具」。

> **没有前端 fs 权限**：`capabilities/chat.json` 里依然没有任何文件系统权限，
> 目录选择器也是在 Rust 命令里调的（`tauri-plugin-dialog`）。所有文件访问都走自定义命令，
> 命令内每次校验。

### 主动互动（会自己说话）

八千代在用户没问它的时候也会冒一句文字气泡。**这一套不需要 API Key**：文案全是内置模板，不调模型，零成本、零延迟。

三条触发：

| 触发 | 文案示例 |
|---|---|
| 离开 5 分钟 | 「这儿安静下来了」 |
| 回到活跃 | 「回来啦」 |
| 前台窗口切到已登记的程序（编辑器 / 浏览器 / 音乐 / 游戏） | 「编辑器打开了：pomodoro.rs」「浏览器打开了」「游戏启动了」 |

「打开某个程序」用**前台窗口变化**判断，不轮询进程表——前台窗口本来就是每次采样都在读的，而且「注意力刚转移」才是适合搭话的时刻；后台悄悄启动的进程（比如自动更新）不会触发。

**隐私分级**（做成了代码里的表，不是口头承诺）：

- **应用名**照常使用：知道你在玩原神，它能给出对应的反应；
- **窗口标题只对已知编辑器**（`Code.exe` / `idea64.exe` / `sublime_text.exe` / `gvim.exe`）取第一段当文件名；
- 其余应用（浏览器、聊天软件）的标题**整条丢弃**：「招商银行 - Chrome」「张三 - 微信」里没有任何东西会流出去；
- 所有判断都在本地 Rust 侧完成，这一阶段**不联网、不调模型**。

**防骚扰靠静默规则，不靠压低频率**：

| 防线 | 规则 |
|---|---|
| 同类冷却 | 事件型（程序切换）5 分钟，状态型（离开 / 回来）20 分钟 |
| 每日总量 | ≤ 30 条 |
| 连续被忽略 3 次 | 同类冷却翻倍（不可关闭） |
| 手动静默 | 托盘勾「今天别烦我」，当天剩下的时间不再主动（跨天自动失效） |
| 全屏 | 状态型不说（打开全屏游戏那一下仍然会说） |
| 正在输入 | 那一句延后一个采样周期（约 5 秒），用户停手就立刻补上 |
| 桌宠隐藏 / 最小化 | 停止采样，不判断、不发言 |

行为上的几个细节：

- 气泡固定显示 5 秒后淡出，点它立刻关闭；同一时刻只有一条，新的直接替换旧的；
- 与气泡菜单互斥：菜单打开时不显示气泡；
- **不改变桌宠窗口尺寸**，它只是窗口内的一块普通 DOM；窗口宽度由当前显示器自动选择；
- 刚启动时不说话：第一帧只用来建立基线，否则一开机就会「报告」你正在做什么；
- 关掉开关后 Rust 侧连前台窗口都不读；设置窗口的开关跟着 `settings.json` 落盘。

> 开发时验「离开」这条不必真等 5 分钟：`$env:YACHIYO_PROACTIVE_IDLE_MS = 10000` 后 `pnpm tauri dev`，
> 空闲阈值就临时变成 10 秒（只在开发版读这个变量）。日志里会打印前台切到了哪个程序、
> 说了哪句话、气泡是被点掉还是自己淡出的。

### 设置文件

设置会落盘、重启后保留（v1 遗留的「重启恢复默认值」缺口已修掉）。文件在 Tauri 的应用数据目录：

```text
%APPDATA%\com.yachiyo.desktop\
├─ settings.json            桌宠设置
├─ ai.json                  AI 配置（服务商 / 模型 / Base URL / 模式）
└─ sessions\<sessionId>.json 对话历史（一个会话一个文件）
```

```json
{
  "schemaVersion": 1,
  "data": {
    "maxFps": 30,
    "alwaysOnTop": true,
    "proactive": { "enabled": true }
  }
}
```

- 文件里只放**跨重启有意义**的三项：`maxFps`（只接受 15 / 30）、`alwaysOnTop` 与 `proactive.enabled`。显示/隐藏属于会话状态，启动时一律可见——否则托盘一旦创建失败，窗口既不在屏幕上也没法从托盘找回来。
- 可以手工编辑，重启后生效。非法值（例如 `maxFps: 60`）会被拒绝并回退默认值，同时在启动时提示原因。
- 写入是原子的：先写 `settings.json.tmp`，把旧内容留一份 `settings.json.bak`，最后改名覆盖。
- 读取顺序是主文件 → `.bak` → 默认值。**文件损坏不会让程序起不来**，而且会保留现场（不覆盖你改坏的那份），只上报原因。
- 启动期（页面还没挂载）的提示会先排队，等主窗口加载完成再显示，避免那句话落到没人听的地方。

`ai.json` 同样的写法与同样的容错（原子写、`.bak`、损坏回退、手工编辑后非法值会被拒绝并说明原因）。它**不含任何密钥**：

```json
{
  "schemaVersion": 1,
  "data": {
    "provider": "deepseek",
    "model": "deepseek-chat",
    "baseUrl": "https://api.deepseek.com/v1",
    "mode": "chat"
  }
}
```

### 托盘与常驻

- 托盘菜单：显示/隐藏、置顶、四种表情、30/15 FPS、退出
- 关闭窗口 = 隐藏（仅当托盘创建成功；创建失败时保留原生边框与任务栏，避免无法找回）
- 隐藏后仍可从托盘恢复
- **主动隐藏或最小化时停止模型更新与画布绘制**，恢复时重置动画时钟

> 菜单不再改变窗口尺寸，因此"屏幕空间不足时翻到左侧 / 紧凑排列"这套逻辑已废弃（详见下方"已知问题"）。

## 常用命令

```powershell
pnpm test                                          # 前端单测（node --test）
pnpm build                                         # 构建前端到 dist/
cargo test  --manifest-path src-tauri/Cargo.toml   # Rust 单测
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
cargo fmt   --manifest-path src-tauri/Cargo.toml -- --check
pnpm tauri dev                                     # 开发运行
pnpm tauri build --bundles nsis                    # 打 NSIS 安装包
```

打包产物：`src-tauri/target/release/bundle/nsis/yachiyo-desktop_0.1.0_x64-setup.exe`

> **`bundle.active` 目前是 `false`**，但显式传 `--bundles nsis` 仍会打包（已实测）。如果要让不带参数的 `pnpm tauri build` 也生成安装包，把它改成 `true`。
> **网络**：首次 NSIS 打包需下载工具链（约 14 MB）。若 GitHub 超时，设 `TAURI_BUNDLER_TOOLS_GITHUB_MIRROR` 指向可用镜像后重试（例如 `https://ghproxy.net`）。
> **测试务必用安装版**：模型资源经 `bundle.resources` 释放到安装目录，直接跑 `target/release/yachiyo-desktop.exe` 会因为找不到模型而报错。

## 项目结构

```text
app/
├─ index.html                     网页入口（主窗口加载 Live2D Core）
├─ vite.config.js                 开发端口 1420 与 Vue 构建
├─ patches/                       easy-live2d 依赖补丁（pnpm 安装时自动应用）
├─ public/live2d/                 浏览器版 Cubism Core
├─ scripts/                       性能测量与汇总脚本
├─ src/
│  ├─ main.js                     按 ?view=settings / ?view=chat 分流三个窗口
│  ├─ App.vue                     桌宠窗口布局、菜单状态、动作分发
│  ├─ SettingsWindow.vue          设置页（含 AI 一节）
│  ├─ ChatWindow.vue              对话窗口
│  ├─ style.css                   全局样式与尺寸（窗口尺寸的唯一来源是同文件 .pet-anchor）
│  ├─ api/pet.js                  桌宠命令与事件，组件不散落命令字符串
│  ├─ api/chat.js                 AI 配置与对话的命令与事件
│  ├─ components/
│  │  ├─ PetStage.vue             Canvas 生命周期、手势、ResizeObserver
│  │  ├─ PetBubbleMenu.vue        气泡菜单
│  │  ├─ DevPanel.vue             仅开发模式的联调面板
│  │  └─ chat/                    对话窗口的零件：消息流、气泡、输入区、
│  │                              工作区栏、工具卡片、写入确认卡片（红绿 diff）
│  ├─ composables/
│  │  ├─ usePet.js                前端状态与事件订阅
│  │  ├─ usePetSettings.js        设置快照（跨窗口共享）
│  │  ├─ useAiConfig.js           AI 配置快照（设置窗口与对话窗口共享）
│  │  ├─ useChat.js               会话、消息、流式增量（50ms 合批）
│  │  ├─ useWorkspace.js          Agent 模式的工作区条目 + 拖拽投放
│  │  ├─ workspace.js             工作区条目的纯逻辑（权限标签、拖拽路径过滤）
│  │  ├─ chatMessages.js          消息列表的纯函数操作
│  │  ├─ deltaBatch.js            delta 合批（纯函数，定时器可注入）
│  │  ├─ aiConfig.js              服务商预设与快照版本比较
│  │  ├─ petSettings.js           快照版本比较，防旧事件覆盖新值
│  │  └─ petMenu.js               菜单结构与状态机（纯函数）
│  ├─ interactions/petGesture.js  单击/拖动判定（纯函数）
│  └─ live2d/
│     ├─ controller.js            加载、表情、缩放定位、暂停与释放
│     ├─ modelConfig.js           模型文件预检与布局计算
│     ├─ mouseFollow.js           鼠标采样与跟随目标
│     ├─ renderPolicy.js          由可见性与设置推导是否运行（纯函数）
│     └─ performanceProbe.js      仅测量构建使用的真实 update/draw 计数
└─ src-tauri/
   ├─ tauri.conf.json             窗口、安全策略、打包资源
   ├─ capabilities/               最小权限白名单
   ├─ assets/models/yachiyo/      随安装包分发的模型（moc3 + 4K 贴图 + 表情 + 物理）
   └─ src/
      ├─ lib.rs                   注册命令、状态、托盘
      ├─ commands.rs              前端可调用的 Rust 入口
      ├─ desktop.rs               显示/隐藏/置顶、光标换算、错误上报
      ├─ ai/                      AI 配置、密钥、服务商、SSE 解析、人设提示词
      │  ├─ agent.rs              Agent 循环（最多 8 轮）+ 工具调用分片拼接
      │  ├─ tools.rs              工具表（JSON schema）+ 执行 + 单轮输出预算
      │  └─ writes.rs             写入确认：diff + 原子写 + 等用户点「应用」
      ├─ chat.rs                  对话编排：发送、流式广播、取消、落盘收尾
      ├─ chat_store.rs            会话与消息落盘（sessions/<id>.json）
      ├─ workspace.rs             工作区条目（目录 / 只读文件）+ 每次调用都重做的路径校验
      ├─ fs_tools.rs              受限文件工具：list_files / read_file / grep
      ├─ expression.rs            表情白名单校验
      ├─ settings.rs              运行时设置（Mutex + revision）+ 读写 settings.json
      ├─ store.rs                 通用文件存储：原子写 + .bak + 损坏回退（只用 std，可直接单测）
      ├─ clock.rs                 可注入时钟（Phase 8 番茄钟用；生产用系统时钟，测试手动推进）
      ├─ broadcast.rs             统一事件广播：新窗口只需在标签表里加一行
      ├─ settings_window.rs       设置窗口的单例创建守卫
      ├─ tray.rs                  原生托盘菜单
      └─ audit.rs                 可选性能验收模块（perf-audit 特性）
```

## 架构约定

理解这几点就能看懂大部分代码：

1. **Rust 是唯一事实源。** 任何状态变更先在 Rust 完成，再广播事件；前端只做视图与乐观更新，不持有唯一副本。
2. **窗口操作在 Rust，渲染在 Vue。** 拖动、置顶、显示隐藏、托盘都走原生命令；Canvas、模型、动画循环都在前端。
3. **不逐帧传输模型参数。** 通信只传离散指令（表情名、显示状态、帧率）。
4. **纯逻辑写成纯函数。** `petMenu.js`、`petGesture.js`、`renderPolicy.js`、`modelConfig.js` 都不依赖 Tauri 和系统时间，直接用 `node --test` 覆盖。
5. **命令名与事件名集中在 `src/api/pet.js`**，组件里不出现裸字符串。
6. **不引入额外框架。** 没有 Pinia、Vue Router、UI 组件库；状态用 composable 管理。

### 通信协议

命令（Vue → Rust），参数在 JS 侧用 camelCase：

| 命令 | 参数 | 返回 |
|---|---|---|
| `request_expression` | `{ name }` | — |
| `get_pet_settings` | — | 设置快照 |
| `get_pet_cursor_position` | — | `{ x, y }`（相对窗口客户区的逻辑坐标） |
| `set_pet_visible` | `{ visible }` | 设置快照 |
| `set_pet_max_fps` | `{ maxFps }` | 设置快照（只接受 15 / 30） |
| `set_pet_always_on_top` | `{ enabled }` | 设置快照 |
| `open_pet_settings` | — | — |
| `open_chat_window` | — | — |
| `reload_pet_model` | — | 仅开发版可用 |
| `quit_pet` | — | — |
| `get_ai_config` | — | `{ provider, model, baseUrl, mode, hasKey, keyMask }` |
| `set_ai_config` | `{ patch }` | 同上（`patch` 只带要改的字段） |
| `set_api_key` | `{ provider, key }` | 同上（**不回显 key**） |
| `clear_api_key` | `{ provider }` | 同上 |
| `test_ai_connection` | — | `{ ok, message }` |
| `create_session` | — | `Session` |
| `list_sessions` | — | `Session[]`（最近更新的在前） |
| `get_messages` | `{ sessionId }` | `Message[]` |
| `send_message` | `{ sessionId, text }` | `{ messageId }`（回答走事件陆续到达） |
| `cancel_stream` | `{ sessionId }` | `{ ok }` |
| `list_workspace_entries` | — | `WorkspaceEntry[]` |
| `add_workspace_dir` | — | `WorkspaceEntry[]`（内部弹系统目录选择器） |
| `add_workspace_files` | `{ paths }` | `WorkspaceEntry[]`（拖拽投放，一律只读） |
| `remove_workspace_entry` | `{ id }` | `WorkspaceEntry[]` |
| `apply_pending_write` | `{ requestId }` | `{ ok, message }`（**这时才写盘**） |
| `reject_pending_write` | `{ requestId }` | `{ ok, message }`（什么都不写） |

事件（Rust → Vue）：

| 事件 | 载荷 |
|---|---|
| `pet-expression-requested` | `{ name }` |
| `pet-expression-observed` | `{ name }`（发往设置窗口） |
| `pet-settings-changed` | 设置快照（含 `revision`） |
| `pet-window-minimized` | `boolean` |
| `pet-model-reload` | —（仅开发版） |
| `pet-desktop-error` | 错误字符串 |
| `ai-config-changed` | AI 配置快照（含 `revision`、`keyMask`，不含 key） |
| `chat-stream-started` | `{ sessionId, messageId }` |
| `chat-stream-delta` | `{ sessionId, messageId, text }`（前端按 50ms 合批） |
| `chat-stream-finished` | `{ sessionId, messageId }` |
| `chat-stream-failed` | `{ sessionId, messageId, error }` |
| `chat-tool-call` | `{ sessionId, messageId, callId, name, label, args }` |
| `chat-tool-result` | `{ sessionId, messageId, callId, name, ok, summary }` |
| `chat-write-request` | `{ sessionId, messageId, requestId, path, diff, reason }` |
| `workspace-changed` | `WorkspaceEntry[]` |

设置快照带 `revision`，前端只接受不小于当前值的快照，避免初始化时用旧值覆盖新事件。

## 性能

Release 构建实测（Windows 11、i5-13420H、Intel UHD、1920×1200 @125%，探针统计真实模型 `update`/`draw` 调用）：

| 状态 | 更新/绘制（次/秒） | 平均 CPU（整机口径） |
|---|---|---|
| 可见 30 FPS | 29.49 / 29.49 | 4.64% |
| 可见 15 FPS | 14.90 / 14.90 | 3.17% |
| 隐藏 | 0 / 0 | 采样精度下约 0% |
| 最小化 | 0 / 0 | 采样精度下约 0% |

30 → 15 FPS 平均 CPU 下降约 32%。暂停期间的更新/绘制增量实测为 0，恢复首帧模型步长为 0 秒，全程模型实例 ID 一致。

完整数据、测量口径与未完成项见 [Release 实测记录](../docs/performance/2026-09-10/README.md)。**注意**：30 分钟连续可见待机验收被中途的隐藏操作打断，记录标记为 `continuousVisibleIdlePassed: false`，尚无长期内存稳定性结论。

### 加入对话 / 主动互动 / Agent 之后

上表是这些功能加入**之前**测的。为了回答「加了这么多之后常驻开销有没有变重」，
在一台 24 线程的机器上做了开/关对照（同一条可见待机路径，30 FPS，各 45 秒）：

| 构建 | 状态 | 45 秒 CPU 时间 | **占一个核心** |
|---|---|---|---|
| debug | 可见待机 | 2750 ms | 6.1% |
| debug | 可见待机，主动互动**关** | 2594 ms | 5.8% |
| release | 可见待机 | 2109 ms | **4.7%** |

结论：

1. **主动互动几乎不花钱**——开与关只差 0.35% 的一个核心，与两次测量之间的波动同量级。
   它的活儿是每 5 秒读四个系统 API，量级本来就可以忽略。
2. **主要的常驻开销仍然是 Live2D 渲染本身**，而那部分代码没有改动。
3. 可见待机的常驻开销与加入这些功能之前处在同一水平。

> **口径说明**：这台机器有 24 个逻辑核心，而上面那张表是 12 线程机器上的「整机口径」，
> **两者不能直接比**。可跨机器比较的是「占一个核心多少」——release 构建约 4.7%。
> 这组数字也没有用 `perf-audit` 探针（那是上面那张表的口径），只是采样进程 CPU 时间，
> 用来做开/关对照。**Agent 模式下的工具调用与文件读写没有计入**，那是按需触发、不常驻的路径。

复现测量：

```powershell
$env:VITE_PERF_AUDIT='1'
pnpm tauri build --no-bundle --features perf-audit
Remove-Item Env:VITE_PERF_AUDIT
./scripts/measure-performance.ps1 -OutputDirectory ../docs/performance/my-run
node scripts/summarize-performance.mjs ../docs/performance/my-run
```

普通发布构建不启用 `perf-audit` 特性、不设置 `VITE_PERF_AUDIT`，因此不含探针模块和上报定时器。

## 已知问题与注意事项

这些都是**当前真实存在的状态**，不是待办清单里的设想：

1. **主窗口宽度按显示器自动切换**：物理宽度低于 2560px 时为 340×480（Canvas 324px 宽），达到 2560px 时为 520×480（Canvas 504px 宽）。Rust 在启动、跨屏移动和 DPI 变化时更新原生窗口；CSS 的 `.pet-anchor` 使用 `width: 100%` 跟随窗口。高度仍由 `.pet-anchor`（440px）、`.pet-view`（394px）和开发模式 `.development-stage .pet-view`（368px）共同约束。
2. **模型缩放是硬编码的**：`live2d/controller.js` 里 `modelZoom = 4` 与 `model.position.set(layout.x, layout.y + 140)`。窗口尺寸变化时 `fitModel` 会重算缩放并把模型居中，`+140` 的偏移因此是脆弱的。
3. **Rust 侧的 `menu_layout.rs` 已被删除**，但 `tauri.conf.json` 里仍有与之对应的历史痕迹；菜单相关的窗口几何逻辑现在完全不存在。
4. **`csp` 为 `null`**（`tauri.conf.json`）。当前应用纯本地、无外联，风险可控；公开发布前建议收紧。
5. **`bundle.active` 为 `false`**（`tauri.conf.json`），因此不带参数的 `pnpm tauri build` 只产出裸 exe；要生成安装包需显式传 `--bundles nsis`。
6. **依赖补丁不能丢**：`patches/easy-live2d@0.4.4.patch` 修复了上游库的加载错误传播、并行贴图失败时的清理时序、底层模型释放与动画时钟。请保留补丁文件、`pnpm-workspace.yaml` 与锁文件。
7. **`pnpm test` 的脚本是显式文件列表**，新增测试目录时要同步修改 `package.json`。
8. **未完成的人工验收项**：托盘完整交互、手动拖动、系统关闭按钮转隐藏、跨不同缩放显示器的拖动（本机只有一个显示器）。另外 `settings.json` 的 `.bak` 回退路径目前只有单元测试覆盖——`.bak` 只在设置真正变更时才产生，需要点托盘或设置窗口才能触发，尚未用安装版端到端演练。
9. **对话功能需要你自己的 API Key**：仓库里没有任何密钥，Key 只存在 Windows 凭据管理器里。「测试连接」会真的发一次最小请求（8 token），确认地址、Key、模型三者对得上。
10. **Agent 模式已经能用，但边界要记住**：它只能碰你在工作区里显式给的那几样（最多 32 条），
    文件条目永远只读；目录条目的写入每次都要你点「应用」。仍然没有的是：撤销（写下去就写下去了，
    用 git 回滚）、逐块采纳、并发冲突检测。另外拖入一个目录会被拒绝——加目录请用「添加文件夹」，
    那条路才会走写入确认。

## 相关文档

### 学习说明（按顺序读）

1. [第一步：启动链与环境配置](../docs/learning/01-startup.md)
2. [第二步：双向通信](../docs/learning/02-bridge.md)
3. [第三步：模型接入](../docs/learning/03-model.md)
4. [第四步：窗口与托盘](../docs/learning/04-desktop.md)
5. [第五步：控制常驻开销](../docs/learning/05-performance.md)
6. [鼠标跟随](../docs/learning/06-mouse-follow.md)
7. [气泡菜单](../docs/learning/07-bubble-menu.md)

> 注意：第 4、7 篇写于菜单还是"窗口展开"模式时，其中"菜单翻到左侧 / 紧凑排列"的描述已不适用。

### 设计与计划

- [最小实现设计草案](../docs/superpowers/specs/2026-09-10-yachiyo-mvp-design.md)
- [六阶段实施计划](../docs/superpowers/plans/2026-09-10-yachiyo-mvp.md)
- [气泡菜单计划](../docs/superpowers/plans/2026-09-11-bubble-menu.md)
- [v2 设计方案：番茄钟 · 养成 · AI Agent · 工作区对话](../docs/superpowers/specs/2026-09-12-yachiyo-companion-v2-design.md)
- [Release 性能实测记录](../docs/performance/2026-09-10/README.md)

## 许可

模型资源（`yachiyo`）与 Live2D Cubism Core 的著作权归各自权利人所有，商用前需确认授权条款。
