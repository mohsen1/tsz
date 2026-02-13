//! Tests for property access in non-strict mode
//!
//! When noImplicitAny is false, accessing non-existent properties should
//! return 'any' without error, matching TypeScript's behavior.

use crate::context::CheckerOptions;
use crate::state::CheckerState;
use tsz_binder::BinderState;
use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

#[test]
fn test_non_existent_property_non_strict_returns_any() {
    // Setup: noImplicitAny = false (non-strict mode)
    let source = r#"
        var obj: { foo: number };
        var result = obj.bar; // bar doesn't exist, should return 'any' without error
    "#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions {
        strict: false,
        no_implicit_any: false,
        target: ScriptTarget::ES2015,
        module: ModuleKind::ESNext,
        ..Default::default()
    };

    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );

    checker.check_source_file(root);

    // Should have NO TS2339 diagnostics
    let ts2339_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2339)
        .count();

    assert_eq!(
        ts2339_count, 0,
        "Non-strict mode should not report TS2339 for non-existent properties. Diagnostics: {:#?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_non_existent_property_strict_reports_error() {
    // Setup: noImplicitAny = true (strict mode)
    let source = r#"
        var obj: { foo: number };
        var result = obj.bar; // bar doesn't exist, should error in strict mode
    "#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions {
        strict: false,
        no_implicit_any: true, // Explicitly enabled
        target: ScriptTarget::ES2015,
        module: ModuleKind::ESNext,
        ..Default::default()
    };

    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );

    checker.check_source_file(root);

    // Should have TS2339 diagnostic
    let ts2339_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2339)
        .count();

    assert!(
        ts2339_count >= 1,
        "Strict mode should report TS2339 for non-existent properties. Got {} errors: {:#?}",
        ts2339_count,
        checker.ctx.diagnostics
    );
}
