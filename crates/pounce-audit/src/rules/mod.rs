//! The rules themselves, in the themed batches `PLAN.md` defines.

pub mod content;
pub mod descriptions;
pub mod indexability;
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
    indexability::register(registry)
}
