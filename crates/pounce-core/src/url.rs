//! Crawl-normalised URLs.
//!
//! Wraps the WHATWG parser from the `url` crate, which already handles
//! percent-encoding, punycode, default ports, dot segments, and relative
//! resolution correctly. What this type adds is the *crawl* policy on top:
//! which schemes are fetchable, and what counts as the same page.
//!
//! The normalisation here is deliberately conservative. Every extra rule is a
//! chance to merge two URLs a server treats as different, and a crawler that
//! silently drops pages is worse than one that crawls a few twice:
//!
//! - Fragments are stripped — never a distinct resource.
//! - Trailing slashes are **kept**; `/foo` and `/foo/` can serve different pages.
//! - Query parameter order is **kept**; sorting merges URLs a server may split.
//! - Path case is **kept**; only scheme and host are case-insensitive.
//!
//! Session-ID stripping and other aggressive rules belong behind explicit
//! configuration, not in the default path.

use std::fmt;
use url::Url;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum UrlError {
    #[error("could not parse URL: {0}")]
    Parse(String),
    #[error("scheme `{0}` is not crawlable; only http and https are")]
    UnsupportedScheme(String),
    #[error("URL has no host")]
    NoHost,
}

/// A URL that is known to be fetchable and normalised for crawl identity.
///
/// Two `CrawlUrl`s compare equal exactly when the crawler should treat them as
/// the same page, which is what makes this type usable as a frontier key.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CrawlUrl(Url);

impl CrawlUrl {
    pub fn parse(input: &str) -> Result<Self, UrlError> {
        let parsed = Url::parse(input).map_err(|e| UrlError::Parse(e.to_string()))?;
        Self::from_url(parsed)
    }

    /// Resolves `link` against this URL, then normalises the result.
    pub fn join(&self, link: &str) -> Result<Self, UrlError> {
        let joined = self
            .0
            .join(link)
            .map_err(|e| UrlError::Parse(e.to_string()))?;
        Self::from_url(joined)
    }

    fn from_url(mut u: Url) -> Result<Self, UrlError> {
        match u.scheme() {
            "http" | "https" => {}
            other => return Err(UrlError::UnsupportedScheme(other.to_string())),
        }
        if !u.has_host() {
            return Err(UrlError::NoHost);
        }

        // A fragment is never a separate resource. Everything else the WHATWG
        // parser has already normalised: scheme and host lowercased, punycode
        // applied, default ports dropped, dot segments resolved.
        u.set_fragment(None);
        Ok(Self(u))
    }

    pub fn host(&self) -> &str {
        // Guaranteed by the has_host check in from_url.
        self.0.host_str().unwrap_or_default()
    }

    pub fn path(&self) -> &str {
        self.0.path()
    }

    pub fn scheme(&self) -> &str {
        self.0.scheme()
    }

    pub fn as_url(&self) -> &Url {
        &self.0
    }
}

impl fmt::Display for CrawlUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn norm(s: &str) -> String {
        CrawlUrl::parse(s).unwrap().to_string()
    }

    fn join(base: &str, link: &str) -> String {
        CrawlUrl::parse(base)
            .unwrap()
            .join(link)
            .unwrap()
            .to_string()
    }

    // ---- what makes two URLs the same page ----

    #[test]
    fn strips_the_fragment() {
        // A fragment never identifies a different resource to a crawler.
        assert_eq!(norm("https://a.com/p#section"), "https://a.com/p");
        assert_eq!(norm("https://a.com/p#"), "https://a.com/p");
    }

    #[test]
    fn lowercases_scheme_and_host_but_not_path() {
        // Host is case-insensitive; path is not, and folding it would merge
        // genuinely different pages.
        assert_eq!(norm("HTTPS://Example.COM/Path"), "https://example.com/Path");
    }

    #[test]
    fn drops_default_ports_only() {
        assert_eq!(norm("http://a.com:80/p"), "http://a.com/p");
        assert_eq!(norm("https://a.com:443/p"), "https://a.com/p");
        assert_eq!(norm("http://a.com:8080/p"), "http://a.com:8080/p");
    }

    #[test]
    fn gives_an_empty_path_a_root_slash() {
        assert_eq!(norm("https://a.com"), "https://a.com/");
    }

    #[test]
    fn converts_unicode_hosts_to_punycode() {
        assert_eq!(norm("https://bücher.de/p"), "https://xn--bcher-kva.de/p");
    }

    #[test]
    fn keeps_a_trailing_slash_distinct_from_its_absence() {
        // /foo and /foo/ are different resources and may serve different
        // content. Merging them is a correctness bug, not a tidy-up.
        assert_ne!(norm("https://a.com/foo"), norm("https://a.com/foo/"));
    }

    #[test]
    fn preserves_query_parameter_order() {
        // Sorting would merge URLs a server may treat as distinct.
        assert_eq!(norm("https://a.com/p?b=2&a=1"), "https://a.com/p?b=2&a=1");
        assert_ne!(
            norm("https://a.com/p?a=1&b=2"),
            norm("https://a.com/p?b=2&a=1")
        );
    }

    #[test]
    fn keeps_an_empty_query_distinct_from_none() {
        assert_ne!(norm("https://a.com/p?"), norm("https://a.com/p"));
    }

    #[test]
    fn resolves_dot_segments() {
        assert_eq!(norm("https://a.com/a/b/../c"), "https://a.com/a/c");
        assert_eq!(norm("https://a.com/a/./b"), "https://a.com/a/b");
    }

    // ---- idempotence: the property that keeps a frontier from looping ----

    #[test]
    fn normalisation_is_idempotent() {
        for s in [
            "HTTPS://Example.COM:443/A/../b?q=1#frag",
            "http://a.com",
            "https://bücher.de/p#x",
            "https://a.com/a/./b/../c/",
            "https://a.com/p?b=2&a=1",
        ] {
            let once = norm(s);
            let twice = norm(&once);
            assert_eq!(once, twice, "not idempotent for {s}");
        }
    }

    // ---- relative resolution ----

    #[test]
    fn resolves_relative_links_against_a_base() {
        assert_eq!(
            join("https://a.com/dir/page", "sibling"),
            "https://a.com/dir/sibling"
        );
        assert_eq!(
            join("https://a.com/dir/page", "/root"),
            "https://a.com/root"
        );
        assert_eq!(join("https://a.com/dir/page", "../up"), "https://a.com/up");
        assert_eq!(
            join("https://a.com/dir/page", "//other.com/x"),
            "https://other.com/x"
        );
    }

    #[test]
    fn an_absolute_link_ignores_the_base() {
        assert_eq!(
            join("https://a.com/dir/", "https://b.com/x"),
            "https://b.com/x"
        );
    }

    #[test]
    fn a_bare_fragment_link_resolves_to_the_current_page() {
        assert_eq!(
            join("https://a.com/dir/page", "#x"),
            "https://a.com/dir/page"
        );
    }

    #[test]
    fn an_empty_link_resolves_to_the_current_page() {
        assert_eq!(join("https://a.com/dir/page", ""), "https://a.com/dir/page");
    }

    // ---- rejection ----

    #[test]
    fn rejects_non_http_schemes() {
        // A crawler must never try to fetch these, and they appear in href
        // attributes constantly.
        for s in [
            "mailto:a@b.com",
            "javascript:void(0)",
            "tel:+123456",
            "data:text/html,<p>x",
            "ftp://a.com/f",
        ] {
            assert!(CrawlUrl::parse(s).is_err(), "{s} should be rejected");
        }
    }

    #[test]
    fn rejects_unparseable_input() {
        assert!(CrawlUrl::parse("not a url").is_err());
        assert!(CrawlUrl::parse("").is_err());
        assert!(CrawlUrl::parse("http://").is_err());
    }

    #[test]
    fn joining_a_non_http_link_is_an_error() {
        let base = CrawlUrl::parse("https://a.com/p").unwrap();
        assert!(base.join("mailto:a@b.com").is_err());
        assert!(base.join("javascript:alert(1)").is_err());
    }

    // ---- accessors the crawler needs ----

    #[test]
    fn exposes_host_and_path() {
        let u = CrawlUrl::parse("https://a.com:8080/dir/page?q=1").unwrap();
        assert_eq!(u.host(), "a.com");
        assert_eq!(u.path(), "/dir/page");
    }

    #[test]
    fn equality_follows_normalisation() {
        let a = CrawlUrl::parse("HTTPS://A.com:443/p#frag").unwrap();
        let b = CrawlUrl::parse("https://a.com/p").unwrap();
        assert_eq!(a, b);
    }
}
