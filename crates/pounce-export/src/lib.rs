//! CSV and JSON out of a crawl, one row at a time.
//!
//! The rule this crate exists to respect is the one the whole design rests on:
//! **the dataset never materialises.** An export of 500,000 pages is a
//! statement handed to SQLite and a `Write` handed to the caller, with exactly
//! one row alive between them. Collecting into a `Vec<Row>` first would work on
//! the developer's test crawl and take a gigabyte on a user's.
//!
//! It takes a `FilterSpec` and a `SortSpec` rather than "everything", so
//! exporting what is on screen and exporting the whole crawl are the same code
//! path with a different spec — a separate "export all" would be a second query
//! builder to keep in step with the first.
//!
//! **Absent is not empty, and the two formats disagree about how to say so.**
//! JSON has `null` and uses it. CSV has one empty field for both, which is a
//! real loss of information and is documented rather than papered over: a
//! missing `<title>` and `<title></title>` are different findings, and anyone
//! who needs to tell them apart wants the JSON.

use pounce_store::{FilterSpec, SortSpec, Store};
use std::io::Write;

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("could not write the export: {0}")]
    Io(#[from] std::io::Error),
    #[error("could not encode a row: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Csv,
    Json,
}

impl Format {
    /// The format a filename implies. `None` for anything else, so the caller
    /// asks rather than guessing on the user's behalf.
    pub fn from_path(path: &std::path::Path) -> Option<Self> {
        match path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("csv") => Some(Format::Csv),
            Some("json") => Some(Format::Json),
            _ => None,
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Format::Csv => "csv",
            Format::Json => "json",
        }
    }
}

/// The columns an export carries.
///
/// The order is the CSV's column order. JSON objects come out key-sorted —
/// `serde_json::Map` is a `BTreeMap` unless the `preserve_order` feature drags
/// in `indexmap`, and a dependency to control the key order of an unordered
/// format is not a trade worth making.
///
/// Wider than the grid's nine and narrower than the whole record: everything
/// here is a scalar on `pages`, so the export is one statement over one table
/// and streams at whatever rate the disk reads. The repeating fields — headings,
/// images, hreflang — belong to the detail pane and to a future JSON mode that
/// joins `page_detail`; putting them in a CSV column would mean flattening a
/// list into a cell, which is the format's classic lie.
const COLUMNS: &[&str] = &[
    "url",
    "status",
    "depth",
    "size",
    "content_type",
    "kind",
    "noindex",
    "title",
    "meta_description",
    "canonical",
    "word_count",
    "elapsed_ms",
];

/// Streams the rows matching `filters` into `out`, and returns how many.
pub fn export(
    store: &Store,
    filters: &FilterSpec,
    sort: &SortSpec,
    format: Format,
    out: &mut impl Write,
) -> Result<u64, ExportError> {
    let (where_sql, params) = filters.compile();
    let sql = format!(
        "SELECT {} FROM pages p {where_sql} {}",
        COLUMNS
            .iter()
            .map(|c| format!("p.{c}"))
            .collect::<Vec<_>>()
            .join(", "),
        sort.compile()
    );
    let mut stmt = store.conn().prepare(&sql)?;
    let mut rows = stmt.query(rusqlite::params_from_iter(params.iter()))?;

    let mut written = 0u64;
    match format {
        Format::Csv => {
            let mut line = String::new();
            for (i, name) in COLUMNS.iter().enumerate() {
                if i > 0 {
                    line.push(',');
                }
                line.push_str(name);
            }
            line.push('\n');
            out.write_all(line.as_bytes())?;

            while let Some(row) = rows.next()? {
                line.clear();
                for i in 0..COLUMNS.len() {
                    if i > 0 {
                        line.push(',');
                    }
                    write_csv_field(&mut line, &cell(row, i)?);
                }
                line.push('\n');
                out.write_all(line.as_bytes())?;
                written += 1;
            }
        }
        Format::Json => {
            // An array written by hand rather than serialised from a `Vec`,
            // which is the whole point: the brackets and commas are the only
            // state this holds.
            out.write_all(b"[")?;
            while let Some(row) = rows.next()? {
                if written > 0 {
                    out.write_all(b",")?;
                }
                out.write_all(b"\n  ")?;
                let mut map = serde_json::Map::with_capacity(COLUMNS.len());
                for (i, name) in COLUMNS.iter().enumerate() {
                    map.insert((*name).to_string(), json_cell(row, i)?);
                }
                serde_json::to_writer(&mut *out, &serde_json::Value::Object(map))?;
                written += 1;
            }
            out.write_all(if written > 0 { b"\n]\n" } else { b"]\n" })?;
        }
    }
    Ok(written)
}

/// One cell as text. `None` is SQL NULL — absent, not empty.
fn cell(row: &rusqlite::Row<'_>, i: usize) -> Result<Option<String>, rusqlite::Error> {
    Ok(match row.get_ref(i)? {
        rusqlite::types::ValueRef::Null => None,
        rusqlite::types::ValueRef::Integer(n) => Some(n.to_string()),
        rusqlite::types::ValueRef::Real(f) => Some(f.to_string()),
        rusqlite::types::ValueRef::Text(t) => Some(String::from_utf8_lossy(t).into_owned()),
        rusqlite::types::ValueRef::Blob(_) => None,
    })
}

fn json_cell(row: &rusqlite::Row<'_>, i: usize) -> Result<serde_json::Value, rusqlite::Error> {
    Ok(match row.get_ref(i)? {
        rusqlite::types::ValueRef::Null => serde_json::Value::Null,
        rusqlite::types::ValueRef::Integer(n) => serde_json::Value::from(n),
        rusqlite::types::ValueRef::Real(f) => serde_json::Value::from(f),
        rusqlite::types::ValueRef::Text(t) => {
            serde_json::Value::from(String::from_utf8_lossy(t).into_owned())
        }
        rusqlite::types::ValueRef::Blob(_) => serde_json::Value::Null,
    })
}

/// RFC 4180: quote when the value contains a comma, a quote, or a newline, and
/// double any quote inside.
///
/// Hand-rolled rather than a crate, because that is the whole specification and
/// the alternative is a dependency for eleven lines. A URL with a comma in it is
/// not exotic — query strings have them — and an unquoted one silently shifts
/// every column after it.
fn write_csv_field(out: &mut String, value: &Option<String>) {
    let Some(value) = value else { return };
    if value.contains([',', '"', '\n', '\r']) {
        out.push('"');
        for c in value.chars() {
            if c == '"' {
                out.push('"');
            }
            out.push(c);
        }
        out.push('"');
    } else {
        out.push_str(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_field_is_quoted_only_when_it_has_to_be() {
        let mut out = String::new();
        write_csv_field(&mut out, &Some("plain".into()));
        assert_eq!(out, "plain");

        out.clear();
        write_csv_field(&mut out, &Some("a,b".into()));
        assert_eq!(out, "\"a,b\"");

        out.clear();
        write_csv_field(&mut out, &Some("say \"hi\"".into()));
        assert_eq!(out, "\"say \"\"hi\"\"\"");

        out.clear();
        write_csv_field(&mut out, &Some("line\nbreak".into()));
        assert_eq!(out, "\"line\nbreak\"");

        // Absent and empty are the same field in CSV, and that is the format's
        // limit rather than a decision this function gets to make.
        out.clear();
        write_csv_field(&mut out, &None);
        assert_eq!(out, "");
        write_csv_field(&mut out, &Some(String::new()));
        assert_eq!(out, "");
    }
}
