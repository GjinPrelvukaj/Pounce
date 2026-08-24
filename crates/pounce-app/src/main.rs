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
    let seed = CrawlUrl::parse(&settings.seed).map_err(|e| ApiError::Crawl {
        message: e.to_string(),
    })?;
    let (limits, fetch) = query_api::to_engine(&settings)?;
    let output = settings.output.clone();
    let lifecycle = Arc::new(CrawlLifecycle::new(limits));
    *state.running.lock().unwrap() = Some(Arc::clone(&lifecycle));

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

/// Opens a `.pounce` file, replacing whatever was open.
///
/// The page count comes back with the handle because the grid needs a
/// scrollbar before it needs rows, and a second round trip to learn the height
/// of the list is a visible stutter on open.
#[tauri::command]
fn open_crawl(path: String, state: State<'_, AppState>) -> Result<CrawlHandle, ApiError> {
    let store = Store::open(&path)?;
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
    let mut registry = Registry::new();
    pounce_audit::register_all(&mut registry).expect("the rule registry must build");

    // `pounce-app <file.pounce>` opens that crawl at startup. T4.8 adds the
    // file dialog and the recents list; this is the same door, and it is what
    // "Open With" and a double-clicked `.pounce` will eventually come through.
    let opened = std::env::args().nth(1).and_then(|path| {
        match Store::open(&path) {
            Ok(store) => Some(OpenCrawl {
                path: PathBuf::from(path),
                store,
            }),
            Err(e) => {
                // A bad path on the command line is worth saying out loud, but
                // it is not worth refusing to start over: the window can open
                // a different file.
                eprintln!("could not open {path}: {e}");
                None
            }
        }
    });

    tauri::Builder::default()
        .manage(AppState {
            open: Mutex::new(opened),
            registry,
            running: Mutex::new(None),
        })
        .setup(|_app| {
            // Launching from a shell can leave the window unfocused and, on a
            // machine with several Spaces, on one the user is not looking at.
            // An app that opens a window the user cannot see has not opened.
            {
                use tauri::Manager;
                if let Some(window) = _app.get_webview_window("main") {
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
            open_crawl,
            close_crawl,
            current_crawl,
            query_rows,
            issue_overview,
            supported_sorts,
            start_crawl
        ])
        .run(tauri::generate_context!())
        .expect("the Tauri runtime failed to start");
}

#[cfg(test)]
mod tests {
    use super::*;

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
