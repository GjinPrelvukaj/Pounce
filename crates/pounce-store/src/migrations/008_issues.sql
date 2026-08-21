-- One row per finding.
--
-- `severity` is denormalised rather than joined from rule metadata: the grid
-- filters millions of issues by it, and a per-row join is exactly the query
-- pattern M3 exists to avoid. It also makes a `.pounce` file self-contained —
-- it records what was found at crawl time, and re-grading a rule in a later
-- build does not silently rewrite history.
--
-- No index here. `issues_page`, `issues_rule` and `issues_severity` are built
-- by `Store::build_query_indices` after the crawl, following what the 500k
-- measurement taught: an index nothing reads during a crawl is not maintained
-- during one.
--
-- The cascade matters. Without it, a re-crawl that removed a page would leave
-- issues pointing at nothing and per-rule counts would drift upward forever.
CREATE TABLE issues (
    id       INTEGER PRIMARY KEY,
    page_id  INTEGER NOT NULL REFERENCES pages (id) ON DELETE CASCADE,
    rule_id  TEXT    NOT NULL,
    severity TEXT    NOT NULL,
    detail   TEXT
) STRICT;
