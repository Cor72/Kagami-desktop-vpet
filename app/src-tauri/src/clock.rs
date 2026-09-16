//! 时钟。
//!
//! 存在的理由只有一个：让「跟时间有关」的逻辑能被测试。设计文档 §4.1 要求番茄钟的
//! 状态机不依赖系统时间、测试里不 `sleep`，所以取时间这件事必须是可替换的：
//! 生产代码用 [`SystemClock`]，测试用 [`FixedClock`] 手动把时间往前推。
//!
//! Phase 7 本阶段还没有调用方（番茄钟是 Phase 8），所以这里先按接口立起来，
//! 并用测试把两种实现钉住。
#![allow(dead_code)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// 当前 Unix 毫秒时间戳。
///
/// 系统时间早于 1970 时（时钟被手动设回过）返回 0 而不是 panic：
/// 取时间失败不该让桌宠崩掉。
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

/// 取时间的统一接口。需要「现在几点」的模块都应该依赖它，而不是直接调 [`now_ms`]。
pub trait Clock: Send + Sync + 'static {
    fn now_ms(&self) -> u64;
}

/// 生产实现：真的读系统时钟。
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        now_ms()
    }
}

/// 测试实现：时间停在指定值，由测试手动推进。
///
/// 「笔记本合盖 40 分钟后唤醒」这类用例在测试里就是 `advance(40 * 60 * 1000)`，
/// 不需要真的等 40 分钟。
#[derive(Debug)]
pub struct FixedClock {
    ms: AtomicU64,
}

impl FixedClock {
    pub fn new(ms: u64) -> Self {
        Self {
            ms: AtomicU64::new(ms),
        }
    }

    /// 直接跳到某个时刻。
    pub fn set(&self, ms: u64) {
        self.ms.store(ms, Ordering::Relaxed);
    }

    /// 往前走一段（毫秒）。返回推进后的时刻。
    pub fn advance(&self, delta_ms: u64) -> u64 {
        self.ms.fetch_add(delta_ms, Ordering::Relaxed) + delta_ms
    }
}

impl Clock for FixedClock {
    fn now_ms(&self) -> u64 {
        self.ms.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_clock_only_moves_when_told_to() {
        let clock = FixedClock::new(1_700_000_000_000);
        assert_eq!(clock.now_ms(), 1_700_000_000_000);
        assert_eq!(clock.now_ms(), 1_700_000_000_000, "没推进就不该变");

        assert_eq!(clock.advance(30_000), 1_700_000_030_000);
        assert_eq!(clock.now_ms(), 1_700_000_030_000);

        clock.set(42);
        assert_eq!(clock.now_ms(), 42);
    }

    #[test]
    fn fixed_clock_can_be_used_through_the_trait_object() {
        // 番茄钟会持有一个 Box<dyn Clock>，这里确认这种用法成立。
        let clock: Box<dyn Clock> = Box::new(FixedClock::new(1_000));
        assert_eq!(clock.now_ms(), 1_000);
    }

    #[test]
    fn system_clock_is_recent_and_never_goes_backwards() {
        let clock = SystemClock;
        let first = clock.now_ms();
        let second = clock.now_ms();
        assert!(second >= first, "时间戳不应回退");
        // 2020-01-01 之后、且不超过当前时刻：足够宽松，不会因为机器慢而失败。
        assert!(first > 1_577_836_800_000, "实际值：{first}");
    }

    #[test]
    fn now_ms_is_a_unix_timestamp_in_milliseconds() {
        let now = now_ms();
        let from_system = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("系统时间应晚于 1970")
            .as_millis() as u64;
        // 两次调用之间允许有少量间隔。
        assert!(from_system.abs_diff(now) < 5_000);
    }
}
