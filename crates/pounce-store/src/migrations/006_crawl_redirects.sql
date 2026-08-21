-- A redirect source is a completed frontier URL, but its 3xx status is not the
-- landing page's 200 status. Keep chains separately so both remain true, and
-- so loops and failed hops do not lose the route that exposed them.
CREATE TABLE crawl_redirects (
    source_url TEXT    PRIMARY KEY REFERENCES frontier(url) ON DELETE CASCADE,
    status     INTEGER NOT NULL CHECK (status IN (301, 302, 303, 307, 308)),
    final_url  TEXT             REFERENCES pages(url),
    chain      TEXT    NOT NULL,
    outcome    TEXT    NOT NULL
) STRICT, WITHOUT ROWID;

CREATE INDEX crawl_redirects_status ON crawl_redirects (status);
