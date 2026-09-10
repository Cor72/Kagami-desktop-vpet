# 八千代桌宠

Rust + Tauri 2 + Vue 3 + JavaScript，当前完成第三阶段：显示八千代 Live2D 模型，使用 4K 运行贴图，通过 Vue ↔ Rust 通信切换真实表情。

```powershell
cd D:\AnChiProject\yachiyodesktop\app
pnpm install --frozen-lockfile --registry=https://registry.npmjs.org
pnpm tauri dev
```

从 [第一步学习说明](../docs/learning/01-startup.md) 了解启动链和环境配置，再读 [第二步：双向通信](../docs/learning/02-bridge.md)，跟踪一次按钮点击如何到达 Rust，又如何返回 Vue。

接下来读 [第三步：模型接入](../docs/learning/03-model.md)，了解 Canvas、模型控制器、本地资源与生命周期。当前联调面板仅在开发模式显示；点击四种表情观察模型变化，点击“重新加载”可验证重新创建和恢复最近表情。

```powershell
pnpm test
pnpm build
cargo test --manifest-path src-tauri/Cargo.toml
```

`patches/easy-live2d@0.4.4.patch` 修复当前依赖的加载错误传播和底层模型释放，由 pnpm 安装时自动应用，请保留补丁、`pnpm-workspace.yaml` 和锁文件。

仍使用普通窗口方便学习。下一阶段接透明窗口和托盘；完整常驻性能测量在第五阶段。

整体安排见 [六阶段实施计划](../docs/superpowers/plans/2026-09-10-yachiyo-mvp.md)。
