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
const FAILED: u8 = 6;
pub const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

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
    /// Stopped by an error rather than by finishing or being told to.
    Failed,
}

impl CrawlStatus {
    pub fn is_terminal(self) -> bool {
        !matches!(self, Self::Running | Self::Paused)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CrawlProgress {
    pub status: CrawlStatus,
    pub admitted: u64,
    pub written: u64,
    pub elapsed: Duration,
}

impl CrawlProgress {
    pub fn urls_per_second(self) -> f64 {
        if self.elapsed.is_zero() {
            0.0
        } else {
            self.written as f64 / self.elapsed.as_secs_f64()
        }
    }
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
    written: AtomicU64,
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
            written: AtomicU64::new(0),
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
            FAILED => CrawlStatus::Failed,
            _ => unreachable!("lifecycle state is internal"),
        }
    }

    pub fn admitted(&self) -> u64 {
        self.admitted.load(Ordering::Relaxed)
    }

    pub fn progress(&self) -> CrawlProgress {
        CrawlProgress {
            status: self.status(),
            admitted: self.admitted(),
            written: self.written.load(Ordering::Relaxed),
            elapsed: self.active_elapsed(),
        }
    }

    pub async fn report_progress(&self, mut emit: impl FnMut(CrawlProgress)) {
        let mut ticker = tokio::time::interval(PROGRESS_INTERVAL);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            let progress = self.progress();
            let terminal = progress.status.is_terminal();
            emit(progress);
            if terminal {
                return;
            }
        }
    }

    pub(crate) fn record_written(&self) {
        self.written.fetch_add(1, Ordering::Relaxed);
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

    /// Marks a crawl stopped by an error.
    ///
    /// Terminal, which is the part that matters beyond the label: a progress
    /// reporter runs until the status is terminal, so a crawl that failed
    /// without saying so leaves that reporter ticking forever — and whoever
    /// awaits it waiting forever with it.
    pub fn fail(&self) {
        let mut state = self.state.load(Ordering::Acquire);
        while state == RUNNING || state == PAUSED {
            match self
                .state
                .compare_exchange(state, FAILED, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => {
                    self.changed.notify_waiters();
                    return;
                }
                Err(current) => state = current,
            }
        }
    }

    /// Marks a crawl finished.
    ///
    /// Public because a *batch* is not a crawl: the runner feeds the pipeline
    /// a bounded slice of the frontier at a time, and if each slice completed
    /// the lifecycle, the second one would be admitted against a terminal
    /// status and crawl nothing. Whoever owns the loop owns this call.
    ///
    /// Only `Running` transitions — a cancelled or limit-stopped crawl keeps
    /// the status that stopped it.
    pub fn complete(&self) {
        let _ =
            self.state
                .compare_exchange(RUNNING, COMPLETED, Ordering::AcqRel, Ordering::Acquire);
    }

    fn active_elapsed(&self) -> Duration {
        let current_pause = if self.state.load(Ordering::Acquire) == PAUSED {
            self.paused_at
                .lock()
                .unwrap()
                .map(|started| started.elapsed())
                .unwrap_or_default()
        } else {
            Duration::ZERO
        };
        self.started
            .elapsed()
            .saturating_sub(Duration::from_nanos(
                self.paused_nanos.load(Ordering::Relaxed),
            ))
            .saturating_sub(current_pause)
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

    #[test]
    fn a_snapshot_reports_counts_elapsed_and_rate() {
        let lifecycle = CrawlLifecycle::new(CrawlLimits::default());
        lifecycle.admitted.store(20, Ordering::Relaxed);
        lifecycle.record_written();
        lifecycle.record_written();

        let progress = lifecycle.progress();
        assert_eq!(progress.status, CrawlStatus::Running);
        assert_eq!(progress.admitted, 20);
        assert_eq!(progress.written, 2);
        assert!(progress.elapsed <= lifecycle.started.elapsed());
        assert!(progress.urls_per_second().is_finite());
        assert_eq!(
            CrawlProgress {
                written: 10,
                elapsed: Duration::from_secs(2),
                ..progress
            }
            .urls_per_second(),
            5.0
        );
        assert_eq!(
            CrawlProgress {
                elapsed: Duration::ZERO,
                ..progress
            }
            .urls_per_second(),
            0.0
        );
    }

    #[tokio::test(start_paused = true)]
    async fn reporting_is_throttled_to_ten_hz_and_includes_the_final_state() {
        let lifecycle = std::sync::Arc::new(CrawlLifecycle::new(CrawlLimits::default()));
        let events = std::sync::Arc::new(Mutex::new(Vec::new()));
        let reported = std::sync::Arc::clone(&events);
        let control = std::sync::Arc::clone(&lifecycle);
        let reporter = tokio::spawn(async move {
            control
                .report_progress(|progress| reported.lock().unwrap().push(progress))
                .await;
        });

        tokio::task::yield_now().await;
        for _ in 0..3 {
            lifecycle.record_written();
        }
        tokio::time::advance(Duration::from_millis(350)).await;
        tokio::task::yield_now().await;
        assert_eq!(events.lock().unwrap().len(), 2, "missed ticks do not burst");
        lifecycle.cancel();
        tokio::time::advance(PROGRESS_INTERVAL).await;
        reporter.await.unwrap();

        let events = events.lock().unwrap();
        assert_eq!(events.len(), 3, "initial + one throttled + terminal tick");
        assert_eq!(events.last().unwrap().status, CrawlStatus::Cancelled);
        assert_eq!(events.last().unwrap().written, 3);
    }
}
