//! Tests for parser improvements to reduce TS1005 and TS2300 false positives — yield generator recovery.

use crate::parser::test_fixture::{parse_source, parse_source_named};
use tsz_common::diagnostics::diagnostic_codes;

#[test]
fn test_yield_after_type_assertion_requires_parens() {
    // yield without parentheses after type assertion should emit TS1109
    let source = r"
function* f() {
    <number> yield 0;
}
";
    let (parser, _root) = parse_source(source);

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
fn test_type_assertion_missing_operand_anchors_at_after_gt_after_conflict_marker() {
    let source = "const x = <div>\n<<<<<<< HEAD";
    let (parser, _root) = parse_source(source);
    let ts1109: Vec<_> = parser
        .parse_diagnostics
        .iter()
        .filter(|d| d.code == 1109)
        .collect();
    assert_eq!(
        ts1109.len(),
        1,
        "Expected exactly one TS1109, got: {ts1109:?}",
    );
    let after_gt = source.find("<div>").unwrap() as u32 + "<div>".len() as u32;
    let actual_start = ts1109[0].start;
    assert_eq!(
        actual_start, after_gt,
        "TS1109 must anchor at end of `<div>` (offset {after_gt}), got offset {actual_start}",
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
    let (parser, _root) = parse_source(source);

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
    let (parser, root) = parse_source(source);

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

#[test]
fn test_decorator_type_assertion_reports_brace_expected_and_expression_expected_at_end_of_type_token()
 {
    let source = "@<[[import(obju2c77,\n";
    let (parser, _root) = parse_source_named("parseUnmatchedTypeAssertion.ts", source);

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
