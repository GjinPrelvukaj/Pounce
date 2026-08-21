-- Explicit columns keep lifecycle settings inspectable and type-checked. NULL
-- means unlimited, preserving the behavior of files created before T1.18.
ALTER TABLE crawl ADD COLUMN max_depth INTEGER CHECK (max_depth BETWEEN 0 AND 65535);
ALTER TABLE crawl ADD COLUMN max_urls INTEGER CHECK (max_urls >= 0);
ALTER TABLE crawl ADD COLUMN max_duration_ns INTEGER CHECK (max_duration_ns >= 0);
