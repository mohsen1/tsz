//! Shared test fixtures to reduce per-test allocation overhead
//!
//! This module provides common test utilities and pre-allocated fixtures
//! to speed up test execution.
//!
//! # Shared Lib Context
//!
//! For tests that need global types (Array, Promise, etc.), lib files must
//! be provided explicitly since embedded libs have been removed.
//!
//! For tests that explicitly test behavior WITHOUT lib files:
//! - `TestContext::new_without_lib()` - Creates a context without lib files

use once_cell::sync::Lazy;
use std::sync::Arc;

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

/// Shared lib files for global type resolution.
///
/// Loads lib.es5.d.ts which contains core global types (Array, Object, Function, etc.).
pub static SHARED_LIB_FILES: Lazy<Vec<Arc<crate::lib_loader::LibFile>>> =
    Lazy::new(|| load_lib_files_from_paths());

/// Shared lib contexts for checker tests.
///
/// Pre-compiled lib contexts from SHARED_LIB_FILES for fast test setup.
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
    /// Create a new test context with fresh allocations.
    ///
    /// NOTE: Embedded libs have been removed. This creates a context WITHOUT
    /// lib files. Tests that need global types should use `new_with_libs()`
    /// and provide lib files explicitly.
    #[inline]
    pub fn new() -> Self {
        Self {
            arena: NodeArena::new(),
            binder: BinderState::new(),
            types: TypeInterner::new(),
            lib_files: Vec::new(),
        }
    }

    /// Create a new test context with provided lib files.
    #[inline]
    pub fn new_with_libs(lib_files: Vec<Arc<crate::lib_loader::LibFile>>) -> Self {
        Self {
            arena: NodeArena::new(),
            binder: BinderState::new(),
            types: TypeInterner::new(),
            lib_files,
        }
    }

    /// Create a new test context with just ES5 lib (smaller, faster).
    /// Use this for tests that only need basic types (Array, Object, Function).
    ///
    /// NOTE: Embedded libs have been removed. This now returns an empty context.
    #[inline]
    pub fn new_es5_only() -> Self {
        Self::new()
    }

    /// Create a new test context WITHOUT loading lib.d.ts.
    /// Use this for testing error emission when lib symbols are missing.
    #[inline]
    pub fn new_without_lib() -> Self {
        Self::new()
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
/// Loads embedded lib types (Array, Object, Function, etc.) into the checker
/// so that global type resolution works correctly during type checking.
#[inline]
pub fn setup_lib_contexts(checker: &mut CheckerState<'_>) {
    let lib_contexts = SHARED_LIB_CONTEXTS.clone();
    let count = lib_contexts.len();
    checker.ctx.set_lib_contexts(lib_contexts);
    checker.ctx.set_actual_lib_file_count(count);
}

/// Merge shared lib symbols into a binder.
///
/// Merges embedded lib symbols (Array, Object, Function, etc.) into the binder
/// so that bindings can reference global types during the binding phase.
#[inline]
pub fn merge_shared_lib_symbols(binder: &mut BinderState) {
    let lib_contexts = &*SHARED_LIB_CONTEXTS;
    if !lib_contexts.is_empty() {
        let binder_contexts: Vec<crate::binder::state::LibContext> = lib_contexts
            .iter()
            .map(|ctx| crate::binder::state::LibContext {
                arena: std::sync::Arc::clone(&ctx.arena),
                binder: std::sync::Arc::clone(&ctx.binder),
            })
            .collect();
        binder.merge_lib_contexts_into_binder(&binder_contexts);
    }
}

/// Helper function to load lib files from disk for tests that need global types.
/// Returns a vector of LibFile objects that can be used with bind_source_file_with_libs.
///
/// This function loads lib.es5.d.ts which contains core global types (Array, Boolean, etc.)
/// from TypeScript's lib directory.
///
/// If lib files are not found, returns an empty vector.
#[inline]
pub fn load_lib_files_for_test() -> Vec<Arc<crate::lib_loader::LibFile>> {
    load_lib_files_from_paths()
}

/// Internal function to load lib files from known paths.
/// Checks multiple locations where TypeScript libs might be installed.
fn load_lib_files_from_paths() -> Vec<Arc<crate::lib_loader::LibFile>> {
    // Use CARGO_MANIFEST_DIR for absolute paths so tests work with nextest
    // (which runs each test in a separate process with a different working directory)
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let lib_paths = [
        manifest_dir.join("scripts/conformance/node_modules/typescript/lib/lib.es5.d.ts"),
        manifest_dir.join("scripts/emit/node_modules/typescript/lib/lib.es5.d.ts"),
        manifest_dir.join("TypeScript/node_modules/typescript/lib/lib.es5.d.ts"),
        manifest_dir.join("../TypeScript/node_modules/typescript/lib/lib.es5.d.ts"),
    ];

    for lib_path in &lib_paths {
        if lib_path.exists() {
            if let Ok(content) = std::fs::read_to_string(lib_path) {
                let lib_file =
                    crate::lib_loader::LibFile::from_source("lib.es5.d.ts".to_string(), content);
                return vec![Arc::new(lib_file)];
            }
        }
    }

    Vec::new()
}
