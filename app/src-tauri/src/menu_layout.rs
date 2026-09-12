use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewWindow};

pub const COMPACT_WIDTH: f64 = 300.0;
pub const EXPANDED_WIDTH: f64 = 400.0;
pub const WINDOW_HEIGHT: f64 = 400.0;
const MENU_WIDTH: f64 = EXPANDED_WIDTH - COMPACT_WIDTH;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MenuSide {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MenuLayout {
    pub side: MenuSide,
    pub model_offset_x: f64,
    pub menu_offset_x: f64,
    pub menu_top: f64,
    pub menu_height: f64,
    pub width: f64,
    pub height: f64,
}

impl MenuLayout {
    fn compact(side: MenuSide) -> Self {
        Self {
            side,
            model_offset_x: 0.0,
            menu_offset_x: if side == MenuSide::Left {
                0.0
            } else {
                COMPACT_WIDTH
            },
            menu_top: 0.0,
            menu_height: WINDOW_HEIGHT,
            width: COMPACT_WIDTH,
            height: WINDOW_HEIGHT,
        }
    }

    fn expanded(side: MenuSide) -> Self {
        Self {
            side,
            model_offset_x: if side == MenuSide::Left {
                MENU_WIDTH
            } else {
                0.0
            },
            menu_offset_x: if side == MenuSide::Left {
                0.0
            } else {
                COMPACT_WIDTH
            },
            menu_top: 0.0,
            menu_height: WINDOW_HEIGHT,
            width: EXPANDED_WIDTH,
            height: WINDOW_HEIGHT,
        }
    }

    fn expanded_visible(
        side: MenuSide,
        menu_offset_x: f64,
        menu_top: f64,
        menu_height: f64,
    ) -> Self {
        Self {
            menu_offset_x,
            menu_top,
            menu_height,
            ..Self::expanded(side)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PhysicalRect {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

impl PhysicalRect {
    const fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    fn right(self) -> i64 {
        i64::from(self.x) + i64::from(self.width)
    }

    fn bottom(self) -> i64 {
        i64::from(self.y) + i64::from(self.height)
    }
}

#[derive(Clone, Copy, Debug)]
struct WindowFrame {
    position_x: i32,
    position_y: i32,
    scale_factor: f64,
    work_area: PhysicalRect,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct TargetGeometry {
    position_x: i32,
    position_y: i32,
    physical_width: u32,
    physical_height: u32,
    layout: MenuLayout,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MenuLayoutState {
    open: bool,
    layout: MenuLayout,
}

pub type MenuLayoutStore = Mutex<MenuLayoutState>;

#[derive(Clone, Copy)]
struct ApplyOutcome {
    layout: MenuLayout,
    changed: bool,
}

impl Default for MenuLayoutState {
    fn default() -> Self {
        Self {
            open: false,
            layout: MenuLayout::compact(MenuSide::Right),
        }
    }
}

impl MenuLayoutState {
    #[cfg(test)]
    fn expanded(side: MenuSide) -> Self {
        Self {
            open: true,
            layout: MenuLayout::expanded(side),
        }
    }

    fn layout(self) -> MenuLayout {
        self.layout
    }

    fn apply(
        &mut self,
        open: bool,
        frame: WindowFrame,
        apply_geometry: impl FnOnce(TargetGeometry) -> Result<(), String>,
    ) -> Result<MenuLayout, String> {
        if self.open == open {
            return Ok(self.layout());
        }

        let target = calculate_target(open, *self, frame);
        apply_geometry(target)?;
        self.open = open;
        self.layout = target.layout;
        Ok(target.layout)
    }
}

fn physical(logical: f64, scale_factor: f64) -> u32 {
    (logical * scale_factor).round() as u32
}

fn logical(physical: i64, scale_factor: f64) -> f64 {
    physical as f64 / scale_factor
}

fn choose_side(frame: WindowFrame, compact_width: u32, expanded_width: u32) -> MenuSide {
    let anchor_x = i64::from(frame.position_x);
    let work_left = i64::from(frame.work_area.x);
    let work_right = frame.work_area.right();
    let menu_width = i64::from(expanded_width - compact_width);

    if anchor_x + i64::from(expanded_width) <= work_right {
        return MenuSide::Right;
    }
    if anchor_x - menu_width >= work_left {
        return MenuSide::Left;
    }

    let right_room = (work_right - anchor_x - i64::from(compact_width)).max(0);
    let left_room = (anchor_x - work_left).max(0);
    if right_room >= left_room {
        MenuSide::Right
    } else {
        MenuSide::Left
    }
}

fn visible_menu_metrics(
    side: MenuSide,
    target_x: i32,
    frame: WindowFrame,
    expanded_width: u32,
    menu_width: u32,
    height: u32,
) -> (f64, f64, f64) {
    let window_left = i64::from(target_x);
    let window_right = window_left + i64::from(expanded_width);
    let visible_left = window_left.max(i64::from(frame.work_area.x));
    let visible_right = window_right.min(frame.work_area.right());
    let preferred_offset = if side == MenuSide::Left {
        0
    } else {
        expanded_width - menu_width
    };
    let min_offset = (visible_left - window_left).clamp(0, i64::from(expanded_width));
    let max_offset = (visible_right - window_left - i64::from(menu_width))
        .clamp(0, i64::from(expanded_width - menu_width));
    let offset = if min_offset <= max_offset {
        i64::from(preferred_offset).clamp(min_offset, max_offset)
    } else {
        min_offset.clamp(0, i64::from(expanded_width - menu_width))
    };

    let window_top = i64::from(frame.position_y);
    let window_bottom = window_top + i64::from(height);
    let visible_top = window_top.max(i64::from(frame.work_area.y));
    let visible_bottom = window_bottom.min(frame.work_area.bottom());
    let top = (visible_top - window_top).clamp(0, i64::from(height));
    let bottom = (visible_bottom - window_top).clamp(top, i64::from(height));

    (
        logical(offset, frame.scale_factor),
        logical(top, frame.scale_factor),
        logical(bottom - top, frame.scale_factor),
    )
}

fn calculate_target(open: bool, state: MenuLayoutState, frame: WindowFrame) -> TargetGeometry {
    let expanded_width = physical(EXPANDED_WIDTH, frame.scale_factor);
    let compact_width = physical(COMPACT_WIDTH, frame.scale_factor);
    let menu_width = physical(MENU_WIDTH, frame.scale_factor);
    let height = physical(WINDOW_HEIGHT, frame.scale_factor);

    if open {
        let side = choose_side(frame, compact_width, expanded_width);
        let position_x = if side == MenuSide::Left {
            frame.position_x - menu_width as i32
        } else {
            frame.position_x
        };
        let (menu_offset_x, menu_top, menu_height) =
            visible_menu_metrics(side, position_x, frame, expanded_width, menu_width, height);
        TargetGeometry {
            position_x,
            position_y: frame.position_y,
            physical_width: expanded_width,
            physical_height: height,
            layout: MenuLayout::expanded_visible(side, menu_offset_x, menu_top, menu_height),
        }
    } else {
        TargetGeometry {
            position_x: if state.open && state.layout.side == MenuSide::Left {
                frame.position_x + menu_width as i32
            } else {
                frame.position_x
            },
            position_y: frame.position_y,
            physical_width: compact_width,
            physical_height: height,
            layout: MenuLayout::compact(state.layout.side),
        }
    }
}

fn read_frame(window: &WebviewWindow, open: bool) -> Result<WindowFrame, String> {
    let position = window.outer_position().map_err(|error| error.to_string())?;
    let scale_factor = window.scale_factor().map_err(|error| error.to_string())?;
    let work_area = if open {
        let monitor = window
            .current_monitor()
            .map_err(|error| error.to_string())?
            .ok_or("无法确定桌宠当前所在的显示器")?;
        PhysicalRect::new(
            monitor.work_area().position.x,
            monitor.work_area().position.y,
            monitor.work_area().size.width,
            monitor.work_area().size.height,
        )
    } else {
        // 收起只依赖窗口当前位置、缩放率和已记录的展开方向。
        PhysicalRect::new(0, 0, 0, 0)
    };
    Ok(WindowFrame {
        position_x: position.x,
        position_y: position.y,
        scale_factor,
        work_area,
    })
}

fn rollback_geometry(
    window: &WebviewWindow,
    position: PhysicalPosition<i32>,
    size: PhysicalSize<u32>,
) -> String {
    let mut failures = Vec::new();
    if let Err(error) = window.set_size(size) {
        failures.push(format!("恢复尺寸失败：{error}"));
    }
    if let Err(error) = window.set_position(position) {
        failures.push(format!("恢复位置失败：{error}"));
    }
    failures.join("；")
}

fn apply_native_geometry(window: &WebviewWindow, target: TargetGeometry) -> Result<(), String> {
    let previous_position = window.outer_position().map_err(|error| error.to_string())?;
    let previous_size = window.inner_size().map_err(|error| error.to_string())?;
    let target_position = PhysicalPosition::new(target.position_x, target.position_y);
    let target_size = PhysicalSize::new(target.physical_width, target.physical_height);

    let expanding = target.physical_width >= previous_size.width;
    let result = if expanding {
        window
            .set_position(target_position)
            .and_then(|_| window.set_size(target_size))
    } else {
        window
            .set_size(target_size)
            .and_then(|_| window.set_position(target_position))
    };

    if let Err(error) = result {
        let rollback_error = rollback_geometry(window, previous_position, previous_size);
        return if rollback_error.is_empty() {
            Err(error.to_string())
        } else {
            Err(format!("{error}；窗口布局回滚不完整：{rollback_error}"))
        };
    }
    Ok(())
}

fn set_open_sync(app: &AppHandle, open: bool) -> Result<ApplyOutcome, String> {
    let window = app.get_webview_window("main").ok_or("桌宠窗口不存在")?;
    if open && !window.is_visible().map_err(|error| error.to_string())? {
        return Err("桌宠隐藏时不能展开菜单".into());
    }
    let store = app.state::<MenuLayoutStore>();
    let mut state = store.lock().map_err(|error| error.to_string())?;
    let changed = state.open != open;
    if !changed {
        return Ok(ApplyOutcome {
            layout: state.layout(),
            changed: false,
        });
    }
    let frame = read_frame(&window, open)?;
    let layout = state.apply(open, frame, |target| apply_native_geometry(&window, target))?;
    Ok(ApplyOutcome {
        layout,
        changed: true,
    })
}

async fn apply_open(app: AppHandle, open: bool) -> Result<ApplyOutcome, String> {
    tauri::async_runtime::spawn_blocking(move || set_open_sync(&app, open))
        .await
        .map_err(|error| error.to_string())?
}

pub async fn set_open(app: AppHandle, open: bool) -> Result<MenuLayout, String> {
    apply_open(app, open).await.map(|outcome| outcome.layout)
}

async fn emit_native_reset(app: &AppHandle, outcome: ApplyOutcome) -> Result<MenuLayout, String> {
    if outcome.changed {
        app.emit_to("main", "pet-menu-layout-changed", outcome.layout)
            .map_err(|error| error.to_string())?;
    }
    Ok(outcome.layout)
}

pub async fn reset(app: AppHandle) -> Result<MenuLayout, String> {
    let outcome = apply_open(app.clone(), false).await?;
    emit_native_reset(&app, outcome).await
}

/// 原生窗口事件和托盘回调运行在 UI 线程；这里只排队，不在回调中等待布局锁或 Wry。
pub fn reset_nonblocking(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        match reset(app.clone()).await {
            Ok(_) => {}
            Err(error) => crate::desktop::report_error(&app, &format!("收起桌宠菜单失败：{error}")),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(x: i32, scale_factor: f64, work_area: PhysicalRect) -> WindowFrame {
        frame_at(x, 120, scale_factor, work_area)
    }

    fn frame_at(x: i32, y: i32, scale_factor: f64, work_area: PhysicalRect) -> WindowFrame {
        WindowFrame {
            position_x: x,
            position_y: y,
            scale_factor,
            work_area,
        }
    }

    #[test]
    fn opens_to_the_right_when_work_area_has_room() {
        let target = calculate_target(
            true,
            MenuLayoutState::default(),
            frame(100, 1.0, PhysicalRect::new(0, 0, 1920, 1040)),
        );

        assert_eq!(target.position_x, 100);
        assert_eq!(target.physical_width, 480);
        assert_eq!(target.physical_height, 440);
        assert_eq!(target.layout, MenuLayout::expanded(MenuSide::Right));
    }

    #[test]
    fn opens_to_the_left_when_right_side_crosses_work_area() {
        let target = calculate_target(
            true,
            MenuLayoutState::default(),
            frame(1600, 1.0, PhysicalRect::new(0, 0, 1920, 1040)),
        );

        assert_eq!(target.position_x, 1460);
        assert_eq!(target.layout, MenuLayout::expanded(MenuSide::Left));
    }

    #[test]
    fn supports_negative_monitor_coordinates() {
        let target = calculate_target(
            true,
            MenuLayoutState::default(),
            frame(-1000, 1.0, PhysicalRect::new(-1920, -300, 1920, 1080)),
        );

        assert_eq!(target.position_x, -1000);
        assert_eq!(target.layout.side, MenuSide::Right);
    }

    #[test]
    fn converts_logical_dimensions_at_150_and_200_percent_dpi() {
        let left = calculate_target(
            true,
            MenuLayoutState::default(),
            frame(1000, 1.5, PhysicalRect::new(0, 0, 1500, 1200)),
        );
        assert_eq!(
            (left.position_x, left.physical_width, left.physical_height),
            (790, 720, 660)
        );
        assert_eq!(left.layout, MenuLayout::expanded(MenuSide::Left));

        let right = calculate_target(
            true,
            MenuLayoutState::default(),
            frame(-1800, 2.0, PhysicalRect::new(-2560, 0, 2560, 1440)),
        );
        assert_eq!(
            (
                right.position_x,
                right.physical_width,
                right.physical_height
            ),
            (-1800, 960, 880)
        );
        assert_eq!(right.layout, MenuLayout::expanded(MenuSide::Right));
    }

    #[test]
    fn uses_work_area_boundary_instead_of_full_monitor_boundary() {
        let target = calculate_target(
            true,
            MenuLayoutState::default(),
            frame(1100, 1.0, PhysicalRect::new(0, 0, 1540, 1040)),
        );

        assert_eq!(target.position_x, 960);
        assert_eq!(target.layout.side, MenuSide::Left);
    }

    #[test]
    fn close_restores_the_original_anchor_and_tracks_expanded_dragging() {
        let compact = MenuLayoutState::default();
        let opened = calculate_target(
            true,
            compact,
            frame(1600, 1.0, PhysicalRect::new(0, 0, 1920, 1040)),
        );
        let expanded = MenuLayoutState::expanded(opened.layout.side);
        let closed = calculate_target(
            false,
            expanded,
            frame(opened.position_x, 1.0, PhysicalRect::new(0, 0, 1920, 1040)),
        );
        assert_eq!(closed.position_x, 1600);
        assert_eq!(closed.layout, MenuLayout::compact(MenuSide::Left));

        let dragged_closed = calculate_target(
            false,
            expanded,
            frame(1300, 1.0, PhysicalRect::new(0, 0, 1920, 1040)),
        );
        assert_eq!(dragged_closed.position_x, 1440);
    }

    #[test]
    fn failed_apply_keeps_previous_state_and_retry_is_not_treated_as_idempotent() {
        let mut state = MenuLayoutState::default();
        let geometry = frame(100, 1.0, PhysicalRect::new(0, 0, 1920, 1040));
        let failed = state.apply(true, geometry, |_| Err("resize failed".into()));

        assert_eq!(failed, Err("resize failed".into()));
        assert_eq!(state, MenuLayoutState::default());

        let mut called = false;
        let layout = state
            .apply(true, geometry, |_| {
                called = true;
                Ok(())
            })
            .unwrap();
        assert!(called);
        assert_eq!(layout, MenuLayout::expanded(MenuSide::Right));
    }

    #[test]
    fn repeated_target_state_is_idempotent() {
        let geometry = frame(100, 1.0, PhysicalRect::new(0, 0, 1920, 1040));
        let mut state = MenuLayoutState::expanded(MenuSide::Right);
        let mut called = false;

        let layout = state
            .apply(true, geometry, |_| {
                called = true;
                Ok(())
            })
            .unwrap();

        assert!(!called);
        assert_eq!(layout, MenuLayout::expanded(MenuSide::Right));
    }

    #[test]
    fn reports_visible_menu_height_when_window_crosses_work_area_bottom() {
        let target = calculate_target(
            true,
            MenuLayoutState::default(),
            frame_at(100, 800, 1.0, PhysicalRect::new(0, 0, 1920, 1040)),
        );

        assert_eq!(target.layout.menu_top, 0.0);
        assert_eq!(target.layout.menu_height, 240.0);
    }

    #[test]
    fn reports_visible_menu_top_when_window_crosses_work_area_top() {
        let target = calculate_target(
            true,
            MenuLayoutState::default(),
            frame_at(100, -120, 1.0, PhysicalRect::new(0, 0, 1920, 1040)),
        );

        assert_eq!(target.layout.menu_top, 120.0);
        assert_eq!(target.layout.menu_height, 320.0);
    }

    #[test]
    fn keeps_emergency_menu_inside_narrow_work_area_when_neither_side_fits() {
        let more_room_on_right = calculate_target(
            true,
            MenuLayoutState::default(),
            frame_at(20, 0, 1.0, PhysicalRect::new(0, 0, 450, 440)),
        );
        assert_eq!(more_room_on_right.layout.side, MenuSide::Right);
        assert_eq!(more_room_on_right.position_x, 20);
        assert_eq!(more_room_on_right.layout.menu_offset_x, 290.0);

        let more_room_on_left = calculate_target(
            true,
            MenuLayoutState::default(),
            frame_at(100, 0, 1.0, PhysicalRect::new(0, 0, 500, 440)),
        );
        assert_eq!(more_room_on_left.layout.side, MenuSide::Left);
        assert_eq!(more_room_on_left.position_x, -40);
        assert_eq!(more_room_on_left.layout.menu_offset_x, 40.0);
    }
}
