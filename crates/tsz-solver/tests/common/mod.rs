//! Shared test fixtures for `tsz-solver` integration tests.
//!
//! This module is loaded into multiple test binaries via `#[path = "common/mod.rs"]`,
//! so consumers vary in which items they actually use. All items carry
//! `#[allow(dead_code)]` to keep cross-binary unused-warning noise quiet.

use crate::TypeEnvironment;
use crate::TypeInterner;
use crate::relations::judge::{DefaultJudge, JudgeConfig};

#[allow(dead_code)]
pub fn create_test_interner() -> TypeInterner {
    TypeInterner::new()
}

/// Owns a fresh `TypeInterner` and `TypeEnvironment` so tests can construct a
/// `DefaultJudge` borrowing from both without repeating the same 3-line setup.
///
/// `DefaultJudge<'a>` borrows `&self.interner` and `&self.env`, so the judge
/// must be built per-call site. This fixture exposes a `judge()` accessor for
/// the default config and a `judge_with_config()` accessor for the rare
/// non-default `JudgeConfig` cases.
#[allow(dead_code)]
pub struct JudgeSetup {
    pub interner: TypeInterner,
    pub env: TypeEnvironment,
}

#[allow(dead_code)]
impl JudgeSetup {
    /// Create a fresh `JudgeSetup` with an empty interner and environment.
    pub fn new() -> Self {
        Self {
            interner: create_test_interner(),
            env: TypeEnvironment::new(),
        }
    }

    /// Build a `DefaultJudge` with the default `JudgeConfig`, borrowing from
    /// this fixture's interner and environment.
    pub fn judge(&self) -> DefaultJudge<'_> {
        DefaultJudge::with_defaults(&self.interner, &self.env)
    }

    /// Build a `DefaultJudge` with a custom `JudgeConfig`.
    pub fn judge_with_config(&self, config: JudgeConfig) -> DefaultJudge<'_> {
        DefaultJudge::new(&self.interner, &self.env, config)
    }
}

impl Default for JudgeSetup {
    fn default() -> Self {
        Self::new()
    }
}
