-- Content facts the audit rules need in SQL.
--
-- `body_hash` exists because `duplicate body` is a `SiteRule` — it compares
-- pages against each other, so the value has to be queryable. Nullable on
-- purpose: NULL is "this page had no text to compare", which is different from
-- the hash of the empty string. Storing 0 would make every blank page a
-- duplicate of every other blank page.
--
-- SQLite integers are signed 64-bit, so the u64 is stored reinterpreted. That
-- is lossless and comparison still works, because equality of the bit pattern
-- is the only thing a duplicate check asks.
--
-- `title_count` is not strictly needed in SQL — `multiple <title>` is a
-- `PageRule` and reads the record directly — but it is a column the grid and
-- exports will want, and adding it now costs one migration instead of two.
ALTER TABLE pages ADD COLUMN title_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE pages ADD COLUMN body_hash INTEGER;
