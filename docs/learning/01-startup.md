# 第一步：让自己的 Vue 页面出现在桌面窗口里

这份说明保留第一阶段的代码讲解。当前页面已进入 [第二步：双向通信](02-bridge.md)，计数按钮已替换为表情联调面板；下面的启动链和环境说明仍适用。

这一阶段只有一个普通窗口、一个标题和一个计数按钮。先认清 Rust 与 Vue 的启动入口，再进入下一阶段的语言通信。

## 启动与停止

在 PowerShell 中执行：

```powershell
cd D:\AnChiProject\yachiyodesktop\app
pnpm install --frozen-lockfile --registry=https://registry.npmjs.org
pnpm tauri dev
```

依赖已经安装后，只需执行最后一条命令。首次 Rust 编译要下载、编译依赖，之后会复用 `src-tauri/target/` 中的缓存。

停止时在运行命令的终端按 `Ctrl+C`；如果终端询问是否终止批处理，输入 `Y`。当前没有托盘，点击窗口右上角的关闭按钮也会退出应用。

| 命令 | 用途 |
|---|---|
| `pnpm dev` | 只启动 Vite 前端开发服务器，可在浏览器访问 `http://127.0.0.1:1420/` |
| `pnpm tauri dev` | 启动 Vite，然后编译 Rust、打开真正的桌面窗口 |
| `pnpm build` | 把 Vue 打包到 `dist/`，只检查前端构建 |
| `cargo fmt --manifest-path src-tauri/Cargo.toml --check` | 检查 Rust 代码格式 |

请从 `app/` 执行这些命令。端口 1420 被占用时 Vite 会报错，不会偷偷换端口；先关闭重复启动的开发进程。

## 两条启动链

```text
pnpm tauri dev
│
├─ 读取 src-tauri/tauri.conf.json
│  └─ beforeDevCommand 执行 pnpm dev → Vite 提供网页
│
└─ 编译并运行 Rust
   └─ src-tauri/src/main.rs
      └─ src-tauri/src/lib.rs 的 run()
         └─ Tauri 按配置创建 Windows 窗口和 WebView2
            └─ WebView2 加载 devUrl 对应的页面
               └─ index.html
                  └─ src/main.js
                     └─ src/App.vue
```

Rust 可执行程序是宿主，WebView2 在窗口里运行 HTML、CSS 和 JavaScript。Vue 管理页面中的状态和界面。两种语言分别编译、运行；Tauri 负责把它们连接起来。

### 先读这五个文件

1. [App.vue](../../app/src/App.vue)：页面内容、按钮行为和计数状态。
2. [main.js](../../app/src/main.js)：导入根组件和 CSS，调用 `createApp(App).mount('#app')`，把组件放进 HTML 容器。
3. [main.rs](../../app/src-tauri/src/main.rs)：Rust 程序入口，调用 `yachiyo_desktop_lib::run()`。
4. [lib.rs](../../app/src-tauri/src/lib.rs)：创建 Tauri 应用，读取配置，启动事件循环。程序会在这里持续处理窗口事件。
5. [tauri.conf.json](../../app/src-tauri/tauri.conf.json)：定义窗口标题、宽高、开发地址和构建命令。`main` 是窗口标识；360×540 是内容区的逻辑尺寸，Windows 缩放和标题栏会影响截图中的实际尺寸。

`Cargo.toml` 的 `[lib] name = "yachiyo_desktop_lib"`，对应 `main.rs` 中的 `yachiyo_desktop_lib::run()`。这就是两个 Rust 文件之间的连接，并没有跨语言调用。

`build.rs` 在编译期间运行，为 Tauri 准备资源和配置信息；`main.rs` 在你启动应用时运行。它们都叫 `main()`，但运行的时机不同。

## 点击一次按钮，究竟发生了什么

```javascript
const count = ref(0)
```

`ref` 创建一个 Vue 能追踪的值，初始值为 0。在 JavaScript 代码里读写它要用 `count.value`；Vue 模板会自动解包，所以模板中可以直接写 `count`。

```html
<button type="button" @click="count++">
  <span aria-live="polite">点击了 {{ count }} 次</span>
</button>
```

点击触发 `count++`，Vue 检测到变化，更新页面数字。整个过程都在前端完成。刷新页面或重新启动窗口后，数字回到 0；现在还没有保存功能。

下一步会加入 `invoke → Rust command → emit → Vue listen`，届时我们再对照这条本地计数链，理解什么时候真正跨过了语言边界。

## 其他文件按需看

| 文件 | 职责 |
|---|---|
| `package.json` / `pnpm-lock.yaml` | JavaScript 工具、依赖和实际锁定版本 |
| `src-tauri/Cargo.toml` / `Cargo.lock` | Rust 依赖和实际锁定版本 |
| `vite.config.js` | Vue 编译插件、本机开发地址、文件监听范围 |
| `jsconfig.json` | 编辑器理解 JavaScript 项目的配置，不会把项目改成 TypeScript |
| `src/style.css` | 页面背景、排列方式、字体和按钮颜色 |
| `src-tauri/capabilities/default.json` | `main` 窗口的 Tauri 权限配置，目前只有框架的核心默认权限 |
| `src-tauri/icons/` | Tauri 模板的临时图标，之后可以替换成桌宠图标 |
| `.cargo/config.toml` | 本工程的 Rust 依赖下载源，使用 HTTPS sparse 镜像 |

`dist/`、`node_modules/`、`src-tauri/target/`、`src-tauri/gen/` 是生成内容，已通过 `.gitignore` 排除。锁文件需要跟随源码保存。

## 你可以亲手改的两处

保持 `pnpm tauri dev` 运行，然后依次尝试：

1. 把 `App.vue` 中的 `<h1>八千代桌宠</h1>` 改成你喜欢的一句话，保存，观察窗口文字变化。
2. 把 `style.css` 中 `button` 的 `background: #73558c` 改成 `#4b7a70`，保存，观察按钮颜色变化。把鼠标移开按钮，以免看到的是 `button:hover` 的颜色。

Vue 模板与 CSS 的修改通常能直接热更新；修改 `<script setup>` 可能重新创建组件并重置计数。修改 Rust 时，Tauri 会重新编译并重启窗口。

## 本机环境说明

已确认 Rust stable 的 MSVC 工具链、Visual Studio C++ 构建工具、Windows SDK 和 WebView2 Runtime 可用。用户用 `test_msvc` 验证了基础 Rust 编译与链接。

如果刚装完 Rust 的旧终端仍然找不到 `cargo`，先重新打开终端。也可以仅对当前 PowerShell 会话补充路径：

```powershell
$env:PATH = (Join-Path $env:USERPROFILE '.cargo\bin') + ';' + $env:PATH
```

本机 `C:\Users\Qiguo\.cargo\config` 的弃用提示来自旧文件名，不影响编译。本工程没有修改这个全局文件，而是在 `app/.cargo/config.toml` 中覆盖旧的 `git://` 下载源；配置参考 [RsProxy 官方说明](https://rsproxy.cn/)。

## 本阶段验证记录

验证日期：2026-09-10。

- `pnpm build` 通过：Vue 页面成功打包到 `dist/`。
- `pnpm tauri dev` 编译成功并打开真实 Windows 窗口。
- 点击按钮，页面从“点击了 0 次”变为“点击了 1 次”。
- 临时把标题改成“八千代，下午好”，窗口自动更新，计数仍为 1；验证后已恢复原标题。
- 在开发终端按 `Ctrl+C` 后，桌宠窗口和进程退出。
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check` 通过。

直接依赖：Vue 3.5.42、Vite 8.2.2、Vue Vite 插件 6.0.8、Tauri CLI 2.11.4、Rust tauri 2.11.3、tauri-build 2.6.3。完整依赖版本见两份锁文件。

本阶段完成后，先做上面的两处修改练习，再开始第二步。
