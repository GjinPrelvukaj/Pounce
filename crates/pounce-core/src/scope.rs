//! What counts as "this site", and which links the crawler follows.
//!
//! The base host is the seed's host with a leading `www.` removed, so a crawl
//! seeded at either the apex or `www` treats the other as the same site. That
//! is the one host rewrite worth doing without a public suffix list: every
//! other subdomain question needs one to answer correctly.

use crate::CrawlUrl;

/// Whether a URL belongs to the site being crawled.
///
/// External URLs are still worth fetching for their status code, which is what
/// the broken-external-link rule reads, but their links are never extracted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Locality {
    Internal,
    External,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SubdomainPolicy {
    /// Only the seed host (and its `www`/apex twin). The default: a seed on one
    /// subdomain of a large host should not pull in every sibling.
    #[default]
    SameHost,
    /// Anything under the base host as well.
    ///
    // ponytail: the base is the seed host, not the registrable domain, so a
    // seed at `blog.example.com` does not reach `shop.example.com`. Correcting
    // that needs a public suffix list (`psl`); add one when a user asks for it.
    IncludeSubdomains,
}

/// The crawl's boundary: which URLs are part of the site, and which links get
/// followed out of it.
#[derive(Debug, Clone)]
pub struct Scope {
    base_host: String,
    port: Option<u16>,
    pub subdomains: SubdomainPolicy,
    /// Follow links marked `rel="nofollow"`. Off by default, as the tag asks.
    pub follow_nofollow: bool,
}

fn base_host(host: &str) -> &str {
    host.strip_prefix("www.").unwrap_or(host)
}

impl Scope {
    pub fn new(seed: &CrawlUrl) -> Self {
        Self {
            base_host: base_host(seed.host()).to_string(),
            // None when the port is the scheme's default, which is how
            // http and https on one host stay the same site.
            port: seed.as_url().port(),
            subdomains: SubdomainPolicy::default(),
            follow_nofollow: false,
        }
    }

    pub fn locality(&self, u: &CrawlUrl) -> Locality {
        if u.as_url().port() != self.port {
            return Locality::External;
        }
        let host = base_host(u.host());
        let same = host == self.base_host
            || (self.subdomains == SubdomainPolicy::IncludeSubdomains
                && host.ends_with(&self.base_host)
                && host[..host.len() - self.base_host.len()].ends_with('.'));
        if same {
            Locality::Internal
        } else {
            Locality::External
        }
    }

    /// Whether to extract and enqueue links from `u`, reached by a link whose
    /// `rel` did or did not carry `nofollow`.
    pub fn should_follow(&self, u: &CrawlUrl, nofollow: bool) -> bool {
        self.locality(u) == Locality::Internal && (!nofollow || self.follow_nofollow)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope(seed: &str) -> Scope {
        Scope::new(&CrawlUrl::parse(seed).unwrap())
    }

    fn is_internal(s: &Scope, u: &str) -> bool {
        s.locality(&CrawlUrl::parse(u).unwrap()) == Locality::Internal
    }

    // ---- same host ----

    #[test]
    fn the_seed_is_internal_to_itself() {
        let s = scope("https://example.com/start");
        assert!(is_internal(&s, "https://example.com/start"));
        assert!(is_internal(&s, "https://example.com/other?q=1"));
    }

    #[test]
    fn a_different_host_is_external() {
        let s = scope("https://example.com/");
        assert!(!is_internal(&s, "https://other.com/p"));
    }

    #[test]
    fn a_host_that_merely_ends_with_the_base_is_external() {
        // The classic prefix bug: notexample.com is not example.com.
        let s = scope("https://example.com/");
        assert!(!is_internal(&s, "https://notexample.com/p"));
        assert!(!is_internal(&s, "https://example.com.evil.net/p"));
    }

    #[test]
    fn scheme_does_not_affect_locality() {
        // http and https on the same host are the same site; the mixed-content
        // audit rule needs both sides classified as internal to fire.
        let s = scope("https://example.com/");
        assert!(is_internal(&s, "http://example.com/p"));
    }

    #[test]
    fn a_different_port_is_external() {
        let s = scope("http://localhost:8080/");
        assert!(is_internal(&s, "http://localhost:8080/p"));
        assert!(!is_internal(&s, "http://localhost:3000/p"));
    }

    #[test]
    fn the_default_port_matches_an_explicit_one() {
        let s = scope("https://example.com/");
        assert!(is_internal(&s, "https://example.com:443/p"));
    }

    // ---- www ----

    #[test]
    fn www_and_the_apex_are_the_same_site_in_both_directions() {
        // Seeding at one and finding the other is the single most common way a
        // crawl silently classifies half a site as external.
        let apex = scope("https://example.com/");
        assert!(is_internal(&apex, "https://www.example.com/p"));

        let www = scope("https://www.example.com/");
        assert!(is_internal(&www, "https://example.com/p"));
    }

    #[test]
    fn stripping_www_does_not_widen_scope_to_other_subdomains() {
        let s = scope("https://www.example.com/");
        assert!(!is_internal(&s, "https://blog.example.com/p"));
    }

    // ---- subdomain policy ----

    #[test]
    fn subdomains_are_external_by_default() {
        let s = scope("https://example.com/");
        assert_eq!(s.subdomains, SubdomainPolicy::SameHost);
        assert!(!is_internal(&s, "https://blog.example.com/p"));
    }

    #[test]
    fn subdomains_are_internal_when_the_policy_says_so() {
        let mut s = scope("https://example.com/");
        s.subdomains = SubdomainPolicy::IncludeSubdomains;
        assert!(is_internal(&s, "https://blog.example.com/p"));
        assert!(is_internal(&s, "https://deep.blog.example.com/p"));
        assert!(is_internal(&s, "https://example.com/p"));
    }

    #[test]
    fn the_subdomain_policy_still_rejects_a_lookalike_host() {
        let mut s = scope("https://example.com/");
        s.subdomains = SubdomainPolicy::IncludeSubdomains;
        assert!(!is_internal(&s, "https://notexample.com/p"));
        assert!(!is_internal(&s, "https://example.com.evil.net/p"));
    }

    #[test]
    fn a_subdomain_seed_includes_its_own_subdomains_only() {
        // Without a public-suffix list the base is the seed host, not the
        // registrable domain, so a sibling subdomain stays external.
        let mut s = scope("https://blog.example.com/");
        s.subdomains = SubdomainPolicy::IncludeSubdomains;
        assert!(is_internal(&s, "https://cdn.blog.example.com/p"));
        assert!(!is_internal(&s, "https://shop.example.com/p"));
    }

    // ---- nofollow ----

    #[test]
    fn an_internal_link_is_followed() {
        let s = scope("https://example.com/");
        let u = CrawlUrl::parse("https://example.com/p").unwrap();
        assert!(s.should_follow(&u, false));
    }

    #[test]
    fn an_external_link_is_never_followed() {
        // External URLs are still fetched for their status code; following
        // means extracting their links, and that is what would run away.
        let s = scope("https://example.com/");
        let u = CrawlUrl::parse("https://other.com/p").unwrap();
        assert!(!s.should_follow(&u, false));
        assert!(!s.should_follow(&u, true));
    }

    #[test]
    fn nofollow_is_respected_by_default() {
        let s = scope("https://example.com/");
        assert!(!s.follow_nofollow);
        let u = CrawlUrl::parse("https://example.com/p").unwrap();
        assert!(!s.should_follow(&u, true));
    }

    #[test]
    fn nofollow_can_be_overridden() {
        let mut s = scope("https://example.com/");
        s.follow_nofollow = true;
        let u = CrawlUrl::parse("https://example.com/p").unwrap();
        assert!(s.should_follow(&u, true));

        // Still bounded by scope.
        let ext = CrawlUrl::parse("https://other.com/p").unwrap();
        assert!(!s.should_follow(&ext, true));
    }

    // ---- against the fixture site ----

    #[test]
    fn fixture_relative_links_resolve_and_classify_as_internal() {
        let seed = CrawlUrl::parse("http://127.0.0.1:8080/").unwrap();
        let s = Scope::new(&seed);
        for href in ["/page/1", "/page/42", "/sitemap.xml", "/static/img-0.jpg"] {
            let u = seed.join(href).unwrap();
            assert_eq!(s.locality(&u), Locality::Internal, "{href}");
        }
    }
}
