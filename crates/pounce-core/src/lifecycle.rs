//! Runtime crawl control and admission limits.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::Notify;

const RUNNING: u8 = 0;
const PAUSED: u8 = 1;
const CANCELLED: u8 = 2;
const COMPLETED: u8 = 3;
const COUNT_LIMIT: u8 = 4;
const TIME_LIMIT: u8 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct CrawlLimits {
    pub max_depth: Option<u16>,
    pub max_urls: Option<u64>,
    pub max_duration: Option<Duration>,
}

impl CrawlLimits {
    pub fn allows_depth(self, depth: u16) -> bool {
        self.max_depth.is_none_or(|limit| depth <= limit)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrawlStatus {
    Running,
    Paused,
    Cancelled,
    Completed,
    CountLimitReached,
    TimeLimitReached,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Admission {
    Allow,
    SkipDepth,
    Stop,
}

pub struct CrawlLifecycle {
    limits: CrawlLimits,
    state: AtomicU8,
    admitted: AtomicU64,
    started: Instant,
    paused_nanos: AtomicU64,
    paused_at: Mutex<Option<Instant>>,
    changed: Notify,
}

impl CrawlLifecycle {
    pub fn new(limits: CrawlLimits) -> Self {
        Self {
            limits,
            state: AtomicU8::new(RUNNING),
            admitted: AtomicU64::new(0),
            started: Instant::now(),
            paused_nanos: AtomicU64::new(0),
            paused_at: Mutex::new(None),
            changed: Notify::new(),
        }
    }

    pub fn status(&self) -> CrawlStatus {
        match self.state.load(Ordering::Acquire) {
            RUNNING => CrawlStatus::Running,
            PAUSED => CrawlStatus::Paused,
            CANCELLED => CrawlStatus::Cancelled,
            COMPLETED => CrawlStatus::Completed,
            COUNT_LIMIT => CrawlStatus::CountLimitReached,
            TIME_LIMIT => CrawlStatus::TimeLimitReached,
            _ => unreachable!("lifecycle state is internal"),
        }
    }

    pub fn admitted(&self) -> u64 {
        self.admitted.load(Ordering::Relaxed)
    }

    pub fn pause(&self) -> bool {
        let mut paused_at = self.paused_at.lock().unwrap();
        if self
            .state
            .compare_exchange(RUNNING, PAUSED, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return false;
        }
        *paused_at = Some(Instant::now());
        true
    }

    pub fn resume(&self) -> bool {
        let mut paused_at = self.paused_at.lock().unwrap();
        if self.state.load(Ordering::Acquire) != PAUSED {
            return false;
        }
        if let Some(started) = paused_at.take() {
            let nanos = started.elapsed().as_nanos().min(u64::MAX as u128) as u64;
            self.paused_nanos.fetch_add(nanos, Ordering::Relaxed);
        }
        self.state.store(RUNNING, Ordering::Release);
        self.changed.notify_waiters();
        true
    }

    pub fn cancel(&self) -> bool {
        let _transition = self.paused_at.lock().unwrap();
        let state = self.state.load(Ordering::Acquire);
        if state >= CANCELLED {
            return false;
        }
        if self
            .state
            .compare_exchange(state, CANCELLED, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return false;
        }
        self.changed.notify_waiters();
        true
    }

    pub(crate) async fn admit(&self, depth: u16) -> Admission {
        loop {
            let changed = self.changed.notified();
            match self.state.load(Ordering::Acquire) {
                PAUSED => {
                    changed.await;
                    continue;
                }
                RUNNING => {}
                _ => return Admission::Stop,
            }

            if self
                .limits
                .max_duration
                .is_some_and(|limit| self.active_elapsed() >= limit)
            {
                if self
                    .state
                    .compare_exchange(RUNNING, TIME_LIMIT, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    return Admission::Stop;
                }
                continue;
            }
            if !self.limits.allows_depth(depth) {
                return Admission::SkipDepth;
            }

            let admitted = self.admitted.load(Ordering::Relaxed);
            if self.limits.max_urls.is_some_and(|limit| admitted >= limit) {
                if self
                    .state
                    .compare_exchange(RUNNING, COUNT_LIMIT, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    return Admission::Stop;
                }
                continue;
            }
            self.admitted.fetch_add(1, Ordering::Relaxed);
            if self.state.load(Ordering::Acquire) != RUNNING {
                self.admitted.fetch_sub(1, Ordering::Relaxed);
                continue;
            }
            return Admission::Allow;
        }
    }

    pub(crate) fn complete(&self) {
        let _ =
            self.state
                .compare_exchange(RUNNING, COMPLETED, Ordering::AcqRel, Ordering::Acquire);
    }

    fn active_elapsed(&self) -> Duration {
        self.started.elapsed().saturating_sub(Duration::from_nanos(
            self.paused_nanos.load(Ordering::Relaxed),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pause_resume_and_cancel_have_valid_transitions() {
        let lifecycle = CrawlLifecycle::new(CrawlLimits::default());
        assert_eq!(lifecycle.status(), CrawlStatus::Running);
        assert!(lifecycle.pause());
        assert_eq!(lifecycle.status(), CrawlStatus::Paused);
        assert!(lifecycle.resume());
        assert_eq!(lifecycle.status(), CrawlStatus::Running);
        assert!(lifecycle.cancel());
        assert_eq!(lifecycle.status(), CrawlStatus::Cancelled);
        assert!(!lifecycle.resume(), "cancel is terminal");
    }

    #[tokio::test]
    async fn depth_and_count_limits_apply_at_admission() {
        let lifecycle = CrawlLifecycle::new(CrawlLimits {
            max_depth: Some(2),
            max_urls: Some(2),
            max_duration: None,
        });

        assert_eq!(lifecycle.admit(3).await, Admission::SkipDepth);
        assert_eq!(lifecycle.admit(0).await, Admission::Allow);
        assert_eq!(lifecycle.admit(2).await, Admission::Allow);
        assert_eq!(lifecycle.admit(1).await, Admission::Stop);
        assert_eq!(lifecycle.admitted(), 2);
        assert_eq!(lifecycle.status(), CrawlStatus::CountLimitReached);
    }

    #[tokio::test]
    async fn a_zero_time_limit_stops_before_the_first_url() {
        let lifecycle = CrawlLifecycle::new(CrawlLimits {
            max_duration: Some(Duration::ZERO),
            ..CrawlLimits::default()
        });

        assert_eq!(lifecycle.admit(0).await, Admission::Stop);
        assert_eq!(lifecycle.status(), CrawlStatus::TimeLimitReached);
        assert_eq!(lifecycle.admitted(), 0);
    }

    #[tokio::test]
    async fn paused_time_does_not_consume_the_time_limit() {
        let lifecycle = CrawlLifecycle::new(CrawlLimits {
            max_duration: Some(Duration::from_millis(20)),
            ..CrawlLimits::default()
        });
        lifecycle.pause();
        tokio::time::sleep(Duration::from_millis(30)).await;
        lifecycle.resume();

        assert_eq!(lifecycle.admit(0).await, Admission::Allow);
    }
}
