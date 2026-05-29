//! Tests for parser improvements to reduce TS1005 and TS2300 false positives — tuple type recovery.

use crate::parser::test_fixture::parse_source;
use tsz_common::diagnostics::diagnostic_codes;

#[test]
fn test_named_tuple_member_rest_type_after_colon_does_not_emit_ts1005() {
    let source = r#"
type T = [first: string, rest: ...string[]?];
"#;
    let (parser, _root) = parse_source(source);

    assert!(
        parser.get_diagnostics().iter().all(|d| d.code != 1005),
        "Named tuple rest types after ':' should defer to later tuple diagnostics without TS1005: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_named_tuple_member_optional_type_after_colon_does_not_emit_ts1005() {
    let source = r#"
type T = [element: string?];
"#;
    let (parser, _root) = parse_source(source);

    assert!(
        parser.get_diagnostics().iter().all(|d| d.code != 1005),
        "Named tuple members with a trailing '?' after the type should defer to later tuple diagnostics without TS1005: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn named_tuple_member_postfix_question_is_not_jsdoc_nullable() {
    let source = "type T = [a: string?];";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics.iter().all(|d| {
            d.code
                != diagnostic_codes::AT_THE_END_OF_A_TYPE_IS_NOT_VALID_TYPESCRIPT_SYNTAX_DID_YOU_MEAN_TO_WRITE
        }),
        "Expected named tuple member `string?` to avoid TS17019, got {diagnostics:?}"
    );
}

#[test]
fn tuple_type_missing_comma_reports_comma_without_bracket_cascade() {
    let source = "type T = [string number];";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let number_pos = source.find("number").expect("number token") as u32;
    assert!(
        diagnostics.iter().any(|d| {
            d.code == diagnostic_codes::EXPECTED
                && d.start == number_pos
                && d.message == "',' expected."
        }),
        "Expected TS1005 ',' expected at `number`, got {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().all(|d| d.message != "']' expected."
            && d.code != diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED),
        "Expected no bracket/TS1128 cascade, got {diagnostics:?}"
    );
}

#[test]
fn test_optional_tuple_element() {
    // [T?] should parse correctly without TS1005/TS1110
    let source = r"
interface Buzz { id: number; }
type T = [Buzz?];
";
    let (parser, _root) = parse_source(source);

    // Should not emit TS1005 or TS1110 for optional tuple element
    let ts1005_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 1005)
        .count();
    let ts1110_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 1110)
        .count();

    assert_eq!(
        ts1005_count, 0,
        "Expected no TS1005 errors for optional tuple element, got {ts1005_count}",
    );
    assert_eq!(
        ts1110_count, 0,
        "Expected no TS1110 errors for optional tuple element, got {ts1110_count}",
    );
}

#[test]
fn test_readonly_optional_tuple_element() {
    // readonly [T?] should parse correctly
    let source = r"
interface Buzz { id: number; }
type T = readonly [Buzz?];
";
    let (parser, _root) = parse_source(source);

    // Should not emit any parser errors
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no parser errors for readonly optional tuple, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_named_tuple_element_still_works() {
    // name?: T should still parse as a named tuple element
    let source = r"
type T = [name?: string];
";
    let (parser, _root) = parse_source(source);

    // Should not emit any parser errors
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no parser errors for named optional tuple element, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_mixed_tuple_elements() {
    // Mix of optional, named, and rest elements should work
    let source = r"
interface A { a: number; }
interface B { b: string; }
type T = [A?, name: B, ...rest: string[]];
";
    let (parser, _root) = parse_source(source);

    // Should not emit any parser errors
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no parser errors for mixed tuple elements, got {:?}",
        parser.get_diagnostics()
    );
}
