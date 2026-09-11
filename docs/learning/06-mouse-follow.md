# 鼠标跟随

参考项目 SolaceBlog 使用 `pixi-live2d-display` 的默认 `autoInteract`：鼠标移动时调用 `focus()`，由模型平滑追踪。桌宠使用的 `easy-live2d 0.4.4` 采用另一套输入逻辑，其内置 `PointerHandler` 只有按下鼠标后才更新跟随，不能只把 `Config.MouseFollow` 改成 `true`。

现在默认让角色看向鼠标，按住角色仍用于拖动桌宠窗口。鼠标位于窗口之外、窗口失焦时也能跟随。

## 从桌面坐标到视线

1. [commands.rs](../../app/src-tauri/src/commands.rs) 注册 `get_pet_cursor_position`，由 Tauri 注入当前窗口。
2. [desktop.rs](../../app/src-tauri/src/desktop.rs) 读取桌面鼠标物理坐标、窗口客户区原点和屏幕缩放率，计算 `(鼠标位置 - 客户区原点) / 缩放率`。使用客户区坐标可兼容托盘失败后恢复的原生边框；不缓存原点或缩放率，因此窗口移动、跨屏之后会重新计算。
3. [mouseFollow.js](../../app/src/live2d/mouseFollow.js) 扣除画布的页面偏移，以画布中心为原点，把右方/上方映射为正值。中心附近按距离变化，远处限制在单位圆内，避免夸张的对角线倾斜。
4. [controller.js](../../app/src/live2d/controller.js) 将目标传给底层 Cubism 模型的 `setDragging(x, y)`。Cubism 自带加速、减速和平滑；随后在物理计算之前叠加头部、眼球和身体参数，继续保留表情、呼吸和头发物理。

`Config.MouseFollow` 仍为 `false`，用于关闭库内部的按键跟随输入，防止它在鼠标释放时把桌面跟随目标覆盖为零。当前锁定的 `easy-live2d` 没有公开 `focus` API，因此这一处使用了 `sprite._model.setDragging`。升级运行库时需要复核这个接口与参数更新顺序。

## 暂停与清理

采样挂在已有 Pixi ticker 上，没有另外启动 `requestAnimationFrame` 或定时器。每帧最多发出一次原生命令，上一条请求没有返回时跳过当前帧，因此不会积压 IPC 请求。

隐藏、最小化或页面不可见时，沿用 `setRunning(false)` 同时停止渲染和采样。请求带有本轮运行的版本号；暂停前的结果即使在恢复后才到达，也会丢弃。卸载和重新加载会先禁用跟随、移除 ticker 回调，再释放模型。读取失败时回正、每秒重试一次，同一轮故障只在控制台报告一次。

## 验证

```powershell
cd D:\AnChiProject\yachiyodesktop\app
pnpm test
pnpm build
cargo test --manifest-path src-tauri/Cargo.toml --offline
```

自动测试覆盖四向坐标、画布留白、窗口外与负屏幕坐标、150%/200% 缩放换算、零尺寸、单条在途请求、隐藏后快速恢复、重载销毁、画布变化和故障恢复。

2026-09-11：前端 20 项测试、Rust 7 项测试、前端生产构建和 Rust 格式检查通过；实际桌宠窗口已检查鼠标改变朝向和重新加载后恢复运行。混合 DPI 多屏，以及完整的拖动、托盘隐藏/恢复与最小化交互，仍需人工验收。

桌面验收可依次检查：无需按键在角色左右、上下移动鼠标；移出窗口；拖动后继续移动鼠标；切换 15 FPS；隐藏/显示和最小化/恢复；重新加载后继续跟随。混合 DPI 多显示器需要在对应硬件上拖动窗口跨屏验证。
