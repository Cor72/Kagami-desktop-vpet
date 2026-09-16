//! 情境信号采集：前台窗口、前台进程、空闲时长、全屏与本地时间。
//!
//! 全部本地、低成本。**不截图、不记录按键内容、不枚举进程表。**
//!
//! | 信号 | API |
//! |---|---|
//! | 前台窗口标题 | `GetForegroundWindow` + `GetWindowTextW` |
//! | 前台进程名 | `GetWindowThreadProcessId` + `OpenProcess` + `QueryFullProcessImageNameW` |
//! | 空闲时长 | `GetLastInputInfo` |
//! | 是否全屏 | `GetWindowRect` + `MonitorFromWindow` / `GetMonitorInfoW` |
//! | 本地日期 | `GetLocalTime`（每日额度跨天归零、「今天别烦我」的到期判断） |
//!
//! **「打开某个程序」用「前台窗口变化」判断，不轮询进程表**（计划 §8.3）：
//!
//! - 前台窗口本来就是每次采样都在读的，不多花一次系统调用；
//! - 用户「打开某个程序」时那个窗口几乎必然成为前台窗口，效果一样；
//! - 更要紧的是，前台窗口变化意味着**用户的注意力刚刚转移**——这才是适合搭话的时刻。
//!   一个在后台悄悄启动的进程（比如自动更新程序）不该触发说话。
//!
//! 隐私：窗口标题在这里照原样读出来，**怎么用由 [`crate::proactive`] 决定**——
//! 只有已知编辑器的标题会被提取文件名，浏览器与聊天软件等其余应用整条丢弃。

use std::path::Path;

use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, HWND, RECT};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::System::SystemInformation::{GetLocalTime, GetTickCount};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowRect, GetWindowTextLengthW, GetWindowTextW,
    GetWindowThreadProcessId,
};

/// 一次采样的结果。字段都是「此刻外面看起来怎么样」，不含任何判断。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sample {
    /// 前台窗口所属进程的可执行文件名，例如 `Code.exe`。取不到时为 `None`。
    pub process_name: Option<String>,
    /// 前台窗口的完整标题（本地用；只有已知编辑器会从中提取文件名）。
    pub window_title: Option<String>,
    /// 距离上一次键盘/鼠标输入的毫秒数。
    pub idle_ms: u64,
    /// 前台窗口是否铺满整个显示器（游戏、演示）。
    pub fullscreen: bool,
    /// 本地日期，形如 `20260915`。用于「每日额度」跨天归零。
    pub local_day: u32,
}

/// 采一次样。任何一项读不到都不算错误：返回 `None` / 0，规则引擎会当作「不触发」。
pub fn sample() -> Sample {
    let window = foreground_window();
    Sample {
        process_name: window.and_then(process_name_of),
        window_title: window.and_then(window_title_of),
        idle_ms: idle_ms(),
        fullscreen: window.map(is_fullscreen).unwrap_or(false),
        local_day: local_day(),
    }
}

/// 前台窗口；没有前台窗口（比如桌面切到了 UAC 提示）时返回 `None`。
fn foreground_window() -> Option<HWND> {
    let window = unsafe { GetForegroundWindow() };
    if window.0.is_null() {
        return None;
    }
    Some(window)
}

fn window_title_of(window: HWND) -> Option<String> {
    let length = unsafe { GetWindowTextLengthW(window) };
    if length <= 0 {
        return None;
    }
    // 长度与内容之间窗口标题可能变化，所以按读回来的实际长度截取。
    let mut buffer = vec![0u16; length as usize + 1];
    let copied = unsafe { GetWindowTextW(window, &mut buffer) };
    if copied <= 0 {
        return None;
    }
    Some(String::from_utf16_lossy(&buffer[..copied as usize]))
}

fn process_name_of(window: HWND) -> Option<String> {
    let mut process_id = 0u32;
    unsafe { GetWindowThreadProcessId(window, Some(&mut process_id)) };
    if process_id == 0 {
        return None;
    }

    // 只申请查询权限：读文件工具需要的权限远超这里该有的。
    let handle =
        unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) }.ok()?;
    let mut buffer = vec![0u16; 512];
    let mut length = buffer.len() as u32;
    let result = unsafe {
        QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR::from_raw(buffer.as_mut_ptr()),
            &mut length,
        )
    };
    // 无论成功与否都要关掉句柄，否则每次采样漏一个内核对象。
    let _ = unsafe { CloseHandle(handle) };
    result.ok()?;

    let path = String::from_utf16_lossy(&buffer[..length as usize]);
    file_name_of(&path)
}

/// 从可执行文件路径里取文件名。测试里也用它，所以是公有的。
pub fn file_name_of(path: &str) -> Option<String> {
    let name = Path::new(path)
        .file_name()?
        .to_string_lossy()
        .trim()
        .to_string();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

fn is_fullscreen(window: HWND) -> bool {
    let mut rect = RECT::default();
    if unsafe { GetWindowRect(window, &mut rect) }.is_err() {
        return false;
    }
    let monitor = unsafe { MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST) };
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..MONITORINFO::default()
    };
    if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        return false;
    }
    window_covers_monitor(rect, info.rcMonitor)
}

/// 窗口是否把显示器整个盖住（含边缘外溢的窗口）。
///
/// 单独抽出来是因为这是这里唯一能脱离系统调用的判断，可以直接单测。
pub fn window_covers_monitor(window: RECT, monitor: RECT) -> bool {
    window.left <= monitor.left
        && window.top <= monitor.top
        && window.right >= monitor.right
        && window.bottom >= monitor.bottom
}

fn idle_ms() -> u64 {
    let mut info = LASTINPUTINFO {
        cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    if !unsafe { GetLastInputInfo(&mut info) }.as_bool() {
        return 0;
    }
    // GetLastInputInfo 给的是 GetTickCount 的 32 位刻度（约 49.7 天绕回），
    // 所以两边都用 u32 算，再取环绕差——直接相减在绕回时会得到一个巨大的数。
    let now = unsafe { GetTickCount() };
    u64::from(now.wrapping_sub(info.dwTime))
}

/// 本地日期（`20260915`）与小时。宽限期：`GetLocalTime` 是唯一带时区的时钟来源。
/// 本地日期，形如 `20260915`。
///
/// 只到「日」这一级：每日额度跨天归零、以及「今天别烦我」的到期判断都用它。
/// 早先还取过小时用于按时段自动静默，那条规则已经移除（静默改成手动）。
pub fn local_day() -> u32 {
    let time = unsafe { GetLocalTime() };
    u32::from(time.wYear) * 10_000 + u32::from(time.wMonth) * 100 + u32::from(time.wDay)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_name_handles_windows_paths_and_spaces() {
        assert_eq!(
            file_name_of("C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe"),
            Some("chrome.exe".to_string())
        );
        assert_eq!(file_name_of("Code.exe"), Some("Code.exe".to_string()));
        assert_eq!(file_name_of("  C:\\a\\b.exe  "), Some("b.exe".to_string()));
        assert_eq!(file_name_of(""), None);
        assert_eq!(file_name_of("C:\\"), None);
    }

    #[test]
    fn a_window_that_covers_its_monitor_counts_as_fullscreen() {
        let monitor = RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        assert!(window_covers_monitor(monitor, monitor));
        assert!(
            window_covers_monitor(
                RECT {
                    left: -8,
                    top: -8,
                    right: 1928,
                    bottom: 1088
                },
                monitor
            ),
            "边缘外溢的无边框窗口仍是全屏"
        );
        assert!(
            !window_covers_monitor(
                RECT {
                    left: 100,
                    top: 100,
                    right: 1100,
                    bottom: 800
                },
                monitor
            ),
            "半屏窗口不是全屏"
        );
    }

    #[test]
    fn sampling_never_panics_and_returns_sane_values() {
        // 不假设前台窗口是什么：这条测试只要求采样能安全跑完，
        // 并且本地日期落在合理范围内（它在测试进程里也确实读得到）。
        let sample = sample();
        assert!(
            sample.local_day >= 20_240_101 && sample.local_day <= 99_991_231,
            "日期应形如 yyyymmdd：{}",
            sample.local_day
        );
    }
}
