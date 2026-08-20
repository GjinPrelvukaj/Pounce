//! Turning a fetched response into a `PageRecord`.

pub mod body;
pub mod extract;
pub mod record;

pub use body::BodyKind;
pub use extract::{extract, parse_body};
pub use record::{Hreflang, Image, Link, MetaRobots, PageRecord};
