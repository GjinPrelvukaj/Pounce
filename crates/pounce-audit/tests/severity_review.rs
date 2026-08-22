//! The severity standard, enforced.
//!
//! Judgement is not mechanical: no test can compute that a rule *deserves* its
//! grade. What is mechanical, and what this file pins, is three things:
//!
//! 1. **Every shipped grade sits in the manifest below.** A re-grade is a
//!    deliberate diff here, in review, rather than a silent side effect of
//!    editing a rule module; and a new rule cannot land ungraded.
//! 2. **The rulings recorded in `PLAN.md` are executable** as orderings and
//!    equalities between rules, so overturning one means editing an assertion
//!    whose comment states what it protected.
//! 3. **The set uses all three levels.** A column where everything collapses
//!    to Warning carries no information, which is a finding about the rules,
//!    not about the readers.
//!
//! The standard these enforce lives on [`pounce_audit::Severity`] itself.

use pounce_audit::{Registry, Severity, register_all};
use std::collections::BTreeMap;

/// Every shipped rule and its reviewed grade, T2.10.
///
/// The comment on each entry is the short form of its why; the long form is on
/// `Severity` and in `PLAN.md`. Keep both honest when a grade changes.
const GRADES: &[(&str, Severity)] = &[
    // -- response ---------------------------------------------------------
    // A 5xx is the site failing; often more pages are broken than the crawl saw.
    ("response.5xx", Severity::Critical),
    // A browser refuses part of the page; a security failure, not a preference.
    ("response.mixed-content", Severity::Critical),
    // The URL never resolves at all.
    ("response.redirect-loop", Severity::Critical),
    // Usually a decision someone made; the harm via links has its own rule.
    ("response.4xx", Severity::Warning),
    // The page loads; it just costs several requests to get there.
    ("response.redirect-chain", Severity::Warning),
    // -- titles -----------------------------------------------------------
    // No declared subject: the one absence that leaves nothing for results
    // and tabs to show.
    ("title.missing", Severity::Critical),
    // Truncated mid-title in results: malformed presentation.
    ("title.too-long", Severity::Warning),
    // Kept above Notice deliberately: an empty title rides this rule, and an
    // empty title is functionally identity-less — demoting the rule would
    // flatten it against title.missing.
    ("title.too-short", Severity::Warning),
    // Invalid HTML whose discarded copies are invisible edits.
    ("title.multiple", Severity::Warning),
    // Pages competing over one title.
    ("title.duplicate", Severity::Warning),
    // -- descriptions -----------------------------------------------------
    // Search engines synthesise; the cost is click-through, not identity.
    ("description.missing", Severity::Warning),
    // Cut off mid-sentence: malformed presentation, as too-long titles.
    ("description.too-long", Severity::Warning),
    // Ruled Notice: a short description still works and merely wastes space.
    ("description.too-short", Severity::Notice),
    // Half-written entities render as garbage characters in the snippet.
    ("description.truncated-entity", Severity::Warning),
    // Same shape, same grade as title.duplicate.
    ("description.duplicate", Severity::Warning),
    // -- headings & content ----------------------------------------------
    // Structure missing, not identity: below title.missing, above multiple-h1.
    ("content.missing-h1", Severity::Warning),
    // HTML5 permits several; clarity, not defect.
    ("content.multiple-h1", Severity::Notice),
    // Markup pretending to be structure; screen readers skip it.
    ("content.empty-h1", Severity::Warning),
    // Thin content is an indexing/quality risk, not mere headroom.
    ("content.thin", Severity::Warning),
    // Bodies competing with no instruction to separate them.
    ("content.duplicate-body", Severity::Warning),
    // -- indexability ------------------------------------------------------
    // Ruled: a working directive someone may have meant; Critical is reserved
    // for noindex on *important* pages, and nothing knows which those are.
    ("indexability.noindex", Severity::Warning),
    // Ruled: pointing a duplicate at its original is correct canonical use.
    ("indexability.canonical-elsewhere", Severity::Notice),
    // An instruction naming a URL that cannot serve the role: engines discard
    // it and duplicates compete with no arbitration.
    ("indexability.canonical-non-200", Severity::Critical),
    // Each hop valid, chain unreliable: engines frequently ignore it.
    ("indexability.canonical-chain", Severity::Warning),
    // Config contradicts linking; budget wasted until reconciled.
    ("indexability.blocked-but-linked", Severity::Warning),
    // -- media & links -----------------------------------------------------
    // A hole in the page for every visitor.
    ("media.broken-image", Severity::Warning),
    // Ruled Notice: weight to trim on a page that works.
    ("media.oversized-image", Severity::Notice),
    // Accessibility defect plus lost image-search entries.
    ("media.missing-alt", Severity::Warning),
    // Internal equity pointed into dead ends.
    ("links.broken-internal", Severity::Warning),
    // Often legitimate (landing pages, sitemap-only utility pages): an
    // observation about discoverability, not a defect.
    ("links.orphan-page", Severity::Notice),
];

/// The real shipped set, read out of `register_all`.
fn shipped() -> BTreeMap<&'static str, Severity> {
    let mut reg = Registry::new();
    register_all(&mut reg).expect("the shipped set registers cleanly");
    let mut map = BTreeMap::new();
    for rule in reg.page_rules() {
        map.insert(rule.meta().id, rule.meta().severity);
    }
    for rule in reg.site_rules() {
        map.insert(rule.meta().id, rule.meta().severity);
    }
    map
}

fn grade_of(shipped: &BTreeMap<&'static str, Severity>, id: &str) -> Severity {
    *shipped
        .get(id)
        .unwrap_or_else(|| panic!("{id} is not registered"))
}

/// Whether `a` outranks `b`.
///
/// `Severity`'s `Ord` is most-urgent-first, so raw comparisons read backwards;
/// naming it keeps the assertions below saying what they mean.
fn outranks(a: Severity, b: Severity) -> bool {
    a < b
}

#[test]
fn every_shipped_rule_is_graded_and_every_grade_names_a_shipped_rule() {
    let shipped = shipped();
    // Against the manifest, not against `MAX_RULES`. The cap is a ceiling the
    // registry already enforces and which is expected to move once the rule
    // SDK lands; asserting it here would fail this severity test on a day
    // nobody touched a severity, and point the reader at the manifest for it.
    // Comparing lengths also catches an id duplicated in `GRADES`, which the
    // two loops below would each accept.
    assert_eq!(
        shipped.len(),
        GRADES.len(),
        "the manifest must name every shipped rule exactly once"
    );
    for (id, grade) in GRADES {
        assert_eq!(
            shipped.get(id),
            Some(grade),
            "{id} drifted from its reviewed grade ({grade:?}). Regrade on purpose: \
             update this manifest, check the orderings below still hold, and record \
             the reason in PLAN.md."
        );
    }
    for id in shipped.keys() {
        assert!(
            GRADES.iter().any(|(g, _)| g == id),
            "{id} shipped without being reviewed against the severity standard"
        );
    }
}

#[test]
fn the_recorded_rulings_hold_as_orderings() {
    let shipped = shipped();
    // A 4xx is usually a decision someone made; a 5xx is the site failing.
    assert!(outranks(
        grade_of(&shipped, "response.5xx"),
        grade_of(&shipped, "response.4xx")
    ));
    // A hole in the page outweighs weight to trim.
    assert!(outranks(
        grade_of(&shipped, "media.broken-image"),
        grade_of(&shipped, "media.oversized-image")
    ));
    // Missing beats present-but-meagre, twice over.
    assert!(outranks(
        grade_of(&shipped, "description.missing"),
        grade_of(&shipped, "description.too-short")
    ));
    assert!(outranks(
        grade_of(&shipped, "content.missing-h1"),
        grade_of(&shipped, "content.multiple-h1")
    ));
    // The identity element outranks optional metadata.
    assert!(outranks(
        grade_of(&shipped, "title.missing"),
        grade_of(&shipped, "description.missing")
    ));
    // A broken directive outranks an unreliable one.
    assert!(outranks(
        grade_of(&shipped, "indexability.canonical-non-200"),
        grade_of(&shipped, "indexability.canonical-chain")
    ));
}

#[test]
fn same_shape_same_grade_across_batches() {
    // Duplicate X and duplicate Y are the same finding about different fields;
    // so are too-long X and too-long Y. Families that drifted apart would make
    // the severity column inconsistent in exactly the way T2.10 reviews for.
    //
    // The symmetry is asserted per family rather than as a general law,
    // because it is not one — see the test below, where the `too-short`
    // family is deliberately asymmetric. Adding a family here is a claim that
    // the two findings really are the same finding.
    let shipped = shipped();
    assert_eq!(
        grade_of(&shipped, "title.duplicate"),
        grade_of(&shipped, "description.duplicate")
    );
    assert_eq!(
        grade_of(&shipped, "title.too-long"),
        grade_of(&shipped, "description.too-long")
    );
}

#[test]
fn the_title_family_outranks_its_description_twin_where_identity_is_at_stake() {
    // T2.10's closest call, made executable so it is a ruling rather than a
    // coincidence. A short description is a complete thought wasting space; a
    // short title is usually a subject that failed to inject, and an empty
    // `<title>` — functionally identity-less — fires the same rule. The title
    // is the page's declared name and nothing else supplies it, which is why
    // this one family breaks the symmetry asserted above.
    let shipped = shipped();
    assert!(outranks(
        grade_of(&shipped, "title.too-short"),
        grade_of(&shipped, "description.too-short")
    ));
}

#[test]
fn a_working_directive_is_never_critical() {
    // The spec reserves Critical for noindex on *important* pages, and nothing
    // in the engine knows which pages are important. If noindex ever reaches
    // Critical, that judgement has arrived somewhere — say so here, not by
    // quietly inflating the grade.
    let shipped = shipped();
    assert_ne!(
        grade_of(&shipped, "indexability.noindex"),
        Severity::Critical
    );
}

#[test]
fn the_set_uses_all_three_levels() {
    // Not a demand for a quota — a floor. If a re-grade empties any level, the
    // column stops discriminating and everything above it is decoration.
    let mut counts = [0usize; 3];
    for severity in shipped().values() {
        match severity {
            Severity::Critical => counts[0] += 1,
            Severity::Warning => counts[1] += 1,
            Severity::Notice => counts[2] += 1,
        }
    }
    assert!(
        counts.iter().all(|c| *c > 0),
        "a severity level went unused: {counts:?}"
    );
}
