//! Writes a PDF report from an existing `.pounce` file, for eyeballing one.
//!
//! `cargo run -p pounce-export --example report_demo -- crawl.pounce out.pdf`
use pounce_audit::rules;
use pounce_export::{ReportMeta, export_report};
use pounce_store::Store;
use std::collections::BTreeMap;

fn main() {
    let mut args = std::env::args().skip(1);
    let input = args
        .next()
        .expect("usage: report_demo <in.pounce> <out.pdf>");
    let output = args
        .next()
        .expect("usage: report_demo <in.pounce> <out.pdf>");
    let store = Store::open(&input).unwrap();

    let mut registry = pounce_audit::Registry::new();
    rules::register_all(&mut registry).unwrap();
    let sentences = registry
        .page_rules()
        .iter()
        .map(|r| r.meta())
        .chain(registry.site_rules().iter().map(|r| r.meta()))
        .map(|m| {
            (
                m.id.to_string(),
                (m.description.to_string(), m.remediation.to_string()),
            )
        })
        .collect::<BTreeMap<_, _>>();

    let summary = export_report(
        &store,
        &sentences,
        &ReportMeta {
            date: "29 August 2026",
            file: "myzion-com.pounce",
        },
        output.as_ref(),
    )
    .unwrap();
    println!("{summary:?}");
}
