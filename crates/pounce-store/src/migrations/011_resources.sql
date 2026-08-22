-- One row per non-page URL the crawl checked with a `HEAD` request: today the
-- `<img src>` targets `media.broken-image` and `media.oversized-image` read.
--
-- Keyed by URL rather than by page, because a resource is referenced from many
-- pages and fetched once. A shared logo on 500k pages is one row and one
-- request; keying by page would be 500k of each.
--
-- Deliberately *not* in `pages`. A page is something the crawl parsed and the
-- grid lists; an image checked with `HEAD` has no body, no title, and no
-- outbound links, and putting it in `pages` would inflate every page count the
-- benchmarks publish and every "pages crawled" number a user reads.
CREATE TABLE resources (
    url            TEXT    PRIMARY KEY,
    status         INTEGER NOT NULL,
    -- Nullable rather than 0: a server that declares no length is a different
    -- report from one that declares zero bytes, so `oversized-image` must read
    -- NULL as *unknown* and not as *small*.
    content_length INTEGER,
    content_type   TEXT
) STRICT, WITHOUT ROWID;
