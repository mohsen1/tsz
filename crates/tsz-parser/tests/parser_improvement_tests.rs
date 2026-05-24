//! Tests for parser improvements to reduce TS1005 and TS2300 false positives

use crate::parser::test_fixture::{parse_source, parse_source_named};
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
    let (parser, _root) = parse_source(source);

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
fn parameter_array_binding_reserved_words_match_tsc_recovery_fingerprints() {
    let source = "function a4([while, for, public]){ }\nfunction a5(...while) { }\n";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let first_comma = source.find(", for").expect("comma after while") as u32;
    let for_pos = source.find("for,").expect("for token") as u32;
    let second_comma = source.find(", public").expect("comma after for") as u32;
    let close_bracket = source.find("])").expect("array close bracket") as u32;
    let close_paren = close_bracket + 1;
    let rest_close_paren =
        source.find("while) {").expect("rest while") as u32 + "while".len() as u32;

    for (code, start, message) in [
        (diagnostic_codes::EXPECTED, first_comma, "'(' expected."),
        (
            diagnostic_codes::EXPRESSION_EXPECTED,
            for_pos,
            "Expression expected.",
        ),
        (diagnostic_codes::EXPECTED, second_comma, "'(' expected."),
        (diagnostic_codes::EXPECTED, close_bracket, "';' expected."),
        (
            diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
            close_paren,
            "Declaration or statement expected.",
        ),
        (
            diagnostic_codes::EXPECTED,
            rest_close_paren,
            "'(' expected.",
        ),
    ] {
        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.code == code && diag.start == start && diag.message == message),
            "Expected diagnostic {code} at {start} with {message:?}; got {diagnostics:?}",
        );
    }

    assert!(
        diagnostics.iter().all(|diag| {
            !(diag.code == diagnostic_codes::EXPECTED
                && diag.start == first_comma
                && diag.message == "';' expected.")
        }),
        "First comma should use tsc's '(' recovery, got {diagnostics:?}",
    );
    assert!(
        diagnostics.iter().all(|diag| {
            !(diag.code == diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED
                && diag.start == for_pos)
        }),
        "TS1128 should be anchored after the recovered binding pattern, got {diagnostics:?}",
    );
}

#[test]
fn block_bodied_arrow_statement_recovers_invalid_conditional_tail_without_branch_cascades() {
    let source = "(a?) => { return a; } ? (b)=>(c)=>81 : (c)=>(d)=>82;\n";
    let question_pos = source.find(" ? (b)").expect("outer question") as u32 + 1;
    let colon_pos = source.find(" : ").expect("outer colon") as u32 + 1;
    let first_branch_arrow = source.find("(b)=>").expect("true branch arrow") as u32 + 3;
    let second_branch_arrow = source.find("(c)=>81").expect("nested true branch arrow") as u32 + 3;
    let (parser, root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let actual: Vec<_> = diagnostics
        .iter()
        .map(|diag| (diag.code, diag.start, diag.message.as_str()))
        .collect();

    assert_eq!(
        actual,
        vec![
            (diagnostic_codes::EXPECTED, question_pos, "';' expected."),
            (diagnostic_codes::EXPECTED, colon_pos, "';' expected."),
        ],
        "invalid conditional tail after a block-bodied arrow expression should recover like tsc, got {diagnostics:?}"
    );
    assert!(
        !diagnostics
            .iter()
            .any(|diag| diag.start == first_branch_arrow || diag.start == second_branch_arrow),
        "branch-local arrows are recovered statements and must not produce cascaded TS1005 diagnostics: {diagnostics:?}"
    );

    let source_file = parser.get_arena().get_source_file_at(root).unwrap();
    assert_eq!(
        source_file.statements.nodes.len(),
        3,
        "tsc keeps the invalid conditional branches as recovered expression statements"
    );
    for &stmt_idx in &source_file.statements.nodes {
        assert_eq!(
            parser.get_arena().get(stmt_idx).unwrap().kind,
            crate::parser::syntax_kind_ext::EXPRESSION_STATEMENT,
            "each recovered conditional piece should remain an expression statement"
        );
    }
}

#[test]
fn block_bodied_arrow_statement_recovers_invalid_tail_with_nested_conditional_branch() {
    let source = "(a?) => { return a; } ? flag ? left : right : fallback;\n";
    let question_pos = source.find(" ? flag").expect("outer question") as u32 + 1;
    let nested_colon_pos = source.find(" : right").expect("nested colon") as u32 + 1;
    let outer_colon_pos = source.find(" : fallback").expect("outer colon") as u32 + 1;
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let actual: Vec<_> = diagnostics
        .iter()
        .map(|diag| (diag.code, diag.start, diag.message.as_str()))
        .collect();

    assert_eq!(
        actual,
        vec![
            (diagnostic_codes::EXPECTED, question_pos, "';' expected."),
            (diagnostic_codes::EXPECTED, outer_colon_pos, "';' expected."),
        ],
        "recovery should skip nested conditional branch contents and anchor at the outer `:`, got {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .all(|diag| diag.start != nested_colon_pos),
        "nested branch colon should not be mistaken for the outer tail separator: {diagnostics:?}"
    );
}

#[test]
fn block_bodied_arrow_statement_conditional_tail_ignores_nested_branch_semicolons() {
    let source = "(a?) => { return a; } ? (() => { foo(); }) : bar;\n";
    let question_pos = source.find(" ? (()").expect("outer question") as u32 + 1;
    let inner_semicolon_pos = source.find("foo();").expect("inner semicolon") as u32 + 5;
    let outer_colon_pos = source.find(" : bar").expect("outer colon") as u32 + 1;
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let actual: Vec<_> = diagnostics
        .iter()
        .map(|diag| (diag.code, diag.start, diag.message.as_str()))
        .collect();

    assert_eq!(
        actual,
        vec![
            (diagnostic_codes::EXPECTED, question_pos, "';' expected."),
            (diagnostic_codes::EXPECTED, outer_colon_pos, "';' expected."),
        ],
        "recovery should skip nested branch semicolons and still anchor at the outer `:`, got {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .all(|diag| diag.start != inner_semicolon_pos),
        "inner branch semicolon should not terminate outer conditional-tail recovery: {diagnostics:?}"
    );
}

#[test]
fn parenthesized_arrow_condition_still_parses_conditional_branch_arrows() {
    let source = "((a?) => { return a; }) ? (b?) => b : (c?) => c;\n";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics.is_empty(),
        "parenthesized arrow expressions are valid conditional conditions and should not use statement-tail recovery, got {diagnostics:?}"
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
    let (parser, _root) = parse_source(source);

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
fn parameter_type_predicate_tail_reports_comma_at_type_name() {
    let source = "function b2(a: b is A) {};";
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
            diagnostic_codes::EXPECTED,
            1,
            18,
            "',' expected.".to_string()
        )),
        "expected TS1005 at `is`, got {fingerprints:?}"
    );
    assert!(
        fingerprints.contains(&(
            diagnostic_codes::EXPECTED,
            1,
            21,
            "',' expected.".to_string()
        )),
        "expected TS1005 at the predicate type name, got {fingerprints:?}"
    );

    let arena = parser.get_arena();
    let source_file = arena.get_source_file_at(root).unwrap();
    let function_node = arena.get(source_file.statements.nodes[0]).unwrap();
    let function = arena.get_function(function_node).unwrap();
    let parameter_texts: Vec<&str> = function
        .parameters
        .nodes
        .iter()
        .map(|&param_idx| {
            let param = arena.get_parameter(arena.get(param_idx).unwrap()).unwrap();
            let name = arena.get(param.name).unwrap();
            &source[name.pos as usize..name.end as usize]
        })
        .collect();
    assert_eq!(
        parameter_texts,
        vec!["a", "is", "A"],
        "invalid parameter type predicates should recover the tail as parameter names"
    );
}

#[test]
fn index_signature_type_predicate_tail_defers_close_brace() {
    let source = "interface I2 {\n    [index: number]: p1 is C;\n}\n";
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
            diagnostic_codes::EXPECTED,
            2,
            25,
            "';' expected.".to_string()
        )),
        "expected TS1005 at the invalid `is` tail, got {fingerprints:?}"
    );
    assert!(
        fingerprints.contains(&(
            diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
            3,
            1,
            "Declaration or statement expected.".to_string()
        )),
        "expected TS1128 at the deferred interface close brace, got {fingerprints:?}"
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
            crate::parser::syntax_kind_ext::INTERFACE_DECLARATION,
            crate::parser::syntax_kind_ext::EXPRESSION_STATEMENT,
            crate::parser::syntax_kind_ext::EXPRESSION_STATEMENT,
        ],
        "invalid index-signature type-predicate tails should recover as top-level statements"
    );
}

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
fn test_missing_arrow_with_typed_parameters_prefers_arrow_recovery() {
    let source = r"
namespace N {
    var d = (x: number, y: string);
    var e = (x: number, y: string): void;
}
";
    let (parser, _root) = parse_source(source);

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
    let (parser, _root) = parse_source(source);

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
    let (parser, _root) = parse_source(source);

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
    let (parser, _root) = parse_source(source);

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
    let (parser, _root) = parse_source(source);

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
    let (parser, _root) = parse_source(source);

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
    let (parser, _root) = parse_source(source);

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
fn test_missing_arrow_expression_body_preserves_close_brace() {
    let source = r"
namespace N {
    namespace Inner {
        var c = (x) => };
    }
}
";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let ts1109_count = diagnostics.iter().filter(|d| d.code == 1109).count();
    let ts1128_count = diagnostics.iter().filter(|d| d.code == 1128).count();

    assert_eq!(
        ts1109_count, 1,
        "Expected only the missing-expression TS1109 for recovered arrow body, got {diagnostics:?}"
    );
    assert_eq!(
        ts1128_count, 1,
        "Recovered expression-bodied arrows should preserve the close brace for outer recovery: {diagnostics:?}"
    );
}

#[test]
fn test_es5_bind_signature_with_this_parameter_parses() {
    let source = r#"
interface Test {
  bind<T, A extends any[], B extends any[], R>(this: (this: T, ...args: [...A, ...B]) => R, thisArg: T, ...args: A): (...args: B) => R;
}
"#;
    let (parser, _root) = parse_source(source);

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
    let (parser, _root) = parse_source_named("test.js", source);

    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics.is_empty(),
        "Parenthesized object literal bodies after arrows should not trigger missing-arrow recovery: {diagnostics:?}"
    );
}

#[test]
fn test_plain_js_strict_binder_parse_diagnostics_are_preserved() {
    let source = r#"
export default 12
const yield = 1
async function f() {
    const await = 2
}
class C {
    #constructor = 3
}
"#;
    let (parser, _root) = parse_source_named("plainJSBinderErrors.js", source);

    let codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    for code in [1359, 18012] {
        assert!(
            codes.contains(&code),
            "Expected parser diagnostic TS{code} in plain JS async/class strict contexts. Got: {codes:?}"
        );
    }
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
    let (parser, _root) = parse_source(source);

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
    let (parser, _root) = parse_source(source);

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
    let (parser, _root) = parse_source(source);

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
    let (parser, _root) = parse_source(source);

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
fn test_middle_dot_identifier_part_parses_without_ts1127() {
    let source = "const a·b = 1;\na·b;\n";
    let (parser, _root) = parse_source_named("middle-dot-identifier.ts", source);

    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics
            .iter()
            .all(|d| d.code != diagnostic_codes::INVALID_CHARACTER),
        "Expected U+00B7 to be accepted as an identifier continuation, got {diagnostics:?}"
    );
}

#[test]
fn test_regex_extended_unicode_escape_above_max_does_not_report_ts1198() {
    // tsc treats out-of-range `\u{...}` inside regex literals as a runtime
    // concern and does not emit TS1198 even with the `u` flag. Match that
    // behavior — the parser must skip past the braced escape without
    // validating its code-point range.
    let source = r#"
const regexes: RegExp[] = [
  /\u{110000}/u,
  /[\u{110000}]/u,
];
"#;
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let ts1198: Vec<_> = diagnostics
        .iter()
        .filter(|d| {
            d.code
                == diagnostic_codes::AN_EXTENDED_UNICODE_ESCAPE_VALUE_MUST_BE_BETWEEN_0X0_AND_0X10FFFF_INCLUSIVE
        })
        .collect();

    assert!(
        ts1198.is_empty(),
        "Expected no TS1198 inside regex literals to match tsc, got {diagnostics:?}"
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
    let (parser, _root) = parse_source(source);

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
fn test_regex_unicode_set_class_operators_follow_v_mode_rules() {
    let source = r#"
const q = /[\q{ab}]/v;
const sub = /[a--b]/v;
const missing = /[a&&]/v;
"#;
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<_> = diagnostics.iter().map(|d| d.code).collect();

    assert!(
        !codes
            .contains(&diagnostic_codes::THIS_CHARACTER_CANNOT_BE_ESCAPED_IN_A_REGULAR_EXPRESSION),
        "Expected valid v-mode \\q string disjunction to avoid TS1535, got {diagnostics:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::RANGE_OUT_OF_ORDER_IN_CHARACTER_CLASS),
        "Expected v-mode set subtraction to avoid legacy TS1517, got {diagnostics:?}"
    );

    let ts1520: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::EXPECTED_A_CLASS_SET_OPERAND)
        .collect();
    assert_eq!(
        ts1520.len(),
        1,
        "Expected exactly one TS1520 for the trailing intersection, got {diagnostics:?}"
    );
    let expected_start = source.rfind("]/v;").expect("trailing class close") as u32;
    assert_eq!(
        ts1520[0].start, expected_start,
        "Expected TS1520 at the missing operand before ']', got {diagnostics:?}"
    );
}

#[test]
fn test_regex_hyphen_after_range_is_literal() {
    let source = "const idSuffixPattern = /^([a-z][a-z0-9-]*)(:[a-z0-9-.]*)?$/i;";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics
            .iter()
            .all(|d| d.code != diagnostic_codes::RANGE_OUT_OF_ORDER_IN_CHARACTER_CLASS),
        "Hyphen after an already-consumed range should be literal: {diagnostics:?}"
    );
}

#[test]
fn test_regex_hex_escape_range_start_does_not_report_ts1517() {
    let source = r"const pattern = /[\x2D-9A-Z\\_a-z\xF8-\u02C1]/u;";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics
            .iter()
            .all(|d| d.code != diagnostic_codes::RANGE_OUT_OF_ORDER_IN_CHARACTER_CLASS),
        "Hex escapes should be decoded as one range atom before range-order checks: {diagnostics:?}"
    );
}

#[test]
fn test_unicode_regex_trailing_hyphen_class_does_not_report_ts1508() {
    let source = r#"
const unicode = /[a-]/u;
const unicode_sets = /[a-]/v;
"#;
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics.iter().all(
            |d| d.code != diagnostic_codes::UNEXPECTED_DID_YOU_MEAN_TO_ESCAPE_IT_WITH_BACKSLASH
        ),
        "Trailing hyphen before a class close should be a literal, got {diagnostics:?}"
    );
}

#[test]
fn test_regex_character_class_escape_does_not_report_ts1517() {
    let source = r#"
/(#?-?\d*\.\d\w*%?)|(@?#?[\w-?]+%?)/g;
"#;
    let (parser, _root) = parse_source(source);

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
    let (parser, _root) = parse_source(source);

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
    let (parser, _root) = parse_source(source);

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
    let (parser, _root) = parse_source(source);

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
fn test_unterminated_regex_class_suppresses_missing_bracket() {
    let source = "let r = /[a/;\n";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    let slash_pos = source.find('/').expect("regex slash") as u32;

    assert!(
        diagnostics.iter().any(|d| {
            d.code == diagnostic_codes::UNTERMINATED_REGULAR_EXPRESSION_LITERAL
                && d.start == slash_pos
        }),
        "expected TS1161 at regex slash, got {diagnostics:?}"
    );
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::EXPECTED && d.message == "']' expected."),
        "unterminated regex class should not also emit missing bracket diagnostic, got {codes:?}: {diagnostics:?}"
    );
}

#[test]
fn test_unterminated_regex_with_angle_text_reports_ts1161() {
    for source in ["const r = /<x>;\n", "const r = /a<x>;\n"] {
        let (parser, _root) = parse_source(source);

        let diagnostics = parser.get_diagnostics();
        let ts1161 = diagnostics
            .iter()
            .filter(|d| d.code == diagnostic_codes::UNTERMINATED_REGULAR_EXPRESSION_LITERAL)
            .collect::<Vec<_>>();

        assert_eq!(
            ts1161.len(),
            1,
            "Expected one TS1161 for ordinary regex angle text in {source:?}, got {diagnostics:?}"
        );
        assert_eq!(ts1161[0].start, source.find('/').unwrap() as u32);
    }
}

#[test]
fn test_regex_annex_b_diagnostic_positions_match_tsc() {
    let source = r#"
const regexes: RegExp[] = [
  /\q\u\i\c\k\_\f\o\x\-\j\u\m\p\s/,
  /[\q\u\i\c\k\_\f\o\x\-\j\u\m\p\s]/,
  /\P[\P\w-_]/,

  // Compare to
  /\q\u\i\c\k\_\f\o\x\-\j\u\m\p\s/u,
  /[\q\u\i\c\k\_\f\o\x\-\j\u\m\p\s]/u,
  /\P[\P\w-_]/u,
];

const regexesWithBraces: RegExp[] = [
  /{??/,
  /{,??/,
  /{,1??/,
  /{1??/,
  /{1,??/,
  /{1,2??/,
  /{2,1??/,
  /{}??/,
  /{,}??/,
  /{,1}??/,
  /{1}??/,
  /{1,}??/,
  /{1,2}??/,
  /{2,1}??/,

  // Compare to
  /{??/u,
  /{,??/u,
  /{,1??/u,
  /{1??/u,
  /{1,??/u,
  /{1,2??/u,
  /{2,1??/u,
  /{}??/u,
  /{,}??/u,
  /{,1}??/u,
  /{1}??/u,
  /{1,}??/u,
  /{1,2}??/u,
  /{2,1}??/u,
];
"#;
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let line_map = LineMap::build(source);

    let mut fingerprints: Vec<(u32, u32, u32, String)> = diagnostics
        .iter()
        .filter(|d| {
            matches!(
                d.code,
                diagnostic_codes::EXPECTED
                    | diagnostic_codes::INCOMPLETE_QUANTIFIER_DIGIT_EXPECTED
                    | diagnostic_codes::NUMBERS_OUT_OF_ORDER_IN_QUANTIFIER
                    | diagnostic_codes::THERE_IS_NOTHING_AVAILABLE_FOR_REPETITION
                    | diagnostic_codes::THIS_CHARACTER_CANNOT_BE_ESCAPED_IN_A_REGULAR_EXPRESSION
            )
        })
        .map(|d| {
            let pos = line_map.offset_to_position(d.start, source);
            (d.code, pos.line + 1, pos.character + 1, d.message.clone())
        })
        .collect();
    fingerprints.sort();

    let mut expected = vec![
        (diagnostic_codes::EXPECTED, 32, 7, "'}' expected."),
        (diagnostic_codes::EXPECTED, 33, 6, "'}' expected."),
        (diagnostic_codes::EXPECTED, 34, 7, "'}' expected."),
        (diagnostic_codes::EXPECTED, 35, 8, "'}' expected."),
        (diagnostic_codes::EXPECTED, 36, 8, "'}' expected."),
        (
            diagnostic_codes::INCOMPLETE_QUANTIFIER_DIGIT_EXPECTED,
            32,
            5,
            "Incomplete quantifier. Digit expected.",
        ),
        (
            diagnostic_codes::INCOMPLETE_QUANTIFIER_DIGIT_EXPECTED,
            38,
            5,
            "Incomplete quantifier. Digit expected.",
        ),
        (
            diagnostic_codes::INCOMPLETE_QUANTIFIER_DIGIT_EXPECTED,
            39,
            5,
            "Incomplete quantifier. Digit expected.",
        ),
        (
            diagnostic_codes::NUMBERS_OUT_OF_ORDER_IN_QUANTIFIER,
            27,
            5,
            "Numbers out of order in quantifier.",
        ),
        (
            diagnostic_codes::NUMBERS_OUT_OF_ORDER_IN_QUANTIFIER,
            36,
            5,
            "Numbers out of order in quantifier.",
        ),
        (
            diagnostic_codes::NUMBERS_OUT_OF_ORDER_IN_QUANTIFIER,
            43,
            5,
            "Numbers out of order in quantifier.",
        ),
    ];

    for (line, column) in [
        (24, 4),
        (24, 8),
        (25, 4),
        (25, 9),
        (26, 4),
        (26, 10),
        (27, 4),
        (27, 10),
        (32, 4),
        (32, 8),
        (33, 4),
        (33, 7),
        (34, 4),
        (34, 8),
        (35, 4),
        (35, 9),
        (36, 4),
        (36, 9),
        (38, 4),
        (38, 8),
        (39, 4),
        (39, 9),
        (40, 4),
        (40, 8),
        (41, 4),
        (41, 9),
        (42, 4),
        (42, 10),
        (43, 4),
        (43, 10),
    ] {
        expected.push((
            diagnostic_codes::THERE_IS_NOTHING_AVAILABLE_FOR_REPETITION,
            line,
            column,
            "There is nothing available for repetition.",
        ));
    }

    for (line, column) in [
        (8, 4),
        (8, 14),
        (8, 18),
        (8, 24),
        (9, 5),
        (9, 13),
        (9, 15),
        (9, 19),
        (9, 25),
    ] {
        expected.push((
            diagnostic_codes::THIS_CHARACTER_CANNOT_BE_ESCAPED_IN_A_REGULAR_EXPRESSION,
            line,
            column,
            "This character cannot be escaped in a regular expression.",
        ));
    }

    let mut expected: Vec<(u32, u32, u32, String)> = expected
        .into_iter()
        .map(|(code, line, column, message)| (code, line, column, message.to_string()))
        .collect();
    expected.sort();

    assert_eq!(
        fingerprints, expected,
        "Annex B regex diagnostic positions should match tsc, got: {diagnostics:?}"
    );
}

#[test]
fn test_parenthesized_conditional_object_literal_true_branch_is_not_treated_as_missing_arrow() {
    let source = r#"
var value = (Math.random() ? {} : null);
"#;
    let (parser, _root) = parse_source(source);

    assert!(
        parser.get_diagnostics().is_empty(),
        "Conditional branches with object literals should not trigger missing-arrow recovery: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parenthesized_arrow_block_tail_keeps_trailing_semicolon_error() {
    let source = "a = (() => { } || a)";
    let (parser, _root) = parse_source(source);

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
fn test_parenthesized_divide_expression_before_block_is_not_treated_as_missing_arrow() {
    let source = "(a/8\n ){}\n";
    let (parser, _root) = parse_source(source);

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
    let (parser, _root) = parse_source(source);

    assert!(
        parser.get_diagnostics().is_empty(),
        "Parameter-modifier arrow heads should stay in arrow parsing: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_top_level_modifier_recovery_keeps_try_block_error() {
    let source = "cla <ss {\n  _ static try\n";
    let (parser, _root) = parse_source(source);

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
    let (parser, _root) = parse_source(source);

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
    let (parser, _root) = parse_source(source);

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
    let (parser, _root) = parse_source(source);

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
    let (parser, _root) = parse_source(source);

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
    let (parser, _root) = parse_source(source);

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
    let (parser, _root) = parse_source(source);

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
    let (parser, _root) = parse_source(source);

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
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();

    assert!(
        diagnostics.iter().all(|diag| diag.code != 1359),
        "Type positions should not reject reserved-word identifiers with TS1359: {diagnostics:?}"
    );
}

#[test]
fn test_variable_list_trailing_comma_reports_at_comma() {
    let source = "var a,\nreturn;";
    let (parser, _root) = parse_source(source);

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
    let (parser, _root) = parse_source(source);

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
    let (parser, _root) = parse_source(source);

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
fn test_empty_element_access_reports_after_open_bracket() {
    let source = r#"
class Z {
 public x = "";
}

var a6: Z[][] = new   Z     [      ]   [  ];
"#;
    let (parser, _root) = parse_source(source);

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
    let (parser, _root) = parse_source_named("types2.ts", source);

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
    let (parser, _root) = parse_source(source);

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
    let (parser, _root) = parse_source(source);

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
    let (parser, _root) = parse_source(source);

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
    let (parser, _root) = parse_source(source);

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
    let (parser, _root) = parse_source(source);

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
    let (parser, _root) = parse_source(source);

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
    let (parser, _root) = parse_source(source);

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
    let (parser, _root) = parse_source(source);

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
