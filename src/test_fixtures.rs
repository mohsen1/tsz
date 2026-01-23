//! Shared test fixtures to reduce per-test allocation overhead
//!
//! This module provides common test utilities and pre-allocated fixtures
//! to speed up test execution.

use once_cell::sync::Lazy;

use crate::binder::BinderState;
use crate::checker::context::CheckerOptions;
use crate::checker::state::CheckerState;
use crate::parser::ParserState;
use crate::parser::node::NodeArena;
use crate::solver::TypeInterner;

/// Common test file name - interned once
pub static TEST_FILE_NAME: Lazy<String> = Lazy::new(|| "test.ts".to_string());

/// Common test file name for TSX files
pub static TEST_TSX_FILE_NAME: Lazy<String> = Lazy::new(|| "test.tsx".to_string());

/// Default checker options - created once
pub static DEFAULT_CHECKER_OPTIONS: Lazy<CheckerOptions> = Lazy::new(CheckerOptions::default);

/// Test context builder for common test setup patterns.
/// Reduces boilerplate while allowing test-specific customization.
pub struct TestContext {
    pub arena: NodeArena,
    pub binder: BinderState,
    pub types: TypeInterner,
}

impl TestContext {
    /// Create a new test context with fresh allocations
    #[inline]
    pub fn new() -> Self {
        Self {
            arena: NodeArena::new(),
            binder: BinderState::new(),
            types: TypeInterner::new(),
        }
    }

    /// Create a checker from this context
    #[inline]
    pub fn checker(&self) -> CheckerState<'_> {
        CheckerState::new(
            &self.arena,
            &self.binder,
            &self.types,
            TEST_FILE_NAME.clone(),
            DEFAULT_CHECKER_OPTIONS.clone(),
        )
    }

    /// Create a checker with custom file name
    #[inline]
    pub fn checker_with_file(&self, file_name: String) -> CheckerState<'_> {
        CheckerState::new(
            &self.arena,
            &self.binder,
            &self.types,
            file_name,
            DEFAULT_CHECKER_OPTIONS.clone(),
        )
    }

    /// Create a checker with custom options
    #[inline]
    pub fn checker_with_options(&self, options: CheckerOptions) -> CheckerState<'_> {
        CheckerState::new(
            &self.arena,
            &self.binder,
            &self.types,
            TEST_FILE_NAME.clone(),
            options,
        )
    }
}

impl Default for TestContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Quick parser setup for simple parsing tests
#[inline]
pub fn parse_source(source: &str) -> ParserState {
    ParserState::new(TEST_FILE_NAME.clone(), source.to_string())
}

/// Quick parser setup for TSX files
#[inline]
pub fn parse_tsx_source(source: &str) -> ParserState {
    ParserState::new(TEST_TSX_FILE_NAME.clone(), source.to_string())
}

/// Macro to create a test context and checker in one line
/// Usage: let (ctx, checker) = test_checker!();
#[macro_export]
macro_rules! test_checker {
    () => {{
        let ctx = $crate::test_fixtures::TestContext::new();
        let checker = ctx.checker();
        (ctx, checker)
    }};
    ($file:expr) => {{
        let ctx = $crate::test_fixtures::TestContext::new();
        let checker = ctx.checker_with_file($file.to_string());
        (ctx, checker)
    }};
}
