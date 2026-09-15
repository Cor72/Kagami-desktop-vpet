# 八千代桌宠

一只常驻 Windows 桌面的 Live2D 伙伴。透明无边框窗口，角色会跟随鼠标，单击换表情，右键弹出菜单；托盘和独立设置窗口管理全部开关。

**技术栈**：Rust + Tauri 2 + Vue 3 + JavaScript + PixiJS 8 + easy-live2d（Live2D Cubism 3+）。
**当前版本**：0.1.0，Release 已实测（30/15 FPS、隐藏与最小化停更），NSIS 安装包已能打出。

## 快速开始

```powershell
cd D:\AnChiProject\yachiyodesktop\app
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

- 一级五项：表情 / 设置 / 置顶 / 隐藏 / 退出
- 二级五项：微笑 / 眯眼 / 泪眼 / 泪滴 / 返回
- 按钮只有图标（52×52 圆形），文字通过**悬浮提示**显示，提示框用 CSS 自绘（原生 `title` 的边框无法调细）
- 菜单与模型的距离由 `src/style.css` 的 `--menu-gap` 控制

### 设置窗口

独立单例窗口（360×360，可缩放、不透明），从气泡菜单或托盘打开：

设置项：

- **动画帧率**：30 FPS / 15 FPS
- **保持置顶**：让角色留在其他窗口上方

开发模式下窗口底部额外提供「开发联调」与「重新加载模型」两个按钮（发布版不含）。

### 设置文件

设置会落盘、重启后保留（v1 遗留的「重启恢复默认值」缺口已修掉）。文件在 Tauri 的应用数据目录：

```text
%APPDATA%\com.yachiyo.desktop\settings.json
```

```json
{
  "schemaVersion": 1,
  "data": {
    "maxFps": 30,
    "alwaysOnTop": true
  }
}
```

- 文件里只放**跨重启有意义**的两项：`maxFps`（只接受 15 / 30）与 `alwaysOnTop`。显示/隐藏属于会话状态，启动时一律可见——否则托盘一旦创建失败，窗口既不在屏幕上也没法从托盘找回来。
- 可以手工编辑，重启后生效。非法值（例如 `maxFps: 60`）会被拒绝并回退默认值，同时在启动时提示原因。
- 写入是原子的：先写 `settings.json.tmp`，把旧内容留一份 `settings.json.bak`，最后改名覆盖。
- 读取顺序是主文件 → `.bak` → 默认值。**文件损坏不会让程序起不来**，而且会保留现场（不覆盖你改坏的那份），只上报原因。
- 启动期（页面还没挂载）的提示会先排队，等主窗口加载完成再显示，避免那句话落到没人听的地方。

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
│  ├─ main.js                     按 ?view=settings 分流主窗口/设置窗口
│  ├─ App.vue                     桌宠窗口布局、菜单状态、动作分发
│  ├─ SettingsWindow.vue          设置页
│  ├─ style.css                   全局样式与尺寸（窗口尺寸的唯一来源是同文件 .pet-anchor）
│  ├─ api/pet.js                  集中封装 invoke / listen，组件不散落命令字符串
│  ├─ components/
│  │  ├─ PetStage.vue             Canvas 生命周期、手势、ResizeObserver
│  │  ├─ PetBubbleMenu.vue        气泡菜单
│  │  └─ DevPanel.vue             仅开发模式的联调面板
│  ├─ composables/
│  │  ├─ usePet.js                前端状态与事件订阅
│  │  ├─ usePetSettings.js        设置快照（跨窗口共享）
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
| `reload_pet_model` | — | 仅开发版可用 |
| `quit_pet` | — | — |

事件（Rust → Vue）：

| 事件 | 载荷 |
|---|---|
| `pet-expression-requested` | `{ name }` |
| `pet-expression-observed` | `{ name }`（发往设置窗口） |
| `pet-settings-changed` | 设置快照（含 `revision`） |
| `pet-window-minimized` | `boolean` |
| `pet-model-reload` | —（仅开发版） |
| `pet-desktop-error` | 错误字符串 |

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

1. **窗口尺寸有三个来源，需要手动保持一致**：`tauri.conf.json` 的 `width`/`height`（340×480）、`style.css` 的 `.pet-anchor`（340×440）、`style.css` 的 `.pet-view`（394px）。改窗口尺寸要同时改这三处，另外开发模式还有 `.development-stage .pet-view`（368px）。
2. **模型缩放是硬编码的**：`live2d/controller.js` 里 `modelZoom = 4` 与 `model.position.set(layout.x, layout.y + 140)`。窗口尺寸变化时 `fitModel` 会重算缩放并把模型居中，`+140` 的偏移因此是脆弱的。
3. **Rust 侧的 `menu_layout.rs` 已被删除**，但 `tauri.conf.json` 里仍有与之对应的历史痕迹；菜单相关的窗口几何逻辑现在完全不存在。
4. **`csp` 为 `null`**（`tauri.conf.json`）。当前应用纯本地、无外联，风险可控；公开发布前建议收紧。
5. **`bundle.active` 为 `false`**（`tauri.conf.json`），因此不带参数的 `pnpm tauri build` 只产出裸 exe；要生成安装包需显式传 `--bundles nsis`。
6. **依赖补丁不能丢**：`patches/easy-live2d@0.4.4.patch` 修复了上游库的加载错误传播、并行贴图失败时的清理时序、底层模型释放与动画时钟。请保留补丁文件、`pnpm-workspace.yaml` 与锁文件。
7. **`pnpm test` 的脚本是显式文件列表**，新增测试目录时要同步修改 `package.json`。
8. **未完成的人工验收项**：托盘完整交互、手动拖动、系统关闭按钮转隐藏、跨不同缩放显示器的拖动（本机只有一个显示器）。另外 `settings.json` 的 `.bak` 回退路径目前只有单元测试覆盖——`.bak` 只在设置真正变更时才产生，需要点托盘或设置窗口才能触发，尚未用安装版端到端演练。

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
