-- Split `pages` into the narrow grid row and its detail.
--
-- The grid reads nine scalar columns and sorts a million rows by them; the six
-- repeating JSON fields are read one row at a time by the detail pane and never
-- sorted, filtered or grouped. Keeping them in `pages` made every grid sort drag
-- their bytes through a B-tree that also carries eight indices.
--
-- Measured before adopting, because the split adds a second insert per page to
-- the crawl's hot path and this project's surprises live there. It did not cost
-- throughput — at 100k pages, medians of five interleaved pairs, the split ran
-- 2.65% *faster* (27,294 vs 26,571 pages/s), because the bytes it removes from
-- `pages` outweigh the row it adds. The 1M file is 106 MB smaller than the
-- duplicated `row_view` alternative, which is not adopted:
-- docs/benchmarks/2026-08-24-narrow-row-shape.md.
--
-- The columns are dropped rather than the table rebuilt: none of the six is
-- indexed, so SQLite's DROP COLUMN can take them, and an existing `.pounce`
-- file keeps its ids — which every link edge and issue points at.
CREATE TABLE page_detail (
    page_id        INTEGER PRIMARY KEY REFERENCES pages (id) ON DELETE CASCADE,
    redirect_chain TEXT NOT NULL,
    h1             TEXT NOT NULL,
    h2             TEXT NOT NULL,
    hreflang       TEXT NOT NULL,
    open_graph     TEXT NOT NULL,
    images         TEXT NOT NULL
) STRICT;

INSERT INTO page_detail (page_id, redirect_chain, h1, h2, hreflang, open_graph, images)
    SELECT id, redirect_chain, h1, h2, hreflang, open_graph, images FROM pages;

ALTER TABLE pages DROP COLUMN redirect_chain;
ALTER TABLE pages DROP COLUMN h1;
ALTER TABLE pages DROP COLUMN h2;
ALTER TABLE pages DROP COLUMN hreflang;
ALTER TABLE pages DROP COLUMN open_graph;
ALTER TABLE pages DROP COLUMN images;
