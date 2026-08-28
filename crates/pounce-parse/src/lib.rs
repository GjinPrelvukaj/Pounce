//! Turning a fetched response into a `PageRecord`.

pub mod body;
pub mod extract;
pub mod hash;
pub mod record;
pub mod sitemap;

pub use body::BodyKind;
pub use extract::{extract, parse_body};
pub use record::{Hreflang, Image, Link, MetaRobots, PageRecord};
pub use sitemap::{MAX_LOCATIONS, Sitemap};
