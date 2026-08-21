use pounce_audit::{Issue, PageRule, Registry, RegistryError, RuleMeta, Severity};
use pounce_core::CrawlUrl;
use pounce_parse::{BodyKind, MetaRobots, PageRecord};

fn blank(url: &str) -> PageRecord {
    PageRecord {
        url: CrawlUrl::parse(url).unwrap(),
        status: 200,
        depth: 0,
        size: 0,
        truncated: false,
        content_type: Some("text/html".into()),
        charset: Some("utf-8".into()),
        kind: BodyKind::Html,
        content_type_mismatch: false,
        elapsed_ms: 0,
        time_to_headers_ms: 0,
        redirect_chain: vec![],
        title: None,
        title_count: 0,
        meta_description: None,
        h1: vec![],
        h2: vec![],
        canonical: None,
        canonical_url: None,
        meta_robots: MetaRobots::default(),
        hreflang: vec![],
        open_graph: vec![],
        links: vec![],
        images: vec![],
        word_count: 0,
        body_hash: None,
    }
}

struct MissingTitle;
impl PageRule for MissingTitle {
    fn meta(&self) -> RuleMeta {
        RuleMeta {
            id: "title.missing",
            severity: Severity::Critical,
            description: "The page has no <title> element.",
            remediation: "Add a unique <title> of 30-60 characters.",
        }
    }
    fn check(&self, page: &PageRecord, out: &mut Vec<Issue>) {
        if page.title.is_none() {
            out.push(Issue {
                rule_id: self.meta().id,
                severity: self.meta().severity,
                detail: None,
            });
        }
    }
}

struct Generated(&'static str);
impl PageRule for Generated {
    fn meta(&self) -> RuleMeta {
        RuleMeta {
            id: self.0,
            severity: Severity::Notice,
            description: "generated",
            remediation: "generated",
        }
    }
    fn check(&self, _p: &PageRecord, _o: &mut Vec<Issue>) {}
}

#[test]
fn a_registered_rule_runs_against_a_page() {
    let mut reg = Registry::new();
    reg.register_page(Box::new(MissingTitle)).unwrap();
    let mut record = blank("https://example.com/a");
    record.title = None;

    let issues = reg.run_page(&record);
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].rule_id, "title.missing");
    assert_eq!(issues[0].severity, Severity::Critical);
}

#[test]
fn a_rule_that_does_not_fire_produces_nothing() {
    let mut reg = Registry::new();
    reg.register_page(Box::new(MissingTitle)).unwrap();
    let mut record = blank("https://example.com/a");
    record.title = Some("A title".into());
    assert!(reg.run_page(&record).is_empty());
}

#[test]
fn an_empty_registry_finds_nothing_and_is_empty() {
    // The control for the 10% budget bench: rules off must cost nothing.
    let reg = Registry::new();
    assert!(reg.is_empty());
    assert_eq!(reg.len(), 0);
    assert!(reg.run_page(&blank("https://example.com/a")).is_empty());
}

#[test]
fn every_registered_rule_runs_not_just_the_first() {
    let mut reg = Registry::new();
    reg.register_page(Box::new(MissingTitle)).unwrap();
    reg.register_page(Box::new(AlwaysFires)).unwrap();
    let issues = reg.run_page(&blank("https://example.com/a"));
    assert_eq!(
        issues.len(),
        2,
        "a loop that stopped early would pass a one-rule test"
    );
}

struct AlwaysFires;
impl PageRule for AlwaysFires {
    fn meta(&self) -> RuleMeta {
        RuleMeta {
            id: "test.always",
            severity: Severity::Notice,
            description: "always",
            remediation: "always",
        }
    }
    fn check(&self, _p: &PageRecord, out: &mut Vec<Issue>) {
        out.push(Issue {
            rule_id: "test.always",
            severity: Severity::Notice,
            detail: None,
        });
    }
}

#[test]
fn duplicate_ids_are_rejected() {
    // Two rules sharing an id makes per-rule counts wrong and --fail-on
    // ambiguous, and it is trivially easy to do by copy-paste.
    let mut reg = Registry::new();
    reg.register_page(Box::new(MissingTitle)).unwrap();
    assert_eq!(
        reg.register_page(Box::new(MissingTitle)),
        Err(RegistryError::DuplicateId("title.missing"))
    );
    assert_eq!(reg.len(), 1, "the rejected rule must not have been stored");
}

#[test]
fn ids_must_follow_the_stable_naming_shape() {
    // Ids are permanent: they appear in --fail-on, in exports and in saved
    // files, so a typo caught here is far cheaper than one caught by a user.
    for bad in [
        "Title Missing",
        "title",
        "title.",
        ".missing",
        "title.Missing",
        "ti tle.x",
    ] {
        let id: &'static str = Box::leak(bad.to_string().into_boxed_str());
        let mut reg = Registry::new();
        assert_eq!(
            reg.register_page(Box::new(Generated(id))),
            Err(RegistryError::MalformedId(id)),
            "{bad:?} should be rejected"
        );
    }
    for good in [
        "title.missing",
        "response.4xx",
        "media.missing-alt",
        "title.too-long",
    ] {
        let id: &'static str = Box::leak(good.to_string().into_boxed_str());
        let mut reg = Registry::new();
        assert!(
            reg.register_page(Box::new(Generated(id))).is_ok(),
            "{good:?}"
        );
    }
}

#[test]
fn the_thirty_rule_cap_is_enforced() {
    // A hard invariant: racing a competitor's feature list is the identified
    // primary failure mode, and a cap that is only documented is not a cap.
    let mut reg = Registry::new();
    for i in 0..30 {
        let id: &'static str = Box::leak(format!("test.rule-{i}").into_boxed_str());
        reg.register_page(Box::new(Generated(id))).unwrap();
    }
    assert_eq!(reg.len(), pounce_audit::MAX_RULES);
    let extra: &'static str = Box::leak("test.rule-30".to_string().into_boxed_str());
    assert_eq!(
        reg.register_page(Box::new(Generated(extra))),
        Err(RegistryError::CapExceeded)
    );
}

// ---- site rules ---------------------------------------------------------

use pounce_audit::SiteRule;
use pounce_store::{Store, StoreError, Writer};

struct DuplicateTitle;
impl SiteRule for DuplicateTitle {
    fn meta(&self) -> RuleMeta {
        RuleMeta {
            id: "title.duplicate",
            severity: Severity::Warning,
            description: "More than one page shares this <title>.",
            remediation: "Give each page a title describing only that page.",
        }
    }
    fn check(&self, store: &Store) -> Result<Vec<(String, Issue)>, StoreError> {
        let mut stmt = store.conn().prepare(
            "SELECT url, title FROM pages WHERE title IS NOT NULL AND title IN \
             (SELECT title FROM pages WHERE title IS NOT NULL \
              GROUP BY title HAVING count(*) > 1) ORDER BY url",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        let mut out = Vec::new();
        for row in rows {
            let (url, title) = row?;
            out.push((
                url,
                Issue {
                    rule_id: self.meta().id,
                    severity: self.meta().severity,
                    detail: Some(title),
                },
            ));
        }
        Ok(out)
    }
}

fn seed_titles(store: &mut Store, rows: &[(&str, Option<&str>)]) {
    let mut writer = Writer::with_batch_size(store, rows.len().max(1));
    for (url, title) in rows {
        let mut record = blank(url);
        record.title = title.map(|t| t.to_string());
        writer.push(&record).unwrap();
    }
    writer.flush().unwrap();
}

#[test]
fn a_site_rule_sees_across_pages() {
    let mut store = Store::in_memory().unwrap();
    seed_titles(
        &mut store,
        &[
            ("https://e.com/a", Some("Shared")),
            ("https://e.com/b", Some("Shared")),
            ("https://e.com/c", Some("Unique")),
        ],
    );

    let mut reg = Registry::new();
    reg.register_site(Box::new(DuplicateTitle)).unwrap();

    let issues = reg.run_site(&store).unwrap();
    assert_eq!(issues.len(), 2, "both sharers are findings, not just one");
    assert!(issues.iter().all(|(_, i)| i.rule_id == "title.duplicate"));
    assert!(!issues.iter().any(|(url, _)| url.ends_with("/c")));
    assert_eq!(issues[0].1.detail.as_deref(), Some("Shared"));
}

#[test]
fn a_null_title_is_not_a_duplicate_of_another_null_title() {
    // NULL is "no title", and two pages missing a title are two separate
    // findings for a different rule — not one shared title.
    let mut store = Store::in_memory().unwrap();
    seed_titles(
        &mut store,
        &[("https://e.com/a", None), ("https://e.com/b", None)],
    );
    let mut reg = Registry::new();
    reg.register_site(Box::new(DuplicateTitle)).unwrap();
    assert!(reg.run_site(&store).unwrap().is_empty());
}

#[test]
fn an_empty_registry_runs_no_site_rules() {
    let store = Store::in_memory().unwrap();
    assert!(Registry::new().run_site(&store).unwrap().is_empty());
}

#[test]
fn page_and_site_rules_share_one_id_space_and_one_count() {
    // "30 rules" has to be one number, or the cap means nothing.
    let mut reg = Registry::new();
    reg.register_page(Box::new(MissingTitle)).unwrap();
    reg.register_site(Box::new(DuplicateTitle)).unwrap();
    assert_eq!(reg.len(), 2);
    assert_eq!(reg.page_rules().len(), 1);
    assert_eq!(reg.site_rules().len(), 1);

    assert_eq!(
        reg.register_site(Box::new(ClashingSite)),
        Err(RegistryError::DuplicateId("title.missing"))
    );
}

struct ClashingSite;
impl SiteRule for ClashingSite {
    fn meta(&self) -> RuleMeta {
        MissingTitle.meta()
    }
    fn check(&self, _s: &Store) -> Result<Vec<(String, Issue)>, StoreError> {
        Ok(vec![])
    }
}

#[test]
fn the_cap_counts_page_and_site_rules_together() {
    // 30 page rules plus one site rule must be rejected, or the cap is really
    // sixty and the invariant is a comment.
    let mut reg = Registry::new();
    for i in 0..30 {
        let id: &'static str = Box::leak(format!("test.rule-{i}").into_boxed_str());
        reg.register_page(Box::new(Generated(id))).unwrap();
    }
    assert_eq!(
        reg.register_site(Box::new(DuplicateTitle)),
        Err(RegistryError::CapExceeded)
    );
}
