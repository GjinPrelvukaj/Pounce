use anyhow::{Result, bail};
use pounce_core::{CrawlUrl, Frontier, FrontierItem, PipelineConfig, Scope, run_pipeline};
use pounce_http::fetch::Fetcher;
use pounce_http::redirect::{Outcome, RedirectChain};
use pounce_parse::{PageRecord, parse_body};
use pounce_store::{CrawlState, Store, Writer};
use std::path::Path;
use std::sync::Arc;

const FRONTIER_BATCH: usize = 4_096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CrawlSummary {
    pub pages: u64,
    pub failures: u64,
}

pub async fn crawl(seed: CrawlUrl, output: &Path) -> Result<CrawlSummary> {
    if output.exists() {
        bail!("output already exists: {}", output.display());
    }

    let mut store = Store::open(output)?;
    CrawlState::new(&mut store).start(&seed)?;
    let mut writer = Writer::new(&mut store);
    let frontier = Frontier::new();
    frontier.push(seed.clone(), 0);
    let scope = Scope::new(&seed);
    let fetcher = Arc::new(Fetcher::new(Default::default())?);
    let mut pages = 0;
    let mut failures = 0;

    while frontier.pending_len() > 0 {
        // ponytail: finite batches let the existing bounded pipeline consume a
        // frontier that grows during writes, without adding another scheduler.
        let items = (0..FRONTIER_BATCH)
            .map_while(|_| frontier.pop())
            .collect::<Vec<_>>();
        let fetch = Arc::clone(&fetcher);

        run_pipeline(
            items,
            PipelineConfig::default(),
            move |item: FrontierItem| {
                let fetch = Arc::clone(&fetch);
                async move {
                    let chain = fetch.follow(&item.url).await;
                    (item, chain)
                }
            },
            parse,
            |outcome| -> std::result::Result<(), pounce_store::StoreError> {
                match outcome {
                    Ok(record) => {
                        let discovered = record
                            .links
                            .iter()
                            .filter_map(|link| {
                                let target = link.target.as_ref()?;
                                scope
                                    .should_follow(target, link.nofollow)
                                    .then(|| (target.clone(), record.depth.saturating_add(1)))
                            })
                            .collect::<Vec<_>>();
                        writer.discover(&discovered)?;
                        for (url, depth) in discovered {
                            frontier.push(url, depth);
                        }
                        writer.push(&record)?;
                        pages += 1;
                        Ok(())
                    }
                    Err(Failure { url, reason }) => {
                        writer.fail(&url, &reason)?;
                        failures += 1;
                        Ok(())
                    }
                }
            },
        )
        .await?;
    }
    writer.flush()?;

    Ok(CrawlSummary { pages, failures })
}

struct Failure {
    url: CrawlUrl,
    reason: String,
}

fn parse((item, chain): (FrontierItem, RedirectChain)) -> std::result::Result<PageRecord, Failure> {
    let RedirectChain { hops, outcome, .. } = chain;
    let redirect_chain = hops.iter().map(|hop| hop.url.to_string()).collect();
    match outcome {
        Outcome::Landed(fetched) => {
            let mut record = PageRecord::from_fetched(&fetched, item.depth, redirect_chain);
            match parse_body(&mut record, &fetched.body) {
                Ok(()) => Ok(record),
                Err(reason) => Err(Failure {
                    url: item.url,
                    reason: format!("parse failed: {reason}"),
                }),
            }
        }
        Outcome::Loop(url) => Err(Failure {
            url: item.url,
            reason: format!("redirect loop at {url}"),
        }),
        Outcome::HopLimit => Err(Failure {
            url: item.url,
            reason: "redirect hop limit reached".into(),
        }),
        Outcome::NoLocation => Err(Failure {
            url: item.url,
            reason: "redirect has no usable location".into(),
        }),
        Outcome::Failed(error) => Err(Failure {
            url: item.url,
            reason: error.to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pounce_bench::graph::{GraphSpec, SiteGraph};
    use pounce_bench::server::{Fixture, serve};
    use pounce_store::Store;
    use std::sync::Arc;

    #[tokio::test(flavor = "multi_thread")]
    async fn a_fixture_crawl_writes_every_reachable_resource_once() {
        const PAGES: u32 = 128;
        const REACHABLE: u64 = PAGES as u64 + 1; // generated pages + sitemap.xml
        let graph = SiteGraph::generate(&GraphSpec {
            page_count: PAGES,
            seed: 42,
            ..GraphSpec::default()
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let fixture = Arc::new(Fixture {
            graph,
            base_url: base_url.clone(),
        });
        let server = tokio::spawn(async move { serve(listener, fixture).await });
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("fixture.pounce");

        let summary = crawl(CrawlUrl::parse(&base_url).unwrap(), &output)
            .await
            .unwrap();
        let overwrite = crawl(CrawlUrl::parse(&base_url).unwrap(), &output)
            .await
            .unwrap_err();

        server.abort();
        assert!(overwrite.to_string().contains("output already exists"));
        assert_eq!(summary.pages, REACHABLE);
        assert_eq!(summary.failures, 0);
        let store = Store::open(output).unwrap();
        let (rows, distinct): (u64, u64) = store
            .conn()
            .query_row(
                "SELECT count(*), count(DISTINCT url) FROM pages",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!((rows, distinct), (REACHABLE, REACHABLE));
        let pending: u64 = store
            .conn()
            .query_row(
                "SELECT count(*) FROM frontier f \
                 LEFT JOIN pages p ON p.url = f.url \
                 LEFT JOIN crawl_failures e ON e.url = f.url \
                 WHERE p.id IS NULL AND e.url IS NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(pending, 0);
    }
}
