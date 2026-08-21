//! Concurrent crawl frontier: priority plus global URL deduplication.

use crate::CrawlUrl;
use dashmap::{DashMap, mapref::entry::Entry};
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering as AtomicOrdering};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrontierItem {
    pub url: CrawlUrl,
    pub depth: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushResult {
    New,
    Shallower,
    Duplicate,
}

#[derive(Debug, Clone, Copy)]
struct Seen {
    depth: u16,
    queued: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Queued {
    url: CrawlUrl,
    depth: u16,
    order: u64,
}

impl Ord for Queued {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .depth
            .cmp(&self.depth)
            .then_with(|| other.order.cmp(&self.order))
            .then_with(|| self.url.cmp(&other.url))
    }
}

impl PartialOrd for Queued {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Default)]
pub struct Frontier {
    seen: DashMap<CrawlUrl, Seen>,
    queue: Mutex<BinaryHeap<Queued>>,
    next_order: AtomicU64,
    pending: AtomicUsize,
}

impl Frontier {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&self, url: CrawlUrl, depth: u16) -> PushResult {
        match self.seen.entry(url.clone()) {
            Entry::Vacant(entry) => {
                entry.insert(Seen {
                    depth,
                    queued: true,
                });
                self.pending.fetch_add(1, AtomicOrdering::Relaxed);
                self.enqueue(url, depth);
                PushResult::New
            }
            Entry::Occupied(mut entry) if depth < entry.get().depth => {
                let seen = entry.get_mut();
                seen.depth = depth;
                if seen.queued {
                    self.enqueue(url, depth);
                }
                PushResult::Shallower
            }
            Entry::Occupied(_) => PushResult::Duplicate,
        }
    }

    pub fn restore(&self, url: CrawlUrl, depth: u16, done: bool) -> PushResult {
        if !done {
            return self.push(url, depth);
        }
        match self.seen.entry(url) {
            Entry::Vacant(entry) => {
                entry.insert(Seen {
                    depth,
                    queued: false,
                });
                PushResult::New
            }
            Entry::Occupied(mut entry) => {
                let seen = entry.get_mut();
                let result = if depth < seen.depth {
                    seen.depth = depth;
                    PushResult::Shallower
                } else {
                    PushResult::Duplicate
                };
                if seen.queued {
                    seen.queued = false;
                    self.pending.fetch_sub(1, AtomicOrdering::Relaxed);
                }
                result
            }
        }
    }

    pub fn pop(&self) -> Option<FrontierItem> {
        loop {
            let item = self.queue.lock().unwrap().pop()?;
            let Some(mut seen) = self.seen.get_mut(&item.url) else {
                continue;
            };
            if !seen.queued || seen.depth != item.depth {
                continue;
            }
            seen.queued = false;
            self.pending.fetch_sub(1, AtomicOrdering::Relaxed);
            return Some(FrontierItem {
                url: item.url,
                depth: item.depth,
            });
        }
    }

    /// Returns an item whose handoff to the fetch stage failed.
    pub fn requeue(&self, item: FrontierItem) -> bool {
        let Some(mut seen) = self.seen.get_mut(&item.url) else {
            return false;
        };
        if seen.queued {
            return false;
        }
        seen.depth = seen.depth.min(item.depth);
        seen.queued = true;
        let depth = seen.depth;
        self.pending.fetch_add(1, AtomicOrdering::Relaxed);
        self.enqueue(item.url, depth);
        true
    }

    pub fn pending_len(&self) -> usize {
        self.pending.load(AtomicOrdering::Relaxed)
    }

    pub fn seen_len(&self) -> usize {
        self.seen.len()
    }

    fn enqueue(&self, url: CrawlUrl, depth: u16) {
        let order = self.next_order.fetch_add(1, AtomicOrdering::Relaxed);
        self.queue
            .lock()
            .unwrap()
            .push(Queued { url, depth, order });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::sync::Arc;

    fn url(path: &str) -> CrawlUrl {
        CrawlUrl::parse(&format!("https://example.com/{path}")).unwrap()
    }

    #[test]
    fn shallower_urls_are_popped_first() {
        let frontier = Frontier::new();
        frontier.push(url("deep"), 4);
        frontier.push(url("seed"), 0);
        frontier.push(url("middle"), 2);

        assert_eq!(frontier.pop().unwrap().url, url("seed"));
        assert_eq!(frontier.pop().unwrap().url, url("middle"));
        assert_eq!(frontier.pop().unwrap().url, url("deep"));
    }

    #[test]
    fn equal_depth_is_fifo() {
        let frontier = Frontier::new();
        for path in ["first", "second", "third"] {
            frontier.push(url(path), 2);
        }

        for path in ["first", "second", "third"] {
            assert_eq!(frontier.pop().unwrap().url, url(path));
        }
    }

    #[test]
    fn duplicate_urls_are_never_queued_twice() {
        let frontier = Frontier::new();
        assert_eq!(frontier.push(url("same"), 2), PushResult::New);
        assert_eq!(frontier.push(url("same"), 2), PushResult::Duplicate);
        assert_eq!(frontier.pending_len(), 1);
        assert!(frontier.pop().is_some());
        assert!(frontier.pop().is_none());
        assert_eq!(frontier.seen_len(), 1);
    }

    #[test]
    fn a_shallower_rediscovery_updates_a_queued_urls_priority() {
        let frontier = Frontier::new();
        frontier.push(url("target"), 5);
        frontier.push(url("other"), 3);
        assert_eq!(frontier.push(url("target"), 1), PushResult::Shallower);

        assert_eq!(
            frontier.pop(),
            Some(FrontierItem {
                url: url("target"),
                depth: 1,
            })
        );
        assert_eq!(frontier.pop().unwrap().url, url("other"));
        assert!(
            frontier.pop().is_none(),
            "the stale depth-5 entry is skipped"
        );
    }

    #[test]
    fn restore_queues_pending_urls_and_only_remembers_completed_ones() {
        let frontier = Frontier::new();
        assert_eq!(frontier.restore(url("pending"), 2, false), PushResult::New);
        assert_eq!(frontier.restore(url("done"), 1, true), PushResult::New);

        assert_eq!(frontier.pending_len(), 1);
        assert_eq!(frontier.seen_len(), 2);
        assert_eq!(frontier.push(url("done"), 1), PushResult::Duplicate);
        assert_eq!(frontier.pop().unwrap().url, url("pending"));
        assert!(frontier.pop().is_none());
    }

    #[test]
    fn concurrent_producers_still_create_one_item_per_url() {
        const URLS: usize = 1_000;
        let frontier = Arc::new(Frontier::new());
        let threads = (0..8)
            .map(|_| {
                let frontier = Arc::clone(&frontier);
                std::thread::spawn(move || {
                    for i in 0..URLS {
                        frontier.push(url(&i.to_string()), 1);
                    }
                })
            })
            .collect::<Vec<_>>();
        for thread in threads {
            thread.join().unwrap();
        }

        assert_eq!(frontier.seen_len(), URLS);
        assert_eq!(frontier.pending_len(), URLS);
        let mut popped = HashSet::new();
        while let Some(item) = frontier.pop() {
            assert!(popped.insert(item.url), "a URL was returned twice");
        }
        assert_eq!(popped.len(), URLS);
    }

    #[test]
    fn a_failed_fetch_handoff_can_be_requeued_without_becoming_a_duplicate() {
        let frontier = Frontier::new();
        frontier.push(url("retry"), 3);
        let item = frontier.pop().unwrap();

        assert!(frontier.requeue(item));
        assert_eq!(frontier.pending_len(), 1);
        assert_eq!(frontier.pop().unwrap().url, url("retry"));
        assert!(frontier.pop().is_none());
    }
}
