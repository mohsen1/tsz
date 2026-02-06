//! Manual tests for strictNullChecks and null/undefined property access
//!
//! Tests error code selection for property access on null/undefined values
//! since TypeScript/tests/ conformance suite is not available.

use crate::state::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

#[test]
fn test_literal_null_property_access_without_strict() {
    let source = r#"
const x: null = null;
x.prop;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // tsc emits TS18047 "'x' is possibly 'null'" for property access on null
    // (TS2531 is the older error code)
    let ts18047_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 18047)
        .count();
    assert!(
        ts18047_count >= 1,
        "Expected at least 1 TS18047 error, got {}",
        ts18047_count
    );
}

#[test]
fn test_literal_undefined_property_access_without_strict() {
    let source = r#"
const x: undefined = undefined;
x.prop;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // tsc emits TS18048 "'x' is possibly 'undefined'" for property access on undefined
    let ts18048_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 18048)
        .count();
    assert!(
        ts18048_count >= 1,
        "Expected at least 1 TS18048 error for undefined, got {}",
        ts18048_count
    );
}

#[test]
fn test_null_union_property_access_without_strict() {
    let source = r#"
const x: string | null = null;
x.prop;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // tsc emits TS18047 for property access on union types containing null
    let ts18047_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 18047)
        .count();
    assert!(
        ts18047_count >= 1,
        "Expected at least 1 TS18047 error, got {}",
        ts18047_count
    );
}

#[test]
fn test_any_property_access_no_error() {
    let source = r#"
const x: any = null;
x.prop;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // With `any`, no error should be emitted for property access
    // Filter out TS2318 (missing lib.d.ts globals) which are unrelated to the test
    let error_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code != 2318)
        .count();
    assert_eq!(
        error_count, 0,
        "Expected no errors with any type, got {}",
        error_count
    );
}
