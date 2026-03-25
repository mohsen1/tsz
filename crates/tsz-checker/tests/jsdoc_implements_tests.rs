//! Tests for JSDoc @implements tag checking.
//!
//! Verifies that classes with @implements tags in JS files are checked
//! for interface/class member compatibility, emitting TS2420 (missing members),
//! TS2416 (incompatible member types), and TS2720 (implementing a class).

use tsz_checker::context::CheckerOptions;

fn check_js(source: &str) -> Vec<u32> {
    let options = CheckerOptions {
        check_js: true,
        strict: true,
        ..CheckerOptions::default()
    };

    let mut parser =
        tsz_parser::parser::ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = tsz_solver::TypeInterner::new();
    let mut checker = tsz_checker::state::CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.js".to_string(),
        options,
    );

    checker.ctx.set_lib_contexts(Vec::new());
    checker.check_source_file(root);

    checker.ctx.diagnostics.iter().map(|d| d.code).collect()
}

fn check_ts(source: &str) -> Vec<u32> {
    let options = CheckerOptions::default();

    let mut parser =
        tsz_parser::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = tsz_solver::TypeInterner::new();
    let mut checker = tsz_checker::state::CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );

    checker.ctx.set_lib_contexts(Vec::new());
    checker.check_source_file(root);

    checker.ctx.diagnostics.iter().map(|d| d.code).collect()
}

/// @implements with class target — missing members → TS2720
#[test]
fn test_jsdoc_implements_class_missing_member_emits_ts2720() {
    let codes = check_js(
        r#"
class A {
    method() { return 0; }
}

/** @implements {A} */
class B {
}
"#,
    );
    assert!(
        codes.contains(&2720),
        "Expected TS2720 for class missing implemented class member, got: {codes:?}"
    );
}

/// @implements without braces — `@implements A` syntax
#[test]
fn test_jsdoc_implements_no_braces() {
    let codes = check_js(
        r#"
class A {
    method() { return 0; }
}

/** @implements A */
class B {
}
"#,
    );
    assert!(
        codes.contains(&2720),
        "Expected TS2720 for @implements without braces, got: {codes:?}"
    );
}

#[test]
fn test_jsdoc_implements_missing_type_emits_ts1003() {
    let codes = check_js(
        r#"
class A { constructor() { this.x = 0; } }
/** @implements */
class B {
}
"#,
    );
    assert!(
        codes.contains(&1003),
        "Expected TS1003 for empty @implements tag, got: {codes:?}"
    );
}

/// @implements with multiple tags — missing member from second target
#[test]
fn test_jsdoc_implements_multiple_tags() {
    let codes = check_js(
        r#"
class A {
    foo() { return 0; }
}
class B {
    bar() { return ""; }
}

/**
 * @implements {A}
 * @implements {B}
 */
class C {
    foo() { return 0; }
}
"#,
    );
    // C is missing B's `bar` method → should emit TS2720
    assert!(
        codes.contains(&2720),
        "Expected TS2720 for missing member from second @implements, got: {codes:?}"
    );
}

/// @implements on TS file → should NOT trigger JSDoc checking
#[test]
fn test_jsdoc_implements_ignored_in_ts_files() {
    let codes = check_ts(
        r#"
class A {
    method(): number { return 0; }
}

/** @implements {A} */
class B {
}
"#,
    );
    // Should NOT emit TS2720 — JSDoc @implements only checked in JS files
    assert!(
        !codes.contains(&2720),
        "Expected no TS2720 in .ts file for @implements JSDoc tag, got: {codes:?}"
    );
}
