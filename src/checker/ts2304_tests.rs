//! Tests for TS2304 emission ("Cannot find name")
//!
//! These tests verify that:
//! 1. TS2304 is emitted when referencing undefined names
//! 2. TS2304 is NOT emitted when lib.d.ts is loaded and provides the name
//! 3. The "Any poisoning" effect is eliminated

use crate::binder::BinderState;
use crate::checker::context::CheckerOptions;
use crate::checker::state::CheckerState;
use crate::parser::ParserState;
use crate::solver::TypeInterner;
use crate::test_fixtures::TestContext;
use std::sync::Arc;

/// Helper function to check source without lib.d.ts and return diagnostics.
/// Creates the checker with the parser's arena directly to ensure proper node resolution.
fn check_without_lib(source: &str) -> Vec<crate::checker::types::Diagnostic> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions::default();

    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );
    // Don't set lib_contexts - no lib files loaded

    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

/// Helper function to check source WITH lib.d.ts and return diagnostics.
fn check_with_lib(source: &str) -> Vec<crate::checker::types::Diagnostic> {
    let ctx = TestContext::new();

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file_with_libs(parser.get_arena(), root, &ctx.lib_files);

    let types = TypeInterner::new();
    let options = CheckerOptions::default();

    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );

    // Set lib contexts for global symbol resolution
    if !ctx.lib_files.is_empty() {
        let lib_contexts: Vec<crate::checker::context::LibContext> = ctx
            .lib_files
            .iter()
            .map(|lib| crate::checker::context::LibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        checker.ctx.set_lib_contexts(lib_contexts);
    }

    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

#[test]
fn test_ts2304_emitted_for_undefined_name() {
    let diagnostics = check_without_lib(r#"const x = undefinedName;"#);

    let ts2304_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2304).collect();
    assert!(
        !ts2304_errors.is_empty(),
        "Expected TS2304 error for undefinedName, got: {:?}",
        diagnostics
    );
}

#[test]
fn test_ts2304_not_emitted_for_lib_globals_with_lib() {
    let diagnostics = check_with_lib(r#"console.log("hello");"#);

    let ts2304_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2304).collect();
    assert!(
        ts2304_errors.is_empty(),
        "Should NOT have TS2304 for console with lib.d.ts, got: {:?}",
        ts2304_errors
    );
}

#[test]
fn test_ts2304_emitted_for_console_without_lib() {
    let diagnostics = check_without_lib(r#"console.log("hello");"#);

    let ts2304_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2304).collect();
    assert!(
        !ts2304_errors.is_empty(),
        "Expected TS2304 for console without lib.d.ts, got: {:?}",
        diagnostics
    );
}
