//! Reading a crawl while it is still being written.
//!
//! The app's biggest single change in feel — results appearing during the
//! crawl rather than twenty minutes later — rests entirely on this working, so
//! it is tested against a real file on disk rather than reasoned about from
//! WAL's documentation.

mod common;

use pounce_store::{Store, StoreError, Writer, schema::SCHEMA_VERSION};

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("pounce-live-reader");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("pounce-wal"));
    let _ = std::fs::remove_file(path.with_extension("pounce-shm"));
    path
}

#[test]
fn a_reader_sees_rows_the_writer_has_already_committed() {
    let path = scratch("committed.pounce");
    let mut store = Store::open(&path).unwrap();
    {
        let mut writer = Writer::with_batch_size(&mut store, 4);
        for i in 0..8 {
            writer
                .push(&common::record_for(&common::url_of(i)))
                .unwrap();
        }
        writer.flush().unwrap();
    }

    // The writer is still open — this is the live case, not "read a closed
    // file". Two `Store`s on one path at once is exactly what the app does
    // between `start_crawl` and the last batch.
    let reader = Store::open_read_only(&path).unwrap();
    let seen: i64 = reader
        .conn()
        .query_row("SELECT count(*) FROM pages", [], |r| r.get(0))
        .unwrap();
    assert_eq!(seen, 8);

    // And it keeps up: a later batch is visible to the same connection without
    // reopening it.
    {
        let mut writer = Writer::with_batch_size(&mut store, 4);
        for i in 8..12 {
            writer
                .push(&common::record_for(&common::url_of(i)))
                .unwrap();
        }
        writer.flush().unwrap();
    }
    let seen: i64 = reader
        .conn()
        .query_row("SELECT count(*) FROM pages", [], |r| r.get(0))
        .unwrap();
    assert_eq!(seen, 12);
}

#[test]
fn a_read_only_store_cannot_write() {
    // The guarantee that makes the second connection safe. If this ever passes
    // a write through, the app has two writers on one file and the failure is
    // a corrupted crawl rather than an error message.
    let path = scratch("read-only.pounce");
    let _writer = Store::open(&path).unwrap();
    let reader = Store::open_read_only(&path).unwrap();
    assert!(
        reader
            .conn()
            .execute("DELETE FROM pages", [])
            .unwrap_err()
            .to_string()
            .contains("readonly")
    );
}

#[test]
fn an_unmigrated_file_is_refused_rather_than_migrated() {
    // A read-only connection cannot migrate, and quietly querying a file at an
    // older schema would mean `no such column` from deep inside a query.
    let path = scratch("unmigrated.pounce");
    rusqlite::Connection::open(&path).unwrap();
    let Err(err) = Store::open_read_only(&path) else {
        panic!("an unmigrated file must not open read-only");
    };
    assert!(
        matches!(err, StoreError::NotMigrated { found: 0, known } if known == SCHEMA_VERSION),
        "{err}"
    );
}
