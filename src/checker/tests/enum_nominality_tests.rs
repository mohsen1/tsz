//! Manual tests for enum nominal assignability rules.
//!
//! Tests that enum members are not assignable to different enum members,
//! even when the values are the same. This validates TypeScript's nominal
//! typing for enums.

use crate::binder::BinderState;
use crate::checker::state::CheckerState;
use crate::parser::ParserState;
use crate::solver::TypeInterner;

fn test_enum_assignability(source: &str, expected_errors: usize) {
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
        crate::checker::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    let error_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .count();
    assert_eq!(
        error_count, expected_errors,
        "Expected {} TS2322 errors, got {}: {:?}",
        expected_errors, error_count, checker.ctx.diagnostics
    );
}

#[test]
fn test_enum_member_to_same_member() {
    // E.A should be assignable to E.A
    let source = r#"
enum E { A = 0, B = 1 }
const x: E.A = E.A;  // OK - same member
"#;
    test_enum_assignability(source, 0);
}

#[test]
fn test_enum_member_to_different_member() {
    // E.A should NOT be assignable to E.B
    let source = r#"
enum E { A = 0, B = 1 }
const x: E.B = E.A;  // ERROR: different member
"#;
    test_enum_assignability(source, 1);
}

#[test]
fn test_enum_member_to_whole_enum() {
    // E.A should be assignable to E (whole enum)
    let source = r#"
enum E { A = 0, B = 1 }
const x: E = E.A;  // OK - member to whole enum
"#;
    test_enum_assignability(source, 0);
}

#[test]
fn test_whole_enum_to_member() {
    // E (whole enum) should NOT be assignable to E.A
    let source = r#"
enum E { A = 0, B = 1 }
const x: E.A = E;  // ERROR: whole enum to member
"#;
    test_enum_assignability(source, 1);
}

#[test]
fn test_different_enums_same_values() {
    // E.A should NOT be assignable to F.A, even if both are 0
    let source = r#"
enum E { A = 0 }
enum F { A = 0 }
const x: F.A = E.A;  // ERROR: different enums
"#;
    test_enum_assignability(source, 1);
}

#[test]
fn test_numeric_enum_to_number() {
    // Numeric enum member should be assignable to number
    let source = r#"
enum E { A = 0 }
const x: number = E.A;  // OK - numeric enum to number
"#;
    test_enum_assignability(source, 0);
}

#[test]
fn test_number_to_numeric_enum_member() {
    // number should NOT be assignable to numeric enum member
    let source = r#"
enum E { A = 0 }
const x: E.A = 1;  // ERROR: number to enum member
"#;
    test_enum_assignability(source, 1);
}

#[test]
fn test_number_to_numeric_enum_type() {
    // number SHOULD be assignable to numeric enum type (but not literal values)
    let source = r#"
enum E { A = 0 }
const x: E = 1;  // OK: number to enum type
"#;
    test_enum_assignability(source, 0);
}

#[test]
fn test_number_literal_to_numeric_enum_type() {
    // number literal SHOULD be assignable to numeric enum type
    let source = r#"
enum E { A = 0 }
const x: E = 0;  // OK: literal to enum type
"#;
    test_enum_assignability(source, 0);
}

#[test]
fn test_string_enum_opacity() {
    // String literal should NOT be assignable to string enum
    let source = r#"
enum E { A = "a" }
const x: E = "a";  // ERROR: string literal to string enum
"#;
    test_enum_assignability(source, 1);
}

#[test]
fn test_string_enum_not_to_string() {
    // String enum should NOT be assignable to string
    let source = r#"
enum E { A = "a" }
const x: string = E.A;  // ERROR: string enum to string
"#;
    test_enum_assignability(source, 1);
}
