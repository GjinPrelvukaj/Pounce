//! Crawl orchestration for Pounce: URL handling, the frontier, and the
//! pipeline that drives fetching, parsing, and storage.
//!
//! Deliberately free of any dependency on the Tauri shell, so the engine is
//! testable without standing up a window.

pub mod frontier;
pub mod lifecycle;
pub mod pipeline;
pub mod scope;
pub mod url;

pub use frontier::{Frontier, FrontierItem, PushResult};
pub use lifecycle::{CrawlLifecycle, CrawlLimits, CrawlProgress, CrawlStatus, PROGRESS_INTERVAL};
pub use pipeline::{
    PipelineConfig, PipelineError, PipelineStats, run_controlled_pipeline, run_pipeline,
};
pub use scope::{Locality, Scope, SubdomainPolicy};
pub use url::{CrawlUrl, UrlError};
