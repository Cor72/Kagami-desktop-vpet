# 八千代桌宠最小实现 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 与用户逐步做出一个能看懂、能修改、可长期常驻的 Windows Live2D 桌宠，并掌握 Vue/JavaScript 与 Rust 的双向通信。

**Architecture:** 参考 BongoCat 的 Vue 页面、JavaScript 模型封装、Rust 系统能力分工，在独立的 app/ 工程中实现。第一版只有一个窗口；Rust 创建托盘，Live2D 更新与绘制共用一个 ticker。用薄接口为以后聊天与角色行为保留接入点。

**Tech Stack:** Rust、Tauri 2、Vue 3、JavaScript、Vite、PixiJS 8、easy-live2d、Cubism Core、pnpm、FFmpeg。

**Spec:** [设计草案](../specs/2026-09-10-yachiyo-mvp-design.md)

**状态：** 2026-09-10 已实现 Task 4：透明窗口、拖动入口、Rust 托盘、共享设置与事件同步。JavaScript 7 个测试、Rust 5 个测试、Clippy 和构建通过。透明显示、Vue 置顶/帧率/表情操作已验证；托盘完整交互、拖动、关闭恢复和退出待用户手动验收，尚未开始 Task 5。

**执行方式：** 选择 executing-plans，在当前对话逐阶段结对完成。每阶段先解释目标，完成后演示、讲解、交给用户做一个小修改。此任务以用户学习和掌握代码为目标，不采用并行分派，也不自动连续完成六阶段。

## Global Constraints

- 第一版仅支持 Windows 桌面。
- 技术栈固定为 Rust + Tauri 2 + Vue 3 + JavaScript。
- 新工程位于工作区的 app/；.gitignore/BongoCat/ 保留为参考源码，yachiyo/ 保留为原始模型资源。
- 第一版仅创建一个 main 桌宠窗口，不常驻第二个设置窗口。
- 角色固定为八千代，使用现有 .moc3、模型配置、物理配置和四个表情。
- 不重新实现 Live2D 渲染器，使用 PixiJS 8 + easy-live2d 及兼容的 Cubism Core。
- 普通模式上限 30 FPS，省电模式上限 15 FPS；主动隐藏和最小化时停止模型更新与画布绘制。
- 不依赖窗口失焦判定休眠；桌宠未获得焦点时仍应正常显示。
- 保留原始 8K 贴图，先生成两张 4K RGBA 贴图用于运行，结合实际观感评估 2K。
- 不实现聊天、语音、长期记忆、全局键鼠跟踪、手柄、多模型管理、开机启动或自动更新。
- 每阶段交付可运行结果、必要验证与一次可由用户完成的小修改练习。
- 每次只推进一个阶段；阶段完成后讲解调用链并让用户体验，再继续下一阶段。

---

## 六个阶段：每次只解决一个主要问题

| 阶段 | 能看到的结果 | 用户学到什么 |
|---|---|---|
| 1. 跑通桌面外壳 | 一个普通窗口，里面是自己的 Vue 页面 | 哪个文件启动 Rust，哪个文件启动 Vue |
| 2. 跑通双向通信 | 点击“微笑”，Rust 校验，Vue 收到事件并显示结果 | invoke、command、参数、返回值、emit、listen、错误定位 |
| 3. 让八千代出现 | 模型显示，有待机表现，按钮能切换真实表情 | Canvas、模型资源、渲染封装、资源加载失败排查 |
| 4. 变成桌宠 | 透明无边框、可拖动、托盘控制、可退出 | 网页事件怎样操作原生窗口，哪些功能由 Rust 负责 |
| 5. 控制常驻开销 | 30/15 FPS，隐藏和最小化停更，恢复后正常 | 更新与绘制循环、生命周期、开发版与发布版性能区别 |
| 6. 打包与代码交接 | 可独立启动的安装包、性能记录、源码阅读指南 | 构建流程、日志、自己改代码后如何验证 |

不设固定天数：每阶段以功能验收和用户理解程度决定是否继续。

## 文件结构与职责

文件按阶段逐步创建，不在第一阶段一次生成全部空目录。

```text
D:/AnChiProject/yachiyodesktop/
├─ .gitignore/BongoCat/                         参考工程
├─ yachiyo/                          原始模型
├─ app/
│  ├─ package.json                  pnpm 脚本和前端依赖
│  ├─ index.html                    网页入口与 Core 加载
│  ├─ vite.config.js                开发端口和 Vue 构建
│  ├─ jsconfig.json                 JavaScript 编辑器配置
│  ├─ public/live2d/                兼容的浏览器 Core
│  ├─ src/
│  │  ├─ main.js                    挂载 Vue
│  │  ├─ App.vue                    组织页面
│  │  ├─ style.css                  透明背景、布局
│  │  ├─ components/PetStage.vue    Canvas 与生命周期
│  │  ├─ components/DevPanel.vue    仅开发模式显示的联调面板
│  │  ├─ api/pet.js                集中封装 invoke、listen 与协议类型
│  │  ├─ composables/usePet.js      前端状态、事件订阅、错误显示
│  │  └─ live2d/
│  │     ├─ controller.js          加载、表情、缩放、暂停与释放
│  │     ├─ renderPolicy.js        从可见性和设置推导是否运行
│  │     └─ renderPolicy.test.js   验证暂停条件和帧率
│  └─ src-tauri/
│     ├─ Cargo.toml                Rust 依赖与构建配置
│     ├─ build.rs                  Tauri 构建入口
│     ├─ tauri.conf.json           一个 main 窗口及资源配置
│     ├─ capabilities/default.json
│     ├─ assets/models/yachiyo/    用于分发的模型与 4K 贴图
│     └─ src/
│        ├─ main.rs               启动 Rust
│        ├─ lib.rs                注册命令、状态与托盘
│        ├─ commands.rs           前端可调用的 Rust 入口
│        ├─ expression.rs         表情白名单校验和单元测试
│        ├─ settings.rs           显示、帧率、置顶的运行时状态
│        ├─ desktop.rs            显示/隐藏/置顶与窗口事件
│        └─ tray.rs               原生托盘菜单
└─ docs/
   ├─ superpowers/                 设计与计划
   └─ learning/
      ├─ 01-startup.md
      ├─ 02-bridge.md
      ├─ 03-model.md
      ├─ 04-desktop.md
      ├─ 05-performance.md
      └─ 06-build-and-edit.md
```

第一版不引入 Vue Router、Pinia、完整 UI 组件库、Rust workspace 或自定义 Tauri 插件。只保留目前用得到的模块；两个语言的依赖各由 package.json 和 src-tauri/Cargo.toml 管理，提交实际生成的锁文件。

## Task 1：跑通桌面外壳

**文件：** 创建 app/ 的 package.json、index.html、vite.config.js、jsconfig.json、src/main.js、src/App.vue、src/style.css，以及 src-tauri 的 Cargo.toml、build.rs、tauri.conf.json、capabilities/default.json、src/main.rs、src/lib.rs；创建 docs/learning/01-startup.md。

**接口：** 一个标签为 main 的窗口；Vue 挂载到 #app；开发页面端口为 1420，生产资源目录为 dist。

- [x] 检查 Rust stable MSVC 工具链、MSVC C++ 构建工具、Windows SDK、WebView2 Runtime，以及 Node/pnpm。用户完成 Rust 安装并验证基础编译；实际 Tauri 编译和桌面运行也已通过。
- [x] 创建最小 Tauri 2 + Vue 3 + JavaScript 工程。锁定依赖，生成 pnpm-lock.yaml 和 Cargo.lock。项目内使用 HTTPS sparse Cargo 镜像，未修改用户的全局 Cargo 配置。
- [x] 页面只显示“八千代桌宠”和一个本地计数按钮。使用普通有边框窗口，初始尺寸 360×540 个逻辑像素。
- [x] 在 package.json 中设置下列入口，启动实际桌面程序：

```json
{
  "scripts": {
    "dev": "vite --port 1420 --strictPort",
    "build": "vite build",
    "tauri": "tauri"
  }
}
```

```powershell
# 工作目录：D:/AnChiProject/yachiyodesktop/app
pnpm tauri dev
```

- [x] 验收：实际桌面窗口启动；按钮从 0 增至 1；修改 Vue 标题后热更新且计数保留；恢复文案；Ctrl+C 后桌宠进程和窗口退出。pnpm build、cargo fmt --check 通过。
- [x] 创建 [第一步学习说明](../../learning/01-startup.md)，说明两条启动链、文件职责、开发命令与环境配置。
- [x] 用户已确认理解两条启动链和本地库，并要求进入第二阶段。修改文案和颜色的练习保留在第一步说明中，可随时回看。

## Task 2：完整体验 Vue ↔ Rust 通信

**文件：** 创建 src/api/pet.js、src/composables/usePet.js、src/components/DevPanel.vue、src-tauri/src/commands.rs、src-tauri/src/expression.rs、docs/learning/02-bridge.md；修改 App.vue、lib.rs。

**消费：** Task 1 的 main 窗口。

**产出协议：**

```javascript
export const PET_EXPRESSIONS = ['smile', 'squint', 'tears', 'teardrop']

// Command：request_expression，参数示例：{ name: 'smile' }
// 成功不返回数据；失败由 invoke 的 Promise reject 传递字符串错误。
// Event：pet-expression-requested，载荷示例：{ name: 'smile' }
```

- [x] 先为 Rust 白名单校验编写两个有实际意义的测试：现有表情通过，未知表情被拒绝。观察到未知表情测试失败，补齐校验后两个测试通过，同时覆盖空字符串、大小写和空格不匹配。

```rust
#[test]
fn accepts_existing_expression() {
    assert!(validate_expression("smile").is_ok());
}

#[test]
fn rejects_unknown_expression() {
    assert!(validate_expression("unknown").is_err());
}
```

```powershell
# 工作目录：D:/AnChiProject/yachiyodesktop/app
cargo test --manifest-path src-tauri/Cargo.toml
```

- [x] 在 expression.rs 中定义 validate_expression(name: &str) -> Result<(), String>；仅允许四个既有名称，错误文字为“未知表情: 名称”。commands.rs 中实现并注册以下命令。校验失败时不发送事件。

```rust
use serde::Serialize;
use tauri::Emitter;
use crate::expression::validate_expression;

#[derive(Clone, Serialize)]
pub struct ExpressionRequested {
    pub name: String,
}

#[tauri::command]
pub fn request_expression(app: tauri::AppHandle, name: String) -> Result<(), String> {
    validate_expression(&name)?;
    app.emit_to("main", "pet-expression-requested", ExpressionRequested { name })
        .map_err(|error| error.to_string())
}
```

- [x] 在 api/pet.js 中集中封装通信。组件不散落命令字符串。

```javascript
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

export const requestExpression = name =>
  invoke('request_expression', { name })

export const onExpressionRequested = handler =>
  listen('pet-expression-requested', event => {
    handler(event.payload.name)
  })
```

- [x] 在 usePet.js 中先 await 事件订阅成功，再启用面板按钮；组件卸载时调用返回的 unlisten。若订阅尚未完成就卸载，在异步完成时立即取消该订阅。用 try/catch 展示错误，取消订阅失败记录日志，防止未处理的 Promise。
- [x] 面板分别显示“请求已发送”和“收到 Rust 事件：smile”，明确命令成功返回与事件到达是两件事。面板仅开发模式可见，此时尚未加载 Live2D。
- [x] 验收：用户手动点击微笑和 unknown 并确认符合预期；助手读取到“收到 Rust 事件：smile”“共 1 次”“未知表情: unknown”，终端记录了两次 Rust 请求。Rust println! 在开发终端查看，console.log 的 DevTools 查看方法已写入学习说明。桌面自动化点击受工具保护拦截，本次点击由用户完成。
- [x] 创建 [第二步学习说明](../../learning/02-bridge.md)，讲解 Rust 语法、完整调用链和参数名不一致、未注册命令、监听未就绪三个排查入口。
- [ ] 用户学习练习：修改 Rust 返回的错误文字，观察桌面应用重新编译，再在 Vue 中调整错误提示的显示方式。

## Task 3：加载八千代，并让通信控制真实表情

**文件：** 创建 src/components/PetStage.vue、src/live2d/controller.js、src-tauri/assets/models/yachiyo/、public/live2d/、docs/learning/03-model.md；修改 package.json、index.html、App.vue、usePet.js、tauri.conf.json、capabilities/default.json。

**消费：** Task 2 的表情名称白名单与 pet-expression-requested 事件。

**产出接口：**

```javascript
// createPetController({ canvas, modelDirectory }) 异步返回 controller。
// canvas 是 Canvas 元素，modelDirectory 是模型目录字符串。
// controller 提供下列方法：
// setExpression(name)：异步切换表情，name 必须在表情白名单中。
// resize(width, height)：调整尺寸，参数单位为像素。
// setRunning(running)：布尔值决定启动或暂停。
// setMaxFps(fps)：只接受 15 或 30。
// destroy()：释放资源。
```

- [x] 仅复制 .moc3、model3.json、physics3.json、cdi3.json、四个 exp3.json 到分发资源目录；保留相对贴图路径。原始 yachiyo/ 不覆盖。
- [x] 为两张贴图分别生成 4K RGBA 副本。以下命令从工作区根目录运行，输出目录先创建，不覆盖输入文件：

```powershell
ffmpeg -n -i ./yachiyo/yachiyo.8192/texture_00.png -vf "scale=4096:4096:flags=lanczos,format=rgba" -frames:v 1 ./app/src-tauri/assets/models/yachiyo/yachiyo.8192/texture_00.png
ffmpeg -n -i ./yachiyo/yachiyo.8192/texture_01.png -vf "scale=4096:4096:flags=lanczos,format=rgba" -frames:v 1 ./app/src-tauri/assets/models/yachiyo/yachiyo.8192/texture_01.png
```

- [x] 核实 BongoCat 使用的 PixiJS 8 / easy-live2d 0.4 系列组合，锁定实际使用版本。检查 Core 对该 .moc3 的兼容性；使用匹配的官方 Core。模型是 Cubism 3+ 格式，暂不引入旧 Cubism 2 的运行链路。
- [x] 参考 BongoCat 的 load 流程：resolveResource 获取打包资源目录、读取固定 yachiyo.model3.json、CubismSetting.redirectPath + convertFileSrc 解析关联文件、创建 Live2DSprite、等待 ready。文件读取与 asset scope 只覆盖模型资源目录。
- [x] 创建一个 Pixi Application，将其 ticker 传给 Live2DSprite，更新与绘制共用该 ticker；从本阶段开始限为 30 FPS。初始渲染分辨率系数使用 1，按实际清晰度评估，不直接跟随所有高 DPI 倍率。
- [x] 事件到达后，按模型配置的 Name 查找表情再调用 setExpression，避免写死数字下标。模型未就绪时只保留最后一个表情请求；加载完成后应用一次。
- [x] 检查运行库的自动眨眼/呼吸能力。若自动呼吸没有驱动本模型，则在同一个 ticker 中驱动 ParamBreath，不另外创建定时器；先关闭相同参数的重复驱动。没有动作文件，不把播放 Idle motion 作为本阶段前提。
- [x] 验收：四个表情能切换；透明纹理边缘正常；缺少模型文件时显示路径与错误，能够重试或退出；卸载组件后不重复加载实例。对比 4K 与原始模型的脸部细节，决定是否值得另测 2K。
- [ ] 学习练习：用户修改角色显示尺寸，并沿 DevPanel → api/pet.js → commands.rs → 事件监听 → controller.js 追踪一次真实表情变化。

**实现说明：** 使用 easy-live2d 0.4.4、PixiJS 8.20.1、原有 Cubism Core 5.1.0（实际验证支持模型的 Moc 版本 5）。依赖补丁补齐加载错误传播与底层模型释放；重试创建新 Canvas，避免复用已释放的 WebGL 上下文。使用 Node 内置测试运行器测试模型路径、表情映射和布局。4K/8K 在当前显示尺寸下未见明显脸部差异，暂不继续制作 2K。详情见 [第三步学习说明](../../learning/03-model.md)。

## Task 4：透明桌宠窗口与原生托盘

**文件：** 创建 src-tauri/src/settings.rs、src-tauri/src/desktop.rs、src-tauri/src/tray.rs、docs/learning/04-desktop.md；修改 lib.rs、commands.rs、Cargo.toml、tauri.conf.json、capabilities/default.json、api/pet.js、usePet.js、PetStage.vue、style.css。

**消费：** Task 3 的 controller.js 和 Task 2 的表情校验规则。

**新增协议：**

```javascript
// 设置快照的数据形状（普通 JavaScript 对象）：
const exampleSettings = {
  revision: 0,
  visible: true,
  maxFps: 30,
  alwaysOnTop: true,
}

// get_pet_settings()：返回设置快照。
// set_pet_visible({ visible: true })：返回更新后的设置快照。
// set_pet_max_fps({ maxFps: 15 })：只允许 15/30，返回设置快照。
// set_pet_always_on_top({ enabled: true })：返回设置快照。
// Event：pet-settings-changed，载荷是上面的设置快照。
```

- [x] Rust 使用一个受 Mutex 保护的运行时设置对象，初始为 revision=0、visible=true、maxFps=30、alwaysOnTop=true。窗口操作成功后才提交设置并增加 revision；解锁后发送事件。命令和托盘调用同一组操作函数，防止有两套行为。
- [x] 前端先订阅设置事件，再读取初始快照；只接受 revision 不小于当前值的快照，避免异步初始化用旧值覆盖新事件。本阶段只保存在内存中，重启恢复默认值。
- [x] 将主窗口改为透明、无边框、隐藏任务栏按钮。保持窗口尺寸接近角色，不使用全屏透明 Canvas。窗口背景、html/body、Vue 容器与 Pixi 背景都支持透明。
- [x] 在画布拖拽区域调用 Tauri startDragging；开发面板按钮不触发窗口拖拽。置顶调用系统能力，不创建反复设置置顶的后台循环。
- [x] 使用 Rust 创建托盘：显示/隐藏、置顶开关、四种表情、30/15 FPS、退出。表情项发送与 Task 2 相同的事件。托盘不依赖 Vue 设置窗口；退出调用应用退出流程。
- [x] 创建托盘成功后才启用“关闭窗口改为隐藏”的行为；如果托盘创建失败，保持窗口可见并提供可用退出入口，避免用户无法恢复应用。
- [ ] 验收：透明背景、拖动、显示恢复、置顶切换与退出有效；隐藏后托盘仍可操作；先在用户当前缩放比例下测试，再测试跨不同缩放显示器的拖动。若只有一个显示器，记录该项未覆盖。
- [ ] 学习练习：用户修改一个 Rust 托盘菜单名称，并增加一次操作日志，理解原生菜单为何不写在 Vue 页面中。

**实现说明：** 窗口为 340×440 个逻辑像素，开发联调面板覆盖在同一个窗口中。设置使用 Mutex 保护，通过 revision 防止旧快照覆盖新状态；FPS 已接到控制器，隐藏/最小化的统一停更策略在 Task 5。托盘创建失败时恢复原生边框和任务栏并禁止隐藏。说明见 [第四步学习说明](../../learning/04-desktop.md)。

## Task 5：控制更新频率与常驻开销

**文件：** 创建 src/live2d/renderPolicy.js、src/live2d/renderPolicy.test.js、docs/learning/05-performance.md；修改 controller.js、usePet.js、desktop.rs、package.json。

**消费：** 设置快照、controller 与页面可见性。

**策略：** running = Rust 的 visible && document.visibilityState === 'visible'。maxFps 始终为用户选定的 15 或 30。原生最小化/恢复与页面 visibilitychange 都接入同一策略，不依据 blur/focus 暂停。

- [ ] 为纯函数 selectRenderPolicy 写以下测试，再实现函数。Vitest 仅用于这类有独立行为价值的测试，不为按钮、样式或框架包装编写重复测试。

```javascript
import { expect, test } from 'vitest'
import { selectRenderPolicy } from './renderPolicy'

test('隐藏时停止，保留用户帧率设置', () => {
  expect(selectRenderPolicy(false, true, 30))
    .toEqual({ running: false, maxFps: 30 })
})

test('页面不可见时停止', () => {
  expect(selectRenderPolicy(true, false, 15))
    .toEqual({ running: false, maxFps: 15 })
})

test('恢复可见后按省电设置运行', () => {
  expect(selectRenderPolicy(true, true, 15))
    .toEqual({ running: true, maxFps: 15 })
})
```

```javascript
export function selectRenderPolicy(nativeVisible, pageVisible, maxFps) {
  return { running: nativeVisible && pageVisible, maxFps }
}
```

- [ ] setRunning(false) 停止共用 ticker，而非仅隐藏 DOM；setRunning(true) 重置时间基准再启动。确认眨眼、物理和绘制没有第二条独立更新循环。
- [ ] 临时记录模型更新次数与绘制次数：30/15 FPS 上限都应作用于两者；隐藏稳定后两者不再增长。显示恢复时没有大时间步长造成的模型跳跃。
- [ ] destroy 取消全部监听和 resize 回调，销毁模型、纹理、Application；不卸载后仍保留定时器。反复显示/隐藏保持同一份模型，不借每次重新加载实现暂停。
- [ ] 用 Release 可执行程序测量，关闭 DevTools；记录主程序及所属 WebView2 进程，并单独记录 GPU 内存。记录硬件、Windows 缩放、角色尺寸、贴图版本、FPS 与采样时长。
- [ ] 验收：30 和 15 FPS 各采样 2 分钟；隐藏 2 分钟；进行 20 次显示/隐藏；可见待机 30 分钟。检查停更是否生效、恢复是否正常、内存是否持续单向增长。CPU/GPU 没有下降时检查实际 ticker、事件与本地线程，不能只根据框架名称宣布优化成功。
- [ ] 学习练习：用户在 30/15 FPS 间切换，查看更新计数与进程占用的区别；解释 PNG 文件大小、纹理内存、CPU 和 GPU 指标各代表什么。

## Task 6：打包、验收与用户修改指南

**文件：** 创建 docs/learning/06-build-and-edit.md；修改 app/package.json、src-tauri/tauri.conf.json、App.vue 和必要的第三方说明文件；补充性能记录。

**消费：** 前五阶段全部能力。

- [ ] DevPanel 仅在 import.meta.env.DEV 为 true 时挂载，发布版保留必要的加载错误提示与托盘控制。使用脚手架默认图标完成可运行交付，图标设计独立于本次最小实现。
- [ ] 将模型与贴图加入 Tauri bundle.resources；确认从安装目录启动时 resolveResource 正确，不依赖开发目录的绝对路径。保留复用代码和 Core 的许可声明。
- [ ] 运行以下质量检查和 Windows 安装包构建：

```powershell
# 工作目录：D:/AnChiProject/yachiyodesktop/app
pnpm build
pnpm exec vitest run
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
pnpm tauri build --bundles nsis
```

- [ ] 在开发服务器关闭的条件下启动安装版，验证模型、表情、拖动、托盘、暂停恢复与退出。此阶段为本机安装包验收，不做公网发布或自动更新。
- [ ] 阅读指南写明：改文案看 App.vue/DevPanel.vue；改初始帧率看 settings.rs；改角色呈现看 controller.js；加一个原生命令看 commands.rs + lib.rs + api/pet.js；终端日志与 WebView 日志分别在哪里。
- [ ] 用户完成一次小修改并重新构建，能说明这次修改涉及哪个语言、哪个文件、怎样验证。之后再讨论聊天模块，不把新功能混入最小实现验收。

## 结对协作约定

1. 每阶段开始，先用几句话说明本阶段最多需要理解的两三个概念。
2. 助手先完成必要脚手架与重复操作，关键通信和渲染代码逐段解释；避免一次给用户几十个陌生文件。
3. 在边界处写中文注释，解释为什么这样做；不为每条显而易见的语句重复翻译。
4. 阶段结束给出运行命令、看到的结果、关键文件入口和一个 5～10 分钟的小练习。
5. 遇到用户不理解的部分，先把这一段讲明白，再继续。用户明确要求加速时可以合并阶段。
6. 每阶段确认通过后，可将该阶段相关文件单独提交；提交前检查暂存范围，不把原始模型和参考工程意外全部加入。推送或发布不属于当前计划。

## 自查结果

- 功能覆盖：显示与待机在 Task 3；拖动、透明窗口、托盘和退出在 Task 4；帧率与停更在 Task 5；独立启动在 Task 6。
- 学习覆盖：启动链在 Task 1；双向联调与错误处理在 Task 2；同一条链连接真实角色在 Task 3。
- 成长空间：表达角色动作的接口与渲染实现分开；当前不创建空聊天服务或通用插件框架。
- 接口一致：表情协议统一使用 name；设置帧率统一使用 maxFps，Rust 字段通过 serde camelCase 序列化；事件名称在 api/pet.js 集中定义。
- 性能结论：计划只指定行为目标和测量方法，当前没有性能实测数据。

## 参考入口

- 本地 .gitignore/BongoCat/src/utils/live2d.ts：资源加载和模型封装。
- 本地 .gitignore/BongoCat/src-tauri/src/lib.rs：Tauri 应用入口与命令注册。
- 本地 .gitignore/BongoCat/src/composables/useDevice.ts：invoke 与事件监听示例；本项目不复用其全局输入业务。
- [Tauri：前端调用 Rust](https://v2.tauri.app/develop/calling-rust/)
- [Tauri：Rust 通知前端](https://v2.tauri.app/develop/calling-frontend/)
- [easy-live2d 文档](https://panzer-jack.github.io/easy-live2d/en/)
