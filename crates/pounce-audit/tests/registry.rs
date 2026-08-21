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
