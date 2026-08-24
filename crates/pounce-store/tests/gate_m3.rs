//! T3.5 — Gate M3, re-run through the real query layer.
//!
//! The probe that opened M3 used the `sqlite3` CLI against a crawled database,
//! and said so: no prepared statements, no connection reuse, no concurrent
//! writer, one filter/sort pair, and a 10 ms timer. This runs every gate item
//! through `Store::query_rows` instead, over a seeded million rows, and
//! **asserts** the thresholds rather than printing them.
//!
//! The seeder, not a crawl: the probe's real 500k and 1M databases no longer
//! exist and re-crawling them is hours. The status mix is stated in
//! `common/mod.rs` and matters — an all-200 database is what made the probe's
//! filter maximally unselective, and that pessimistic case is kept as one of
//! the measured pairs.
//!
//! ```text
//! cargo test --release -p pounce-store --test gate_m3 -- --ignored --nocapture
//! ```

mod common;

use pounce_parse::BodyKind;
use pounce_store::{
    Comparison, Filter, FilterSpec, MAX_WINDOW, SortColumn, SortDirection, SortSpec, Store, Writer,
};
use std::time::{Duration, Instant};

/// Gate M3's two timing thresholds.
const SORT_BUDGET: Duration = Duration::from_millis(150);
const FILTER_SORT_BUDGET: Duration = Duration::from_millis(300);

fn pages() -> u64 {
    std::env::var("GATE_M3_PAGES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1_000_000)
}

/// Resident memory in KB, or `None` where `ps` cannot answer.
///
/// `ps` rather than a crate: this is a benchmark assertion, and adding a
/// process-inspection dependency to a *library* crate to make one number
/// printable is the wrong trade. The test is `#[ignore]`, so the platforms
/// where this returns `None` simply do not measure it.
fn rss_kb() -> Option<u64> {
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

/// Median of five, which is what the M2 re-take established here. The probe's
/// best-of-three with a 10 ms timer could not tell 1 ms from 9.
fn median_of<T>(runs: usize, mut f: impl FnMut() -> T) -> (Duration, T) {
    let mut times = Vec::with_capacity(runs);
    let mut last = None;
    for _ in 0..runs {
        let start = Instant::now();
        let value = f();
        times.push(start.elapsed());
        last = Some(value);
    }
    times.sort();
    (times[times.len() / 2], last.unwrap())
}

/// Every supported pair the grid can actually offer, with an unselective value
/// where the kind has one — the probe measured a single pair and said so.
fn pairs() -> Vec<(&'static str, Filter)> {
    vec![
        (
            "status=200 (94% of rows)",
            Filter::Status(Comparison::Eq, 200),
        ),
        ("status>=400", Filter::Status(Comparison::Ge, 400)),
        ("kind=html (94%)", Filter::Kind(BodyKind::Html)),
        ("noindex=false (90%)", Filter::Noindex(false)),
        ("depth<=2 (60%)", Filter::Depth(Comparison::Le, 2)),
        ("has_issue (67%)", Filter::HasIssue(None)),
        (
            "word_count<300 (22%)",
            Filter::WordCount(Comparison::Lt, 300),
        ),
        ("url~/blog/ (20%)", Filter::UrlContains("/blog/".into())),
    ]
}

#[test]
#[ignore = "seeds a million rows; run with --release --ignored --nocapture"]
fn gate_m3_through_the_real_query_layer() {
    let pages = pages();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gate.pounce");

    let mut store = Store::open(&path).unwrap();
    let start = Instant::now();
    common::seed_pages_only(&mut store, pages);
    let seeded = start.elapsed();
    let indexed = Instant::now();
    store.build_query_indices().unwrap();
    let index_build = indexed.elapsed();
    let bytes: u64 = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum();
    eprintln!(
        "seeded {pages} rows in {seeded:?}, indices built in {index_build:?}, file {} MB\n",
        bytes / 1_048_576
    );

    // ---- gate item 1: sorting, unfiltered ---------------------------------
    //
    // "Sort of 500k rows returns in under 150 ms." Paged to the middle, since
    // the first window of a sorted table is free whatever the row count.
    eprintln!("=== sort, unfiltered, offset {} ===", pages / 2);
    let mut worst_sort = Duration::ZERO;
    for &column in SortColumn::all() {
        let filters = FilterSpec::new();
        let sort = SortSpec::new(&filters, column, SortDirection::Asc).unwrap();
        let (took, page) = median_of(5, || {
            store.query_rows(&filters, &sort, pages / 2, 200).unwrap()
        });
        assert_eq!(page.rows.len(), 200);
        assert_eq!(page.total, pages);
        worst_sort = worst_sort.max(took);
        eprintln!("  {:<12} {:>9.1} ms", column.column(), ms(took));
    }
    assert!(
        worst_sort <= SORT_BUDGET,
        "sorting missed the 150 ms gate at {pages} rows: {worst_sort:?}"
    );

    // ---- gate item 2: filter + sort + paginate ----------------------------
    //
    // The item the probe failed at 18,270 ms. Every supported pair, not one.
    eprintln!("\n=== filter + sort + paginate, offset = half the match set ===");
    eprint!("{:26}", "");
    for column in SortColumn::all() {
        eprint!("{:>12}", column.column());
    }
    eprintln!();

    let mut worst = (Duration::ZERO, String::new());
    for (name, filter) in pairs() {
        eprint!("{name:26}");
        for &column in SortColumn::all() {
            let filters = FilterSpec::new().with(filter.clone());
            let Ok(sort) = SortSpec::new(&filters, column, SortDirection::Asc) else {
                eprint!("{:>12}", "—");
                continue;
            };
            // Half way into *this filter's* result, so the window always lands
            // inside it. An offset past the match set measures a walk to the
            // end, which is not a scroll any UI can perform.
            let total = store.query_rows(&filters, &sort, 0, 1).unwrap().total;
            let offset = total / 2;
            let (took, page) = median_of(5, || {
                store.query_rows(&filters, &sort, offset, 200).unwrap()
            });
            assert!(
                !page.rows.is_empty(),
                "{name} + {} returned nothing",
                column.column()
            );
            if took > worst.0 {
                worst = (took, format!("{name} sorted by {}", column.column()));
            }
            eprint!("{:>12.1}", ms(took));
        }
        eprintln!();
    }
    eprintln!("  worst pair: {} at {:.1} ms", worst.1, ms(worst.0));
    assert!(
        worst.0 <= FILTER_SORT_BUDGET,
        "{} missed the 300 ms gate at {pages} rows: {:?}",
        worst.1,
        worst.0
    );

    // ---- gate item 3: memory flat regardless of result size ---------------
    //
    // The claim the whole architecture rests on. 200x the rows, because
    // `MAX_WINDOW` makes that the largest change a caller can ask for.
    let filters = FilterSpec::new();
    let sort = SortSpec::new(&filters, SortColumn::WordCount, SortDirection::Asc).unwrap();
    let small = store.query_rows(&filters, &sort, 0, 5).unwrap();
    let before = rss_kb();
    let large = store.query_rows(&filters, &sort, 0, MAX_WINDOW).unwrap();
    let after = rss_kb();
    assert_eq!(small.rows.len(), 5);
    assert_eq!(large.rows.len(), MAX_WINDOW as usize);
    match (before, after) {
        (Some(before), Some(after)) => {
            eprintln!(
                "\n=== memory ===\n  {} MB for 5 rows, {} MB for {MAX_WINDOW} — {}x the result",
                before / 1024,
                after / 1024,
                MAX_WINDOW / 5
            );
            // Flat means flat: a window 200x larger is 995 more rows of nine
            // columns, a few hundred KB. Anything approaching the dataset
            // would be tens of MB.
            assert!(
                after.saturating_sub(before) < 50 * 1024,
                "a 200x larger window cost {} MB — the window is materialising \
                 more than it returns",
                (after - before) / 1024
            );
        }
        _ => eprintln!("\n=== memory ===\n  ps unavailable on this platform; not measured"),
    }

    // ---- gate item 4: a live writer ---------------------------------------
    //
    // Untested in the probe, and the case the app is in for the whole of a
    // crawl. The reader must return, not block, while a batch is open.
    let reader = Store::open(&path).unwrap();
    {
        let mut writer = Writer::with_batch_size(&mut store, 10_000);
        let url = pounce_core::CrawlUrl::parse("http://e.com/live").unwrap();
        writer.discover(&[(url.clone(), 1)]).unwrap();
        writer.fail(&url, "held open on purpose").unwrap();

        let filters = FilterSpec::new().with(Filter::Status(Comparison::Eq, 200));
        let sort = SortSpec::new(&filters, SortColumn::WordCount, SortDirection::Asc).unwrap();
        let (took, page) = median_of(5, || {
            reader.query_rows(&filters, &sort, 200_000, 200).unwrap()
        });
        eprintln!(
            "\n=== live writer ===\n  reader returned {} rows in {:.1} ms with a batch open",
            page.rows.len(),
            ms(took)
        );
        assert_eq!(page.rows.len(), 200);
        assert!(
            took <= FILTER_SORT_BUDGET,
            "the reader was throttled by the open write batch: {took:?}"
        );
        writer.flush().unwrap();
    }

    // ---- the overview, while we have a million rows -----------------------
    let (took, overview) = median_of(5, || store.issue_overview().unwrap());
    eprintln!(
        "\n=== issue overview ===\n  {} rules, {} issues, {:.1} ms",
        overview.by_rule.len(),
        overview.total_issues,
        ms(took)
    );
    assert!(
        took <= FILTER_SORT_BUDGET,
        "the issue overview missed the grid's budget: {took:?}"
    );
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}
