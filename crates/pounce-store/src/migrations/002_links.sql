-- One row per anchor, queried in both directions.
--
-- The source is a foreign key because an edge cannot exist without the page
-- it was extracted from. The target stays a URL rather than a page id: links
-- are discovered before their targets are crawled, and external or broken
-- targets may never get a page row at all.
--
-- ponytail: one edge table serves both inlinks and outlinks. Mirroring the
-- graph into two tables would double writes and create two copies to reconcile.
CREATE TABLE links (
    id             INTEGER PRIMARY KEY,
    source_page_id INTEGER NOT NULL REFERENCES pages (id) ON DELETE CASCADE,
    href           TEXT    NOT NULL,
    target_url     TEXT,
    anchor_text    TEXT    NOT NULL,
    nofollow       INTEGER NOT NULL
) STRICT;

CREATE INDEX links_source ON links (source_page_id);
CREATE INDEX links_target ON links (target_url);
