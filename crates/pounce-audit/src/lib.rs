//! Audit rules: what to check, and what was found.

pub mod issue;
pub mod registry;
pub mod rule;

pub use issue::{Issue, RuleMeta, Severity};
pub use registry::{MAX_RULES, Registry, RegistryError};
pub use rule::{PageRule, SiteRule};
