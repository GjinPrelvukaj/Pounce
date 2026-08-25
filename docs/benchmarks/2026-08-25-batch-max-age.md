# A batch that commits on age as well as size

**Date:** 2026-08-25
**Why:** reported as a bug — *"when I start crawl it only waits to show items"*.

---

## The bug

Uncommitted rows are invisible to every reader, including the app's own live
view of the file being written. The writer committed on **count alone**: one
transaction per 500 records.

At full speed that is a tenth of a second and nobody notices. At the pace a
polite crawl actually runs — 5 URL/s against a site you do not own — **500 rows
is a hundred seconds**, and for those hundred seconds a running crawl shows a
climbing "pages done" counter above an empty grid. It reads as a hang, and the
findings rail is empty too, which is what made it look like the app was waiting
for issues rather than for a commit.

## The change

`BATCH_MAX_AGE = 2s`. A batch commits when it reaches 500 rows **or** when the
open transaction is older than two seconds, whichever comes first.

It costs nothing when the crawl is fast, because the deadline never fires: 500
rows arrive in about a tenth of a second. It only ever shortens a batch that was
going to be slow anyway, which is exactly when an extra commit is affordable.

## Measured — no throughput cost

`bench-runner --pages 100000`, three runs each side, same session, same machine:

| | Run 1 | Run 2 | Run 3 | Median |
| --- | ---: | ---: | ---: | ---: |
| Count only (before) | 21.8 s | 22.5 s | 21.8 s | **21.8 s** |
| Count or 2 s (after) | 22.3 s | 21.2 s | 21.6 s | **21.6 s** |

Within noise, and if anything faster. Peak RSS unchanged at 64–71 MB. At this
speed the fixture delivers 500 rows long before the deadline, so the timer never
fires — which is the whole design.

## Measured — the thing it was for

The app against the local fixture at **1 request at a time with a 200 ms wait**
(5 URL/s, more polite than any default):

- **Before:** nothing in the grid until page 500 — about 100 seconds.
- **After:** 54 pages on screen at 10.8 seconds, with the findings rail already
  showing three rules and their counts.

## The invariant this does not break

`CLAUDE.md` says the writer batches ~500 per transaction, and it still does. The
size is what stops a fast crawl paying a durability barrier per row; the age is
what stops a slow crawl being invisible. The crash window is unchanged in the
only direction that matters — it can now only be *smaller* than one batch.
