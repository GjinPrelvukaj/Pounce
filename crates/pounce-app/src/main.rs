//! The desktop shell.
//!
//! Deliberately thin. The GUI is one consumer of the engine, never its owner —
//! the CLI and the app are peers over the same crates — so anything that could
//! live in `pounce-core`, `pounce-store` or `pounce-audit` does, and what is
//! left here is the Tauri boundary itself.
//!
//! The load-bearing rule this crate exists to respect: **the UI never receives
//! the crawl dataset.** Commands return windows and counts. A command that
//! returned every row would work on the developer's 500-page test crawl and
//! collapse on a user's 500,000-page one, which the spec names as the single
//! most likely way this project fails.

// A release build must not also open a console window on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod query_api;

use pounce_audit::Registry;
use pounce_core::{CrawlLifecycle, CrawlProgress, CrawlUrl};
use pounce_store::{SCHEMA_VERSION, Store};
use query_api::{ApiError, FilterDto, SortColumnDto, SortDirectionDto};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::State;

/// What the shell holds between commands: one open crawl, or none.
///
/// A `Mutex` rather than a connection pool because SQLite's connection is not
/// `Sync` and the grid issues one query at a time — the window can only show
/// one scroll position. If the detail pane ever needs to read while the grid
/// reads, that is a second connection, not a second lock.
#[derive(Default)]
struct AppState {
    open: Mutex<Option<OpenCrawl>>,
    registry: Registry,
    /// The crawl in flight, if any. Held so T4.7's pause, resume and cancel
    /// have something to talk to — the same handle progress is read from.
    running: Mutex<Option<Arc<CrawlLifecycle>>>,
    /// Why the file on the command line did not open, if there was one and it
    /// did not. The window is built after `main` has already tried, so without
    /// this the user who double-clicked the wrong file gets the welcome screen
    /// and a line on a stderr they will never see.
    startup_error: Option<ApiError>,
}

struct OpenCrawl {
    path: PathBuf,
    store: Store,
}

/// What the UI learns when a file opens.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct CrawlHandle {
    path: String,
    pages: u64,
    schema_version: u32,
}

/// What the shell can say about the engine before any crawl exists.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct EngineInfo {
    version: &'static str,
    schema_version: u32,
    /// Rules the registry actually has, not a number written in a doc.
    rules: usize,
}

/// One rule, as the interface needs to talk about it.
///
/// The store keeps rule *ids* on every issue row — a `&'static str` costs
/// nothing per row and cannot go stale — so the sentences live in the registry
/// and are fetched once, here. The alternative, denormalising a description
/// onto four million issue rows, would be the same prose written four million
/// times and wrong the moment a rule's wording improved.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct RuleInfo {
    id: &'static str,
    /// What was found, in the user's words.
    description: &'static str,
    /// What to do about it. A finding without a fix is noise.
    remediation: &'static str,
    severity: &'static str,
}

impl From<pounce_audit::RuleMeta> for RuleInfo {
    fn from(m: pounce_audit::RuleMeta) -> Self {
        Self {
            id: m.id,
            description: m.description,
            remediation: m.remediation,
            severity: m.severity.as_str(),
        }
    }
}

/// Every rule this build has, so the interface can render a finding as a
/// sentence rather than as the database key it is filtered by.
#[tauri::command]
fn rules(state: State<'_, AppState>) -> Vec<RuleInfo> {
    state
        .registry
        .page_rules()
        .iter()
        .map(|r| r.meta().into())
        .chain(state.registry.site_rules().iter().map(|r| r.meta().into()))
        .collect()
}

/// Proves the bridge: the frontend asks, the engine answers.
///
/// Kept even once real commands exist — it is the one call that fails loudly
/// when the two halves of the app are built from different versions.
#[tauri::command]
fn engine_info() -> Result<EngineInfo, String> {
    let mut registry = Registry::new();
    pounce_audit::register_all(&mut registry).map_err(|e| e.to_string())?;
    Ok(EngineInfo {
        version: env!("CARGO_PKG_VERSION"),
        schema_version: SCHEMA_VERSION,
        rules: registry.page_rules().len() + registry.site_rules().len(),
    })
}

/// One progress tick, as the UI reads it.
///
/// `urlsPerSecond` is computed here rather than in the UI because the engine
/// owns what "elapsed" means — a paused crawl's clock stops, and a rate
/// divided by wall time would quietly lie about every paused crawl.
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ProgressEvent {
    status: &'static str,
    admitted: u64,
    written: u64,
    elapsed_ms: u64,
    urls_per_second: f64,
    queued: u64,
    /// `[1xx, 2xx, 3xx, 4xx, 5xx]`.
    by_class: [u64; 5],
    failed: u64,
}

impl From<CrawlProgress> for ProgressEvent {
    fn from(p: CrawlProgress) -> Self {
        Self {
            status: match p.status {
                pounce_core::CrawlStatus::Running => "running",
                pounce_core::CrawlStatus::Paused => "paused",
                pounce_core::CrawlStatus::Completed => "completed",
                pounce_core::CrawlStatus::Cancelled => "cancelled",
                pounce_core::CrawlStatus::CountLimitReached => "countLimitReached",
                pounce_core::CrawlStatus::TimeLimitReached => "timeLimitReached",
                pounce_core::CrawlStatus::Failed => "failed",
            },
            admitted: p.admitted,
            written: p.written,
            elapsed_ms: p.elapsed.as_millis() as u64,
            urls_per_second: p.urls_per_second(),
            queued: p.queued,
            by_class: p.by_class,
            failed: p.failed,
        }
    }
}

/// Starts a crawl and streams progress into `on_progress` until it ends.
///
/// The 10 Hz throttle is not implemented here — `CrawlLifecycle::report_progress`
/// ticks at `PROGRESS_INTERVAL` and this forwards each tick. That matters: the
/// invariant is that the engine never emits an event per URL, and putting the
/// throttle in the shell would leave the CLI and any future consumer free to
/// ignore it.
#[tauri::command]
async fn start_crawl(
    settings: query_api::CrawlSettings,
    on_progress: tauri::ipc::Channel<ProgressEvent>,
    state: State<'_, AppState>,
) -> Result<CrawlHandle, ApiError> {
    let seed = CrawlUrl::parse(&settings.seed).map_err(|e| ApiError::BadSeed {
        input: settings.seed.clone(),
        message: e.to_string(),
        suggestion: query_api::seed_suggestion(&settings.seed),
    })?;
    // Checked here rather than left to `crawl_with`, which reports it as a
    // string. The refusal is right; what was missing is the way out of it.
    if Path::new(&settings.output).exists() {
        return Err(ApiError::OutputExists {
            path: settings.output.clone(),
            suggestion: query_api::free_name(Path::new(&settings.output)),
        });
    }
    let (limits, fetch) = query_api::to_engine(&settings)?;
    let output = settings.output.clone();
    let lifecycle = Arc::new(CrawlLifecycle::new(limits));
    {
        // Claimed under one lock: checking and then setting in two steps is
        // the race this exists to prevent.
        let mut running = state.running.lock().unwrap();
        query_api::may_start(running.as_ref().map(|l| l.status()))?;
        *running = Some(Arc::clone(&lifecycle));
    }

    let reporter = {
        let lifecycle = Arc::clone(&lifecycle);
        tauri::async_runtime::spawn(async move {
            lifecycle
                .report_progress(|p| {
                    // A closed channel means the window went away mid-crawl.
                    // The crawl keeps going — it is writing to disk, and a
                    // half-written file is worse than an unwatched one.
                    let _ = on_progress.send(ProgressEvent::from(p));
                })
                .await;
        })
    };

    // Its own registry rather than the one in state: a `MutexGuard` cannot be
    // held across an await, and 30 rules cost less to rebuild than the
    // lifetime gymnastics of borrowing them would.
    let mut registry = Registry::new();
    pounce_audit::register_all(&mut registry).map_err(|e| ApiError::Crawl {
        message: e.to_string(),
    })?;

    // Whatever happens below, the lifecycle must end terminal before the
    // reporter is awaited — see `CrawlLifecycle::fail`.
    let result = pounce_run::crawl_with(
        seed,
        Path::new(&output),
        &registry,
        pounce_run::CrawlOptions {
            check_images: settings.images,
            fetch,
        },
        Arc::clone(&lifecycle),
    )
    .await;
    if result.is_err() {
        lifecycle.fail();
    }
    let _ = reporter.await;
    *state.running.lock().unwrap() = None;
    result.map_err(|e| ApiError::Crawl {
        message: e.to_string(),
    })?;

    // The finished file becomes the open crawl, so the grid has something to
    // query without a second trip through the file dialog.
    let store = Store::open(&output)?;
    let pages: i64 = store
        .conn()
        .query_row("SELECT count(*) FROM pages", [], |r| r.get(0))
        .map_err(pounce_store::StoreError::from)?;
    let handle = CrawlHandle {
        path: output.clone(),
        pages: pages as u64,
        schema_version: store.version()?,
    };
    *state.open.lock().unwrap() = Some(OpenCrawl {
        path: PathBuf::from(output),
        store,
    });
    Ok(handle)
}

/// Pause, resume and cancel, applied to the crawl in flight.
///
/// Each returns whether it changed anything, so the UI can tell "paused" from
/// "there was nothing to pause" without a second call. A crawl that has
/// already ended is not an error to cancel — the button may still be on
/// screen when the last batch lands.
#[tauri::command]
fn pause_crawl(state: State<'_, AppState>) -> bool {
    state
        .running
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|l| l.pause())
}

#[tauri::command]
fn resume_crawl(state: State<'_, AppState>) -> bool {
    state
        .running
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|l| l.resume())
}

/// Ends the crawl, keeping what it has already written.
///
/// Cancelling is not discarding: the file is a real crawl of however much was
/// reached, and the alternative — deleting it — would make cancel the most
/// expensive button in the app.
#[tauri::command]
fn cancel_crawl(state: State<'_, AppState>) -> bool {
    state
        .running
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|l| l.cancel())
}

/// Opens a `.pounce` file, replacing whatever was open.
///
/// The page count comes back with the handle because the grid needs a
/// scrollbar before it needs rows, and a second round trip to learn the height
/// of the list is a visible stutter on open.
///
/// **A file being written right now is opened read-only.** That is what lets
/// the grid fill during a crawl instead of after it — WAL gives one writer and
/// many readers — and the read-only flag is not politeness but the guarantee:
/// `Store::open` migrates, and a second connection racing the writer's
/// `PRAGMA user_version` is the two-writers hazard `may_start` refuses at the
/// front door, arrived at through the back.
#[tauri::command]
fn open_crawl(path: String, state: State<'_, AppState>) -> Result<CrawlHandle, ApiError> {
    let live = state
        .running
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|l| !l.status().is_terminal());
    // Checked before opening, because `Store::open` creates what is missing.
    // That is correct for a new crawl and wrong for an old one: a recent whose
    // file has been deleted would come back as an empty database rather than as
    // a file that is gone.
    if !Path::new(&path).exists() {
        return Err(ApiError::Missing { path });
    }
    let opened = if live {
        Store::open_read_only(&path)
    } else {
        Store::open(&path)
    };
    // Named at the boundary rather than passed through as a store error: "this
    // file is a database, but not a Pounce crawl" is a sentence about the file
    // the user just chose, and the interface can say which one.
    let store = match opened {
        Err(pounce_store::StoreError::NotACrawl) => {
            return Err(ApiError::NotACrawl { path: path.clone() });
        }
        other => other?,
    };
    let pages: i64 = store
        .conn()
        .query_row("SELECT count(*) FROM pages", [], |r| r.get(0))
        .map_err(pounce_store::StoreError::from)?;
    let handle = CrawlHandle {
        path: path.clone(),
        pages: pages as u64,
        schema_version: store.version()?,
    };
    *state.open.lock().unwrap() = Some(OpenCrawl {
        path: PathBuf::from(path),
        store,
    });
    Ok(handle)
}

/// Why the file handed to the process on the command line did not open.
///
/// `None` in the ordinary case, including when there was no file at all.
#[tauri::command]
fn startup_error(state: State<'_, AppState>) -> Option<ApiError> {
    state.startup_error.clone()
}

#[tauri::command]
fn close_crawl(state: State<'_, AppState>) {
    *state.open.lock().unwrap() = None;
}

#[tauri::command]
fn current_crawl(state: State<'_, AppState>) -> Option<String> {
    state
        .open
        .lock()
        .unwrap()
        .as_ref()
        .map(|c| c.path.display().to_string())
}

#[tauri::command]
fn query_rows(
    filters: Vec<FilterDto>,
    sort: SortColumnDto,
    direction: SortDirectionDto,
    offset: u64,
    limit: u32,
    state: State<'_, AppState>,
) -> Result<pounce_store::Page, ApiError> {
    let open = state.open.lock().unwrap();
    let crawl = open.as_ref().ok_or(ApiError::NoCrawlOpen)?;
    query_api::rows(
        &crawl.store,
        &state.registry,
        &filters,
        sort,
        direction,
        offset,
        limit,
    )
}

/// Where a crawl of `seed` would be saved unless the user says otherwise.
///
/// Proposed rather than required: the new-crawl screen shows it under the
/// address and offers to change it, so starting a crawl is one field and a
/// button. Falls back to the home directory, and then to a bare filename, so a
/// machine with no Documents folder still gets an answer.
#[tauri::command]
fn suggest_output(app: tauri::AppHandle, seed: String) -> String {
    use tauri::Manager;
    let name = query_api::output_name(&seed);
    let dir = app
        .path()
        .document_dir()
        .or_else(|_| app.path().home_dir())
        .ok();
    match dir {
        // Counted past whatever is already there, so the suggestion is a name
        // the engine will actually accept.
        Some(dir) => {
            let candidate = dir.join(&name);
            if candidate.exists() {
                query_api::free_name(&candidate)
            } else {
                candidate.display().to_string()
            }
        }
        None => name,
    }
}

/// Everything about one page.
///
/// Takes the row id rather than the URL: the grid already has it, ids are
/// stable across a re-fetch (pages are upserted on `url`, never replaced), and
/// a URL round-tripping through JSON is a canonicalisation waiting to differ.
#[tauri::command]
fn page_detail(
    id: i64,
    state: State<'_, AppState>,
) -> Result<Option<pounce_store::PageDetail>, ApiError> {
    let open = state.open.lock().unwrap();
    let crawl = open.as_ref().ok_or(ApiError::NoCrawlOpen)?;
    Ok(crawl.store.page_detail(id)?)
}

/// Writes the current view to `path`, and answers with how many rows it wrote.
///
/// Synchronous on purpose. It is a single statement streamed to a file, and the
/// only thing an async version would add is a second way for the store's
/// `Mutex` to be held across an await.
#[tauri::command]
fn export_rows(
    path: String,
    filters: Vec<FilterDto>,
    sort: SortColumnDto,
    direction: SortDirectionDto,
    state: State<'_, AppState>,
) -> Result<u64, ApiError> {
    let open = state.open.lock().unwrap();
    let crawl = open.as_ref().ok_or(ApiError::NoCrawlOpen)?;
    query_api::write_export(
        &crawl.store,
        &state.registry,
        &filters,
        sort,
        direction,
        Path::new(&path),
    )
}

/// What the crawl contains, for the overview panel.
#[tauri::command]
fn crawl_overview(state: State<'_, AppState>) -> Result<pounce_store::CrawlOverview, ApiError> {
    let open = state.open.lock().unwrap();
    let crawl = open.as_ref().ok_or(ApiError::NoCrawlOpen)?;
    Ok(crawl.store.crawl_overview()?)
}

#[tauri::command]
fn issue_overview(state: State<'_, AppState>) -> Result<pounce_store::IssueOverview, ApiError> {
    let open = state.open.lock().unwrap();
    let crawl = open.as_ref().ok_or(ApiError::NoCrawlOpen)?;
    query_api::overview(&crawl.store)
}

/// The sorts the UI may offer for the filters currently applied.
#[tauri::command]
fn supported_sorts(
    filters: Vec<FilterDto>,
    state: State<'_, AppState>,
) -> Result<Vec<SortColumnDto>, ApiError> {
    query_api::supported_sorts(&state.registry, &filters)
}

fn main() {
    // Cold start, measured rather than asserted. `POUNCE_STARTUP=1` prints the
    // time from process entry to the webview finishing its first load, which is
    // the number Gate M4 puts a 400 ms ceiling on. Env-gated and one `Instant`:
    // a permanent measurement costs less than re-instrumenting the binary every
    // time someone wants to check.
    let started = std::time::Instant::now();
    let mut registry = Registry::new();
    pounce_audit::register_all(&mut registry).expect("the rule registry must build");

    // `pounce-app <file.pounce>` opens that crawl at startup. T4.8 adds the
    // file dialog and the recents list; this is the same door, and it is what
    // "Open With" and a double-clicked `.pounce` will eventually come through.
    // Named apart from the command of the same job: `generate_handler!` expands
    // in this scope, and a local binding would shadow the function it names.
    let mut startup_failure = None;
    let opened = std::env::args().nth(1).and_then(|path| {
        // The same check the command does, for the same reason: `Store::open`
        // creates what is missing, and "Open With" on a file that has been
        // moved must not silently produce an empty crawl.
        if !Path::new(&path).exists() {
            eprintln!("no such file: {path}");
            startup_failure = Some(ApiError::Missing { path });
            return None;
        }
        match Store::open(&path) {
            Ok(store) => Some(OpenCrawl {
                path: PathBuf::from(path),
                store,
            }),
            Err(e) => {
                // A bad path on the command line is worth saying out loud, but
                // it is not worth refusing to start over: the window can open a
                // different file. It is kept for the window to say, because
                // "Open With" is a door people arrive through and stderr is not
                // somewhere they look.
                eprintln!("could not open {path}: {e}");
                startup_failure = Some(match e {
                    pounce_store::StoreError::NotACrawl => ApiError::NotACrawl { path },
                    other => ApiError::Store {
                        message: other.to_string(),
                    },
                });
                None
            }
        }
    });

    tauri::Builder::default()
        .on_page_load(move |_webview, payload| {
            if std::env::var_os("POUNCE_STARTUP").is_some()
                && payload.event() == tauri::webview::PageLoadEvent::Finished
            {
                println!("startup: {} ms", started.elapsed().as_millis());
            }
        })
        // The file dialogs. Only open and save are granted — see
        // `capabilities/default.json`; a window that can ask for anything is a
        // window that can be talked into asking for anything.
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            open: Mutex::new(opened),
            registry,
            running: Mutex::new(None),
            startup_error: startup_failure,
        })
        .setup(|_app| {
            // Launching from a shell can leave the window unfocused and, on a
            // machine with several Spaces, on one the user is not looking at.
            // An app that opens a window the user cannot see has not opened.
            {
                use tauri::Manager;
                if let Some(window) = _app.get_webview_window("main") {
                    // Maximised, not fullscreen. This is a table application:
                    // the default 1200x800 window shows a third of the columns
                    // and half the rows, and every crawl starts with the user
                    // dragging a corner. Screaming Frog opens expanded for the
                    // same reason. Fullscreen would hide the menu bar and take
                    // over a Space, which is a different and unwanted thing.
                    let _ = window.maximize();
                    let _ = window.set_focus();
                }
            }
            // `POUNCE_DEVTOOLS=1` opens the inspector on launch. Debug builds
            // could open it unconditionally, but it takes half the window and
            // most runs do not want it. A blank window with no inspector gives
            // no way to tell a CSP refusal from a failed navigation, which is
            // the first thing this scaffold hit.
            #[cfg(any(debug_assertions, feature = "devtools"))]
            if std::env::var_os("POUNCE_DEVTOOLS").is_some() {
                use tauri::Manager;
                if let Some(window) = _app.get_webview_window("main") {
                    window.open_devtools();
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            engine_info,
            rules,
            open_crawl,
            close_crawl,
            current_crawl,
            startup_error,
            query_rows,
            issue_overview,
            crawl_overview,
            page_detail,
            export_rows,
            suggest_output,
            supported_sorts,
            start_crawl,
            pause_crawl,
            resume_crawl,
            cancel_crawl
        ])
        .run(tauri::generate_context!())
        .expect("the Tauri runtime failed to start");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full_registry() -> Registry {
        let mut registry = Registry::new();
        pounce_audit::register_all(&mut registry).unwrap();
        registry
    }

    #[test]
    fn a_taken_name_suggests_a_free_one_beside_it() {
        let dir = std::env::temp_dir().join("pounce-free-name");
        std::fs::create_dir_all(&dir).unwrap();
        let taken = dir.join("site.pounce");
        std::fs::write(&taken, b"").unwrap();
        let suggestion = query_api::free_name(&taken);
        assert!(suggestion.ends_with("site-2.pounce"), "{suggestion}");
        assert!(!Path::new(&suggestion).exists());

        // And it keeps counting past the ones already taken, rather than
        // suggesting a name the user will be refused for a second time.
        std::fs::write(dir.join("site-2.pounce"), b"").unwrap();
        assert!(query_api::free_name(&taken).ends_with("site-3.pounce"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_seed_missing_its_scheme_gets_one_offered() {
        assert_eq!(
            query_api::seed_suggestion("example.com"),
            Some("https://example.com".into())
        );
        // Already has a scheme, so the failure is something else and there is
        // nothing honest to suggest.
        assert_eq!(query_api::seed_suggestion("ftp://example.com"), None);
        assert_eq!(query_api::seed_suggestion("   "), None);
    }

    #[test]
    fn every_rule_carries_a_sentence_and_a_fix() {
        // The interface renders these directly. A rule that shipped with an
        // empty description would show a blank row where a finding belongs —
        // and an id in a tooltip is not a finding.
        let registry = full_registry();
        let rules: Vec<RuleInfo> = registry
            .page_rules()
            .iter()
            .map(|r| r.meta().into())
            .chain(registry.site_rules().iter().map(|r| r.meta().into()))
            .collect();
        assert_eq!(rules.len(), registry.len());
        for rule in &rules {
            assert!(
                !rule.description.is_empty(),
                "{} has no description",
                rule.id
            );
            assert!(
                !rule.remediation.is_empty(),
                "{} has no remediation",
                rule.id
            );
            assert!(
                rule.description.ends_with('.'),
                "{} reads as a fragment, not a sentence: {:?}",
                rule.id,
                rule.description
            );
        }
    }

    #[test]
    fn engine_info_reports_the_versions_the_build_actually_has() {
        // The point of the command is that a mismatched frontend and engine
        // say so out loud. A hard-coded number here would defeat that, so both
        // sides are read from the code that owns them.
        let info = engine_info().unwrap();
        assert_eq!(info.schema_version, pounce_store::SCHEMA_VERSION);
        assert!(info.rules >= 30, "v0.1 caps at ~30 rules: {}", info.rules);
        assert!(!info.version.is_empty());
    }
}
