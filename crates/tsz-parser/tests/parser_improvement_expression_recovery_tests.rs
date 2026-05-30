//! Tests for parser improvements to reduce TS1005 and TS2300 false positives — expression recovery.

use crate::parser::syntax_kind_ext;
use crate::parser::test_fixture::parse_source;
use tsz_scanner::SyntaxKind;

/// Walk every node in the arena and return the operator tokens of binary
/// expressions whose left operand is a missing identifier (zero-width synthesized
/// node). A statement that begins with a binary operator is recovered by tsc as
/// `<missing> <op> <rhs>`; this helper lets tests assert the operator is kept in
/// the tree rather than skipped.
fn binary_ops_with_missing_left(source: &str) -> Vec<SyntaxKind> {
    let (parser, _root) = parse_source(source);
    let arena = parser.get_arena();
    arena
        .nodes
        .iter()
        .filter(|node| node.kind == syntax_kind_ext::BINARY_EXPRESSION)
        .filter_map(|node| arena.get_binary_expr(node))
        .filter(|binary| {
            // A missing/synthesized left operand is a zero-width node (pos == end).
            arena
                .get(binary.left)
                .is_some_and(|left| left.pos == left.end)
        })
        .map(|binary| {
            SyntaxKind::try_from_u16(binary.operator_token).unwrap_or(SyntaxKind::Unknown)
        })
        .collect()
}

fn conditional_exprs_with_missing_condition(source: &str) -> usize {
    let (parser, _root) = parse_source(source);
    let arena = parser.get_arena();
    arena
        .nodes
        .iter()
        .filter(|node| node.kind == syntax_kind_ext::CONDITIONAL_EXPRESSION)
        .filter_map(|node| arena.get_conditional_expr(node))
        .filter(|conditional| {
            arena
                .get(conditional.condition)
                .is_some_and(|condition| condition.pos == condition.end)
        })
        .count()
}

#[test]
fn test_incomplete_binary_expression_recovery() {
    // Test recovery from incomplete binary expression: a +
    let source = r"const result = a +;
const next = 1;";

    let (parser, _root) = parse_source(source);

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

    let (parser, _root) = parse_source(source);

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

    let (parser, _root) = parse_source(source);

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

    let (parser, _root) = parse_source(source);

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

    let (parser, _root) = parse_source(source);

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

#[test]
fn test_statement_starting_with_logical_or_keeps_operator_as_missing_left_binary() {
    // `a = () => { } || b` — tsc parses `a = () => { }` as the first statement
    // (the arrow short-circuits `parseAssignmentExpressionOrHigher`, so `||`
    // begins a new statement). The trailing `|| b` is recovered as
    // `<missing> || b`: `parsePrimaryExpression` synthesizes a missing
    // identifier without consuming the operator, then `parseBinaryExpressionRest`
    // consumes `||` and parses the right operand. The operator must survive in
    // the tree so the emitter prints ` || b` rather than dropping it.
    let ops = binary_ops_with_missing_left("a = () => { } || b\n");
    assert!(
        ops.contains(&SyntaxKind::BarBarToken),
        "expected a `<missing> || b` binary expression for `|| b` statement, got {ops:?}"
    );
}

#[test]
fn test_statement_starting_with_binary_operator_varies_with_operator_and_names() {
    // The recovery rule is keyed on "statement begins with a binary operator",
    // not on a specific operator spelling or identifier name. A block-bodied
    // arrow as an assignment value short-circuits `parseAssignmentExpression`,
    // so the trailing operator reliably begins a new statement. Vary both the
    // operator and the operand names; every case must keep the operator with a
    // synthesized missing left operand. Only operators that are NOT also
    // expression starts are used: `+`/`-`/`*`/`/`/`<` are unary/JSX/regex at
    // statement start, so they take a different (pre-existing) recovery path.
    // `!=`, `&&`, `|`, `==` are purely binary and exercise the seeded chain.
    let cases = [
        ("x = () => { } != y\n", SyntaxKind::ExclamationEqualsToken),
        (
            "foo = () => { } && bar\n",
            SyntaxKind::AmpersandAmpersandToken,
        ),
        (
            "gamma = () => { } == delta\n",
            SyntaxKind::EqualsEqualsToken,
        ),
        ("u = () => { } | v\n", SyntaxKind::BarToken),
    ];
    for (source, op) in cases {
        let ops = binary_ops_with_missing_left(source);
        assert!(
            ops.contains(&op),
            "expected `<missing> {op:?} rhs` recovery for source {source:?}, got {ops:?}"
        );
    }
}

#[test]
fn test_statement_starting_with_binary_operator_does_not_drop_operator() {
    // Regression guard: previously the parser skipped a leading binary operator
    // and produced just the right operand (`|| b` became `b`). Confirm the
    // recovered second statement is a binary expression, not the bare operand.
    let (parser, root) = parse_source("c = () => { } || d\n");
    let arena = parser.get_arena();
    let sf = arena.get_source_file_at(root).expect("source file");
    let second_is_binary_expr_statement = sf
        .statements
        .nodes
        .iter()
        .filter_map(|&stmt| arena.get(stmt))
        .filter(|node| node.kind == syntax_kind_ext::EXPRESSION_STATEMENT)
        .filter_map(|node| arena.get_expression_statement(node))
        .any(|expr_stmt| {
            arena
                .get(expr_stmt.expression)
                .is_some_and(|expr| expr.kind == syntax_kind_ext::BINARY_EXPRESSION)
        });
    assert!(
        second_is_binary_expr_statement,
        "leading-binary-operator statement should recover as a binary expression, not a bare operand; diagnostics: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_statement_starting_with_question_does_not_seed_conditional_condition() {
    // `?` has conditional-expression precedence, but it is not a pure binary
    // operator. At statement start it must stay on the existing skip/recovery
    // path instead of fabricating a missing conditional condition.
    let missing_condition_count =
        conditional_exprs_with_missing_condition("q = () => { } ? a : b\n");
    assert_eq!(
        missing_condition_count, 0,
        "statement-start `?` should not become a conditional expression with a missing condition"
    );
}
