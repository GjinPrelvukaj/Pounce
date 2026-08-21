-- One .pounce file contains one crawl and the durable form of its frontier.
CREATE TABLE crawl (
    id       INTEGER PRIMARY KEY CHECK (id = 1),
    seed_url TEXT NOT NULL
) STRICT;

CREATE TABLE frontier (
    url   TEXT    PRIMARY KEY,
    depth INTEGER NOT NULL CHECK (depth BETWEEN 0 AND 65535)
) STRICT, WITHOUT ROWID;

-- Resume walks shallow URLs first; page existence supplies completion state,
-- so there is no second flag that can disagree with the durable result.
CREATE INDEX frontier_order ON frontier (depth, url);
