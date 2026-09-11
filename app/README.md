# 八千代桌宠

Rust + Tauri 2 + Vue 3 + JavaScript，已实现第五阶段的功能：透明无边框桌宠、Rust 托盘、表情、30/15 FPS、隐藏和最小化停更，以及恢复时钟重置。使用 4K 运行贴图。

```powershell
cd D:\AnChiProject\yachiyodesktop\app
pnpm install --frozen-lockfile --registry=https://registry.npmjs.org
pnpm tauri dev
```

从 [第一步学习说明](../docs/learning/01-startup.md) 了解启动链和环境配置，再读 [第二步：双向通信](../docs/learning/02-bridge.md)，跟踪一次按钮点击如何到达 Rust，又如何返回 Vue。

接着读 [第三步：模型接入](../docs/learning/03-model.md) 和 [第四步：窗口与托盘](../docs/learning/04-desktop.md)。按住角色可拖动窗口；右键系统托盘图标打开菜单，左键显示桌宠。鼠标进入角色区域后显示工具栏，开发模式可打开“联调”。

鼠标跟随默认开启：无需按键，头部、眼睛和身体会平滑朝向鼠标，鼠标移出桌宠窗口也会继续跟随。采样沿用 30/15 FPS 渲染循环，隐藏或最小化时一起暂停。坐标换算使用当前窗口的位置和屏幕缩放率。实现与验证说明见 [鼠标跟随](../docs/learning/06-mouse-follow.md)。

```powershell
pnpm test
pnpm build
cargo test --manifest-path src-tauri/Cargo.toml
```

`patches/easy-live2d@0.4.4.patch` 修复当前依赖的加载错误传播、并行贴图加载失败时的清理时序、底层模型释放和动画时钟，由 pnpm 安装时自动应用，请保留补丁、`pnpm-workspace.yaml` 和锁文件。

继续阅读 [第五步：控制常驻开销](../docs/learning/05-performance.md) 和 [Release 实测记录](../docs/performance/2026-09-10/README.md)。30/15 FPS、隐藏、最小化各 2 分钟以及 20 次隐藏/显示已验证；连续可见待机被额外隐藏打断，用户选择将 30 分钟验收留到后续。第四阶段保留的托盘完整交互等人工验收项仍待完成。

整体安排见 [六阶段实施计划](../docs/superpowers/plans/2026-09-10-yachiyo-mvp.md)。
