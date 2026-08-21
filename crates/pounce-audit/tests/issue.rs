use pounce_audit::{Issue, RuleMeta, Severity};

#[test]
fn severity_round_trips_through_its_stored_form() {
    // Stored as text in SQL and read back by the query layer, so the two
    // directions must agree or a filter silently matches nothing.
    for s in [Severity::Critical, Severity::Warning, Severity::Notice] {
        assert_eq!(Severity::from_str(s.as_str()), Some(s));
    }
}

#[test]
fn an_unknown_severity_is_rejected_rather_than_defaulted() {
    // Defaulting would turn a corrupt or newer-format file into a silently
    // mis-severitied report.
    assert_eq!(Severity::from_str("catastrophic"), None);
    assert_eq!(Severity::from_str(""), None);
    assert_eq!(
        Severity::from_str("Critical"),
        None,
        "the stored form is lowercase"
    );
}

#[test]
fn severity_orders_most_urgent_first() {
    // The grid sorts by it, and "critical after notice" is a wrong report.
    assert!(Severity::Critical < Severity::Warning);
    assert!(Severity::Warning < Severity::Notice);
    let mut all = [Severity::Notice, Severity::Critical, Severity::Warning];
    all.sort();
    assert_eq!(
        all,
        [Severity::Critical, Severity::Warning, Severity::Notice]
    );
}

#[test]
fn there_is_no_pass_severity() {
    // Pass is a UI state for "checked, nothing found". Storing a row per rule
    // per page is 15M rows at 500k to record absence.
    assert_eq!(Severity::from_str("pass"), None);
}

#[test]
fn severity_serialises_as_its_stored_form() {
    // The JSON export and the SQL column must not disagree about spelling.
    let json = serde_json::to_string(&Severity::Critical).unwrap();
    assert_eq!(json, "\"critical\"");
    assert_eq!(json.trim_matches('"'), Severity::Critical.as_str());
}

#[test]
fn an_issue_carries_its_rule_and_an_optional_detail() {
    let issue = Issue {
        rule_id: "title.too-long",
        severity: Severity::Warning,
        detail: Some("84 characters".into()),
    };
    assert_eq!(issue.rule_id, "title.too-long");
    // Detail is optional: "missing title" needs no elaboration, and an empty
    // string would render as a blank cell rather than as nothing.
    let bare = Issue {
        detail: None,
        ..issue.clone()
    };
    assert_eq!(bare.detail, None);
}

#[test]
fn rule_metadata_carries_remediation_not_just_a_complaint() {
    let meta = RuleMeta {
        id: "title.missing",
        severity: Severity::Critical,
        description: "The page has no <title> element.",
        remediation: "Add a unique <title> of 30-60 characters.",
    };
    assert!(
        !meta.remediation.is_empty(),
        "a finding without a fix is noise"
    );
    assert!(!meta.description.is_empty());
}
