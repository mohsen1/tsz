//! Tests for parser improvements to reduce TS1005 and TS2300 false positives — type member recovery.

use crate::parser::test_fixture::parse_source;
use tsz_common::diagnostics::diagnostic_codes;
use tsz_common::position::LineMap;

#[test]
fn invalid_type_literal_statement_tail_recovers_as_source_statements() {
    let source = "type T = {\n    return true;\n}\nlet x = 1;\n";
    let (parser, root) = parse_source(source);
    let line_map = LineMap::build(source);

    let fingerprints: Vec<(u32, u32, u32, String)> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| {
            let pos = line_map.offset_to_position(diag.start, source);
            (
                diag.code,
                pos.line + 1,
                pos.character + 1,
                diag.message.clone(),
            )
        })
        .collect();

    assert!(
        fingerprints.contains(&(
            diagnostic_codes::PROPERTY_OR_SIGNATURE_EXPECTED,
            2,
            5,
            "Property or signature expected.".to_string()
        )),
        "expected TS1131 at the invalid type member statement, got {fingerprints:?}"
    );
    assert!(
        fingerprints.contains(&(
            diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
            3,
            1,
            "Declaration or statement expected.".to_string()
        )),
        "expected TS1128 at the deferred type-literal close brace, got {fingerprints:?}"
    );

    let arena = parser.get_arena();
    let source_file = arena.get_source_file_at(root).unwrap();
    let statement_kinds: Vec<u16> = source_file
        .statements
        .nodes
        .iter()
        .map(|&stmt_idx| arena.get(stmt_idx).unwrap().kind)
        .collect();
    assert_eq!(
        statement_kinds,
        vec![
            crate::parser::syntax_kind_ext::TYPE_ALIAS_DECLARATION,
            crate::parser::syntax_kind_ext::RETURN_STATEMENT,
            crate::parser::syntax_kind_ext::VARIABLE_STATEMENT,
        ],
        "invalid type-literal statement tails should be preserved for statement recovery"
    );
}

#[test]
fn test_type_literal_stray_generic_member_reports_missing_open_paren_once() {
    let source = r"
var v: {
   A: B
   <T>;
};
";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let ts1005: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::EXPECTED)
        .collect();

    assert_eq!(
        ts1005.len(),
        1,
        "Expected only the missing '(' recovery diagnostic for a stray generic member, got {diagnostics:?}"
    );
    assert!(
        ts1005[0].message.contains("'(' expected."),
        "Expected the stray generic member to report a missing '(', got {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .all(|d| !(d.code == diagnostic_codes::EXPECTED && d.message.contains("')' expected."))),
        "Stray generic members should not cascade into a missing ')' diagnostic: {diagnostics:?}"
    );
}

#[test]
fn test_interface_property_initializer_emits_ts1246() {
    let source = r"
interface I {
    x: number = 1;
}
";
    let (parser, _root) = parse_source(source);

    let ts1246_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 1246)
        .count();
    assert_eq!(
        ts1246_count, 1,
        "Expected 1 TS1246 error for interface property initializer, got {ts1246_count}",
    );
}

#[test]
fn test_type_literal_property_initializer_emits_ts1247() {
    let source = r"
type T = {
    x: number = 1;
};
";
    let (parser, _root) = parse_source(source);

    let ts1247_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 1247)
        .count();
    assert_eq!(
        ts1247_count, 1,
        "Expected 1 TS1247 error for type literal property initializer, got {ts1247_count}",
    );
}

#[test]
fn test_ts1131_emitted_for_invalid_interface_member() {
    // Invalid token inside an interface body should emit TS1131
    // "Property or signature expected."
    let source = r"
interface Foo {
    ?;
}
";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let ts1131_count = diagnostics.iter().filter(|d| d.code == 1131).count();
    assert!(
        ts1131_count >= 1,
        "Expected at least 1 TS1131 for invalid interface member, got {ts1131_count}. Diagnostics: {diagnostics:?}",
    );
}

#[test]
fn test_ts1131_emitted_for_invalid_type_literal_member() {
    // Invalid token inside a type literal should emit TS1131
    let source = r"
type T = {
    !;
};
";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let ts1131_count = diagnostics.iter().filter(|d| d.code == 1131).count();
    assert!(
        ts1131_count >= 1,
        "Expected at least 1 TS1131 for invalid type literal member, got {ts1131_count}. Diagnostics: {diagnostics:?}",
    );
}

#[test]
fn test_postfix_optional_method_signature_recovers_with_semicolon_expected() {
    let source = r"
type T = { x()?: number; };
interface I { y()?: string; }
";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let x_question = source.find("x()?").expect("x method") as u32 + 3;
    let y_question = source.find("y()?").expect("y method") as u32 + 3;
    let question_positions = vec![x_question, y_question];
    let colon_positions = vec![x_question + 1, y_question + 1];

    let actual_positions: Vec<u32> = diagnostics
        .iter()
        .filter(|diag| diag.code == diagnostic_codes::EXPECTED && diag.message == "';' expected.")
        .map(|diag| diag.start)
        .collect();
    assert_eq!(
        actual_positions, question_positions,
        "Expected TS1005 ';' expected at postfix optional method markers, got {diagnostics:?}",
    );

    let actual_ts1131_positions: Vec<u32> = diagnostics
        .iter()
        .filter(|diag| diag.code == diagnostic_codes::PROPERTY_OR_SIGNATURE_EXPECTED)
        .map(|diag| diag.start)
        .collect();
    assert_eq!(
        actual_ts1131_positions, colon_positions,
        "Expected TS1131 at the colon following postfix optional method markers: {diagnostics:?}",
    );

    let ts1131_at_question = diagnostics
        .iter()
        .filter(|diag| {
            diag.code == diagnostic_codes::PROPERTY_OR_SIGNATURE_EXPECTED
                && question_positions.contains(&diag.start)
        })
        .count();
    assert_eq!(
        ts1131_at_question, 0,
        "Postfix optional method markers should not fall through to TS1131: {diagnostics:?}",
    );
}

#[test]
fn test_type_literal_statement_recovery_matches_interface_extending_class2() {
    let source = r"
class Foo {
    x: string;
    y() { }
    get Z() {
        return 1;
    }
    [x: string]: Object;
}

interface I2 extends Foo {
    a: {
        toString: () => {
            return 1;
        };
    }
";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<_> = diagnostics.iter().map(|d| d.code).collect();

    assert_eq!(
        codes,
        vec![1131, 1128, 1128],
        "Expected parser recovery to match tsc for malformed type literal member body, got {diagnostics:?}"
    );
}

#[test]
fn test_ts1131_not_emitted_for_valid_interface() {
    // Valid interface should not emit TS1131
    let source = r"
interface Foo {
    x: number;
    y: string;
    z(): void;
}
";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let ts1131_count = diagnostics.iter().filter(|d| d.code == 1131).count();
    assert_eq!(
        ts1131_count, 0,
        "Expected no TS1131 for valid interface, got {ts1131_count}. Diagnostics: {diagnostics:?}",
    );
}

#[test]
fn test_type_literal_invalid_member_lt_minus_reports_ts1109_not_ts1128() {
    let source = r#"
var f: {
    x: number;
    <-
};
"#;
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&diagnostic_codes::TYPE_PARAMETER_DECLARATION_EXPECTED),
        "Expected TS1139 from malformed call-signature type parameters, got diagnostics: {diagnostics:?}"
    );
    assert!(
        codes.contains(&diagnostic_codes::EXPRESSION_EXPECTED),
        "Expected TS1109 at the type-literal synchronizing close brace, got diagnostics: {diagnostics:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED),
        "Expected no top-level TS1128 stray-brace cascade for `<-` type-member recovery, got diagnostics: {diagnostics:?}"
    );
}
