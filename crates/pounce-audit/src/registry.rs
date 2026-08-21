//! Every rule, and the invariants that are about the set rather than any one.

use crate::issue::Issue;
use crate::rule::PageRule;
use pounce_parse::PageRecord;
use std::collections::HashSet;

/// v0.1's hard cap. Feature parity is explicitly not the goal; racing a list we
/// are behind on is the identified primary failure mode for this project.
pub const MAX_RULES: usize = 30;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RegistryError {
    #[error("rule id `{0}` is already registered")]
    DuplicateId(&'static str),
    #[error("rule id `{0}` must look like `batch.rule-name`")]
    MalformedId(&'static str),
    #[error("v0.1 caps at 30 rules")]
    CapExceeded,
}

#[derive(Default)]
pub struct Registry {
    page_rules: Vec<Box<dyn PageRule>>,
    ids: HashSet<&'static str>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_page(&mut self, rule: Box<dyn PageRule>) -> Result<(), RegistryError> {
        let id = rule.meta().id;
        self.claim(id)?;
        self.page_rules.push(rule);
        Ok(())
    }

    /// Checks and reserves an id, or explains why it cannot have one.
    ///
    /// Reserving happens only after every check passes, so a rejected rule
    /// leaves the registry exactly as it found it — a half-registered rule
    /// would make the next duplicate check wrong.
    fn claim(&mut self, id: &'static str) -> Result<(), RegistryError> {
        if !valid_id(id) {
            return Err(RegistryError::MalformedId(id));
        }
        if self.ids.contains(id) {
            return Err(RegistryError::DuplicateId(id));
        }
        if self.len() >= MAX_RULES {
            return Err(RegistryError::CapExceeded);
        }
        self.ids.insert(id);
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.page_rules.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn page_rules(&self) -> &[Box<dyn PageRule>] {
        &self.page_rules
    }

    /// Runs every page rule against one record.
    pub fn run_page(&self, page: &PageRecord) -> Vec<Issue> {
        let mut out = Vec::new();
        for rule in &self.page_rules {
            rule.check(page, &mut out);
        }
        out
    }
}

/// `batch.rule-name`: lowercase, exactly one dot, hyphens and digits allowed
/// in the name.
///
/// Enforced because ids are permanent — they appear in `--fail-on`, in
/// exported reports and in saved `.pounce` files — so a typo caught at
/// registration is far cheaper than one caught by a user's CI, where a filter
/// on a misspelled id matches nothing rather than erroring.
fn valid_id(id: &str) -> bool {
    let Some((batch, name)) = id.split_once('.') else {
        return false;
    };
    !batch.is_empty()
        && !name.is_empty()
        && batch.chars().all(|c| c.is_ascii_lowercase())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}
