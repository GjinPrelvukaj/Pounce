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
use pounce_store::{SCHEMA_VERSION, Store};
use query_api::{ApiError, FilterDto, SortColumnDto, SortDirectionDto};
use std::path::PathBuf;
use std::sync::Mutex;
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
            supported_sorts
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
