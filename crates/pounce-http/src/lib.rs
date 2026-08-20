//! HTTP for Pounce: fetching, politeness, and the redirect chain.

pub mod fetch;
pub mod limit;
pub mod retry;
pub mod robots;

/// The product token, matched against `User-agent:` lines in robots.txt.
pub const PRODUCT_TOKEN: &str = "PounceBot";

/// The `User-Agent` header sent on every request.
///
/// Honest and identifiable, carrying a URL a sysadmin can look up before
/// deciding whether to block us. That is a politeness requirement rather than a
/// courtesy: an anonymous fast crawler is indistinguishable from an attack.
///
/// TODO(before crawling any site we do not own): the repository is private, so
/// this URL 404s for anyone who follows it. An unreachable URL in a user-agent
/// is worse than none, because it reads as a crawler pretending to be
/// accountable. Point it at a public page before v0.1.
pub const USER_AGENT: &str = concat!(
    "PounceBot/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/GjinPrelvukaj/Pounce)"
);

/// The HTTP client every part of Pounce fetches through.
///
/// Exists so the two things that must never vary are stated once. Auto-redirect
/// is disabled because the redirect chain is data the crawler records, not
/// plumbing to be followed transparently — a call site that forgets that
/// silently destroys the evidence rather than failing.
pub fn client() -> reqwest::Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_user_agent_leads_with_the_product_token() {
        assert!(USER_AGENT.starts_with(PRODUCT_TOKEN));
    }

    #[test]
    fn the_user_agent_carries_a_url() {
        assert!(USER_AGENT.contains("(+http"));
    }

    #[test]
    fn the_full_header_still_matches_our_robots_group() {
        // robots.txt group selection is a prefix match, so the whole header
        // works wherever the product token does. Asserted so that renaming the
        // agent cannot silently stop robots.txt from applying to us.
        let txt = b"User-agent: PounceBot\nDisallow: /x";
        for agent in [PRODUCT_TOKEN, USER_AGENT] {
            let r = robots::Robots::from_bytes(txt, agent);
            assert!(!r.is_relative_allowed("/x"), "{agent}");
            assert!(r.is_relative_allowed("/y"), "{agent}");
        }
    }
}
