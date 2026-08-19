//! Crawl orchestration for Pounce: URL handling, the frontier, and the
//! pipeline that drives fetching, parsing, and storage.
//!
//! Deliberately free of any dependency on the Tauri shell, so the engine is
//! testable without standing up a window.

pub mod url;

pub use url::{CrawlUrl, UrlError};
