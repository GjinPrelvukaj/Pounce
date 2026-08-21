//! Bounded handoffs between crawl stages.

use crate::lifecycle::Admission;
use crate::{CrawlLifecycle, CrawlLimits};
use std::future::Future;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::task::JoinSet;

#[derive(Debug, Clone, Copy)]
pub struct PipelineConfig {
    pub channel_capacity: usize,
    pub fetch_concurrency: usize,
    pub parse_concurrency: usize,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            channel_capacity: 64,
            fetch_concurrency: 16,
            parse_concurrency: 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PipelineStats {
    pub received: u64,
    pub written: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum PipelineError<E> {
    #[error("pipeline stage stopped: {0}")]
    Stage(String),
    #[error("writer failed: {0}")]
    Writer(E),
}

pub async fn run_pipeline<Items, I, Fetch, FetchFuture, Fetched, Parse, Parsed, Write, E>(
    items: Items,
    config: PipelineConfig,
    fetch: Fetch,
    parse: Parse,
    write: Write,
) -> Result<PipelineStats, PipelineError<E>>
where
    Items: IntoIterator<Item = I> + Send + 'static,
    Items::IntoIter: Send,
    I: Send + 'static,
    Fetch: Fn(I) -> FetchFuture + Clone + Send + 'static,
    FetchFuture: Future<Output = Fetched> + Send + 'static,
    Fetched: Send + 'static,
    Parse: Fn(Fetched) -> Parsed + Clone + Send + 'static,
    Parsed: Send + 'static,
    Write: FnMut(Parsed) -> Result<(), E> + Send,
{
    run_controlled_pipeline(
        items,
        config,
        Arc::new(CrawlLifecycle::new(CrawlLimits::default())),
        |_| 0,
        fetch,
        parse,
        write,
    )
    .await
}

pub async fn run_controlled_pipeline<
    Items,
    I,
    Depth,
    Fetch,
    FetchFuture,
    Fetched,
    Parse,
    Parsed,
    Write,
    E,
>(
    items: Items,
    config: PipelineConfig,
    lifecycle: Arc<CrawlLifecycle>,
    depth: Depth,
    fetch: Fetch,
    parse: Parse,
    mut write: Write,
) -> Result<PipelineStats, PipelineError<E>>
where
    Items: IntoIterator<Item = I> + Send + 'static,
    Items::IntoIter: Send,
    I: Send + 'static,
    Depth: Fn(&I) -> u16 + Send + 'static,
    Fetch: Fn(I) -> FetchFuture + Clone + Send + 'static,
    FetchFuture: Future<Output = Fetched> + Send + 'static,
    Fetched: Send + 'static,
    Parse: Fn(Fetched) -> Parsed + Clone + Send + 'static,
    Parsed: Send + 'static,
    Write: FnMut(Parsed) -> Result<(), E> + Send,
{
    let capacity = config.channel_capacity.max(1);
    let (frontier_tx, frontier_rx) = mpsc::channel(capacity);
    let (fetch_tx, fetch_rx) = mpsc::channel(capacity);
    let (parse_tx, mut parse_rx) = mpsc::channel(capacity);

    let control = Arc::clone(&lifecycle);
    let produce = async move {
        let mut received = 0;
        for item in items {
            match control.admit(depth(&item)).await {
                Admission::Allow => {}
                Admission::SkipDepth => continue,
                Admission::Stop => break,
            }
            frontier_tx
                .send(item)
                .await
                .map_err(|_| "frontier → fetch channel closed".to_string())?;
            received += 1;
        }
        Ok::<_, String>(received)
    };
    let fetch_stage = run_async_stage(
        frontier_rx,
        fetch_tx,
        config.fetch_concurrency.max(1),
        fetch,
        "fetch",
    );
    let parse_stage =
        run_blocking_stage(fetch_rx, parse_tx, config.parse_concurrency.max(1), parse);
    let writer = async {
        let mut written = 0;
        while let Some(item) = parse_rx.recv().await {
            write(item).map_err(PipelineError::Writer)?;
            lifecycle.record_written();
            written += 1;
        }
        Ok::<_, PipelineError<E>>(written)
    };

    let (received, fetched, parsed, written) =
        tokio::join!(produce, fetch_stage, parse_stage, writer);
    let received = received.map_err(PipelineError::Stage)?;
    fetched.map_err(PipelineError::Stage)?;
    parsed.map_err(PipelineError::Stage)?;
    let written = written?;
    lifecycle.complete();
    Ok(PipelineStats { received, written })
}

async fn run_async_stage<I, O, F, Fut>(
    mut input: mpsc::Receiver<I>,
    output: mpsc::Sender<O>,
    concurrency: usize,
    work: F,
    name: &'static str,
) -> Result<(), String>
where
    I: Send + 'static,
    O: Send + 'static,
    F: Fn(I) -> Fut + Clone + Send + 'static,
    Fut: Future<Output = O> + Send + 'static,
{
    let mut jobs = JoinSet::new();
    let mut closed = false;
    loop {
        if closed && jobs.is_empty() {
            return Ok(());
        }
        if closed || jobs.len() >= concurrency {
            send_joined(&mut jobs, &output, name).await?;
            continue;
        }
        tokio::select! {
            joined = jobs.join_next(), if !jobs.is_empty() => {
                let item = joined.expect("guarded by is_empty")
                    .map_err(|error| format!("{name} worker failed: {error}"))?;
                output.send(item).await
                    .map_err(|_| format!("{name} output channel closed"))?;
            }
            item = input.recv() => match item {
                Some(item) => {
                    let work = work.clone();
                    jobs.spawn(async move { work(item).await });
                }
                None => closed = true,
            }
        }
    }
}

async fn run_blocking_stage<I, O, F>(
    mut input: mpsc::Receiver<I>,
    output: mpsc::Sender<O>,
    concurrency: usize,
    work: F,
) -> Result<(), String>
where
    I: Send + 'static,
    O: Send + 'static,
    F: Fn(I) -> O + Clone + Send + 'static,
{
    let mut jobs = JoinSet::new();
    let mut closed = false;
    loop {
        if closed && jobs.is_empty() {
            return Ok(());
        }
        if closed || jobs.len() >= concurrency {
            send_joined(&mut jobs, &output, "parse").await?;
            continue;
        }
        tokio::select! {
            joined = jobs.join_next(), if !jobs.is_empty() => {
                let item = joined.expect("guarded by is_empty")
                    .map_err(|error| format!("parse worker failed: {error}"))?;
                output.send(item).await
                    .map_err(|_| "parse output channel closed".to_string())?;
            }
            item = input.recv() => match item {
                Some(item) => {
                    let work = work.clone();
                    jobs.spawn_blocking(move || work(item));
                }
                None => closed = true,
            }
        }
    }
}

async fn send_joined<T: Send + 'static>(
    jobs: &mut JoinSet<T>,
    output: &mpsc::Sender<T>,
    name: &str,
) -> Result<(), String> {
    let item = jobs
        .join_next()
        .await
        .expect("called only with an active worker")
        .map_err(|error| format!("{name} worker failed: {error}"))?;
    output
        .send(item)
        .await
        .map_err(|_| format!("{name} output channel closed"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, mpsc};
    use std::time::Duration;

    #[tokio::test(flavor = "multi_thread")]
    async fn every_item_crosses_all_three_handoffs_once() {
        let sum = Arc::new(AtomicUsize::new(0));
        let written = Arc::clone(&sum);
        let stats = run_pipeline(
            0usize..1_000,
            PipelineConfig {
                channel_capacity: 8,
                fetch_concurrency: 4,
                parse_concurrency: 2,
            },
            |n| async move { n + 1 },
            |n| n * 2,
            move |n| {
                written.fetch_add(n, Ordering::Relaxed);
                Ok::<_, ()>(())
            },
        )
        .await
        .unwrap();

        assert_eq!(
            stats,
            PipelineStats {
                received: 1_000,
                written: 1_000
            }
        );
        assert_eq!(sum.load(Ordering::Relaxed), 1_001_000);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn terminal_fetch_failures_reach_the_writer() {
        let outcomes = Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen = Arc::clone(&outcomes);
        run_pipeline(
            0u8..4,
            PipelineConfig::default(),
            |n| async move { if n == 2 { Err("terminal") } else { Ok(n) } },
            std::convert::identity,
            move |outcome| {
                seen.lock().unwrap().push(outcome);
                Ok::<_, ()>(())
            },
        )
        .await
        .unwrap();

        let outcomes = outcomes.lock().unwrap();
        assert_eq!(outcomes.len(), 4);
        assert!(outcomes.contains(&Err("terminal")));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_slow_writer_stops_the_source_from_running_ahead() {
        const TOTAL: usize = 10_000;
        let pulled = Arc::new(AtomicUsize::new(0));
        let source_count = Arc::clone(&pulled);
        let source = (0..TOTAL).inspect(move |_| {
            source_count.fetch_add(1, Ordering::Relaxed);
        });
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();

        let crawl = tokio::spawn(run_pipeline(
            source,
            PipelineConfig {
                channel_capacity: 2,
                fetch_concurrency: 1,
                parse_concurrency: 1,
            },
            |n| async move { n },
            std::convert::identity,
            move |_| {
                entered_tx.send(()).ok();
                release_rx.recv().unwrap();
                Ok::<_, ()>(())
            },
        ));

        tokio::task::spawn_blocking(move || entered_rx.recv_timeout(Duration::from_secs(2)))
            .await
            .unwrap()
            .unwrap();
        tokio::time::sleep(Duration::from_millis(25)).await;
        assert!(pulled.load(Ordering::Relaxed) < TOTAL);

        for _ in 0..TOTAL {
            if release_tx.send(()).is_err() {
                break;
            }
        }
        assert_eq!(crawl.await.unwrap().unwrap().written, TOTAL as u64);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_paused_pipeline_admits_nothing_until_resumed() {
        let lifecycle = Arc::new(CrawlLifecycle::new(Default::default()));
        lifecycle.pause();
        let fetched = Arc::new(AtomicUsize::new(0));
        let fetch_count = Arc::clone(&fetched);
        let control = Arc::clone(&lifecycle);
        let crawl = tokio::spawn(run_controlled_pipeline(
            0u16..3,
            PipelineConfig::default(),
            control,
            |depth| *depth,
            move |depth| {
                fetch_count.fetch_add(1, Ordering::Relaxed);
                async move { depth }
            },
            std::convert::identity,
            |_| Ok::<_, ()>(()),
        ));

        tokio::time::sleep(Duration::from_millis(25)).await;
        assert_eq!(fetched.load(Ordering::Relaxed), 0);
        lifecycle.resume();
        assert_eq!(crawl.await.unwrap().unwrap().written, 3);
        assert_eq!(lifecycle.status(), crate::CrawlStatus::Completed);
        assert_eq!(lifecycle.progress().written, 3);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn cancelling_a_paused_pipeline_wakes_and_stops_it() {
        let lifecycle = Arc::new(CrawlLifecycle::new(Default::default()));
        lifecycle.pause();
        let control = Arc::clone(&lifecycle);
        let crawl = tokio::spawn(run_controlled_pipeline(
            0u16..3,
            PipelineConfig::default(),
            control,
            |depth| *depth,
            |depth| async move { depth },
            std::convert::identity,
            |_| Ok::<_, ()>(()),
        ));

        lifecycle.cancel();
        assert_eq!(crawl.await.unwrap().unwrap().written, 0);
        assert_eq!(lifecycle.status(), crate::CrawlStatus::Cancelled);
    }
}
