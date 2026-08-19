//! Endpoints that exercise the failure modes real crawlers hit: redirect
//! chains and loops, slow responses, oversized bodies, and markup that does
//! not parse.
//!
//! Every size and count is clamped. A crawler misbehaving against the fixture
//! must not be able to exhaust its memory or wedge it indefinitely — the
//! fixture is test infrastructure, and infrastructure that a bug can take
//! down is worse than no infrastructure.

pub const MAX_HOPS: u32 = 20;
pub const MAX_DELAY_MS: u64 = 30_000;
pub const MAX_HUGE_MB: usize = 16;

#[derive(Debug, PartialEq, Eq)]
pub enum Redirect {
    To(String),
    Terminal,
}

/// A finite chain: `/redirect-chain/N` → `/redirect-chain/N-1` → … → 200 at 0.
pub fn redirect_chain_target(n: u32) -> Redirect {
    if n == 0 {
        Redirect::Terminal
    } else {
        Redirect::To(format!("/redirect-chain/{}", n - 1))
    }
}

/// An infinite loop of `size` steps. A crawler without loop detection will
/// follow this forever, which is exactly the point.
pub fn redirect_loop_target(step: u32, size: u32) -> String {
    let size = size.max(1);
    let next = (step + 1) % size;
    format!("/redirect-loop/{size}/{next}")
}

pub fn clamp_hops(n: u32) -> u32 {
    n.min(MAX_HOPS)
}

pub fn clamp_delay_ms(ms: u64) -> u64 {
    ms.min(MAX_DELAY_MS)
}

/// Markup with unclosed tags, a stray `<`, and no closing `</html>`.
/// A correct parser still recovers both links.
pub fn malformed_html() -> String {
    "<!DOCTYPE html>\n<html><head><title>Malformed</title>\n\
     <body>\n<p>unclosed paragraph\n\
     <a href=\"/malformed-target-1\">one</a>\n\
     <div><span>nested but never closed\n\
     <a href=\"/malformed-target-2\">two</a>\n\
     <p>a stray < character and an <unknown-tag attr=unquoted>\n"
        .to_string()
}

/// A page of approximately `mb` megabytes, clamped to [`MAX_HUGE_MB`].
pub fn huge_html(mb: usize) -> String {
    let mb = mb.clamp(1, MAX_HUGE_MB);
    let target = mb * 1024 * 1024;
    let head = "<!DOCTYPE html>\n<html><head><title>Huge</title></head><body>\n\
                <a href=\"/huge-target\">link</a>\n<p>";
    let tail = "</p>\n</body>\n</html>\n";
    let filler = "Large body content used to test streaming parsers and memory ceilings. ";

    let mut s = String::with_capacity(target + 256);
    s.push_str(head);
    while s.len() + tail.len() < target {
        s.push_str(filler);
    }
    s.push_str(tail);
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redirect_chain_advances_until_it_terminates() {
        assert_eq!(
            redirect_chain_target(5),
            Redirect::To("/redirect-chain/4".into())
        );
        assert_eq!(
            redirect_chain_target(1),
            Redirect::To("/redirect-chain/0".into())
        );
        assert_eq!(redirect_chain_target(0), Redirect::Terminal);
    }

    #[test]
    fn redirect_loop_always_points_back_into_the_loop() {
        assert_eq!(redirect_loop_target(0, 3), "/redirect-loop/3/1");
        assert_eq!(redirect_loop_target(1, 3), "/redirect-loop/3/2");
        assert_eq!(redirect_loop_target(2, 3), "/redirect-loop/3/0");
    }

    #[test]
    fn redirect_loop_survives_a_zero_size() {
        // A crawler asking for /redirect-loop/0/0 must not divide by zero.
        assert_eq!(redirect_loop_target(0, 0), "/redirect-loop/1/0");
    }

    #[test]
    fn malformed_html_is_actually_malformed_but_has_recoverable_links() {
        let html = malformed_html();
        assert!(html.contains("<a href=\"/malformed-target-1\""));
        assert!(html.contains("<p>unclosed"));
        assert!(!html.contains("</html>"));
    }

    #[test]
    fn huge_page_hits_the_requested_size() {
        let html = huge_html(2);
        let mb = html.len() as f64 / (1024.0 * 1024.0);
        assert!((2.0..2.5).contains(&mb), "expected ~2MB, got {mb}MB");
    }

    #[test]
    fn huge_page_always_carries_a_link() {
        // The body is filler, but the crawler must still find something to follow.
        assert!(huge_html(1).contains("href=\"/huge-target\""));
    }

    #[test]
    fn size_and_hop_requests_are_clamped() {
        assert!(huge_html(9999).len() <= MAX_HUGE_MB * 1024 * 1024 + 256);
        assert_eq!(clamp_hops(9999), MAX_HOPS);
        assert_eq!(clamp_delay_ms(999_999), MAX_DELAY_MS);
    }

    #[test]
    fn clamps_leave_ordinary_values_alone() {
        assert_eq!(clamp_hops(3), 3);
        assert_eq!(clamp_delay_ms(250), 250);
    }
}
