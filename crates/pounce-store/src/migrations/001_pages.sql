-- One row per crawled URL.
--
-- Scalar columns are the ones the grid can sort and filter on, so each carries
-- an index. The repeating parts of a page — headings, images, hreflang,
-- Open Graph — are JSON text: they belong to the detail pane, which is fetched
-- one row at a time, and normalising them into child tables would cost a join
-- per visible row to display something nobody sorts by.
--
-- ponytail: JSON columns until an audit rule needs to filter inside one. The
-- upgrade path is a generated column plus an index on it, which SQLite can add
-- to an existing table without rewriting the JSON.
CREATE TABLE pages (
    id                    INTEGER PRIMARY KEY,
    url                   TEXT    NOT NULL UNIQUE,
    status                INTEGER NOT NULL,
    depth                 INTEGER NOT NULL,
    size                  INTEGER NOT NULL,
    truncated             INTEGER NOT NULL,
    content_type          TEXT,
    charset               TEXT,
    kind                  TEXT    NOT NULL,
    content_type_mismatch INTEGER NOT NULL,
    elapsed_ms            INTEGER NOT NULL,
    time_to_headers_ms    INTEGER NOT NULL,
    redirect_chain        TEXT    NOT NULL,
    title                 TEXT,
    meta_description      TEXT,
    h1                    TEXT    NOT NULL,
    h2                    TEXT    NOT NULL,
    canonical             TEXT,
    canonical_url         TEXT,
    noindex               INTEGER NOT NULL,
    nofollow              INTEGER NOT NULL,
    noarchive             INTEGER NOT NULL,
    nosnippet             INTEGER NOT NULL,
    hreflang              TEXT    NOT NULL,
    open_graph            TEXT    NOT NULL,
    images                TEXT    NOT NULL,
    word_count            INTEGER NOT NULL
) STRICT;

CREATE INDEX pages_status     ON pages (status);
CREATE INDEX pages_depth      ON pages (depth);
CREATE INDEX pages_size       ON pages (size);
CREATE INDEX pages_word_count ON pages (word_count);
CREATE INDEX pages_elapsed    ON pages (elapsed_ms);
CREATE INDEX pages_kind       ON pages (kind);
CREATE INDEX pages_title      ON pages (title);
CREATE INDEX pages_noindex    ON pages (noindex);
