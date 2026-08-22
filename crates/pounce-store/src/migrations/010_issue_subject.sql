-- An issue's subject is a URL, which may not be a page.
--
-- A redirect loop never produces a page row: the source lands in
-- `crawl_redirects` and `crawl_failures` while only its landing — which a loop
-- does not have — would become a page. The same holds for a hop-limit blowout
-- and an unreachable host. With `page_id NOT NULL` those findings were
-- unrecordable, so `--fail-on critical` would have passed a site full of
-- redirect loops.
--
-- `url` is therefore the subject and is always present; `page_id` stays as a
-- nullable fast join for the common case, so the grid does not pay a text join
-- per row and `ON DELETE CASCADE` still cleans up when a page goes away. An
-- issue with no page is not touched by that cascade, which is correct: it was
-- never about a page.
--
-- SQLite cannot relax a NOT NULL in place, so the table is rebuilt.
ALTER TABLE issues RENAME TO issues_old;

CREATE TABLE issues (
    id       INTEGER PRIMARY KEY,
    url      TEXT    NOT NULL,
    page_id  INTEGER REFERENCES pages (id) ON DELETE CASCADE,
    rule_id  TEXT    NOT NULL,
    severity TEXT    NOT NULL,
    detail   TEXT
) STRICT;

INSERT INTO issues (id, url, page_id, rule_id, severity, detail)
SELECT o.id, p.url, o.page_id, o.rule_id, o.severity, o.detail
FROM issues_old o
JOIN pages p ON p.id = o.page_id;

DROP TABLE issues_old;
