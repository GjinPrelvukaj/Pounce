-- What the site *says* it has, beside what the crawl *found*.
--
-- Every technical audit opens with two files — robots.txt and the sitemap —
-- and until now Pounce read the first for politeness, said nothing about it,
-- and never looked at the second. The comparison is the point: a URL in the
-- sitemap that no link reaches is an orphan the site owner believes is fine,
-- and a crawled page missing from the sitemap is a page they do not know they
-- have. Neither is visible from either list alone.
--
-- Keyed by URL rather than by sitemap: the same URL appearing in two sitemaps
-- is one address, and `source` keeps the first one that named it so a finding
-- can say which file to edit.
CREATE TABLE sitemap_urls (
    url    TEXT NOT NULL PRIMARY KEY,
    -- The sitemap document this URL was listed in.
    source TEXT NOT NULL
) STRICT, WITHOUT ROWID;

-- One row per sitemap document fetched, index or not, including the ones that
-- failed. A sitemap declared in robots.txt that answers 404 is a finding, and
-- a table holding only the successes cannot report it.
CREATE TABLE sitemaps (
    url      TEXT    NOT NULL PRIMARY KEY,
    status   INTEGER NOT NULL,
    -- URLs this document contributed. Zero on a 404, and zero on a sitemap
    -- that parsed to nothing, which are different rows because `status` says
    -- which.
    urls     INTEGER NOT NULL,
    is_index INTEGER NOT NULL,
    -- How it was found: `robots` or `guess` for the conventional
    -- /sitemap.xml, or the index that listed it.
    found_by TEXT    NOT NULL
) STRICT, WITHOUT ROWID;

-- robots.txt as served, so the audit can show it rather than describe it.
-- One row per origin: http and https on the same host are separate documents
-- and may disagree.
CREATE TABLE robots_files (
    origin TEXT    NOT NULL PRIMARY KEY,
    status INTEGER NOT NULL,
    -- NULL when the file could not be read at all. Not an empty string: an
    -- empty robots.txt permits everything and is a deliberate choice, while an
    -- unreachable one forbids everything and is usually an accident.
    body   TEXT
) STRICT, WITHOUT ROWID;
