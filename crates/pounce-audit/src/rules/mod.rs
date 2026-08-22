//! The rules themselves, in the themed batches `PLAN.md` defines.
//!
//! Before adding a rule, read the grading standard on
//! [`crate::issue::Severity`] — the shipped grades are held to it by
//! `tests/severity_review.rs`, which will reject a grade chosen without it.

pub mod content;
pub mod descriptions;
pub mod indexability;
pub mod media_links;
pub mod response;
pub mod titles;

use crate::registry::{Registry, RegistryError};

/// Every rule shipped in this build.
///
/// One place, so the 30-rule cap is enforced against reality rather than
/// against whatever a caller happened to register.
pub fn register_all(registry: &mut Registry) -> Result<(), RegistryError> {
    response::register(registry)?;
    titles::register(registry)?;
    descriptions::register(registry)?;
    content::register(registry)?;
    indexability::register(registry)?;
    media_links::register(registry)
}
