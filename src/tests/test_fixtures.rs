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
/// NOTE: Embedded libs have been removed. This now returns an empty vector.
/// Tests that need lib symbols should load them explicitly from disk.
pub static SHARED_LIB_FILES: Lazy<Vec<Arc<crate::lib_loader::LibFile>>> = Lazy::new(Vec::new);

/// Shared lib contexts for checker - parsed from embedded libs once on first access.
/// Provides global types (Array, Object, Function, Promise, etc.) to tests.
#[cfg(feature = "embedded_libs")]
pub static SHARED_LIB_CONTEXTS: Lazy<Vec<crate::checker::context::LibContext>> = Lazy::new(|| {
    use crate::binder::BinderState;
    use crate::checker::context::LibContext;
    use crate::parser::ParserState;

    let embedded = crate::embedded_libs::get_default_libs_for_target(
        crate::common::ScriptTarget::ES5,
    );

    let lib_contexts: Vec<LibContext> = embedded
        .into_iter()
        .filter_map(|lib| {
            let mut parser =
                ParserState::new(lib.file_name.to_string(), lib.content.to_string());
            let root = parser.parse_source_file();
            if !parser.get_diagnostics().is_empty() {
                return None;
            }
            let mut binder = BinderState::new();
            binder.bind_source_file(parser.get_arena(), root);
            Some(LibContext {
                arena: Arc::new(parser.into_arena()),
                binder: Arc::new(binder),
            })
        })
        .collect();

    // Merge all lib contexts into a single one (same approach as CLI driver)
    if !lib_contexts.is_empty() {
        let mut merged_binder = BinderState::new();
        let binder_contexts: Vec<crate::binder::state::LibContext> = lib_contexts
            .iter()
            .map(|ctx| crate::binder::state::LibContext {
                arena: Arc::clone(&ctx.arena),
                binder: Arc::clone(&ctx.binder),
            })
            .collect();
        merged_binder.merge_lib_contexts_into_binder(&binder_contexts);

        let merged_arena = Arc::clone(&lib_contexts[0].arena);
        vec![LibContext {
            arena: merged_arena,
            binder: Arc::new(merged_binder),
        }]
    } else {
        Vec::new()
    }
});

#[cfg(not(feature = "embedded_libs"))]
pub static SHARED_LIB_CONTEXTS: Lazy<Vec<crate::checker::context::LibContext>> =
    Lazy::new(Vec::new);

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

/// Helper function to load lib.d.ts from disk for tests that need global types.
/// Returns a vector of LibFile objects that can be used with bind_source_file_with_libs.
///
/// This function tries multiple locations to find lib.d.ts:
/// 1. TypeScript/node_modules/typescript/lib/lib.d.ts (repo structure)
/// 2. ../TypeScript/node_modules/typescript/lib/lib.d.ts (parent directory)
///
/// If lib.d.ts is not found, returns an empty vector.
#[inline]
pub fn load_lib_files_for_test() -> Vec<Arc<crate::lib_loader::LibFile>> {
    let lib_paths = [
        std::path::PathBuf::from("TypeScript/node_modules/typescript/lib/lib.d.ts"),
        std::path::PathBuf::from("../TypeScript/node_modules/typescript/lib/lib.d.ts"),
    ];

    for lib_path in &lib_paths {
        if lib_path.exists() {
            if let Ok(content) = std::fs::read_to_string(lib_path) {
                let lib_file =
                    crate::lib_loader::LibFile::from_source("lib.d.ts".to_string(), content);
                return vec![Arc::new(lib_file)];
            }
        }
    }

    Vec::new()
}
