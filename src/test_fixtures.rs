//! Shared test fixtures to reduce per-test allocation overhead
//!
//! This module provides common test utilities and pre-allocated fixtures
//! to speed up test execution.

use once_cell::sync::Lazy;
use std::sync::Arc;

use crate::binder::BinderState;
use crate::checker::context::CheckerOptions;
use crate::checker::state::CheckerState;
use crate::lib_loader::load_default_lib_dts;
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
    /// Lib files loaded for global type resolution (console, Promise, Array, etc.)
    pub lib_files: Vec<Arc<crate::lib_loader::LibFile>>,
}

impl TestContext {
    /// Create a new test context with fresh allocations and lib.d.ts loaded.
    #[inline]
    pub fn new() -> Self {
        let lib_files = Self::load_default_lib();
        Self {
            arena: NodeArena::new(),
            binder: BinderState::new(),
            types: TypeInterner::new(),
            lib_files,
        }
    }

    /// Load default lib.d.ts for global type resolution.
    /// Returns empty Vec if lib.d.ts cannot be found (graceful degradation).
    fn load_default_lib() -> Vec<Arc<crate::lib_loader::LibFile>> {
        let mut libs = Vec::new();
        if let Some(lib_file) = load_default_lib_dts() {
            libs.push(lib_file);
        }
        libs
    }

    /// Create a new test context WITHOUT loading lib.d.ts.
    /// Use this for testing error emission when lib symbols are missing.
    #[inline]
    pub fn new_without_lib() -> Self {
        Self {
            arena: NodeArena::new(),
            binder: BinderState::new(),
            types: TypeInterner::new(),
            lib_files: Vec::new(),
        }
    }

    /// Create a checker from this context with lib contexts set.
    #[inline]
    pub fn checker(&self) -> CheckerState<'_> {
        let mut checker = CheckerState::new(
            &self.arena,
            &self.binder,
            &self.types,
            TEST_FILE_NAME.clone(),
            DEFAULT_CHECKER_OPTIONS.clone(),
        );

        // Set lib contexts for global symbol resolution
        if !self.lib_files.is_empty() {
            let lib_contexts: Vec<crate::binder::LibContext> = self
                .lib_files
                .iter()
                .map(|lib| crate::binder::LibContext {
                    arena: Arc::clone(&lib.arena),
                    binder: Arc::clone(&lib.binder),
                })
                .collect();
            checker.ctx.set_lib_contexts(lib_contexts);
        }

        checker
    }

    /// Create a checker with custom file name
    #[inline]
    pub fn checker_with_file(&self, file_name: String) -> CheckerState<'_> {
        let mut checker = CheckerState::new(
            &self.arena,
            &self.binder,
            &self.types,
            file_name,
            DEFAULT_CHECKER_OPTIONS.clone(),
        );

        // Set lib contexts for global symbol resolution
        if !self.lib_files.is_empty() {
            let lib_contexts: Vec<crate::binder::LibContext> = self
                .lib_files
                .iter()
                .map(|lib| crate::binder::LibContext {
                    arena: Arc::clone(&lib.arena),
                    binder: Arc::clone(&lib.binder),
                })
                .collect();
            checker.ctx.set_lib_contexts(lib_contexts);
        }

        checker
    }

    /// Create a checker with custom options
    #[inline]
    pub fn checker_with_options(&self, options: CheckerOptions) -> CheckerState<'_> {
        let mut checker = CheckerState::new(
            &self.arena,
            &self.binder,
            &self.types,
            TEST_FILE_NAME.clone(),
            options,
        );

        // Set lib contexts for global symbol resolution
        if !self.lib_files.is_empty() {
            let lib_contexts: Vec<crate::binder::LibContext> = self
                .lib_files
                .iter()
                .map(|lib| crate::binder::LibContext {
                    arena: Arc::clone(&lib.arena),
                    binder: Arc::clone(&lib.binder),
                })
                .collect();
            checker.ctx.set_lib_contexts(lib_contexts);
        }

        checker
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
