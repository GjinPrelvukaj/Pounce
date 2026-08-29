//! Exports, against a real store.

use pounce_core::CrawlUrl;
use pounce_export::{Format, Subject, export};
use pounce_parse::{BodyKind, MetaRobots, PageRecord};
use pounce_store::{
    Comparison, Filter, FilterSpec, SortColumn, SortDirection, SortSpec, Store, Writer,
};

fn record(url: &str, status: u16, title: Option<&str>) -> PageRecord {
    PageRecord {
        url: CrawlUrl::parse(url).unwrap(),
        status,
        depth: 0,
        size: 100,
        truncated: false,
        content_type: Some("text/html".into()),
        charset: None,
        kind: BodyKind::Html,
        content_type_mismatch: false,
        elapsed_ms: 5,
        time_to_headers_ms: 3,
        redirect_chain: vec![],
        title: title.map(str::to_string),
        title_count: 1,
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
        word_count: 42,
        body_hash: None,
    }
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("pounce-export-tests");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("pounce-wal"));
    let _ = std::fs::remove_file(path.with_extension("pounce-shm"));
    path
}

fn seeded(name: &str) -> Store {
    let mut store = Store::open(scratch(name)).unwrap();
    {
        let mut writer = Writer::with_batch_size(&mut store, 8);
        // A comma and a quote in one title, which is the case an unquoted CSV
        // silently shifts every later column on.
        writer
            .push(&record("http://e.com/a", 200, Some("Hello, \"world\"")))
            .unwrap();
        writer.push(&record("http://e.com/b", 404, None)).unwrap();
        writer
            .push(&record("http://e.com/c", 200, Some("")))
            .unwrap();
        writer.flush().unwrap();
    }
    store
}

fn all() -> (FilterSpec, SortSpec) {
    let spec = FilterSpec::new();
    let sort = SortSpec::new(&spec, SortColumn::Url, SortDirection::Asc).unwrap();
    (spec, sort)
}

#[test]
fn csv_quotes_what_it_must_and_nothing_else() {
    let store = seeded("csv.pounce");
    let (spec, sort) = all();
    let mut out = Vec::new();
    let n = export(&store, Subject::Pages, &spec, &sort, Format::Csv, &mut out).unwrap();
    assert_eq!(n, 3);

    let text = String::from_utf8(out).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert!(lines[0].starts_with("url,status,depth,size"));
    assert_eq!(lines.len(), 4);
    assert!(
        lines[1].contains("\"Hello, \"\"world\"\"\""),
        "a title with a comma and quotes must survive: {}",
        lines[1]
    );
    // Every row has the same number of fields — the property an unquoted comma
    // breaks, and the reason the quoting exists at all.
    let commas = lines[0].matches(',').count();
    assert_eq!(
        lines[2].matches(',').count(),
        commas,
        "an absent title must still leave its column: {}",
        lines[2]
    );
}

#[test]
fn json_keeps_absent_and_empty_apart() {
    let store = seeded("json.pounce");
    let (spec, sort) = all();
    let mut out = Vec::new();
    export(&store, Subject::Pages, &spec, &sort, Format::Json, &mut out).unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&out).unwrap();
    let rows = parsed.as_array().unwrap();
    assert_eq!(rows.len(), 3);

    // This is the distinction the store has carried from the parser, and the
    // one CSV cannot express.
    assert_eq!(rows[0]["title"], serde_json::json!("Hello, \"world\""));
    assert_eq!(rows[1]["title"], serde_json::Value::Null);
    assert_eq!(rows[2]["title"], serde_json::json!(""));
}

#[test]
fn an_export_is_the_filtered_view_not_the_whole_crawl() {
    // T4.15's whole content: the same code path with a different spec, so
    // "export what I am looking at" cannot drift from "export everything".
    let store = seeded("filtered.pounce");
    let spec = FilterSpec::new().with(Filter::Status(Comparison::Ge, 400));
    let sort = SortSpec::new(&spec, SortColumn::Url, SortDirection::Asc).unwrap();
    let mut out = Vec::new();
    let n = export(&store, Subject::Pages, &spec, &sort, Format::Csv, &mut out).unwrap();
    assert_eq!(n, 1);
    assert!(String::from_utf8(out).unwrap().contains("http://e.com/b"));
}

#[test]
fn an_empty_result_is_still_a_valid_file() {
    // An export that matched nothing is a header and no rows, or `[]` — not an
    // empty file, which reads as a failed export.
    let store = seeded("empty.pounce");
    let spec = FilterSpec::new().with(Filter::Status(Comparison::Eq, 500));
    let sort = SortSpec::new(&spec, SortColumn::Url, SortDirection::Asc).unwrap();

    let mut csv = Vec::new();
    assert_eq!(
        export(&store, Subject::Pages, &spec, &sort, Format::Csv, &mut csv).unwrap(),
        0
    );
    assert_eq!(String::from_utf8(csv).unwrap().lines().count(), 1);

    let mut json = Vec::new();
    export(
        &store,
        Subject::Pages,
        &spec,
        &sort,
        Format::Json,
        &mut json,
    )
    .unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&json).unwrap(),
        serde_json::json!([])
    );
}

#[test]
fn a_filename_chooses_the_format() {
    assert_eq!(
        Format::from_path(std::path::Path::new("/tmp/x.csv")),
        Some(Format::Csv)
    );
    assert_eq!(
        Format::from_path(std::path::Path::new("/tmp/x.JSON")),
        Some(Format::Json)
    );
    // Anything else is a question for the caller, not a guess.
    assert_eq!(Format::from_path(std::path::Path::new("/tmp/x.txt")), None);
}

/// Exports a store you point it at, so the streaming claim can be measured
/// rather than asserted.
///
/// ```
/// SEED_IN=/tmp/big.pounce SEED_OUT=/tmp/big.csv \
///   /usr/bin/time -l cargo test --release -p pounce-export --test export \
///   stream_a_seeded_store -- --ignored --nocapture
/// ```
///
/// `/usr/bin/time -l` reports maximum resident set size, which is the number
/// the claim is about: if the rows accumulated, peak RSS would track the file.
#[test]
#[ignore = "needs a seeded store; run with --release --ignored --nocapture"]
fn stream_a_seeded_store() {
    let input = std::env::var("SEED_IN").expect("set SEED_IN to a .pounce file");
    let output = std::env::var("SEED_OUT").expect("set SEED_OUT to the file to write");
    let store = Store::open_read_only(&input).unwrap();
    let (spec, sort) = all();
    let started = std::time::Instant::now();
    let file = std::fs::File::create(&output).unwrap();
    let mut out = std::io::BufWriter::new(file);
    let format = if output.ends_with(".json") {
        Format::Json
    } else {
        Format::Csv
    };
    let rows = export(&store, Subject::Pages, &spec, &sort, format, &mut out).unwrap();
    drop(out);
    let bytes = std::fs::metadata(&output).unwrap().len();
    eprintln!(
        "{rows} rows → {output} ({} MB) in {:?}",
        bytes / 1_000_000,
        started.elapsed()
    );
}
