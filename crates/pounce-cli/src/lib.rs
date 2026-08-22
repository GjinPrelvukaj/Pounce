use anyhow::{Result, bail};
use pounce_audit::Registry;
use pounce_core::{
    CrawlUrl, Frontier, FrontierItem, PipelineConfig, PushResult, Scope, run_pipeline,
};
use pounce_http::fetch::Fetcher;
use pounce_http::redirect::{Outcome, RedirectChain};
use pounce_parse::{PageRecord, parse_body};
use pounce_store::{CrawlState, RedirectHop, Store, Writer};
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

const FRONTIER_BATCH: usize = 4_096;

/// Concurrent `HEAD` requests during the image pass.
///
/// Its own number, deliberately below the page pipeline's, because the whole
/// point of a separate budget is that checking images can never be the reason
/// a user's pages crawl slowly. The per-host limiter and robots.txt still
/// apply on top of it — this bounds the total, not the politeness.
const IMAGE_CONCURRENCY: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CrawlSummary {
    pub pages: u64,
    pub failures: u64,
    /// Distinct image URLs checked with `HEAD`; 0 when the pass was off.
    pub resources: u64,
}

pub async fn crawl(
    seed: CrawlUrl,
    output: &Path,
    registry: &Registry,
    check_images: bool,
) -> Result<CrawlSummary> {
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
    // ponytail: distinct image URLs held in memory, like the frontier already
    // holds distinct page URLs. Templates share their images, so this collapses
    // hard — the bench fixture's whole site references five. The ceiling is a
    // site with per-page unique images; the upgrade path is a durable table
    // drained in batches, the same shape the frontier already has.
    let mut image_urls: HashSet<CrawlUrl> = HashSet::new();

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
                    Ok(Parsed { record, redirect }) => {
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
                        // Dedupe in memory *before* persisting. The frontier's
                        // DashMap is exact, and on a real graph ~28 links per
                        // page resolve to about one new URL — so persisting the
                        // raw list first meant ~28x more upserts into a
                        // WITHOUT ROWID text-keyed table than the crawl needed.
                        // A Duplicate would have been a no-op row anyway;
                        // Shallower still has to land, because resume reads the
                        // depth back out.
                        let fresh = discovered
                            .into_iter()
                            .filter(|(url, depth)| {
                                !matches!(frontier.push(url.clone(), *depth), PushResult::Duplicate)
                            })
                            .collect::<Vec<_>>();
                        writer.discover(&fresh)?;
                        // Rules run here, where the record exists and its
                        // findings can join the same transaction as the page.
                        if check_images {
                            // Resolved against the page, because `src` is
                            // written relative and the same file referenced
                            // from two depths must be one resource.
                            image_urls.extend(
                                record
                                    .images
                                    .iter()
                                    .filter_map(|i| record.url.join(&i.src).ok()),
                            );
                        }
                        let issues = registry.run_page(&record);
                        let page_id = writer.push(&record)?;
                        if !issues.is_empty() {
                            let rows: Vec<(&'static str, &'static str, Option<&str>)> = issues
                                .iter()
                                .map(|i| (i.rule_id, i.severity.as_str(), i.detail.as_deref()))
                                .collect();
                            writer.issues(&record.url.to_string(), Some(page_id), &rows)?;
                        }
                        if let Some(redirect) = redirect {
                            writer.redirect(
                                &redirect.source,
                                Some(&record.url),
                                &redirect.hops,
                                "landed",
                            )?;
                        }
                        pages += 1;
                        Ok(())
                    }
                    Err(failure) => {
                        let Failure {
                            url,
                            reason,
                            redirect,
                        } = *failure;
                        if let Some(redirect) = redirect {
                            writer.redirect(&redirect.source, None, &redirect.hops, &reason)?;
                        } else {
                            writer.fail(&url, &reason)?;
                        }
                        failures += 1;
                        Ok(())
                    }
                }
            },
        )
        .await?;
    }
    writer.flush()?;
    // Built here rather than maintained during the crawl: the inlink index is a
    // TEXT index over randomly-ordered URLs, and paying for it per discovered
    // link is what made throughput collapse with scale. One sorted pass at the
    // end costs a fraction of 14M random inserts. A crawl killed before this
    // point simply leaves a file whose inlink queries scan until it is built.
    drop(writer);

    // The image pass runs after the crawl loop rather than alongside it. The
    // spec's requirement is that checking images cannot starve page fetching,
    // and running it afterwards satisfies that by construction instead of by a
    // scheduler that has to be trusted: page-crawl throughput is untouched
    // because no image request exists while a page request is in flight. The
    // cost is that the two do not overlap, which is wall time the user opted
    // into by asking for image checks.
    let resources = if image_urls.is_empty() {
        0
    } else {
        head_pass(&mut store, &fetcher, image_urls).await?
    };

    // Site rules run after the crawl loop but before the query indices: they
    // need the crawl-time indices to be fast, and their own output should be
    // indexed along with everything else.
    let site_issues = registry.run_site(&store)?;
    if !site_issues.is_empty() {
        let mut writer = Writer::new(&mut store);
        for (url, issue) in &site_issues {
            // `page_id` is a fast join when the subject is a page, and `None`
            // when it is not. A redirect loop's source is crawled but never
            // becomes a page, and dropping its finding here would let
            // `--fail-on` pass a site full of loops.
            let page_id = writer.page_id(url)?;
            writer.issues(
                url,
                page_id,
                &[(
                    issue.rule_id,
                    issue.severity.as_str(),
                    issue.detail.as_deref(),
                )],
            )?;
        }
        writer.flush()?;
    }

    store.build_query_indices()?;

    Ok(CrawlSummary {
        pages,
        failures,
        resources,
    })
}

/// `HEAD`s every distinct image URL and records what came back.
///
/// Reuses the same bounded pipeline the page crawl runs on, with its own
/// `fetch_concurrency`. A second scheduler written for this would be a second
/// place for backpressure to be got wrong.
async fn head_pass(
    store: &mut Store,
    fetcher: &Arc<Fetcher>,
    urls: HashSet<CrawlUrl>,
) -> Result<u64> {
    let mut writer = Writer::new(store);
    let mut checked = 0u64;
    let fetch = Arc::clone(fetcher);
    run_pipeline(
        urls.into_iter().collect::<Vec<_>>(),
        PipelineConfig {
            fetch_concurrency: IMAGE_CONCURRENCY,
            ..PipelineConfig::default()
        },
        move |url: CrawlUrl| {
            let fetch = Arc::clone(&fetch);
            async move { fetch.head(&url).await }
        },
        |result| result,
        |result| -> std::result::Result<(), pounce_store::StoreError> {
            // A URL that never answered is left out of `resources` entirely.
            // Absent is not the same as a status, and inventing a 0 would give
            // `media.broken-image` a finding the crawl never established.
            if let Ok(head) = result {
                writer.resource(
                    &head.url,
                    head.status.as_u16(),
                    head.content_length,
                    head.content_type.as_deref(),
                )?;
                checked += 1;
            }
            Ok(())
        },
    )
    .await?;
    writer.flush()?;
    Ok(checked)
}

struct Failure {
    url: CrawlUrl,
    reason: String,
    redirect: Option<Redirect>,
}

struct Parsed {
    record: PageRecord,
    redirect: Option<Redirect>,
}

struct Redirect {
    source: CrawlUrl,
    hops: Vec<RedirectHop>,
}

fn parse(
    (item, chain): (FrontierItem, RedirectChain),
) -> std::result::Result<Parsed, Box<Failure>> {
    let RedirectChain { hops, outcome, .. } = chain;
    let redirect_chain = hops.iter().map(|hop| hop.url.to_string()).collect();
    let redirect = (!hops.is_empty()).then(|| Redirect {
        source: item.url.clone(),
        hops: hops
            .into_iter()
            .map(|hop| RedirectHop {
                url: hop.url,
                status: hop.status.as_u16(),
                location: hop.location,
                target: hop.target,
            })
            .collect(),
    });
    match outcome {
        Outcome::Landed(fetched) => {
            let mut record = PageRecord::from_fetched(&fetched, item.depth, redirect_chain);
            match parse_body(&mut record, &fetched.body) {
                Ok(()) => Ok(Parsed { record, redirect }),
                Err(reason) => Err(Box::new(Failure {
                    url: item.url,
                    reason: format!("parse failed: {reason}"),
                    redirect,
                })),
            }
        }
        Outcome::Loop(url) => Err(Box::new(Failure {
            url: item.url,
            reason: format!("redirect loop at {url}"),
            redirect,
        })),
        Outcome::HopLimit => Err(Box::new(Failure {
            url: item.url,
            reason: "redirect hop limit reached".into(),
            redirect,
        })),
        Outcome::NoLocation => Err(Box::new(Failure {
            url: item.url,
            reason: "redirect has no usable location".into(),
            redirect,
        })),
        Outcome::Failed(error) => Err(Box::new(Failure {
            url: item.url,
            reason: error.to_string(),
            redirect,
        })),
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

        let summary = crawl(
            CrawlUrl::parse(&base_url).unwrap(),
            &output,
            &Registry::new(),
            false,
        )
        .await
        .unwrap();
        let overwrite = crawl(
            CrawlUrl::parse(&base_url).unwrap(),
            &output,
            &Registry::new(),
            false,
        )
        .await
        .unwrap_err();
        let redirect_output = dir.path().join("redirect.pounce");
        let redirect_seed = CrawlUrl::parse(&format!("{base_url}/redirect-chain/2")).unwrap();
        let redirect_summary = crawl(redirect_seed, &redirect_output, &Registry::new(), false)
            .await
            .unwrap();
        let loop_output = dir.path().join("loop.pounce");
        let loop_seed = CrawlUrl::parse(&format!("{base_url}/redirect-loop/3/0")).unwrap();
        let loop_summary = crawl(loop_seed, &loop_output, &Registry::new(), false)
            .await
            .unwrap();

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

        // A finished crawl leaves a file the UI can query. The index is built
        // at the end, so its presence is the marker that the crawl completed.
        let indexed: i64 = store
            .conn()
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='index' AND name='links_target'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(indexed, 1, "the inlink index must exist once a crawl ends");

        assert_eq!(redirect_summary.pages, 1);
        let mut redirect_store = Store::open(redirect_output).unwrap();
        assert!(CrawlState::new(&mut redirect_store).load().unwrap()[0].done);
        let (source_status, landing_status): (u16, u16) = redirect_store
            .conn()
            .query_row(
                "SELECT r.status, p.status FROM crawl_redirects r \
                 JOIN pages p ON p.url = r.final_url",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!((source_status, landing_status), (301, 200));

        assert_eq!(loop_summary.failures, 1);
        let loop_store = Store::open(loop_output).unwrap();
        let (status, outcome): (u16, String) = loop_store
            .conn()
            .query_row("SELECT status, outcome FROM crawl_redirects", [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .unwrap();
        assert_eq!(status, 302);
        assert!(outcome.contains("redirect loop"));
    }

    // ---- audit rules running during and after the crawl (T2.2) ----------

    use pounce_audit::{Issue, PageRule, RuleMeta, Severity, SiteRule};
    use pounce_store::StoreError;

    /// A page rule: fires on any page that asks not to be indexed.
    struct NoindexRule;
    impl PageRule for NoindexRule {
        fn meta(&self) -> RuleMeta {
            RuleMeta {
                id: "indexability.noindex",
                severity: Severity::Critical,
                description: "The page asks search engines not to index it.",
                remediation: "Remove the noindex directive if the page should rank.",
            }
        }
        fn check(&self, page: &PageRecord, out: &mut Vec<Issue>) {
            if page.meta_robots.noindex {
                out.push(Issue {
                    rule_id: self.meta().id,
                    severity: self.meta().severity,
                    detail: None,
                });
            }
        }
    }

    /// A site rule: needs every page before it can say anything.
    struct DeepestPages;
    impl SiteRule for DeepestPages {
        fn meta(&self) -> RuleMeta {
            RuleMeta {
                id: "structure.deep",
                severity: Severity::Notice,
                description: "The page sits at the deepest level of the crawl.",
                remediation: "Shorten the click path to important pages.",
            }
        }
        fn check(&self, store: &Store) -> Result<Vec<(String, Issue)>, StoreError> {
            let mut stmt = store
                .conn()
                .prepare("SELECT url FROM pages WHERE depth = (SELECT max(depth) FROM pages)")?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
            let mut out = Vec::new();
            for url in rows {
                out.push((
                    url?,
                    Issue {
                        rule_id: self.meta().id,
                        severity: self.meta().severity,
                        detail: None,
                    },
                ));
            }
            Ok(out)
        }
    }

    async fn spawn_fixture(pages: u32) -> (String, tokio::task::JoinHandle<()>) {
        let graph = SiteGraph::generate(&GraphSpec {
            page_count: pages,
            seed: 42,
            ..GraphSpec::default()
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let fixture = Arc::new(Fixture {
            graph,
            base_url: base_url.clone(),
        });
        let server = tokio::spawn(async move {
            let _ = serve(listener, fixture).await;
        });
        (base_url, server)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_crawl_records_the_issues_its_page_rules_find() {
        let (base_url, server) = spawn_fixture(128).await;
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("audited.pounce");

        let mut registry = Registry::new();
        registry.register_page(Box::new(NoindexRule)).unwrap();
        crawl(
            CrawlUrl::parse(&base_url).unwrap(),
            &output,
            &registry,
            false,
        )
        .await
        .unwrap();

        let store = Store::open(&output).unwrap();
        let found: i64 = store
            .conn()
            .query_row(
                "SELECT count(*) FROM issues WHERE rule_id = 'indexability.noindex'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let noindex_pages: i64 = store
            .conn()
            .query_row("SELECT count(*) FROM pages WHERE noindex = 1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert!(noindex_pages > 0, "the fixture seeds noindex pages");
        assert_eq!(found, noindex_pages, "one issue per noindex page, no more");

        // The issue must point at the page it was found on, not at any page.
        let mismatched: i64 = store
            .conn()
            .query_row(
                "SELECT count(*) FROM issues i JOIN pages p ON p.id = i.page_id \
                 WHERE i.rule_id = 'indexability.noindex' AND p.noindex = 0",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(mismatched, 0, "issues are attached to the wrong pages");

        server.abort();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_crawl_records_the_issues_its_site_rules_find() {
        let (base_url, server) = spawn_fixture(128).await;
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("site.pounce");

        let mut registry = Registry::new();
        registry.register_site(Box::new(DeepestPages)).unwrap();
        crawl(
            CrawlUrl::parse(&base_url).unwrap(),
            &output,
            &registry,
            false,
        )
        .await
        .unwrap();

        let store = Store::open(&output).unwrap();
        let found: i64 = store
            .conn()
            .query_row(
                "SELECT count(*) FROM issues WHERE rule_id = 'structure.deep'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let deepest: i64 = store
            .conn()
            .query_row(
                "SELECT count(*) FROM pages WHERE depth = (SELECT max(depth) FROM pages)",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(deepest > 0);
        assert_eq!(found, deepest, "a site rule sees the whole crawl");

        server.abort();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_empty_registry_records_no_issues_and_still_crawls() {
        let (base_url, server) = spawn_fixture(64).await;
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("plain.pounce");

        let summary = crawl(
            CrawlUrl::parse(&base_url).unwrap(),
            &output,
            &Registry::new(),
            false,
        )
        .await
        .unwrap();
        assert!(summary.pages > 0);

        let store = Store::open(&output).unwrap();
        let issues: i64 = store
            .conn()
            .query_row("SELECT count(*) FROM issues", [], |r| r.get(0))
            .unwrap();
        assert_eq!(issues, 0, "rules off must cost nothing and find nothing");

        server.abort();
    }

    // ---- the image HEAD pass --------------------------------------------

    async fn fixture_site(pages: u32) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let fixture = Arc::new(Fixture {
            graph: SiteGraph::generate(&GraphSpec {
                page_count: pages,
                seed: 42,
                ..GraphSpec::default()
            }),
            base_url: base_url.clone(),
        });
        let handle = tokio::spawn(async move {
            let _ = serve(listener, fixture).await;
        });
        (base_url, handle)
    }

    fn resources(path: &Path) -> Vec<(String, i64, Option<i64>, Option<String>)> {
        let store = Store::open(path).unwrap();
        store
            .conn()
            .prepare("SELECT url, status, content_length, content_type FROM resources ORDER BY url")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn without_the_flag_no_image_is_requested() {
        // The default has to stay a page crawl. Every benchmark this project
        // publishes measures one, and a default that quietly multiplied
        // request count would change what those numbers mean without changing
        // the command that produces them.
        let (base_url, server) = fixture_site(40).await;
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("no-images.pounce");
        let summary = crawl(
            CrawlUrl::parse(&base_url).unwrap(),
            &output,
            &Registry::new(),
            false,
        )
        .await
        .unwrap();

        assert!(summary.pages > 0);
        assert_eq!(summary.resources, 0);
        assert!(resources(&output).is_empty());
        server.abort();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_pass_records_what_each_image_server_declared() {
        let (base_url, server) = fixture_site(40).await;
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("images.pounce");
        let summary = crawl(
            CrawlUrl::parse(&base_url).unwrap(),
            &output,
            &Registry::new(),
            true,
        )
        .await
        .unwrap();

        let rows = resources(&output);
        assert_eq!(rows.len() as u64, summary.resources);
        // Every page in the fixture draws from /static/img-0..4.jpg, so five
        // rows is deduplication working. Without it this would be one row per
        // <img> across forty pages.
        assert!(!rows.is_empty() && rows.len() <= 5, "got {rows:?}");

        let by_name = |name: &str| {
            rows.iter()
                .find(|(url, ..)| url.ends_with(name))
                .unwrap_or_else(|| panic!("{name} missing from {rows:?}"))
                .clone()
        };
        // img-3 is the fixture's deliberately missing image, img-4 its
        // deliberately oversized one. Both are chosen by index so the expected
        // findings are worked out from the fixture, not read off the crawl.
        assert_eq!(by_name("/static/img-3.jpg").1, 404);
        assert_eq!(by_name("/static/img-4.jpg").2, Some(300 * 1024));
        assert_eq!(by_name("/static/img-0.jpg").2, Some(20 * 1024));
        assert_eq!(
            by_name("/static/img-0.jpg").3.as_deref(),
            Some("image/jpeg")
        );
        server.abort();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_missing_image_is_recorded_rather_than_dropped() {
        // The whole point of the pass. A 404 image that simply did not appear
        // in `resources` would leave `media.broken-image` with nothing to find
        // and the rule would look like it was passing.
        let (base_url, server) = fixture_site(40).await;
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("broken.pounce");
        crawl(
            CrawlUrl::parse(&base_url).unwrap(),
            &output,
            &Registry::new(),
            true,
        )
        .await
        .unwrap();
        assert_eq!(
            resources(&output)
                .iter()
                .filter(|(_, status, ..)| *status >= 400)
                .count(),
            1
        );
        server.abort();
    }
}
