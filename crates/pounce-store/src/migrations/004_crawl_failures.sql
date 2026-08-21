-- A request that exhausted retries has no PageRecord, but is still a terminal
-- crawl outcome. Keeping it beside pages lets resume distinguish it from work
-- that was interrupted before an outcome was committed.
CREATE TABLE crawl_failures (
    url    TEXT PRIMARY KEY REFERENCES frontier(url) ON DELETE CASCADE,
    reason TEXT NOT NULL
) STRICT, WITHOUT ROWID;
