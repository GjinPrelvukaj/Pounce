//! When to try again, and how long to wait first.
//!
//! A crawler retries far less than an API client should. Most failing statuses
//! are the answer rather than an obstacle: a 404 is a broken link and a 500 is
//! a broken page, and both are findings the audit wants recorded, not
//! conditions to wait out. Only an explicit "not now" from the server, or a
//! failure with no status at all, is worth a second attempt.
//!
//! This module decides *whether* and *how long*. The loop belongs to the fetch
//! pool, which is also where a retry has to be sent back through the rate
//! limiter — a retry is another request to a host that just asked for room.

use reqwest::StatusCode;
use reqwest::header::{HeaderMap, RETRY_AFTER};
use std::time::{Duration, SystemTime};

#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Attempts in total, not retries after the first.
    pub max_attempts: u32,
    /// The wait after the first failure; doubles from there.
    pub base: Duration,
    /// Ceiling on our own backoff.
    pub max_delay: Duration,
    /// A `Retry-After` longer than this is refused rather than obeyed.
    pub max_retry_after: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base: Duration::from_millis(500),
            max_delay: Duration::from_secs(30),
            max_retry_after: Duration::from_secs(60),
        }
    }
}

impl RetryPolicy {
    /// How long to wait before attempt `attempt + 1`, or `None` to accept this
    /// response as the answer. `attempt` is 1-based.
    pub fn after_status(
        &self,
        attempt: u32,
        status: StatusCode,
        headers: &HeaderMap,
    ) -> Option<Duration> {
        if !is_retryable(status) {
            return None;
        }
        let backoff = self.backoff(attempt)?;
        match retry_after(headers) {
            // Obeying "come back in an hour" literally would park a fetch slot
            // for an hour. Dropping the URL costs less than the wait.
            Some(wait) if wait > self.max_retry_after => None,
            // Never sooner than our own backoff: a server asking for less time
            // than we had already decided to wait is not a reason to hurry.
            Some(wait) => Some(wait.max(backoff)),
            None => Some(backoff),
        }
    }

    /// A reset connection, a timeout, a DNS failure — no status to reason
    /// about, and the case retries exist for.
    pub fn after_transport_error(&self, attempt: u32) -> Option<Duration> {
        self.backoff(attempt)
    }

    fn backoff(&self, attempt: u32) -> Option<Duration> {
        // `>=` and not `>`: `attempt` has already happened, so at the limit
        // there is no budget left for another one.
        if attempt == 0 || attempt >= self.max_attempts {
            return None;
        }
        // ponytail: no jitter. Per-host rate limiting already spaces requests,
        // so there is no herd to disperse. Add it if a real crawl shows
        // retries synchronising.
        let factor = 1u32.checked_shl(attempt - 1)?;
        let delay = self.base.checked_mul(factor).unwrap_or(self.max_delay);
        Some(delay.min(self.max_delay))
    }
}

/// The `Retry-After` header, in either form RFC 9110 allows.
///
/// A date already in the past reads as zero rather than as absent: the server
/// named a moment, and that moment has arrived.
pub fn retry_after(headers: &HeaderMap) -> Option<Duration> {
    let raw = headers.get(RETRY_AFTER)?.to_str().ok()?.trim();

    if let Ok(seconds) = raw.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    // The date form is what CDNs send, and ignoring it means retrying early
    // against exactly the servers most likely to ban us for it.
    let when = httpdate::parse_http_date(raw).ok()?;
    Some(
        when.duration_since(SystemTime::now())
            .unwrap_or(Duration::ZERO),
    )
}

fn is_retryable(status: StatusCode) -> bool {
    matches!(
        status.as_u16(),
        // Too Many Requests, and the three gateway-side failures that mean the
        // origin was never asked. 500 is deliberately absent: that is the
        // origin answering, and the answer is the finding.
        429 | 502 | 503 | 504
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::{HeaderValue, RETRY_AFTER};
    use std::time::SystemTime;

    fn headers(retry_after: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(RETRY_AFTER, HeaderValue::from_str(retry_after).unwrap());
        h
    }

    fn none() -> HeaderMap {
        HeaderMap::new()
    }

    fn policy() -> RetryPolicy {
        RetryPolicy {
            max_attempts: 3,
            base: Duration::from_secs(1),
            max_delay: Duration::from_secs(30),
            max_retry_after: Duration::from_secs(60),
        }
    }

    // ---- which failures are worth trying again ----

    #[test]
    fn a_rate_limit_or_an_unavailable_server_is_retried() {
        // Each of these is the server explicitly saying "not now", which is
        // the one case where trying again is the polite thing to do.
        let p = policy();
        for code in [429, 502, 503, 504] {
            let status = StatusCode::from_u16(code).unwrap();
            assert!(
                p.after_status(1, status, &none()).is_some(),
                "{code} should be retried"
            );
        }
    }

    #[test]
    fn a_500_is_treated_as_data_rather_than_a_hiccup() {
        // A page that errors is an audit finding. Retrying it three times
        // triples the cost of crawling a broken site and changes nothing.
        let p = policy();
        assert_eq!(
            p.after_status(1, StatusCode::INTERNAL_SERVER_ERROR, &none()),
            None
        );
    }

    #[test]
    fn client_errors_and_successes_are_never_retried() {
        let p = policy();
        for code in [200, 301, 400, 403, 404, 410] {
            let status = StatusCode::from_u16(code).unwrap();
            assert_eq!(p.after_status(1, status, &none()), None, "{code}");
        }
    }

    #[test]
    fn a_transport_error_is_retried() {
        // A reset connection or a timeout carries no status to reason about,
        // and is the case retries exist for.
        assert!(policy().after_transport_error(1).is_some());
    }

    // ---- the backoff schedule ----

    #[test]
    fn the_delay_doubles_with_each_attempt() {
        let p = policy();
        let s = StatusCode::SERVICE_UNAVAILABLE;
        assert_eq!(p.after_status(1, s, &none()), Some(Duration::from_secs(1)));
        assert_eq!(p.after_status(2, s, &none()), Some(Duration::from_secs(2)));
    }

    #[test]
    fn giving_up_happens_at_the_attempt_limit_not_after_it() {
        // `max_attempts` counts requests sent, so a policy of 3 sends three and
        // stops. Reading it as "3 retries after the first" costs a real server
        // an extra request per failure, which is the wrong way to be wrong.
        let p = policy();
        assert_eq!(p.max_attempts, 3);
        for spent in [3, 4] {
            assert_eq!(
                p.after_status(spent, StatusCode::SERVICE_UNAVAILABLE, &none()),
                None,
                "attempt {spent}"
            );
            assert_eq!(p.after_transport_error(spent), None, "attempt {spent}");
        }
    }

    #[test]
    fn the_backoff_is_capped() {
        let p = RetryPolicy {
            max_attempts: 20,
            max_delay: Duration::from_secs(10),
            ..policy()
        };
        assert_eq!(p.after_transport_error(15), Some(Duration::from_secs(10)));
    }

    // ---- Retry-After ----

    #[test]
    fn a_delta_seconds_retry_after_is_honoured() {
        let p = policy();
        let d = p.after_status(1, StatusCode::TOO_MANY_REQUESTS, &headers("5"));
        assert_eq!(d, Some(Duration::from_secs(5)));
    }

    #[test]
    fn an_http_date_retry_after_is_honoured() {
        // CDNs use the date form, and ignoring it means retrying early against
        // exactly the servers most likely to ban us for it.
        let when = SystemTime::now() + Duration::from_secs(30);
        let h = headers(&httpdate::fmt_http_date(when));
        let d = policy()
            .after_status(1, StatusCode::TOO_MANY_REQUESTS, &h)
            .expect("should retry");
        // Whole-second resolution, and a moment passes during the test.
        assert!(
            d >= Duration::from_secs(28) && d <= Duration::from_secs(31),
            "{d:?}"
        );
    }

    #[test]
    fn a_retry_after_in_the_past_means_now() {
        let when = SystemTime::now() - Duration::from_secs(600);
        let h = headers(&httpdate::fmt_http_date(when));
        assert_eq!(retry_after(&h), Some(Duration::ZERO));
    }

    #[test]
    fn we_never_retry_sooner_than_our_own_backoff() {
        // A server asking for 1s while we are already backing off 4s is not a
        // reason to speed up; the shorter of the two is never the safe choice.
        let p = RetryPolicy {
            base: Duration::from_secs(4),
            ..policy()
        };
        let d = p.after_status(1, StatusCode::TOO_MANY_REQUESTS, &headers("1"));
        assert_eq!(d, Some(Duration::from_secs(4)));
    }

    #[test]
    fn an_absurd_retry_after_gives_up_instead_of_parking_a_worker() {
        // Honouring "come back in an hour" literally would hold a fetch slot
        // for an hour. Dropping the URL is the cheaper answer.
        let p = policy();
        let d = p.after_status(1, StatusCode::SERVICE_UNAVAILABLE, &headers("3600"));
        assert_eq!(d, None);
    }

    #[test]
    fn a_malformed_retry_after_falls_back_to_the_backoff() {
        let p = policy();
        for junk in ["soon", "", "-5", "12.5", "Tuesday"] {
            assert_eq!(retry_after(&headers(junk)), None, "{junk:?}");
            assert_eq!(
                p.after_status(1, StatusCode::SERVICE_UNAVAILABLE, &headers(junk)),
                Some(Duration::from_secs(1)),
                "{junk:?}"
            );
        }
    }

    #[test]
    fn retry_after_is_ignored_on_a_status_we_do_not_retry() {
        // A 404 that happens to carry the header is still a 404.
        let p = policy();
        assert_eq!(
            p.after_status(1, StatusCode::NOT_FOUND, &headers("5")),
            None
        );
    }

    #[test]
    fn absent_retry_after_reads_as_absent() {
        assert_eq!(retry_after(&none()), None);
    }

    // ---- defaults ----

    #[test]
    fn the_defaults_are_conservative() {
        let p = RetryPolicy::default();
        assert!(p.max_attempts <= 3, "a crawler retrying more is a nuisance");
        assert!(p.max_retry_after <= Duration::from_secs(120));
        assert!(p.max_delay <= Duration::from_secs(60));
    }
}
