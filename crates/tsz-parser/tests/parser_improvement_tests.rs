//! Tests for parser improvements to reduce TS1005 and TS2300 false positives

use crate::parser::ParserState;
use tsz_common::diagnostics::diagnostic_codes;

#[test]
fn test_index_signature_with_modifier_emits_ts1071() {
    // Index signature with public modifier should emit TS1071, not TS1184
    // TS1071: '{0}' modifier cannot appear on an index signature.
    // TS1184: Modifiers cannot appear here. (too generic)
    let source = r"
interface I {
  public [a: string]: number;
}
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();

    // Should emit TS1071 for modifier on index signature
    let ts1071_count = diagnostics.iter().filter(|d| d.code == 1071).count();
    assert_eq!(
        ts1071_count, 1,
        "Expected 1 TS1071 error for modifier on index signature, got {ts1071_count}",
    );

    // Should NOT emit the generic TS1184
    let ts1184_count = diagnostics.iter().filter(|d| d.code == 1184).count();
    assert_eq!(
        ts1184_count, 0,
        "Expected no TS1184 errors (should be TS1071 instead), got {ts1184_count}",
    );
}

#[test]
fn test_arrow_function_with_line_break_no_false_positive() {
    // Arrow function where => is missing but there's a line break
    // Should be more permissive to avoid false positives
    let source = r"
const fn = (a: number, b: string)
=> a + b;
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should not have cascading TS1005 errors
    let ts1005_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 1005)
        .count();
    assert!(
        ts1005_count <= 1,
        "Expected at most 1 TS1005 error, got {ts1005_count}",
    );
}

#[test]
fn test_missing_arrow_with_typed_parameters_prefers_arrow_recovery() {
    let source = r"
namespace N {
    var d = (x: number, y: string);
    var e = (x: number, y: string): void;
}
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts1005_count = diagnostics.iter().filter(|d| d.code == 1005).count();
    let ts1109_count = diagnostics.iter().filter(|d| d.code == 1109).count();

    assert_eq!(
        ts1005_count, 2,
        "Expected one missing-arrow TS1005 per declaration, got {diagnostics:?}"
    );
    assert_eq!(
        ts1109_count, 0,
        "Typed parameter heads without => should not fall back to expression recovery: {diagnostics:?}"
    );
}

#[test]
fn test_missing_arrow_statement_body_consumes_synthetic_close_brace() {
    let source = r"
namespace N {
    var c = (x) => var k = 10;};
}
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts1005_count = diagnostics.iter().filter(|d| d.code == 1005).count();
    let ts1128_count = diagnostics.iter().filter(|d| d.code == 1128).count();

    assert_eq!(
        ts1005_count, 1,
        "Expected only the missing-block TS1005 for recovered arrow body, got {diagnostics:?}"
    );
    assert_eq!(
        ts1128_count, 0,
        "Recovered statement-bodied arrows should consume their synthetic close brace: {diagnostics:?}"
    );
}

#[test]
fn test_missing_arrow_expression_body_consumes_synthetic_close_brace() {
    let source = r"
namespace N {
    namespace Inner {
        var c = (x) => };
    }
}
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts1109_count = diagnostics.iter().filter(|d| d.code == 1109).count();
    let ts1128_count = diagnostics.iter().filter(|d| d.code == 1128).count();

    assert_eq!(
        ts1109_count, 1,
        "Expected only the missing-expression TS1109 for recovered arrow body, got {diagnostics:?}"
    );
    assert_eq!(
        ts1128_count, 0,
        "Recovered expression-bodied arrows should consume their synthetic close brace: {diagnostics:?}"
    );
}

#[test]
fn test_parenthesized_object_literal_after_arrow_is_not_treated_as_missing_arrow() {
    let source = r"
/** @template T @param {T|undefined} value @returns {T} */
const cloneObjectGood = value => /** @type {T} */({ ...value });
";
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics.is_empty(),
        "Parenthesized object literal bodies after arrows should not trigger missing-arrow recovery: {diagnostics:?}"
    );
}

#[test]
fn test_parenthesized_destructuring_assignment_is_not_treated_as_missing_arrow() {
    let source = r#"
class C {
    constructor() {
        ({ x, y: y1, "y": y1 } = this);
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics
            .iter()
            .all(|d| !(d.code == 1005 && d.message.contains("'=>' expected"))),
        "Parenthesized destructuring assignments should not trigger missing-arrow recovery: {diagnostics:?}"
    );
}

#[test]
fn test_parenthesized_conditional_expression_is_not_treated_as_missing_arrow() {
    let source = r#"
var x: boolean = (true ? 1 : "");
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics
            .iter()
            .all(|d| !(d.code == 1005 && d.message.contains("';' expected"))),
        "Parenthesized ternaries should not trigger typed-arrow recovery: {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .all(|d| !(d.code == 1005 && d.message.contains("',' expected"))),
        "Parenthesized ternaries should not trigger comma recovery from typed-arrow parsing: {diagnostics:?}"
    );
}

#[test]
fn test_parenthesized_conditional_comma_expression_is_not_treated_as_missing_arrow() {
    let source = r#"
let xx: any;
xx = (xx ? 3 : 4, 10);
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    assert!(
        parser.get_diagnostics().is_empty(),
        "Conditional comma expressions should parse without missing-arrow recovery: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_regex_extended_unicode_escape_without_u_or_v_reports_ts1538() {
    let source = r#"
const regexes: RegExp[] = [
  /\u{10000}[\u{10000}]/,
  /\u{10000}[\u{10000}]/u,
  /\u{10000}[\u{10000}]/v,
];
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts1538_count = diagnostics
        .iter()
        .filter(|d| {
            d.code
                == diagnostic_codes::UNICODE_ESCAPE_SEQUENCES_ARE_ONLY_AVAILABLE_WHEN_THE_UNICODE_U_FLAG_OR_THE_UNICO
        })
        .count();

    assert_eq!(
        ts1538_count, 2,
        "Expected exactly two TS1538 diagnostics for regexes without /u or /v, got {diagnostics:?}"
    );
}

#[test]
fn test_regex_character_class_range_order_reports_ts1517() {
    let source = r#"
const regexes: RegExp[] = [
  /[𝘈-𝘡][𝘡-𝘈]/,
  /[𝘈-𝘡][𝘡-𝘈]/u,
  /[𝘈-𝘡][𝘡-𝘈]/v,

  /[\u{1D608}-\u{1D621}][\u{1D621}-\u{1D608}]/,
  /[\u{1D608}-\u{1D621}][\u{1D621}-\u{1D608}]/u,
  /[\u{1D608}-\u{1D621}][\u{1D621}-\u{1D608}]/v,

  /[\uD835\uDE08-\uD835\uDE21][\uD835\uDE21-\uD835\uDE08]/,
  /[\uD835\uDE08-\uD835\uDE21][\uD835\uDE21-\uD835\uDE08]/u,
  /[\uD835\uDE08-\uD835\uDE21][\uD835\uDE21-\uD835\uDE08]/v,
];
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts1517_count = diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::RANGE_OUT_OF_ORDER_IN_CHARACTER_CLASS)
        .count();

    assert_eq!(
        ts1517_count, 11,
        "Expected exactly eleven TS1517 diagnostics for out-of-order regex ranges, got {diagnostics:?}"
    );
}

#[test]
fn test_regex_character_class_escape_does_not_report_ts1517() {
    let source = r#"
/(#?-?\d*\.\d\w*%?)|(@?#?[\w-?]+%?)/g;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics
            .iter()
            .all(|d| d.code != diagnostic_codes::RANGE_OUT_OF_ORDER_IN_CHARACTER_CLASS),
        "Character class escapes like \\w should not trigger TS1517: {diagnostics:?}"
    );
}

#[test]
fn test_regex_missing_parenthesis_reports_ts1005_at_regex_end() {
    let source = "// @target: es2015\nvar x = /fo(o/;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let expected_pos = source.rfind('/').expect("unterminated regex slash") as u32;
    let ts1005 = diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::EXPECTED && d.message == "')' expected.")
        .collect::<Vec<_>>();

    assert_eq!(
        ts1005.len(),
        1,
        "Expected exactly one missing ')' diagnostic: {diagnostics:?}"
    );
    assert_eq!(ts1005[0].start, expected_pos);
}

#[test]
fn test_parenthesized_conditional_object_literal_true_branch_is_not_treated_as_missing_arrow() {
    let source = r#"
var value = (Math.random() ? {} : null);
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    assert!(
        parser.get_diagnostics().is_empty(),
        "Conditional branches with object literals should not trigger missing-arrow recovery: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parenthesized_arrow_block_tail_keeps_trailing_semicolon_error() {
    let source = "a = (() => { } || a)";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts1005: Vec<_> = diagnostics.iter().filter(|d| d.code == 1005).collect();
    let missing_close_paren = source.find("||").expect("operator position") as u32;
    let trailing_close_paren = source.rfind(')').expect("closing paren") as u32;

    assert_eq!(
        ts1005.len(),
        2,
        "Expected both TS1005 recovery diagnostics for malformed parenthesized arrow tail, got {diagnostics:?}"
    );
    assert!(
        ts1005
            .iter()
            .any(|diag| diag.start == missing_close_paren && diag.message == "')' expected."),
        "Expected TS1005 ') expected' at the binary tail, got {diagnostics:?}"
    );
    assert!(
        ts1005
            .iter()
            .any(|diag| diag.start == trailing_close_paren && diag.message == "';' expected."),
        "Expected TS1005 ';' expected at the trailing ')', got {diagnostics:?}"
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

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
fn test_parenthesized_divide_expression_before_block_is_not_treated_as_missing_arrow() {
    let source = "(a/8\n ){}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let block_pos = source.find('{').expect("block position") as u32;

    assert!(
        diagnostics
            .iter()
            .all(|diag| !(diag.code == 1005 && diag.message == "',' expected.")),
        "Parenthesized divide expressions should not trigger arrow-parameter comma recovery: {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .all(|diag| !(diag.code == 1005 && diag.message == "'=>' expected.")),
        "Parenthesized divide expressions should not trigger missing-arrow recovery: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().any(|diag| {
            diag.code == diagnostic_codes::EXPECTED
                && diag.start == block_pos
                && diag.message == "';' expected."
        }),
        "Expected the downstream ';' recovery at the block start, got {diagnostics:?}"
    );
}

#[test]
fn test_parameter_modifier_arrow_head_still_parses_as_arrow() {
    let source = "var v = (public x: string) => { };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    assert!(
        parser.get_diagnostics().is_empty(),
        "Parameter-modifier arrow heads should stay in arrow parsing: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_top_level_modifier_recovery_keeps_try_block_error() {
    let source = "cla <ss {\n  _ static try\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts1005: Vec<_> = diagnostics.iter().filter(|d| d.code == 1005).collect();
    let ts1434: Vec<_> = diagnostics.iter().filter(|d| d.code == 1434).collect();
    let ts1128: Vec<_> = diagnostics.iter().filter(|d| d.code == 1128).collect();

    assert_eq!(
        ts1005.len(),
        2,
        "Expected both the leading ';' recovery and the trailing try-block '{{' recovery, got {diagnostics:?}"
    );
    assert!(
        ts1005
            .iter()
            .any(|diag| diag.start == 8 && diag.message == "';' expected."),
        "Expected the leading ';' recovery at the stray '{{', got {diagnostics:?}"
    );
    assert!(
        ts1005
            .iter()
            .any(|diag| diag.start == 25 && diag.message == "'{' expected."),
        "Expected the downstream try-statement '{{' recovery at EOF, got {diagnostics:?}"
    );
    assert_eq!(
        ts1434.len(),
        1,
        "Expected the stray identifier recovery to remain, got {diagnostics:?}"
    );
    assert_eq!(
        ts1128.len(),
        1,
        "Expected the top-level modifier recovery to keep TS1128, got {diagnostics:?}"
    );
}

#[test]
fn test_object_literal_statement_recovery_after_shorthand_property() {
    let source = "var v = { a\nreturn;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let return_pos = source.find("return").expect("return position") as u32;
    let semicolon_pos = source.rfind(';').expect("semicolon position") as u32;
    assert!(
        diagnostics.iter().any(|diag| diag.code == 1005
            && diag.start == return_pos
            && diag.message == "',' expected."),
        "Expected missing comma at the statement keyword, got {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().any(|diag| diag.code == 1005
            && diag.start == semicolon_pos
            && diag.message == "':' expected."),
        "Expected missing ':' at the trailing semicolon, got {diagnostics:?}"
    );
    // tsc suppresses '}}' expected at EOF when a recent error (within 1 char)
    // already reported the issue. Matching that behavior here.
}

#[test]
fn test_object_literal_statement_recovery_after_missing_initializer() {
    let source = "var v = { a:\nreturn;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let return_pos = source.find("return").expect("return position") as u32;
    let semicolon_pos = source.rfind(';').expect("semicolon position") as u32;

    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == 1109 && diag.start == return_pos),
        "Expected TS1109 at the statement keyword after a missing initializer, got {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().all(|diag| !(diag.code == 1005
            && diag.start == return_pos
            && diag.message == "',' expected.")),
        "Missing initializer recovery should not inject a comma error at the next statement keyword: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().any(|diag| diag.code == 1005
            && diag.start == semicolon_pos
            && diag.message == "':' expected."),
        "Expected missing ':' at the trailing semicolon, got {diagnostics:?}"
    );
    // tsc suppresses '}}' expected at EOF when a recent error (within 1 char)
    // already reported the issue. Matching that behavior here.
}

#[test]
fn test_object_literal_statement_recovery_after_trailing_comma() {
    let source = "var v = { a: 1,\nreturn;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let return_pos = source.find("return").expect("return position") as u32;
    let semicolon_pos = source.rfind(';').expect("semicolon position") as u32;

    assert!(
        diagnostics.iter().all(|diag| !(diag.code == 1005
            && diag.start == return_pos
            && diag.message == "',' expected.")),
        "Trailing-comma recovery should not add an extra comma error at the next statement keyword: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().any(|diag| diag.code == 1005
            && diag.start == semicolon_pos
            && diag.message == "':' expected."),
        "Expected missing ':' at the trailing semicolon, got {diagnostics:?}"
    );
    // tsc suppresses '}}' expected at EOF when a recent error (within 1 char)
    // already reported the issue. Matching that behavior here.
}

#[test]
fn test_function_parameter_list_missing_close_paren_reports_at_body_end() {
    let source = "function f(a {\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let body_start = source.find('{').expect("body start") as u32;
    let body_end = source.rfind('}').expect("body end") as u32 + 1;
    let close_paren_diags: Vec<_> = diagnostics
        .iter()
        .filter(|diag| diag.code == 1005 && diag.message == "')' expected.")
        .collect();

    assert!(
        diagnostics.iter().any(|diag| diag.code == 1005
            && diag.start == body_start
            && diag.message == "',' expected."),
        "Expected missing comma at the body opener, got {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().any(|diag| diag.code == 1005
            && diag.start == body_end
            && diag.message == "')' expected."),
        "Expected missing ')' after the recovered body, got {diagnostics:?}"
    );
    assert_eq!(
        close_paren_diags.len(),
        1,
        "Expected only one missing ')' recovery diagnostic, got {diagnostics:?}"
    );
}

#[test]
fn test_missing_arrow_return_type_is_not_treated_as_typed_arrow() {
    let source = "var v = (a): => { };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let colon_pos = source.find(':').expect("colon position") as u32;
    let equals_pos = source.find("=>").expect("arrow position") as u32;

    assert!(
        diagnostics.iter().all(|diag| diag.code != 1110),
        "Missing arrow return types should not fall into TS1110 typed-arrow recovery: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().any(|diag| diag.code == 1005
            && diag.start == colon_pos
            && diag.message == "',' expected."),
        "Expected missing comma at ':', got {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().any(|diag| diag.code == 1005
            && diag.start == equals_pos
            && diag.message == "';' expected."),
        "Expected missing semicolon at '=>', got {diagnostics:?}"
    );
}

#[test]
fn test_array_literal_semicolon_recovers_as_missing_comma() {
    let source = "var texCoords = [2, 2, 0.5000001192092895, 0.8749999 ; 403953552, 0.5000001192092895, 0.8749999403953552];";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let semicolon_pos = source.find(';').expect("semicolon position") as u32;
    let close_bracket_pos = source.rfind(']').expect("close bracket position") as u32;

    assert!(
        diagnostics.iter().any(|diag| diag.code == 1005
            && diag.start == semicolon_pos
            && diag.message == "',' expected."),
        "Expected missing comma at the array literal semicolon, got {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().any(|diag| diag.code == 1005
            && diag.start == close_bracket_pos
            && diag.message == "';' expected."),
        "Expected trailing ';' recovery at the array close bracket, got {diagnostics:?}"
    );
}

#[test]
fn test_optional_rest_parameter_reports_at_question_mark() {
    let source = "(...arg?) => 102;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let question_pos = source.find('?').expect("question position") as u32;

    assert!(
        diagnostics.iter().any(|diag| {
            diag.code == 1047
                && diag.start == question_pos
                && diag.message == "A rest parameter cannot be optional."
        }),
        "Expected TS1047 at the question mark, got {diagnostics:?}"
    );
}

#[test]
fn test_reserved_word_type_reference_in_parameter_does_not_emit_ts1359() {
    let source = "class Foo { public banana(x: break) { } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();

    assert!(
        diagnostics.iter().all(|diag| diag.code != 1359),
        "Type positions should not reject reserved-word identifiers with TS1359: {diagnostics:?}"
    );
}

#[test]
fn test_variable_list_trailing_comma_reports_at_comma() {
    let source = "var a,\nreturn;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let comma_pos = source.find(',').expect("comma position") as u32;

    assert!(
        diagnostics.iter().any(|diag| {
            diag.code == 1009
                && diag.start == comma_pos
                && diag.message == "Trailing comma not allowed."
        }),
        "Expected TS1009 at the trailing comma, got {diagnostics:?}"
    );
}

#[test]
fn test_missing_function_parameter_comma_before_arrow_is_not_suppressed() {
    let source = "function (a => b;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let arrow_pos = source.find("=>").expect("arrow position") as u32;

    assert!(
        diagnostics.iter().any(|diag| {
            diag.code == 1005 && diag.start == arrow_pos && diag.message == "',' expected."
        }),
        "Expected missing comma at the arrow token, got {diagnostics:?}"
    );
}

#[test]
fn test_repeated_top_level_close_parens_emit_separate_ts1128() {
    let source = "function foo() {\n}\n\nfunction foo() {\n}\n\n)\n)";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts1128_count = diagnostics.iter().filter(|diag| diag.code == 1128).count();

    assert_eq!(
        ts1128_count, 2,
        "Expected one TS1128 per stray top-level close paren, got {diagnostics:?}"
    );
}

#[test]
fn test_named_tuple_member_rest_type_after_colon_does_not_emit_ts1005() {
    let source = r#"
type T = [first: string, rest: ...string[]?];
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    assert!(
        parser.get_diagnostics().iter().all(|d| d.code != 1005),
        "Named tuple members with a trailing '?' after the type should defer to later tuple diagnostics without TS1005: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_empty_element_access_reports_after_open_bracket() {
    let source = r#"
class Z {
 public x = "";
}

var a6: Z[][] = new   Z     [      ]   [  ];
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let ts1011_starts: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 1011)
        .map(|d| d.start)
        .collect();

    assert_eq!(
        ts1011_starts,
        vec![59, 70],
        "Empty element-access diagnostics should anchor immediately after '[' even with inner whitespace: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_export_type_alias_missing_type_at_eof_reports_after_equals() {
    let source = "import test from \"./test\";\nexport type test = \n";
    let mut parser = ParserState::new("types2.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let ts1110_starts: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_EXPECTED)
        .map(|d| d.start)
        .collect();
    let expected_start = source.find("= \n").unwrap() as u32 + 1;

    assert_eq!(
        ts1110_starts,
        vec![expected_start],
        "Missing export type bodies at EOF should anchor TS1110 after '=': {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parameters_with_line_break_no_comma() {
    // Function parameters without comma but with line break
    // Should be more permissive to avoid false positives
    let source = r"
function foo(
    a: number
    b: string
) {
    return a + b;
}
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should not emit TS1005 for missing comma when there's a line break
    let ts1005_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 1005)
        .count();
    assert!(
        ts1005_count <= 1,
        "Expected at most 1 TS1005 error, got {ts1005_count}",
    );
}

#[test]
fn test_interface_merging_no_duplicate() {
    // Interface merging should not emit TS2300
    let source = r"
interface Foo {
    a: number;
}
interface Foo {
    b: string;
}
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should not emit TS2300 for interface merging
    let ts2300_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 2300)
        .count();
    assert_eq!(
        ts2300_count, 0,
        "Expected no TS2300 errors for interface merging, got {ts2300_count}",
    );
}

#[test]
fn test_function_overloads_no_duplicate() {
    // Function overloads should not emit TS2300
    let source = r"
function foo(x: number): void;
function foo(x: string): void;
function foo(x: number | string): void {
    console.log(x);
}
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should not emit TS2300 for function overloads
    let ts2300_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 2300)
        .count();
    assert_eq!(
        ts2300_count, 0,
        "Expected no TS2300 errors for function overloads, got {ts2300_count}",
    );
}

#[test]
fn test_namespace_function_merging_no_duplicate() {
    // Namespace + function merging should not emit TS2300
    let source = r#"
namespace Utils {
    export function helper(): void {
        console.log("helper");
    }
}
function Utils() {
    console.log("constructor");
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should not emit TS2300 for namespace + function merging
    let ts2300_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 2300)
        .count();
    assert_eq!(
        ts2300_count, 0,
        "Expected no TS2300 errors for namespace+function merging, got {ts2300_count}",
    );
}

#[test]
fn test_asi_after_return() {
    // ASI (automatic semicolon insertion) should work after return
    let source = r"
function foo() {
    return
    42;
}
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should not emit TS1005 for missing semicolon after return with line break
    let ts1005_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 1005)
        .count();
    assert_eq!(
        ts1005_count, 0,
        "Expected no TS1005 errors for ASI after return, got {ts1005_count}",
    );
}

#[test]
fn test_trailing_comma_in_object_literal() {
    // Trailing commas should be allowed in object literals
    let source = r"
const obj = {
    a: 1,
    b: 2,
};
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should not emit any errors for trailing comma
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no errors for trailing comma in object literal, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_trailing_comma_in_array_literal() {
    // Trailing commas should be allowed in array literals
    let source = r"
const arr = [
    1,
    2,
    3,
];
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should not emit any errors for trailing comma
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no errors for trailing comma in array literal, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_trailing_comma_in_parameters() {
    // Trailing commas should be allowed in function parameters
    let source = r"
function foo(
    a: number,
    b: string,
) {
    return a + b;
}
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should not emit any errors for trailing comma in parameters
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no errors for trailing comma in parameters, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_interface_property_initializer_emits_ts1246() {
    let source = r"
interface I {
    x: number = 1;
}
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

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

// =============================================================================
// Primitive Type Keywords Tests
// =============================================================================

#[test]
fn test_void_return_type() {
    // void return type should be parsed correctly without TS1110/TS1109 errors
    let source = r"
declare function fn(arg0: boolean): void;
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should not emit any parser errors for void return type
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no parser errors for void return type, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_primitive_type_keywords() {
    // All primitive type keywords should be parsed correctly
    let source = r"
declare function fn1(): void;
declare function fn2(): string;
declare function fn3(): number;
declare function fn4(): boolean;
declare function fn5(): symbol;
declare function fn6(): bigint;
declare function fn7(): any;
declare function fn8(): unknown;
declare function fn9(): never;
declare function fn10(): null;
declare function fn11(): undefined;
declare function fn12(): object;
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should not emit any parser errors for primitive type keywords
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no parser errors for primitive type keywords, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_primitive_types_in_type_aliases() {
    // Primitive type keywords should work in type aliases
    let source = r"
type T1 = void;
type T2 = string;
type T3 = number;
type T4 = boolean;
type T5 = any;
type T6 = unknown;
type T7 = never;
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should not emit any parser errors
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no parser errors for primitive types in type aliases, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_primitive_types_in_parameters() {
    // Primitive type keywords should work in parameter types
    let source = r"
declare function fn(a: void, b: string, c: number): boolean;
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should not emit any parser errors
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no parser errors for primitive types in parameters, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_primitive_types_in_arrow_functions() {
    // Primitive type keywords should work in arrow function types
    let source = r#"
const arrow1: () => void = () => {};
const arrow2: (x: number) => string = (x) => "";
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should not emit any parser errors
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no parser errors for primitive types in arrow functions, got {:?}",
        parser.get_diagnostics()
    );
}

// =============================================================================
// Incremental Parsing Tests
// =============================================================================

#[test]
fn test_incremental_parse_from_middle_of_file() {
    // Test parsing from an offset in the middle of a source file
    let source = r"const a = 1;
const b = 2;
function foo() {
    return a + b;
}
const c = 3;";

    // Parse from the start of "function foo()"
    let offset = u32::try_from(
        source
            .find("function")
            .expect("pattern should exist in source"),
    )
    .expect("function offset should fit in u32");

    let mut parser = ParserState::new("test.ts".to_string(), String::new());
    let result = parser.parse_source_file_statements_from_offset(
        "test.ts".to_string(),
        source.to_string(),
        offset,
    );

    // Should have parsed the remaining statements (function and const c)
    let statement_count = result.statements.len();
    assert!(
        statement_count >= 2,
        "Expected at least 2 statements from offset, got {statement_count}",
    );

    // Should not produce errors for valid code
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no errors for incremental parse, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_incremental_parse_from_start() {
    // Test incremental parsing from offset 0 (should be equivalent to full parse)
    let source = r#"const x = 42;
let y = "hello";"#;

    let mut parser = ParserState::new("test.ts".to_string(), String::new());
    let result = parser.parse_source_file_statements_from_offset(
        "test.ts".to_string(),
        source.to_string(),
        0,
    );

    // Should have parsed both statements
    let statement_count = result.statements.len();
    assert_eq!(
        statement_count, 2,
        "Expected 2 statements, got {statement_count}",
    );

    // reparse_start should be 0
    assert_eq!(result.reparse_start, 0);

    // Should not produce errors
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no errors, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_incremental_parse_from_end() {
    // Test incremental parsing from beyond the end of file
    let source = "const x = 1;";

    let mut parser = ParserState::new("test.ts".to_string(), String::new());
    let result = parser.parse_source_file_statements_from_offset(
        "test.ts".to_string(),
        source.to_string(),
        1000, // Beyond EOF
    );

    // Should handle gracefully - clamped to source length
    assert!(
        result.statements.is_empty(),
        "Expected no statements when starting at EOF"
    );
}

#[test]
fn test_incremental_parse_records_reparse_start() {
    // Test that reparse_start is recorded correctly
    let source = "const a = 1;\nconst b = 2;";
    let offset = 13u32; // Start of "const b"

    let mut parser = ParserState::new("test.ts".to_string(), String::new());
    let result = parser.parse_source_file_statements_from_offset(
        "test.ts".to_string(),
        source.to_string(),
        offset,
    );

    // reparse_start should match the offset we provided
    let reparse_start = result.reparse_start;
    assert_eq!(
        reparse_start, offset,
        "Expected reparse_start to be {offset}, got {reparse_start}",
    );
}

#[test]
fn test_incremental_parse_with_syntax_error() {
    // Test incremental parsing recovers from syntax errors
    let source = r"const a = 1;
const b = ;
const c = 3;";

    // Parse from start of "const b = ;" (syntax error)
    let offset = u32::try_from(
        source
            .find("const b")
            .expect("pattern should exist in source"),
    )
    .expect("const b offset should fit in u32");

    let mut parser = ParserState::new("test.ts".to_string(), String::new());
    let result = parser.parse_source_file_statements_from_offset(
        "test.ts".to_string(),
        source.to_string(),
        offset,
    );

    // Should still parse statements (with recovery)
    let statement_count = result.statements.len();
    assert!(
        !result.statements.is_empty(),
        "Expected at least 1 statement after recovery, got {statement_count}",
    );

    // Should produce an error for the syntax issue
    assert!(
        !parser.get_diagnostics().is_empty(),
        "Expected at least one diagnostic for syntax error"
    );
}

// =============================================================================
// Conditional Type ASI Tests
// =============================================================================

#[test]
fn test_interface_extends_property_with_asi() {
    // 'extends' as a property name in interface with ASI (no semicolons)
    // Should NOT parse as conditional type
    let source = r"
interface JSONSchema4 {
  a?: number
  extends?: string | string[]
}
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diags = parser.get_diagnostics();
    assert!(
        diags.is_empty(),
        "Expected no parser errors for 'extends' property with ASI, got {diags:?}",
    );
}

// =============================================================================
// Expression Statement Recovery Tests
// =============================================================================

#[test]
fn test_incomplete_binary_expression_recovery() {
    // Test recovery from incomplete binary expression: a +
    let source = r"const result = a +;
const next = 1;";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should produce an error for missing RHS
    let has_error = !parser.get_diagnostics().is_empty();
    assert!(has_error, "Expected error for incomplete binary expression");

    // Parser should recover and continue parsing
    // The error count should be limited (no cascading errors)
    let error_count = parser.get_diagnostics().len();
    assert!(
        error_count <= 2,
        "Expected at most 2 errors for recovery, got {error_count}",
    );
}

#[test]
fn test_incomplete_assignment_recovery() {
    // Test recovery from incomplete assignment: x =
    let source = r"let x =;
let y = 2;";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should produce an error for missing RHS
    assert!(
        !parser.get_diagnostics().is_empty(),
        "Expected error for incomplete assignment"
    );

    // Parser should recover - not too many errors
    let error_count = parser.get_diagnostics().len();
    assert!(
        error_count <= 2,
        "Expected at most 2 errors after recovery, got {error_count}",
    );
}

#[test]
fn test_incomplete_conditional_expression_recovery() {
    // Test recovery from incomplete conditional: a ? b :
    let source = r"const result = a ? b :;
const next = 1;";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should produce error for missing false branch
    assert!(
        !parser.get_diagnostics().is_empty(),
        "Expected error for incomplete conditional"
    );
}

#[test]
fn test_expression_recovery_at_statement_boundary() {
    // Test that parser properly recovers at statement boundaries
    let source = r"const a = 1 +
const b = 2;";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should have errors but recover for next statement
    assert!(
        !parser.get_diagnostics().is_empty(),
        "Expected error for incomplete expression"
    );
}

#[test]
fn test_expression_recovery_preserves_valid_code() {
    // Test that valid code after error is still parsed correctly
    let source = r"const bad = ;
function validFunction() {
    return 42;
}";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should have error for bad assignment
    assert!(
        !parser.get_diagnostics().is_empty(),
        "Expected error for invalid assignment"
    );

    // Error count should be limited
    let error_count = parser.get_diagnostics().len();
    assert!(
        error_count <= 2,
        "Expected limited errors with recovery, got {error_count}",
    );
}

// =============================================================================
// Import Type Tests
// =============================================================================

#[test]
fn test_typeof_import_with_member_access() {
    // typeof import("...").A.foo should parse without TS1005
    // This is a valid TypeScript syntax for accessing static members
    let source = r#"
export const foo: typeof import("./a").A.foo;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should not emit TS1005 for member access after import()
    let ts1005_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 1005)
        .count();
    assert_eq!(
        ts1005_count, 0,
        "Expected no TS1005 errors for typeof import with member access, got {ts1005_count}",
    );

    // Should have no errors at all
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no parser errors for typeof import with member access, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_typeof_import_with_nested_member_access() {
    // typeof import("...").A.B.C should parse correctly
    let source = r#"
export const foo: typeof import("./module").A.B.C;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should not emit any errors for nested member access after import()
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no parser errors for typeof import with nested member access, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_typeof_import_without_member_access() {
    // typeof import("...") without member access should still work
    let source = r#"
export const foo: typeof import("./module");
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should not emit any errors
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no parser errors for typeof import without member access, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_import_type_without_typeof() {
    // import("...").Type should parse without typeof
    let source = r#"
export const a: import("./test1").T = null as any;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should not emit parse errors
    let ts1005_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 1005)
        .count();
    let ts1109_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 1109)
        .count();
    let ts1359_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 1359)
        .count();

    assert_eq!(
        ts1005_count, 0,
        "Expected no TS1005 errors for import type, got {ts1005_count}",
    );
    assert_eq!(
        ts1109_count, 0,
        "Expected no TS1109 errors for import type, got {ts1109_count}",
    );
    assert_eq!(
        ts1359_count, 0,
        "Expected no TS1359 errors for import type, got {ts1359_count}",
    );
}

#[test]
fn test_import_type_with_member_access() {
    // import("...").Type.SubType should parse correctly
    let source = r#"
export const a: import("./test1").T.U = null as any;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should not emit parse errors
    assert!(
        parser.get_diagnostics().iter().all(|d| d.code >= 2000),
        "Expected no parser errors (1xxx) for import type with member access, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_import_type_with_generic_arguments() {
    // import("...").Type<T> should parse correctly
    let source = r#"
export const a: import("./test1").T<typeof import("./test2").theme> = null as any;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should not emit parse errors
    let parse_errors = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code < 2000)
        .count();
    assert_eq!(
        parse_errors,
        0,
        "Expected no parser errors for import type with generics, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_import_type_with_invalid_import_attribute_key_reports_ts1478() {
    let source = r#"
const a = (null as any as import("pkg", { with: {1234, "resolution-mode": "require"} }).RequireInterface);
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&diagnostic_codes::IDENTIFIER_OR_STRING_LITERAL_EXPECTED),
        "Expected TS1478 for invalid import-attribute key, got {:?}",
        parser.get_diagnostics()
    );
    assert!(
        codes.contains(&diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED),
        "Expected tail recovery to surface TS1128 diagnostics, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_type_argument_with_empty_jsdoc_wildcard_has_no_ts1110() {
    // `Foo<?>` should emit TS8020 but avoid TS1110 cascading.
    let source = r#"
type T = Foo<?>;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert!(
        diagnostics.contains(
            &diagnostic_codes::JSDOC_TYPES_CAN_ONLY_BE_USED_INSIDE_DOCUMENTATION_COMMENTS
        ),
        "Expected TS8020 for `Foo<?>`, got {:?}",
        parser.get_diagnostics(),
    );
    assert!(
        !diagnostics.contains(&diagnostic_codes::TYPE_EXPECTED),
        "Expected no TS1110 for `Foo<?>`, got {:?}",
        parser.get_diagnostics(),
    );
}

#[test]
fn test_type_argument_with_jsdoc_prefix_type_emits_ts17020() {
    // `Foo<?string>` should still emit TS17020 for the JSDoc-style leading '?'
    // plus TS8020 because the syntax is documentation-only.
    let source = r#"
type T = Foo<?string>;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert!(
        diagnostics.contains(
            &diagnostic_codes::JSDOC_TYPES_CAN_ONLY_BE_USED_INSIDE_DOCUMENTATION_COMMENTS
        ),
        "Expected TS8020 for `Foo<?string>`, got {:?}",
        parser.get_diagnostics(),
    );
    assert!(
        diagnostics.contains(&diagnostic_codes::AT_THE_START_OF_A_TYPE_IS_NOT_VALID_TYPESCRIPT_SYNTAX_DID_YOU_MEAN_TO_WRITE),
        "Expected TS17020 for `Foo<?string>`, got {:?}",
        parser.get_diagnostics(),
    );
}

#[test]
fn test_old_jsdoc_qualified_name_generic_reports_ts8020() {
    // Old JSDoc generic syntax `Array.<T>` should recover with TS8020.
    let source = r#"
type T = Array.<string>;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert!(
        diagnostics.contains(
            &diagnostic_codes::JSDOC_TYPES_CAN_ONLY_BE_USED_INSIDE_DOCUMENTATION_COMMENTS
        ),
        "Expected TS8020 for `Array.<string>`, got {:?}",
        parser.get_diagnostics(),
    );
    assert!(
        !diagnostics.contains(&diagnostic_codes::IDENTIFIER_EXPECTED),
        "Expected no TS1003 fallback for `Array.<string>`, got {:?}",
        parser.get_diagnostics(),
    );
    assert!(
        !diagnostics.contains(&diagnostic_codes::TYPE_EXPECTED),
        "Expected no TS1110 fallback for `Array.<string>`, got {:?}",
        parser.get_diagnostics(),
    );
}

#[test]
fn test_jsdoc_legacy_function_type_reports_ts8020_without_parse_cascade() {
    let source = r#"
function hof(ctor: function(new: number, string)) {
    return new ctor('hi');
}

function hof2(f: function(this: number, string): string) {
    return f(12, 'hullo');
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();

    let ts8020_count = diagnostics
        .iter()
        .filter(|code| {
            **code == diagnostic_codes::JSDOC_TYPES_CAN_ONLY_BE_USED_INSIDE_DOCUMENTATION_COMMENTS
        })
        .count();
    assert_eq!(
        ts8020_count,
        2,
        "Expected TS8020 for both legacy function types, got {:?}",
        parser.get_diagnostics()
    );

    assert!(
        diagnostics.contains(&2554),
        "Expected TS2554 from bad call with `this` signature, got {:?}",
        parser.get_diagnostics()
    );

    assert!(
        !diagnostics
            .iter()
            .any(|code| *code == 1003 || *code == 1005 || *code == 1109),
        "Did not expect parser-level recovery diagnostics for legacy function types, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_jsdoc_wildcard_type_reports_ts8020_only() {
    let source = r"
let whatevs: * = 1001;
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert_eq!(
        diagnostics,
        vec![diagnostic_codes::JSDOC_TYPES_CAN_ONLY_BE_USED_INSIDE_DOCUMENTATION_COMMENTS],
        "Expected only TS8020 for wildcard type, got {:?}",
        parser.get_diagnostics()
    );
}

// =============================================================================
// Tuple Type Tests
// =============================================================================

#[test]
fn test_optional_tuple_element() {
    // [T?] should parse correctly without TS1005/TS1110
    let source = r"
interface Buzz { id: number; }
type T = [Buzz?];
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should not emit any parser errors
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no parser errors for mixed tuple elements, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_argument_list_recovery_on_return_keyword() {
    let source = r"
const x = fn(
  return
);
const y = 1;
";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts1135_count = diagnostics.iter().filter(|d| d.code == 1135).count();
    let ts1005_count = diagnostics.iter().filter(|d| d.code == 1005).count();

    assert!(
        ts1135_count >= 1,
        "Expected at least 1 TS1135 for malformed argument list, got diagnostics: {diagnostics:?}"
    );
    assert!(
        ts1005_count <= 2,
        "Expected limited TS1005 cascade for malformed argument list, got {ts1005_count} diagnostics: {diagnostics:?}",
    );
}

#[test]
fn test_invalid_unicode_escape_in_var_no_extra_semicolon_error() {
    let source = r"var arg\uxxxx";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts1127_count = diagnostics.iter().filter(|d| d.code == 1127).count();
    let ts1005_count = diagnostics.iter().filter(|d| d.code == 1005).count();

    assert!(
        ts1127_count >= 1,
        "Expected TS1127 for invalid unicode escape, got diagnostics: {diagnostics:?}"
    );
    assert_eq!(
        ts1005_count, 0,
        "Expected no extra TS1005 for invalid unicode escape, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_invalid_unicode_escape_as_variable_name_no_var_decl_cascade() {
    let source = r"var \u0031a;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts1127_count = diagnostics.iter().filter(|d| d.code == 1127).count();
    let ts1123_count = diagnostics.iter().filter(|d| d.code == 1123).count();
    let ts1134_count = diagnostics.iter().filter(|d| d.code == 1134).count();

    assert!(
        ts1127_count >= 1,
        "Expected TS1127 for invalid unicode escape, got diagnostics: {diagnostics:?}"
    );
    assert_eq!(
        ts1123_count, 0,
        "Expected no TS1123 variable declaration cascade, got diagnostics: {diagnostics:?}"
    );
    assert_eq!(
        ts1134_count, 0,
        "Expected no TS1134 variable declaration cascade, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_class_method_string_names_use_string_literal_nodes() {
    let source = r#"
class C {
    "foo"();
    "bar"() { }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let source_file = parser.get_arena().get_source_file_at(root).unwrap();
    let class_idx = source_file.statements.nodes[0];
    let class_node = parser.get_arena().get(class_idx).unwrap();
    let class_data = parser.get_arena().get_class(class_node).unwrap();
    let kinds: Vec<_> = class_data
        .members
        .nodes
        .iter()
        .filter_map(|&member_idx| {
            let member_node = parser.get_arena().get(member_idx)?;
            (member_node.kind == crate::parser::syntax_kind_ext::METHOD_DECLARATION).then_some({
                let method = parser.get_arena().get_method_decl(member_node)?;
                let name_node = parser.get_arena().get(method.name)?;
                (
                    method.name,
                    name_node.kind,
                    parser
                        .get_arena()
                        .get_literal(name_node)
                        .map(|lit| lit.text.clone()),
                )
            })
        })
        .collect();

    assert_eq!(kinds.len(), 2);
    for (_name_idx, kind, text) in kinds {
        assert_eq!(
            kind,
            tsz_scanner::SyntaxKind::StringLiteral as u16,
            "expected string literal name node"
        );
        assert!(text.is_some());
    }
}

// =============================================================================
// Yield Expression Tests
// =============================================================================

#[test]
fn test_yield_after_type_assertion_requires_parens() {
    // yield without parentheses after type assertion should emit TS1109
    let source = r"
function* f() {
    <number> yield 0;
}
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts1109_count = diagnostics.iter().filter(|d| d.code == 1109).count();

    assert_eq!(
        ts1109_count, 1,
        "Expected 1 TS1109 error for yield without parens after type assertion, got {ts1109_count}. Diagnostics: {diagnostics:?}",
    );

    // Check that the error mentions expression
    let has_expression_expected = diagnostics
        .iter()
        .any(|d| d.code == 1109 && d.message.to_lowercase().contains("expression"));
    assert!(
        has_expression_expected,
        "Expected TS1109 error to mention 'expression', got diagnostics: {diagnostics:?}",
    );
}

#[test]
fn test_yield_with_parens_after_type_assertion_is_valid() {
    // yield with parentheses after type assertion should be valid
    let source = r"
function* f() {
    <number> (yield 0);
}
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();

    // Should not emit TS1109 for yield in parentheses
    let ts1109_count = diagnostics.iter().filter(|d| d.code == 1109).count();
    assert_eq!(
        ts1109_count, 0,
        "Expected no TS1109 errors for yield with parens, got {ts1109_count}. Diagnostics: {diagnostics:?}",
    );
}

#[test]
fn test_generator_recovery_keeps_yield_statement_after_broken_initializer() {
    let source = r"
function* f() {
    )
    yield 1;
    const ok = 2;
}
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let source_file = parser.get_arena().get_source_file_at(root).unwrap();
    let function_idx = source_file.statements.nodes[0];
    let function_node = parser.get_arena().get(function_idx).unwrap();
    let function_data = parser.get_arena().get_function(function_node).unwrap();
    let body = parser.get_arena().get_block_at(function_data.body).unwrap();

    assert!(!parser.get_diagnostics().is_empty());
    assert_eq!(
        body.statements.nodes.len(),
        2,
        "Expected parser recovery to keep yield statement after an invalid token"
    );

    let yield_stmt_node = parser
        .get_arena()
        .get(body.statements.nodes[0])
        .expect("expected yield statement in generator body");
    assert_eq!(
        yield_stmt_node.kind,
        crate::parser::syntax_kind_ext::EXPRESSION_STATEMENT,
        "Expected first recovered statement to be an expression statement containing yield"
    );

    let yield_stmt_data = parser
        .get_arena()
        .get_expression_statement(yield_stmt_node)
        .expect("expected expression statement data for recovered yield statement");
    let yield_expr_node = parser
        .get_arena()
        .get(yield_stmt_data.expression)
        .expect("expected recovered yield expression node");
    let yield_text = &source[yield_expr_node.pos as usize..yield_expr_node.end as usize];
    assert!(
        yield_text.trim_start().starts_with("yield"),
        "Expected recovered statement text to start with `yield`, got: {yield_text:?}"
    );
}

// =============================================================================
// Orphan Catch/Finally Tests
// =============================================================================

#[test]
fn test_orphan_catch_block_emits_ts1005() {
    // catch block without try should emit TS1005: 'try' expected
    let source = r"
function fn() {
    catch(x) { }
}
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts1005_count = diagnostics.iter().filter(|d| d.code == 1005).count();

    assert_eq!(
        ts1005_count, 1,
        "Expected 1 TS1005 error for orphan catch block, got {ts1005_count}. Diagnostics: {diagnostics:?}",
    );

    // Check that the error message mentions 'try'
    let has_try_expected = diagnostics
        .iter()
        .any(|d| d.code == 1005 && d.message.to_lowercase().contains("try"));
    assert!(
        has_try_expected,
        "Expected TS1005 error to mention 'try', got diagnostics: {diagnostics:?}",
    );
}

#[test]
fn test_orphan_finally_block_emits_ts1005() {
    // finally block without try should emit TS1005: 'try' expected
    let source = r"
function fn() {
    finally { }
}
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts1005_count = diagnostics.iter().filter(|d| d.code == 1005).count();

    assert_eq!(
        ts1005_count, 1,
        "Expected 1 TS1005 error for orphan finally block, got {ts1005_count}. Diagnostics: {diagnostics:?}",
    );

    // Check that the error message mentions 'try'
    let has_try_expected = diagnostics
        .iter()
        .any(|d| d.code == 1005 && d.message.to_lowercase().contains("try"));
    assert!(
        has_try_expected,
        "Expected TS1005 error to mention 'try', got diagnostics: {diagnostics:?}",
    );
}

#[test]
fn test_multiple_orphan_blocks_emit_separate_ts1005() {
    // Multiple orphan catch/finally blocks should each emit TS1005
    let source = r"
function fn() {
    finally { }
    catch (x) { }
}
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts1005_count = diagnostics.iter().filter(|d| d.code == 1005).count();

    assert_eq!(
        ts1005_count, 2,
        "Expected 2 TS1005 errors for two orphan blocks, got {ts1005_count}. Diagnostics: {diagnostics:?}",
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts1131_count = diagnostics.iter().filter(|d| d.code == 1131).count();
    assert!(
        ts1131_count >= 1,
        "Expected at least 1 TS1131 for invalid type literal member, got {ts1131_count}. Diagnostics: {diagnostics:?}",
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts1131_count = diagnostics.iter().filter(|d| d.code == 1131).count();
    assert_eq!(
        ts1131_count, 0,
        "Expected no TS1131 for valid interface, got {ts1131_count}. Diagnostics: {diagnostics:?}",
    );
}

// =============================================================================
// Import Defer Tests
// =============================================================================

#[test]
fn test_import_defer_namespace_parses_clean() {
    // `import defer * as ns from "mod"` is valid — no parse errors
    let source = r#"import defer * as ns from "./a";"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let parse_errors: Vec<_> = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code < 2000)
        .collect();
    assert!(
        parse_errors.is_empty(),
        "Expected no parse errors for valid defer namespace import, got {parse_errors:?}",
    );
}

#[test]
fn test_import_defer_as_binding_name() {
    // `import defer from "mod"` — defer is the default import NAME, not a modifier
    let source = r#"import defer from "./a";"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let parse_errors: Vec<_> = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code < 2000)
        .collect();
    assert!(
        parse_errors.is_empty(),
        "Expected no parse errors when 'defer' is used as binding name, got {parse_errors:?}",
    );
}

#[test]
fn test_import_dot_defer_call_no_parse_error() {
    // `import.defer("./a")` — valid dynamic defer import, no parse error
    let source = r#"import.defer("./a.js");"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let parse_errors: Vec<_> = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code < 2000)
        .collect();
    assert!(
        parse_errors.is_empty(),
        "Expected no parse errors for import.defer() call, got {parse_errors:?}",
    );
}

#[test]
fn test_import_dot_defer_standalone_emits_ts1005() {
    // `import.defer` without () should emit TS1005 "'(' expected."
    let source = r"const x = import.defer;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let ts1005_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 1005)
        .count();
    assert_eq!(
        ts1005_count, 1,
        "Expected 1 TS1005 for standalone import.defer, got {ts1005_count}",
    );
}

#[test]
fn test_import_dot_invalid_meta_property_ts17012() {
    // `import.foo` (not in call) should emit TS17012
    let source = r"const x = import.foo;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let ts17012_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 17012)
        .count();
    assert_eq!(
        ts17012_count, 1,
        "Expected 1 TS17012 for invalid import.foo, got {ts17012_count}",
    );
}

#[test]
fn test_import_dot_invalid_meta_property_call_ts18061() {
    // `import.foo()` (in call) should emit TS18061
    let source = r#"import.foo("./a");"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let ts18061_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 18061)
        .count();
    assert_eq!(
        ts18061_count, 1,
        "Expected 1 TS18061 for import.foo() call, got {ts18061_count}",
    );
}

#[test]
fn test_import_defer_with_default_sets_deferred_flag() {
    // `import defer foo from "./a"` — defer is modifier, foo is default name
    // Parser should set is_deferred = true
    let source = r#"import defer foo from "./a";"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let sf = arena.get_source_file_at(root).unwrap();
    let stmt = sf.statements.nodes[0];
    let stmt_node = arena.get(stmt).unwrap();
    let import = arena.get_import_decl(stmt_node).unwrap();
    let clause_node = arena.get(import.import_clause).unwrap();
    let clause = arena.get_import_clause(clause_node).unwrap();
    assert!(
        clause.is_deferred,
        "Expected is_deferred to be true for 'import defer foo from'"
    );
    assert!(
        clause.name.is_some(),
        "Expected default import name to be present"
    );
}

#[test]
fn test_import_defer_from_as_name_not_deferred() {
    // `import defer from "./a"` — defer is the import NAME, not modifier
    // Parser should NOT set is_deferred = true
    let source = r#"import defer from "./a";"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let sf = arena.get_source_file_at(root).unwrap();
    let stmt = sf.statements.nodes[0];
    let stmt_node = arena.get(stmt).unwrap();
    let import = arena.get_import_decl(stmt_node).unwrap();
    let clause_node = arena.get(import.import_clause).unwrap();
    let clause = arena.get_import_clause(clause_node).unwrap();
    assert!(
        !clause.is_deferred,
        "Expected is_deferred to be false for 'import defer from' (defer is name)"
    );
}

// =============================================================================
// Bare Hash Character Recovery (TS1127)
// =============================================================================

#[test]
fn test_bare_hash_at_top_level_emits_ts1127() {
    // Bare `#` at top level should emit TS1127, not cascading errors
    let source = "# foo";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts1127_count = diagnostics.iter().filter(|d| d.code == 1127).count();
    assert!(
        ts1127_count >= 1,
        "Expected TS1127 for bare '#', got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_bare_hash_in_class_emits_ts1127() {
    // Bare `#` in class body should emit TS1127, not cascading errors
    let source = r"
class C {
    # name;
}
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts1127_count = diagnostics.iter().filter(|d| d.code == 1127).count();
    assert!(
        ts1127_count >= 1,
        "Expected TS1127 for bare '#' in class body, got diagnostics: {diagnostics:?}"
    );
    // Should NOT cascade into TS1003/TS1005/TS1068/TS1128
    let cascade_count = diagnostics
        .iter()
        .filter(|d| matches!(d.code, 1003 | 1005 | 1068 | 1128))
        .count();
    assert_eq!(
        cascade_count, 0,
        "Bare '#' should not cascade into other errors, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_valid_private_name_no_ts1127() {
    // Valid private names should not emit TS1127
    let source = r"
class C {
    #name = 42;
    get #value() { return this.#name; }
}
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts1127_count = diagnostics.iter().filter(|d| d.code == 1127).count();
    assert_eq!(
        ts1127_count, 0,
        "Valid private names should not emit TS1127, got diagnostics: {diagnostics:?}"
    );
}

// =============================================================================
// Nullable Type Syntax Recovery (TS17019/TS17020)
// =============================================================================

#[test]
fn test_postfix_question_emits_ts17019() {
    // `string?` should emit TS17019, not TS1005 or TS1110
    let source = "let x: string?;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts17019_count = diagnostics.iter().filter(|d| d.code == 17019).count();
    assert!(
        ts17019_count >= 1,
        "Expected TS17019 for postfix '?' on type, got diagnostics: {diagnostics:?}"
    );
    // Should NOT emit TS1005 or TS1110 cascade
    let ts1005_count = diagnostics.iter().filter(|d| d.code == 1005).count();
    let ts1110_count = diagnostics.iter().filter(|d| d.code == 1110).count();
    assert_eq!(
        ts1005_count, 0,
        "Should not emit TS1005 for nullable type, got diagnostics: {diagnostics:?}"
    );
    assert_eq!(
        ts1110_count, 0,
        "Should not emit TS1110 for nullable type, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_prefix_question_emits_ts17020() {
    // `?string` should emit TS17020, not TS1110
    let source = "let x: ?string;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts17020_count = diagnostics.iter().filter(|d| d.code == 17020).count();
    assert!(
        ts17020_count >= 1,
        "Expected TS17020 for prefix '?' on type, got diagnostics: {diagnostics:?}"
    );
    // Should NOT emit TS1110 cascade
    let ts1110_count = diagnostics.iter().filter(|d| d.code == 1110).count();
    assert_eq!(
        ts1110_count, 0,
        "Should not emit TS1110 for nullable type, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_multiple_nullable_types() {
    // Multiple nullable types in different positions
    let source = r"
function f(x: string?): ?number {
    return null;
}
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts17019_count = diagnostics.iter().filter(|d| d.code == 17019).count();
    let ts17020_count = diagnostics.iter().filter(|d| d.code == 17020).count();
    assert!(
        ts17019_count >= 1,
        "Expected at least 1 TS17019 for postfix '?', got diagnostics: {diagnostics:?}"
    );
    assert!(
        ts17020_count >= 1,
        "Expected at least 1 TS17020 for prefix '?', got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_nullable_type_in_type_predicate() {
    // `x is ?string` should emit TS17020
    let source = "function f(x: any): x is ?string { return true; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts17020_count = diagnostics.iter().filter(|d| d.code == 17020).count();
    assert!(
        ts17020_count >= 1,
        "Expected TS17020 for '?string' in type predicate, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_nullable_type_no_cascade() {
    // Nullable type should not cause cascading errors
    let source = r#"
let a: string? = "hello";
let b: ?number = 42;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    // Should only have TS17019 and TS17020, no cascade
    let cascade_codes: Vec<u32> = diagnostics
        .iter()
        .filter(|d| d.code == 1005 || d.code == 1109 || d.code == 1110 || d.code == 1128)
        .map(|d| d.code)
        .collect();
    assert!(
        cascade_codes.is_empty(),
        "Nullable types should not cause cascading errors, got: {cascade_codes:?}. All: {diagnostics:?}"
    );
}

#[test]
fn test_adjacent_jsx_roots_in_tsx_report_ts2657() {
    let source = r"
declare namespace JSX { interface Element { } }

<div></div>
<div></div>

var x = <div></div><div></div>
";
    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts2657_count = diagnostics.iter().filter(|d| d.code == 2657).count();
    let ts1003_count = diagnostics.iter().filter(|d| d.code == 1003).count();
    let ts1109_count = diagnostics.iter().filter(|d| d.code == 1109).count();

    // tsc emits TS2657 for adjacent JSX roots in ALL JSX files (.tsx, .jsx, .js)
    assert!(
        ts2657_count >= 1,
        "Expected TS2657 for adjacent JSX siblings in TSX, got diagnostics: {diagnostics:?}"
    );
    assert_eq!(
        ts1003_count, 0,
        "Adjacent JSX recovery should not leak TS1003, got diagnostics: {diagnostics:?}"
    );
    assert_eq!(
        ts1109_count, 0,
        "Adjacent JSX recovery should not leak TS1109, got diagnostics: {diagnostics:?}"
    );
}

#[test]
#[ignore] // TODO: JSX type argument recovery in JS files not yet implemented
fn test_jsx_type_arguments_in_js_report_ts2657() {
    let source = r#"
/// <reference path="/.lib/react.d.ts" />
import { MyComp, Prop } from "./component";
import * as React from "react";

let x = <MyComp<Prop> a={10} b="hi" />; // error, no type arguments in js
"#;
    let mut parser = ParserState::new("file.jsx".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&2657),
        "Expected TS2657 for JSX type arguments in JS recovery, got diagnostics: {diagnostics:?}"
    );
    assert!(
        codes.contains(&1003),
        "Expected TS1003 alongside TS2657 for illegal JSX type-argument syntax, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_js_call_type_argument_syntax_prefers_relational_parsing() {
    let source = r#"
Foo<number>();
Foo<number>(1);
Foo<number>``;
"#;
    let mut parser = ParserState::new("a.jsx".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts1109_count = diagnostics.iter().filter(|d| d.code == 1109).count();
    let ts1003_count = diagnostics.iter().filter(|d| d.code == 1003).count();

    assert_eq!(
        ts1109_count, 1,
        "Expected only the empty-call JS generic syntax case to emit TS1109, got diagnostics: {diagnostics:?}"
    );
    assert_eq!(
        ts1003_count, 0,
        "Non-JSX JS generic-call syntax should not leak JSX TS1003 recovery diagnostics: {diagnostics:?}"
    );
}

#[test]
#[ignore] // TODO: JSX type argument closing tag recovery in JS files not yet implemented
fn test_jsx_type_arguments_in_js_with_closing_tag_report_ts17002() {
    let source = r#"
<Foo<number>></Foo>;
"#;
    let mut parser = ParserState::new("a.jsx".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&17002),
        "Expected TS17002 for the mismatched closing tag after JS JSX type-argument recovery, got diagnostics: {diagnostics:?}"
    );
    assert!(
        codes.contains(&2657),
        "Expected TS2657 for the recovered adjacent JSX roots, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_let_array_ambiguity_reports_ts1181_then_statement_recovery() {
    let source = r#"
var let: any;
let[0] = 100;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert_eq!(
        codes,
        vec![1181, 1005, 1128],
        "Expected TS1181/TS1005/TS1128 recovery for ambiguous `let[` statement, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_for_header_let_disambiguation_matches_invalid_for_of_recovery() {
    let source = r#"
var let = 10;
for (let of [1,2,3]) {}

for (let in [1,2,3]) {}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert_eq!(
        codes,
        vec![1005, 1181, 1005, 1128],
        "Expected TS1005/TS1181/TS1005/TS1128 recovery for `for (let of [...])`, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_invalid_nonnullable_type_recovery_reports_ts17019_and_ts17020() {
    let source = r#"
function f1(a: string): a is string! { return true; }
function f2(a: string): a is !string { return true; }
const a = 1 as any!;
const b = 1 as !any;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert_eq!(
        codes,
        vec![17019, 17020, 17019, 17020],
        "Expected TS17019/TS17020 recovery for invalid non-nullable type syntax, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_unclosed_jsx_fragment_after_unary_plus_in_tsx_suppresses_ts17014() {
    let source = r#"
const x = "oops";
const y = + <> x;
"#;
    let mut parser = ParserState::new("index.tsx".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert!(
        !codes.contains(&17014),
        "Expected TSX unary `+ <>` recovery to avoid TS17014, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_js_unclosed_jsx_fragment_after_unary_plus_reports_ts17014() {
    let source = r#"
const x = "oops";
const y = + <> x;
"#;
    let mut parser = ParserState::new("index.js".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&17014),
        "Expected TS17014 for JS unary `+ <>` JSX-fragment recovery, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_js_unary_tilde_then_malformed_jsx_reports_ts1003() {
    let source = "~< <";
    let mut parser = ParserState::new("a.js".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    let ts1003_count = diagnostics.iter().filter(|d| d.code == 1003).count();
    let ts1109_count = diagnostics.iter().filter(|d| d.code == 1109).count();

    assert!(
        codes.contains(&1003),
        "Expected TS1003 for malformed JSX after unary `~`, got diagnostics: {diagnostics:?}"
    );
    assert_eq!(
        ts1003_count, 1,
        "Expected exactly one TS1003 for malformed JSX after unary `~`, got diagnostics: {diagnostics:?}"
    );
    assert_eq!(
        ts1109_count, 1,
        "Expected exactly one trailing TS1109 for malformed JSX after unary `~`, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_tsx_fragment_errors_conformance_shape_has_no_diagnostics() {
    let source = r#"
declare namespace JSX {
	interface Element { }
	interface IntrinsicElements {
		[s: string]: any;
	}
}
declare var React: any;

<>hi</div>

<>eof
"#;
    let mut parser = ParserState::new("file.tsx".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.is_empty(),
        "Expected no parser diagnostics for current tsxFragmentErrors conformance shape, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_tsx_fragment_errors_actual_conformance_file_has_no_diagnostics() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../TypeScript/tests/cases/conformance/jsx/tsxFragmentErrors.tsx"
    ))
    .unwrap();
    let mut parser = ParserState::new("file.tsx".to_string(), source);
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.is_empty(),
        "Expected no parser diagnostics for actual tsxFragmentErrors conformance file, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_trailing_decimal_numeric_literal_recovery_matches_conformance_shape() {
    let source = "1.toString();\nvar test2 = 2.toString();\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert_eq!(
        codes,
        vec![
            diagnostic_codes::AN_IDENTIFIER_OR_KEYWORD_CANNOT_IMMEDIATELY_FOLLOW_A_NUMERIC_LITERAL,
            diagnostic_codes::AN_IDENTIFIER_OR_KEYWORD_CANNOT_IMMEDIATELY_FOLLOW_A_NUMERIC_LITERAL,
            diagnostic_codes::EXPECTED,
            diagnostic_codes::EXPRESSION_EXPECTED,
        ],
        "Trailing-decimal recovery should match the numeric literal conformance shape, got diagnostics: {diagnostics:?}"
    );

    let standalone_identifier_pos = source.find("toString").unwrap();
    assert!(
        diagnostics
            .iter()
            .all(|diag| !(diag.code == diagnostic_codes::EXPECTED
                && diag.start as usize == standalone_identifier_pos)),
        "Standalone `1.toString()` should not emit a spurious missing-semicolon diagnostic: {diagnostics:?}"
    );

    let var_stmt_start = source.find("var test2 = 2.toString();").unwrap();
    let open_paren_pos = var_stmt_start + "var test2 = 2.toString".len();
    let close_paren_pos = open_paren_pos + 1;

    let ts1005 = diagnostics
        .iter()
        .find(|diag| diag.code == diagnostic_codes::EXPECTED)
        .expect("expected TS1005 for the recovered call tail");
    assert_eq!(
        ts1005.start as usize, open_paren_pos,
        "TS1005 should anchor at the opening paren after the recovered identifier tail: {diagnostics:?}"
    );

    let ts1109 = diagnostics
        .iter()
        .find(|diag| diag.code == diagnostic_codes::EXPRESSION_EXPECTED)
        .expect("expected TS1109 for the empty recovered call expression");
    assert_eq!(
        ts1109.start as usize, close_paren_pos,
        "TS1109 should anchor at the closing paren after the recovered empty call: {diagnostics:?}"
    );
}

#[test]
fn test_decorator_type_assertion_reports_brace_expected_and_expression_expected_at_end_of_type_token()
 {
    let source = "@<[[import(obju2c77,\n";
    let mut parser = ParserState::new(
        "parseUnmatchedTypeAssertion.ts".to_string(),
        source.to_string(),
    );
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts1109_positions: Vec<_> = diagnostics
        .iter()
        .filter(|diag| diag.code == diagnostic_codes::EXPRESSION_EXPECTED)
        .map(|diag| diag.start as usize)
        .collect();
    let ts1005_positions: Vec<_> = diagnostics
        .iter()
        .filter(|diag| diag.code == diagnostic_codes::EXPECTED)
        .map(|diag| diag.start)
        .collect();

    let ts1109_count = diagnostics
        .iter()
        .filter(|diag| diag.code == diagnostic_codes::EXPRESSION_EXPECTED)
        .count();
    assert_eq!(
        ts1109_count, 1,
        "Decorator type assertion recovery should emit one TS1109 diagnostic at the type assertion start, got {diagnostics:?}"
    );
    assert_eq!(
        ts1109_positions,
        vec![1],
        "TS1109 should anchor at the decorator type assertion start, got positions: {ts1109_positions:?}. Full diagnostics: {diagnostics:?}"
    );

    let ts1005_count = diagnostics
        .iter()
        .filter(|diag| diag.code == diagnostic_codes::EXPECTED)
        .count();
    assert_eq!(
        ts1005_count, 1,
        "Decorator type assertion recovery should emit a single TS1005 for the missing class body brace, got {diagnostics:?}"
    );
    assert_eq!(
        ts1005_positions,
        vec![21],
        "TS1005 should anchor at decorator tail, got positions: {ts1005_positions:?}. Full diagnostics: {diagnostics:?}"
    );
    let ts1146_count = diagnostics
        .iter()
        .filter(|diag| diag.code == diagnostic_codes::DECLARATION_EXPECTED)
        .count();
    assert_eq!(
        ts1146_count, 0,
        "Decorator type assertion recovery should not emit TS1146, got {diagnostics:?}"
    );
}
