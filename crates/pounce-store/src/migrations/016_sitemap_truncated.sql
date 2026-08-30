-- Whether a sitemap document had more URLs than we read.
--
-- `MAX_LOCATIONS` stops at 50,000 per document — the protocol's own limit, and
-- a bound on what a malformed file can cost. Hitting it silently was the bug:
-- a 100k crawl stored 50,000 of the site's ~97,000 listed URLs, so the sitemap
-- was reported smaller than it is, and every URL past the cap became a page
-- "crawled but listed nowhere" — a finding invented by our own limit rather
-- than found on the site.
ALTER TABLE sitemaps ADD COLUMN truncated INTEGER NOT NULL DEFAULT 0;
