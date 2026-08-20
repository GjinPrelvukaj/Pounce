//! Per-host politeness limits: how often we may start a request, and how many
//! may be open at once.
//!
//! Two separate constraints, deliberately. The interval protects a server from
//! a fast crawler; the concurrency cap protects it from a wide one. A crawler
//! that respects only one of them still hammers a site.
//!
//! Spacing is an even interval rather than a token bucket. A bucket lets a host
//! that has been quiet for a minute absorb sixty requests at once, which is the
//! opposite of polite, and `Crawl-delay` means a minimum gap between requests
//! anyway.
//!
//! Keyed by host, not by origin: a rate limit protects a server, and
//! `https://a.com` and `http://a.com` are the same server. This is deliberately
//! unlike [`crate::robots`], which keys by origin because the two documents can
//! genuinely disagree.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::time::Instant;

/// Politeness gate in front of the fetch pool.
///
/// The interval is passed per call rather than stored, because the effective
/// value is whatever robots.txt said for that host and the caller already has
/// it. Nothing here goes stale.
pub struct Limiter {
    per_host: usize,
    hosts: Mutex<HashMap<String, Arc<Host>>>,
}

struct Host {
    /// The earliest instant at which the next request may start. Advanced at
    /// reservation time, not on completion, so callers queue in arrival order
    /// instead of racing for the same slot.
    next: Mutex<Instant>,
    permits: Arc<Semaphore>,
}

impl Limiter {
    pub fn new(max_concurrent_per_host: usize) -> Self {
        Self {
            per_host: max_concurrent_per_host,
            hosts: Mutex::new(HashMap::new()),
        }
    }

    /// Waits until a request to `host` may start, and returns the permit that
    /// holds its concurrency slot. The slot is released when the permit drops,
    /// so the caller keeps it for the lifetime of the request.
    pub async fn acquire(&self, host: &str, min_interval: Duration) -> OwnedSemaphorePermit {
        let entry = self.host(host);

        // Reserve this request's slot in the schedule, then wait for it.
        let slot = {
            let mut next = entry.next.lock().unwrap();
            let at = (*next).max(Instant::now());
            *next = at + min_interval;
            at
        };
        tokio::time::sleep_until(slot).await;

        // Deliberately after the wait. Sleeping out a Crawl-delay is not an
        // open connection, and holding a permit through it would let a slow
        // interval eat the concurrency budget.
        entry
            .permits
            .clone()
            .acquire_owned()
            .await
            .expect("the semaphore is never closed")
    }

    fn host(&self, host: &str) -> Arc<Host> {
        // ponytail: one entry per host, never evicted. A site crawl sees a
        // handful; checking external links could see thousands of small ones.
        // Add an LRU only if that shows up as real memory.
        let mut hosts = self.hosts.lock().unwrap();
        hosts
            .entry(host.to_string())
            .or_insert_with(|| {
                Arc::new(Host {
                    next: Mutex::new(Instant::now()),
                    permits: Arc::new(Semaphore::new(self.per_host)),
                })
            })
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::timeout;

    const NO_DELAY: Duration = Duration::ZERO;

    // Timing tests run on tokio's paused clock: the assertions are about the
    // schedule the limiter produces, not about how fast the machine is.

    #[tokio::test(start_paused = true)]
    async fn requests_to_one_host_are_spaced_by_the_interval() {
        let l = Limiter::new(8);
        let interval = Duration::from_millis(100);
        let start = Instant::now();

        for _ in 0..5 {
            let _permit = l.acquire("a.com", interval).await;
        }

        // Four gaps between five requests; the first is not delayed.
        assert_eq!(start.elapsed(), interval * 4);
    }

    #[tokio::test(start_paused = true)]
    async fn the_first_request_to_a_host_is_not_delayed() {
        let l = Limiter::new(8);
        let start = Instant::now();
        let _permit = l.acquire("a.com", Duration::from_secs(10)).await;
        assert_eq!(start.elapsed(), Duration::ZERO);
    }

    #[tokio::test(start_paused = true)]
    async fn a_zero_interval_imposes_no_delay() {
        let l = Limiter::new(8);
        let start = Instant::now();
        for _ in 0..100 {
            let _permit = l.acquire("a.com", NO_DELAY).await;
        }
        assert_eq!(start.elapsed(), Duration::ZERO);
    }

    #[tokio::test(start_paused = true)]
    async fn a_host_that_has_been_idle_does_not_bank_credit() {
        // The alternative — a token bucket — would let a host that was quiet
        // for a minute absorb a burst of sixty requests at once. Politeness
        // wants an even spacing, which is also what Crawl-delay means.
        let l = Limiter::new(8);
        let interval = Duration::from_millis(100);

        let _p = l.acquire("a.com", interval).await;
        tokio::time::sleep(Duration::from_secs(5)).await;

        let start = Instant::now();
        let _p1 = l.acquire("a.com", interval).await;
        let _p2 = l.acquire("a.com", interval).await;
        assert_eq!(start.elapsed(), interval);
    }

    #[tokio::test(start_paused = true)]
    async fn separate_hosts_do_not_delay_each_other() {
        let l = Limiter::new(8);
        let interval = Duration::from_secs(1);
        let start = Instant::now();

        let _a = l.acquire("a.com", interval).await;
        let _b = l.acquire("b.com", interval).await;
        let _c = l.acquire("c.com", interval).await;

        assert_eq!(start.elapsed(), Duration::ZERO);
    }

    #[tokio::test(start_paused = true)]
    async fn the_limit_is_per_host_not_per_scheme_or_port() {
        // A rate limit protects a server, and https://a.com and http://a.com
        // are the same server. This is deliberately unlike the robots.txt
        // cache, which is keyed by origin because the documents differ.
        let l = Limiter::new(8);
        let interval = Duration::from_millis(100);
        let start = Instant::now();

        let _p1 = l.acquire("a.com", interval).await;
        let _p2 = l.acquire("a.com", interval).await;

        assert_eq!(start.elapsed(), interval);
    }

    // ---- concurrency cap ----

    #[tokio::test(start_paused = true)]
    async fn no_more_than_the_cap_is_in_flight_at_once() {
        let l = Limiter::new(2);
        let _a = l.acquire("a.com", NO_DELAY).await;
        let _b = l.acquire("a.com", NO_DELAY).await;

        let blocked = timeout(Duration::from_secs(30), l.acquire("a.com", NO_DELAY)).await;
        assert!(blocked.is_err(), "a third request should have to wait");
    }

    #[tokio::test(start_paused = true)]
    async fn finishing_a_request_lets_the_next_one_start() {
        let l = Limiter::new(1);
        let first = l.acquire("a.com", NO_DELAY).await;
        drop(first);

        let next = timeout(Duration::from_secs(1), l.acquire("a.com", NO_DELAY)).await;
        assert!(next.is_ok(), "the permit should have been returned on drop");
    }

    #[tokio::test(start_paused = true)]
    async fn the_cap_is_counted_per_host() {
        let l = Limiter::new(1);
        let _a = l.acquire("a.com", NO_DELAY).await;

        let other = timeout(Duration::from_secs(1), l.acquire("b.com", NO_DELAY)).await;
        assert!(other.is_ok(), "a different host has its own budget");
    }
}
