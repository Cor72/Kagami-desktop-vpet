# 八千代桌宠

Rust + Tauri 2 + Vue 3 + JavaScript，当前已实现第四阶段：透明无边框桌宠、Rust 托盘、显示/隐藏、置顶、表情和 30/15 FPS 设置。使用 4K 运行贴图。

```powershell
cd D:\AnChiProject\yachiyodesktop\app
pnpm install --frozen-lockfile --registry=https://registry.npmjs.org
pnpm tauri dev
```

从 [第一步学习说明](../docs/learning/01-startup.md) 了解启动链和环境配置，再读 [第二步：双向通信](../docs/learning/02-bridge.md)，跟踪一次按钮点击如何到达 Rust，又如何返回 Vue。

接着读 [第三步：模型接入](../docs/learning/03-model.md) 和 [第四步：窗口与托盘](../docs/learning/04-desktop.md)。按住角色可拖动窗口；右键系统托盘图标打开菜单，左键显示桌宠。鼠标进入角色区域后显示工具栏，开发模式可打开“联调”。

```powershell
pnpm test
pnpm build
cargo test --manifest-path src-tauri/Cargo.toml
```

`patches/easy-live2d@0.4.4.patch` 修复当前依赖的加载错误传播、并行贴图加载失败时的清理时序和底层模型释放，由 pnpm 安装时自动应用，请保留补丁、`pnpm-workspace.yaml` 和锁文件。

第四阶段的自动测试、构建及部分桌面操作已验证，托盘完整交互等待手动验收。主动隐藏和最小化停更、恢复时间基准以及完整常驻性能测量在第五阶段。

整体安排见 [六阶段实施计划](../docs/superpowers/plans/2026-09-10-yachiyo-mvp.md)。
