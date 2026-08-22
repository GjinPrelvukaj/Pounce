//! What the eleven site rules cost once, against a finished database.
//!
//! Site rules are `GROUP BY`/join queries whose cost scales with rows rather
//! than pages, so they cannot be expressed as ns/page and are excluded from
//! `benches/audit.rs`. This seeds stores at three sizes with the fixture's
//! realistic shape — ~28 links per page, unique titles and bodies, mostly-200s
//! — times `Registry::run_site` over each, then times each rule alone.
//!
//! Ignored by default because seeding 500k pages plus 14M link edges takes
//! minutes; run it under `--release`:
//!
//! ```text
//! cargo test --release -p pounce-audit --test site_rule_overhead -- --ignored --nocapture
//! ```
//!
//! Every size also plants one deterministic finding per finding-shaped rule,
//! asserted after the timing: a query that returned instantly because it
//! matched nothing would have timed an empty branch. The assertions prove the
//! work happened; the timings say what it cost.

use pounce_audit::Registry;
use pounce_core::CrawlUrl;
use pounce_parse::{BodyKind, Link, MetaRobots, PageRecord};
use pounce_store::{RedirectHop, Store, Writer};
use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

const LINKS_PER_PAGE: usize = 28;

fn record(url: &str) -> PageRecord {
    PageRecord {
        url: CrawlUrl::parse(url).unwrap(),
        status: 200,
        depth: 1,
        size: 20_000,
        truncated: false,
        content_type: Some("text/html".into()),
        charset: Some("utf-8".into()),
        kind: BodyKind::Html,
        content_type_mismatch: false,
        elapsed_ms: 7,
        time_to_headers_ms: 3,
        redirect_chain: vec![],
        title: None,
        title_count: 1,
        meta_description: None,
        h1: vec!["A heading naming this page's subject".into()],
        h2: vec![],
        canonical: None,
        canonical_url: None,
        meta_robots: MetaRobots::default(),
        hreflang: vec![],
        open_graph: vec![],
        links: vec![],
        images: vec![],
        word_count: 400,
        body_hash: None,
    }
}

/// The path for page id `i`, shaped like the fixture's (~15 chars).
///
/// Ids run `2..=pages+1`; id 12 is kept out of every target list so it is the
/// one true orphan, and id 13 exists only as a robots-denied *failure* — it
/// never gets a page row.
fn path(i: u64) -> String {
    format!("/section/word-{i}")
}

/// The `k`-th outbound target of page `i`: spread over all seeded ids except
/// the orphan. Overflow past `pages+1` wraps to 2 rather than naming a row
/// that was never pushed.
fn target_of(i: u64, k: u64, pages: u64) -> String {
    let mut t = 2 + (i + k * 7 + 1) % pages;
    if t >= 12 {
        t += 1;
    }
    if t > pages + 1 {
        t = 2;
    }
    path(t)
}

/// Distinct body hash for page `i`; an odd multiplier keeps them distinct.
fn hash(i: u64) -> u64 {
    i.wrapping_mul(0x9E37_79B9_7F4A_7C15)
}

/// Seeds `pages` pages with fixture-shaped data plus the deterministic
/// findings: a shared title (3, 4), a shared body hash (5, 6), a canonical to
/// a 500 (7 → 8), a canonical chain (9 → 10 → 11), a true orphan (12), a
/// robots-denied but linked URL (13), a redirect loop, a broken image and an
/// oversized one.
fn seed(dir: &Path, pages: u64) -> Store {
    let mut store = Store::open(dir.join("seed.pounce")).unwrap();
    let mut writer = Writer::with_batch_size(&mut store, 5_000);
    let base = "http://e.com";

    for i in 2..=pages + 1 {
        let url = format!("{base}{}", path(i));
        let mut r = record(&url);
        r.title = Some(if i == 3 || i == 4 {
            "Shared between exactly two pages here".into()
        } else {
            format!("Title number {i} with a few more words")
        });
        // ~12% missing, like the fixture; the rest unique mid-length text.
        r.meta_description = if i % 8 == 0 {
            None
        } else if i == 15 || i == 17 {
            // A real duplicate pair. Asserted at 0 this rule proved only that
            // it ran no slower than an empty branch — the opposite of what the
            // timing needs to mean. 15 and 17, not 15 and 16: the `i % 8`
            // arm above claims 16 first, which is how the first attempt at
            // this seeded one lonely description and still called it a pair.
            Some("Two pages sharing one description, exactly as a template does.".into())
        } else {
            Some(format!(
                "Description {i}: a sentence or so of ordinary summary text."
            ))
        };
        r.body_hash = match i {
            5 | 6 => Some(hash(u64::MAX)),
            _ => Some(hash(i)),
        };
        // The canonical-to-non-200 pair: 7 points at 8, which is a 500.
        if i == 7 {
            r.canonical = Some(path(8));
            r.canonical_url = Some(CrawlUrl::parse(&format!("{base}{}", path(8))).unwrap());
        }
        // The chain: 9 -> 10 -> 11, both hops real redirections of authority.
        if i == 9 || i == 10 {
            let next = if i == 9 { 10 } else { 11 };
            r.canonical = Some(path(next));
            r.canonical_url = Some(CrawlUrl::parse(&format!("{base}{}", path(next))).unwrap());
        }
        // The broken target the broken-internal join must find via its index.
        if i == 8 {
            r.status = 500;
        }
        if i != 12 {
            r.links = (0..LINKS_PER_PAGE as u64)
                .map(|k| {
                    let t = target_of(i, k, pages);
                    Link {
                        href: t.clone(),
                        target: Some(CrawlUrl::parse(&format!("{base}{t}")).unwrap()),
                        text: "somewhere else".into(),
                        nofollow: false,
                    }
                })
                .collect();
        }
        writer.push(&r).unwrap();
    }
    // One explicit inbound edge at the robots-denied URL, from page 14, so it
    // is "blocked but linked" rather than silently uncrawled.
    {
        let mut r = record(&format!("{base}{}", path(14)));
        r.links = vec![Link {
            href: path(13),
            target: Some(CrawlUrl::parse(&format!("{base}{}", path(13))).unwrap()),
            text: "blocked".into(),
            nofollow: false,
        }];
        writer.push(&r).unwrap();
    }

    // Findings that live outside `pages`, one per rule that reads those
    // tables. Both failure and redirect rows carry a foreign key to the
    // *frontier*, because a real crawl discovers a URL before it can be
    // denied or loop on it — so the seed discovers these two first.
    let blocked_url = CrawlUrl::parse(&format!("{base}{}", path(13))).unwrap();
    writer.discover(&[(blocked_url.clone(), 2)]).unwrap();
    writer
        .fail(&blocked_url, "robots.txt disallows this path")
        .unwrap();
    let loop_url = CrawlUrl::parse(&format!("{base}/loop")).unwrap();
    writer.discover(&[(loop_url.clone(), 1)]).unwrap();
    writer
        .redirect(
            &loop_url,
            None,
            &[RedirectHop {
                url: loop_url.clone(),
                status: 302,
                location: "/loop".into(),
                target: Some(loop_url.clone()),
            }],
            "redirect loop at /loop",
        )
        .unwrap();
    writer
        .resource(
            &CrawlUrl::parse("http://e.com/static/img-broken.jpg").unwrap(),
            404,
            Some(13),
            Some("text/html"),
        )
        .unwrap();
    writer
        .resource(
            &CrawlUrl::parse("http://e.com/static/img-huge.jpg").unwrap(),
            200,
            Some(300 * 1024),
            Some("image/jpeg"),
        )
        .unwrap();
    writer.flush().unwrap();
    drop(writer);
    store
}

#[test]
#[ignore = "minutes of seeding; run with --release --ignored --nocapture"]
fn site_rules_over_finished_stores() {
    let mut full = Registry::new();
    pounce_audit::register_all(&mut full).unwrap();

    // SITE_RULE_SIZES=10000,500000 trims the sweep while iterating on the
    // harness; the default covers all three published sizes.
    let sizes: Vec<u64> = match std::env::var("SITE_RULE_SIZES") {
        Ok(s) => s.split(',').map(|n| n.parse().unwrap()).collect(),
        Err(_) => vec![10_000, 100_000, 500_000],
    };

    for pages in sizes {
        let dir = tempfile::tempdir().unwrap();
        let store = seed(dir.path(), pages);
        // Exactly what `pounce_seo::crawl` does at this point. Without it the
        // three link-joining rules re-scan `links` per row: 45 s for
        // `links.orphan-page` at 10k pages, and O(pages x links) thereafter.
        store.build_link_index().unwrap();

        // Diagnostics: which indices exist at site-rule time (production runs
        // these queries before `build_query_indices`, so `links_target` is
        // deliberately absent), and what plan the two link-reading rules get.
        let indices: Vec<String> = store
            .conn()
            .prepare(
                "SELECT name FROM sqlite_master WHERE type = 'index' AND name NOT LIKE 'sqlite_%'",
            )
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        eprintln!("indices: {indices:?}");
        assert!(
            indices.iter().any(|n| n == "links_target"),
            "the harness must mirror production, which builds the inlink index \
             before the site rules that join on it: {indices:?}"
        );
        let plan_sql = "EXPLAIN QUERY PLAN SELECT p.url FROM pages p \
             WHERE p.depth > 0 \
               AND NOT EXISTS (SELECT 1 FROM links l WHERE l.target_url = p.url)";
        let plans: Vec<String> = store
            .conn()
            .prepare(plan_sql)
            .unwrap()
            .query_map([], |r| r.get::<_, String>(3))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        eprintln!("plan: {plans:?}");
        // SQLite reports `USING COVERING INDEX` here, not plain `USING INDEX`,
        // so match the index name rather than the phrase.
        assert!(
            plans.iter().any(|p| p.contains("links_target")),
            "orphan-page fell back to scanning links: {plans:?}"
        );

        let start = Instant::now();
        let issues = full.run_site(&store).unwrap();
        let total = start.elapsed();

        eprintln!("\n=== {pages} pages ===");
        eprintln!("run_site total: {:?} ({} issues)", total, issues.len());
        for rule in full.site_rules() {
            let start = Instant::now();
            let found = rule.check(&store).unwrap().len();
            eprintln!(
                "  {:40} {:>12?}  {found} found",
                rule.meta().id,
                start.elapsed()
            );
        }

        // The deterministic findings: proves each query did its work at this
        // scale rather than returning instantly on empty tables.
        let mut counts: HashMap<&'static str, usize> = HashMap::new();
        for (_, issue) in &issues {
            *counts.entry(issue.rule_id).or_default() += 1;
        }
        let expect = [
            ("response.redirect-loop", 1),
            ("title.duplicate", 2),
            ("description.duplicate", 2),
            ("content.duplicate-body", 2),
            ("indexability.canonical-non-200", 1),
            ("indexability.canonical-chain", 1),
            ("indexability.blocked-but-linked", 1),
            ("links.broken-internal", 1),
            ("links.orphan-page", 1),
            ("media.broken-image", 1),
            ("media.oversized-image", 1),
        ];
        for (id, n) in expect {
            assert_eq!(
                counts.get(id).copied().unwrap_or(0),
                n,
                "{id} did not produce its deterministic findings at {pages} pages"
            );
        }

        drop(store);
    }
}
