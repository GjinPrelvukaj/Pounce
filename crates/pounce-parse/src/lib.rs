//! Turning a fetched response into a `PageRecord`.

pub mod extract;
pub mod record;

pub use extract::extract;
pub use record::{Hreflang, Image, Link, MetaRobots, PageRecord};
