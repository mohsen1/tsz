// Tests for parser improvements to reduce TS1005 and TS2300 false positives

use crate::parser::ParserState;
use tsz_common::diagnostics::diagnostic_codes;
use tsz_common::position::LineMap;

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
fn test_typed_parenthesized_expression_followed_by_property_access_prefers_missing_arrow() {
    let source = "var v = (inspectedElement: any).props;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let dot_pos = source.find('.').expect("property access dot") as u32;

    assert!(
        diagnostics.iter().any(|diag| {
            diag.code == 1005 && diag.start == dot_pos && diag.message == "'=>' expected."
        }),
        "Typed parenthesized heads should recover as missing-arrow at property access: {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .all(|diag| !(diag.code == 1005 && diag.message == "')' expected.")),
        "Typed parenthesized heads should not report a missing ')' here: {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .all(|diag| !(diag.code == 1005 && diag.message == "',' expected.")),
        "Typed parenthesized heads should not report a comma recovery at this tail: {diagnostics:?}"
    );
}
#[test]
fn test_parenthesized_initializer_with_stray_equals_before_block_prefers_semicolon_recovery() {
    let source = "x = (y = z ==== 'function') {";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == 1005 && diag.message == "';' expected."),
        "Malformed ==== tails should recover with ';' expected at '{{': {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .all(|diag| !(diag.code == 1005 && diag.message == "'=>' expected.")),
        "Malformed ==== tails should not recover as missing arrow at '{{': {diagnostics:?}"
    );
}
#[test]
fn test_await_using_array_target_assignment_recovers_with_semicolon_expected() {
    let source = r"
{
    await using [a] = null;
}
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let equals_pos = source.find('=').expect("assignment token") as u32;

    assert!(
        diagnostics.iter().any(|diag| {
            diag.code == 1005 && diag.message == "';' expected." && diag.start == equals_pos
        }),
        "Expected TS1005 ';' expected at '=' for await using recovery, got {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .all(|diag| diag.message != "Expression expected."),
        "Should not emit TS1109 for this recovery shape: {diagnostics:?}"
    );
}
#[test]
fn test_in_expression_assignment_recovers_with_semicolon_expected() {
    let source = "'prop' in v = 10;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let equals_pos = source.find('=').expect("assignment token") as u32;

    assert!(
        diagnostics.iter().any(|diag| {
            diag.code == 1005 && diag.message == "';' expected." && diag.start == equals_pos
        }),
        "Expected TS1005 ';' expected at '=' for in-expression assignment recovery, got {diagnostics:?}"
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
fn test_property_access_missing_name_at_eof_reports_ts1003_after_dot() {
    let source = "var p2 = window. ";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let expected_start = source.find('.').expect("dot position") as u32 + 1;

    assert!(
        diagnostics.iter().any(|diag| {
            diag.code == diagnostic_codes::IDENTIFIER_EXPECTED
                && diag.message == "Identifier expected."
                && diag.start == expected_start
        }),
        "Expected TS1003 immediately after '.' at EOF, got diagnostics: {diagnostics:?}"
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
fn test_es5_bind_signature_with_this_parameter_parses() {
    let source = r#"
interface Test {
  bind<T, A extends any[], B extends any[], R>(this: (this: T, ...args: [...A, ...B]) => R, thisArg: T, ...args: A): (...args: B) => R;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics.is_empty(),
        "ES5-style bind signature with a this parameter should parse cleanly: {diagnostics:?}"
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
fn test_regex_annexb_p_escape_does_not_consume_following_escape() {
    // Annex B (no /u flag): `\P` without braces is the literal character `P`.
    // Previously, scan_character_class_escape returned None for this case
    // after advancing pos past `P`, causing the caller to over-consume the
    // following backslash. That mis-parsed `\P\w-_` as `P`, `w`, `-`, `_`
    // and then mis-detected `w-_` as an out-of-order range (TS1517).
    let source = "const a = /\\P[\\P\\w-_]/;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics
            .iter()
            .all(|d| d.code != diagnostic_codes::RANGE_OUT_OF_ORDER_IN_CHARACTER_CLASS),
        "Annex B `\\P` should not cause TS1517 on following character class atoms: {diagnostics:?}"
    );
}
#[test]
fn test_regex_non_bmp_inline_flags_emit_unknown_flag_diagnostics() {
    let source = r"
const 𝘳𝘦𝘨𝘦𝘹 = /(?𝘴𝘪-𝘮:^𝘧𝘰𝘰.)/𝘨𝘮𝘶;
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts1499_count = diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::UNKNOWN_REGULAR_EXPRESSION_FLAG)
        .count();

    assert_eq!(
        ts1499_count, 6,
        "Expected six TS1499 diagnostics for unknown inline and trailing non-BMP flags, got {diagnostics:?}"
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
fn test_typeof_import_defer_reports_missing_parens_in_type_query() {
    let source = r#"
export type X = typeof import.defer("./a").Foo;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts1005_messages: Vec<&str> = diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::EXPECTED)
        .map(|d| d.message.as_str())
        .collect();

    assert!(
        ts1005_messages.iter().any(|m| m.contains("'(' expected.")),
        "Expected TS1005 '(' expected for typeof import.defer, got {diagnostics:?}",
    );
    assert!(
        ts1005_messages.iter().any(|m| m.contains("')' expected.")),
        "Expected TS1005 ')' expected for typeof import.defer, got {diagnostics:?}",
    );
}
#[test]
fn test_import_attributes_double_comma_recovers_with_missing_brace_and_ts1128() {
    let source = r#"
export type Test3 = typeof import("./a.json", {
  with: {
    type: "json"
  },,
});
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::EXPECTED && d.message.contains("'}' expected.")),
        "Expected TS1005 '}}' expected recovery for malformed import attributes, got {diagnostics:?}",
    );

    let ts1128_count = diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED)
        .count();
    assert!(
        ts1128_count >= 2,
        "Expected at least two TS1128 diagnostics in tail recovery, got {diagnostics:?}",
    );
}
#[test]
fn test_import_attributes_nested_double_comma_reports_ts1478_without_ts1128_tail() {
    let source = r#"
export type Test4 = typeof import("./a.json", {
  with: {
    type: "json",,
  }
});
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::IDENTIFIER_OR_STRING_LITERAL_EXPECTED),
        "Expected TS1478 for malformed nested import-attribute key, got {diagnostics:?}",
    );
    assert!(
        diagnostics
            .iter()
            .all(|d| d.code != diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED),
        "Expected no TS1128 tail cascade for nested comma invalid-key recovery, got {diagnostics:?}",
    );
}
#[test]
fn test_import_type_options_array_recovery_in_intersection_reports_semicolon_and_ts1128() {
    let source = r#"
export type LocalInterface =
    & import("pkg", [ {"resolution-mode": "require"} ]).RequireInterface
    & import("pkg", [ {"resolution-mode": "import"} ]).ImportInterface;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::EXPECTED && d.message.contains("'{' expected.")),
        "Expected TS1005 '{{' expected for array import options recovery, got {diagnostics:?}",
    );
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::EXPECTED && d.message.contains("';' expected.")),
        "Expected TS1005 ';' expected for array import options recovery in intersections, got {diagnostics:?}",
    );
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED),
        "Expected TS1128 statement-tail recovery for array import options in intersections, got {diagnostics:?}",
    );
}
