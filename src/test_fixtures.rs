//! Shared test fixtures to reduce per-test allocation overhead
//!
//! This module provides common test utilities and pre-allocated fixtures
//! to speed up test execution.
//!
//! # Shared Lib Context
//!
//! For tests that need global types (Array, Promise, etc.), use:
//! - `TestContext::new()` - Creates a context with lib files loaded
//! - `SHARED_LIB_FILES` - Static lazy-loaded lib files that can be reused
//!
//! For tests that explicitly test behavior WITHOUT lib files:
//! - `TestContext::new_without_lib()` - Creates a context without lib files

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

/// Shared lib files for global type resolution (Array, Promise, etc.)
/// This is lazily loaded once and reused across all tests.
///
/// Uses ES2020 WITHOUT DOM to avoid declaration conflicts in tests.
/// Includes:
/// - Core types: Array, Object, Function, String, Number, Boolean
/// - ES2015+ types: Promise, Map, Set, Symbol, Iterator
/// - Async types: AsyncIterator, Awaited, IterableIterator
///
/// Does NOT include DOM types (console, Document, Element) to avoid
/// redeclaration conflicts with test code.
pub static SHARED_LIB_FILES: Lazy<Vec<Arc<crate::lib_loader::LibFile>>> = Lazy::new(|| {
    use crate::common::ScriptTarget;
    use crate::lib_loader::load_embedded_libs;
    // Load ES2020 WITHOUT DOM - avoids conflicts with ActiveXObject, etc.
    load_embedded_libs(ScriptTarget::ES2020, false)
});

/// Shared lib contexts for checker - derived from SHARED_LIB_FILES
pub static SHARED_LIB_CONTEXTS: Lazy<Vec<crate::checker::context::LibContext>> = Lazy::new(|| {
    SHARED_LIB_FILES
        .iter()
        .map(|lib| crate::checker::context::LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect()
});

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
    /// Create a new test context with fresh allocations and ES2020 lib files loaded.
    ///
    /// This uses the shared lib files (ES2020 without DOM) which includes:
    /// - Core types: Array, Object, Function, String, Number, Boolean
    /// - ES2015+ types: Promise, Map, Set, Symbol, Iterator  
    /// - Async types: AsyncIterator, Awaited, IterableIterator
    ///
    /// Does NOT include DOM types to avoid declaration conflicts in tests.
    #[inline]
    pub fn new() -> Self {
        // Use shared lib files to avoid re-parsing for every test
        let lib_files: Vec<Arc<crate::lib_loader::LibFile>> =
            SHARED_LIB_FILES.iter().map(Arc::clone).collect();
        Self {
            arena: NodeArena::new(),
            binder: BinderState::new(),
            types: TypeInterner::new(),
            lib_files,
        }
    }

    /// Create a new test context with just ES5 lib (smaller, faster).
    /// Use this for tests that only need basic types (Array, Object, Function).
    #[inline]
    pub fn new_es5_only() -> Self {
        let mut libs = Vec::new();
        if let Some(lib_file) = load_default_lib_dts() {
            libs.push(lib_file);
        }
        Self {
            arena: NodeArena::new(),
            binder: BinderState::new(),
            types: TypeInterner::new(),
            lib_files: libs,
        }
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
            let lib_contexts: Vec<crate::checker::context::LibContext> = self
                .lib_files
                .iter()
                .map(|lib| crate::checker::context::LibContext {
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
            let lib_contexts: Vec<crate::checker::context::LibContext> = self
                .lib_files
                .iter()
                .map(|lib| crate::checker::context::LibContext {
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
            let lib_contexts: Vec<crate::checker::context::LibContext> = self
                .lib_files
                .iter()
                .map(|lib| crate::checker::context::LibContext {
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

/// Set up shared lib contexts on a checker.
///
/// This is a helper for tests that create checkers manually but need global types.
/// It uses the shared lib contexts to avoid re-parsing lib files.
///
/// # Example
/// ```rust
/// let mut checker = CheckerState::new(...);
/// setup_lib_contexts(&mut checker);
/// ```
#[inline]
pub fn setup_lib_contexts(checker: &mut CheckerState<'_>) {
    if !SHARED_LIB_CONTEXTS.is_empty() {
        checker.ctx.set_lib_contexts(SHARED_LIB_CONTEXTS.clone());
    }
}

/// Merge shared lib symbols into a binder.
///
/// This is a helper for tests that create binders manually but need global types.
/// It uses the shared lib files to avoid re-parsing.
///
/// # Example
/// ```rust
/// let mut binder = BinderState::new();
/// merge_shared_lib_symbols(&mut binder);
/// binder.bind_source_file(arena, root);
/// ```
#[inline]
pub fn merge_shared_lib_symbols(binder: &mut BinderState) {
    if !SHARED_LIB_FILES.is_empty() {
        binder.merge_lib_symbols(&SHARED_LIB_FILES);
    }
}
