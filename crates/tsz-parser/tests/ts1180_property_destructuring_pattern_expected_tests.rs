//! TS1180 (`Property destructuring pattern expected.`) was previously
//! unwired: an invalid property-name token inside an object *binding*
//! pattern (`const { <bad> } = x`) fell through to the object-*literal*
//! message TS1136 (`Property assignment expected.`) instead, because both
//! contexts shared one property-name parser with a single hardcoded
//! diagnostic. tsc's `parsingContextErrors` picks the message from the
//! enclosing `ParsingContext` (`ObjectBindingElements` vs
//! `ObjectLiteralMembers`); this generalizes the shared parser the same way.
//!
//! Oracle: typescript@7.0.2, `--noEmit --strict --target es2022`.

use crate::parser::test_fixture::parse_source;
use tsz_common::diagnostics::diagnostic_codes;

fn parse_codes(source: &str) -> Vec<u32> {
    let (parser, _root) = parse_source(source);
    parser.parse_diagnostics.iter().map(|d| d.code).collect()
}

fn assert_ts1180_at_bad_token(source: &str, bad_token: &str) {
    let (parser, _root) = parse_source(source);
    let expected_start = source.find(bad_token).expect("bad token in source") as u32;
    let first = parser
        .parse_diagnostics
        .first()
        .unwrap_or_else(|| panic!("expected at least one diagnostic for {source:?}"));
    assert_eq!(
        first.code,
        diagnostic_codes::PROPERTY_DESTRUCTURING_PATTERN_EXPECTED,
        "expected TS1180 for {source:?}, got {:?}",
        parser
            .parse_diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        first.start, expected_start,
        "expected TS1180 at the bad token's position for {source:?}"
    );
}

#[test]
fn const_decl_with_invalid_property_token_emits_ts1180() {
    assert_ts1180_at_bad_token("const {!} = x;", "!");
}

#[test]
fn function_parameter_object_binding_with_invalid_token_emits_ts1180() {
    assert_ts1180_at_bad_token("function f({!}: any) {}", "!");
}

#[test]
fn catch_clause_object_binding_with_invalid_token_emits_ts1180() {
    assert_ts1180_at_bad_token("try {} catch ({!}) {}", "!");
}

#[test]
fn for_of_head_object_binding_with_invalid_token_emits_ts1180() {
    assert_ts1180_at_bad_token("for (const {!} of []) {}", "!");
}

#[test]
fn nested_array_binding_with_invalid_token_emits_ts1180() {
    assert_ts1180_at_bad_token("const [{!}] = [];", "!");
}

#[test]
fn different_invalid_token_still_emits_ts1180() {
    // Proves the diagnostic is not keyed to the specific offending character.
    assert_ts1180_at_bad_token("const {@} = x;", "@");
}

#[test]
fn object_literal_with_invalid_property_token_still_emits_ts1136_not_ts1180() {
    let codes = parse_codes("const o = {!};");
    assert!(
        codes.contains(&diagnostic_codes::PROPERTY_ASSIGNMENT_EXPECTED),
        "expected TS1136 for an object-literal expression, got {codes:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::PROPERTY_DESTRUCTURING_PATTERN_EXPECTED),
        "must not emit TS1180 for an object-literal expression, got {codes:?}"
    );
}

#[test]
fn destructuring_assignment_pattern_with_invalid_token_still_emits_ts1136_not_ts1180() {
    // `({...} = x)` is parsed as an object-literal expression and later
    // reinterpreted as an assignment pattern, so it stays on tsc's
    // `ObjectLiteralMembers` message — not the binding-declaration one.
    let codes = parse_codes("declare let x: any; ({!} = x);");
    assert!(
        codes.contains(&diagnostic_codes::PROPERTY_ASSIGNMENT_EXPECTED),
        "expected TS1136 for a destructuring-assignment pattern, got {codes:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::PROPERTY_DESTRUCTURING_PATTERN_EXPECTED),
        "must not emit TS1180 for a destructuring-assignment pattern, got {codes:?}"
    );
}

#[test]
fn well_formed_object_binding_pattern_emits_no_diagnostics() {
    let source = "declare const obj: {a: number, b: number}; const {a, b: c, ...rest} = obj;";
    let codes = parse_codes(source);
    assert!(
        codes.is_empty(),
        "expected zero parser diagnostics for well-formed binding pattern, got {codes:?}"
    );
}

#[test]
fn numeric_property_name_in_binding_pattern_still_parses_cleanly() {
    // Numeric/string property names are handled by an earlier match arm in
    // the shared parser and must stay unaffected by the TS1180/TS1136 split.
    let source = "declare const obj: {123: number}; const {123: x} = obj;";
    let codes = parse_codes(source);
    assert!(
        codes.is_empty(),
        "expected zero parser diagnostics for numeric property binding, got {codes:?}"
    );
}
