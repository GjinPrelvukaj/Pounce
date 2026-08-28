//! Writes a workbook from an existing `.pounce` file, for eyeballing one.
//!
//! `cargo run -p pounce-export --example workbook_demo -- crawl.pounce out.xlsx`
use pounce_store::{FilterSpec, SortColumn, SortDirection, SortSpec, Store};
use std::collections::BTreeMap;

fn main() {
    let mut args = std::env::args().skip(1);
    let input = args
        .next()
        .expect("usage: workbook_demo <in.pounce> <out.xlsx>");
    let output = args
        .next()
        .expect("usage: workbook_demo <in.pounce> <out.xlsx>");
    let store = Store::open(&input).unwrap();
    let filters = FilterSpec::new();
    let sort = SortSpec::new(&filters, SortColumn::Url, SortDirection::Asc).unwrap();
    let rules = BTreeMap::new();
    let summary =
        pounce_export::export_workbook(&store, &filters, &sort, &rules, output.as_ref()).unwrap();
    println!("{summary:?}");
}
