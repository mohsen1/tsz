//! Tests for parser improvements to reduce TS1005 and TS2300 false positives — arrow recovery.

use crate::parser::test_fixture::{parse_source, parse_source_named};
use tsz_common::diagnostics::diagnostic_codes;

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
fn parenthesized_arrow_with_line_break_before_arrow_is_parsed_as_arrow_function() {
    // Unlike simple arrows, a line terminator between `(x)` and `=>` is allowed.
    // tsc reports TS1200 but still parses it as an arrow function.
    let source = "(x)\n=> x;\n";
    let (parser, root) = parse_source(source);
    let arena = parser.get_arena();

    let source_file = arena.get_source_file_at(root).unwrap();
    // tsc parses `(x)\n=> x` as an arrow function expression (with TS1200)
    // and keeps it in a single expression statement
    assert_eq!(
        source_file.statements.nodes.len(),
        1,
        "Parenthesized arrow with line break before `=>` should still parse as one statement, \
         got {} statements",
        source_file.statements.nodes.len()
    );
}
