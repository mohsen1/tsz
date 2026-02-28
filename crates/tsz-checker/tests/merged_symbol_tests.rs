//! Tests for merged type+value symbol resolution in expression context.
//!
//! When `type X = ...` and `const X = ...` share the same name, the binder
//! creates ONE symbol with both `TYPE_ALIAS` and `BLOCK_SCOPED_VARIABLE` flags.
//! In expression context, the value side must take precedence so the const's
//! literal type is preserved rather than resolving via the type-only branch.

use crate::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn get_error_codes(source: &str) -> Vec<u32> {
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
    checker.ctx.diagnostics.iter().map(|d| d.code).collect()
}

#[test]
fn test_merged_type_const_literal_preserves_value() {
    // A type alias and const with the same name and same literal value.
    // In expression context, the const's literal type "FAILURE" must be used,
    // NOT the type alias's resolved type (string).
    let codes = get_error_codes(
        r#"
type FAILURE = "FAILURE";
const FAILURE = "FAILURE";

function test(): "FAILURE" | "SUCCESS" {
    return FAILURE;  // Should return literal "FAILURE", not widened string
}
"#,
    );
    assert!(
        !codes.contains(&2322),
        "Should not emit TS2322 for merged type+value literal, got: {codes:?}"
    );
}

#[test]
fn test_merged_enum_type_alias_used_as_value() {
    // Merged enum + type alias should still allow using the enum as a value.
    let codes = get_error_codes(
        r#"
enum E { A, B }
type E = typeof E;
const e: E = E;
"#,
    );
    assert!(
        !codes.contains(&2693),
        "Should not emit TS2693 for merged enum+type alias, got: {codes:?}"
    );
}

#[test]
fn test_pure_type_alias_still_errors_as_value() {
    // A pure type alias (no value counterpart) used as a value should still error.
    let codes = get_error_codes(
        r#"
type Bar = { x: number; };
const x = Bar;
"#,
    );
    assert!(
        codes.contains(&2693) || codes.contains(&2749),
        "Should emit TS2693 or TS2749 for type-only alias used as value, got: {codes:?}"
    );
}

#[test]
fn test_pure_interface_still_errors_as_value() {
    // A pure interface (no value counterpart) used as a value should still error.
    let codes = get_error_codes(
        r#"
interface Foo { x: number; }
const x = Foo;
"#,
    );
    assert!(
        codes.contains(&2693) || codes.contains(&2749),
        "Should emit TS2693 or TS2749 for interface used as value, got: {codes:?}"
    );
}

#[test]
fn test_merged_type_const_no_false_type_only() {
    // Type alias and const with same name: in expression context the const
    // should be used, so typeof should see the const's type, not error.
    let codes = get_error_codes(
        r#"
type Tag = "a" | "b";
const Tag = "a";

const x: typeof Tag = "a";  // typeof Tag is "a" (the const), not the type alias
"#,
    );
    // Filter out TS2318 (global type not found in unit tests)
    let real_errors: Vec<_> = codes.iter().filter(|&&c| c != 2318).copied().collect();
    assert!(
        !real_errors.contains(&2322),
        "Should not emit TS2322 for typeof merged const, got: {real_errors:?}"
    );
}
