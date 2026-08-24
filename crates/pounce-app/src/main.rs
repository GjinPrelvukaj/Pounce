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

use pounce_audit::Registry;
use pounce_store::SCHEMA_VERSION;

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

fn main() {
    tauri::Builder::default()
        .setup(|_app| {
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
        .invoke_handler(tauri::generate_handler![engine_info])
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
