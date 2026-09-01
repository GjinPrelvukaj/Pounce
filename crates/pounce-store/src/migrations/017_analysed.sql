-- Whether the work that runs *after* the crawl loop ever ran.
--
-- A crawl that is stopped, or whose process dies, keeps every page it fetched
-- — and silently loses the inlink index, the site rules (duplicates, orphans,
-- broken internal links) and the whole sitemap comparison, because all of them
-- run once at the end.
--
-- The file then looks complete. The Sitemap tab reports nothing found on a site
-- that has one; the duplicate findings are absent on a site full of them. A
-- reader cannot tell "we looked and found nothing" from "we never looked",
-- which is the same failure the panel's "not checked in this version" list
-- exists to prevent, arrived at from a different direction.
ALTER TABLE crawl ADD COLUMN analysed INTEGER NOT NULL DEFAULT 0;

-- Backfilled from physical evidence, because a default of 0 would accuse every
-- file written before this migration of being interrupted. `links_target` is
-- built in the same end-of-crawl block and only there, so its presence is proof
-- that block ran. A file without it either was interrupted — which is exactly
-- what the banner should say — or is a crawl still in flight, where the banner
-- is suppressed anyway.
UPDATE crawl SET analysed = 1
WHERE EXISTS (
    SELECT 1 FROM sqlite_master WHERE type = 'index' AND name = 'links_target'
);
