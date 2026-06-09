//! Tests for parser improvements to reduce TS1005 and TS2300 false positives — expression recovery.

use crate::parser::syntax_kind_ext;
use crate::parser::test_fixture::parse_source;
use tsz_common::diagnostics::diagnostic_codes;
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
fn test_statement_starting_with_assignment_operator_keeps_missing_left_binary() {
    let cases = [
        ("^= replacement;\n", SyntaxKind::CaretEqualsToken),
        ("&&= fallback;\n", SyntaxKind::AmpersandAmpersandEqualsToken),
        (
            "??= defaultValue;\n",
            SyntaxKind::QuestionQuestionEqualsToken,
        ),
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
fn test_statement_starting_with_assignment_operator_reports_statement_recovery() {
    let cases = [
        ("= replacement;\n", SyntaxKind::EqualsToken, "="),
        ("^= replacement;\n", SyntaxKind::CaretEqualsToken, "^="),
        (
            "&&= fallback;\n",
            SyntaxKind::AmpersandAmpersandEqualsToken,
            "&&=",
        ),
        (
            "??= defaultValue;\n",
            SyntaxKind::QuestionQuestionEqualsToken,
            "??=",
        ),
        (
            "function bar() { } *= value;\n",
            SyntaxKind::AsteriskEqualsToken,
            "*=",
        ),
    ];

    for (source, op, op_text) in cases {
        let (parser, _root) = parse_source(source);
        let diagnostics = parser.get_diagnostics();
        let operator_pos = source.find(op_text).expect("source contains operator") as u32;

        assert!(
            diagnostics.iter().any(|diag| {
                diag.code == diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED
                    && diag.start == operator_pos
            }),
            "expected TS1128 at {op:?} for source {source:?}, got {diagnostics:?}"
        );
        assert!(
            diagnostics
                .iter()
                .all(|diag| diag.code != diagnostic_codes::EXPRESSION_EXPECTED
                    || diag.start != operator_pos),
            "statement-start assignment recovery should not report TS1109 at {op:?}; got {diagnostics:?}"
        );
    }
}

#[test]
fn test_invalid_private_name_indexed_access_does_not_gain_assignment_statement_cascade() {
    let source = r#"
class C {
    foo = 3;
    #bar = 3;
    constructor () {
        const badForNow: C[#bar] = 3;
    }
}
"#;
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();

    assert!(
        diagnostics
            .iter()
            .all(|diag| diag.code != diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED),
        "invalid private-name indexed access should not cascade into TS1128: {diagnostics:?}"
    );
}

#[test]
fn test_statement_starting_with_equals_recovers_to_rhs_expression() {
    let ops = binary_ops_with_missing_left("= replacement;\n");
    assert!(
        !ops.contains(&SyntaxKind::EqualsToken),
        "plain `=` statement recovery should resume at the RHS expression, got {ops:?}"
    );
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

// =====================================================================
// Empty element access (`x[]`) followed by a stray close token.
//
// `x[]` reports TS1011 ("An element access expression should take an
// argument."). When the completed postfix expression is then followed by a
// stray close token that cannot start a statement (for example `)`), tsc
// reports TS1005 ("';' expected.") at that token — its `parseErrorAtPosition`
// dedups missing-semicolon errors by exact start only, so the nearby TS1011
// does not suppress it. tsz's distance-based suppression would otherwise drop
// the `';' expected.`, leaving the close token to fall through to the
// statement list as a spurious TS1128 ("Declaration or statement expected.").
// =====================================================================

fn expression_recovery_codes(source: &str) -> Vec<u32> {
    let (parser, _root) = parse_source(source);
    let mut codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    codes.sort_unstable();
    codes
}

#[test]
fn empty_element_access_then_stray_paren_reports_semicolon_not_ts1128() {
    let codes = expression_recovery_codes("probe[] )\n");
    assert!(
        codes.contains(&diagnostic_codes::AN_ELEMENT_ACCESS_EXPRESSION_SHOULD_TAKE_AN_ARGUMENT),
        "empty subscript must still report TS1011; got {codes:?}"
    );
    assert!(
        codes.contains(&diagnostic_codes::EXPECTED),
        "the stray close token must report TS1005 ';' expected; got {codes:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED),
        "the stray close token must not fall through to a spurious TS1128; got {codes:?}"
    );
}

#[test]
fn empty_element_access_then_stray_bracket_close_reports_semicolon_not_ts1128() {
    // `value[])` shape from the witness `...rest: string[]) {...}` reduced to
    // statement level: empty subscript immediately followed by `)`.
    let codes = expression_recovery_codes("value[]) \n");
    assert!(
        codes.contains(&diagnostic_codes::AN_ELEMENT_ACCESS_EXPRESSION_SHOULD_TAKE_AN_ARGUMENT)
    );
    assert!(codes.contains(&diagnostic_codes::EXPECTED));
    assert!(!codes.contains(&diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED));
}

#[test]
fn nonempty_element_access_then_stray_paren_unchanged() {
    // Negative control: a non-empty subscript already completed correctly —
    // TS1005 at the stray `)`, no TS1011, no TS1128. The fix must not perturb
    // this path.
    let codes = expression_recovery_codes("probe[0] )\n");
    assert!(
        !codes.contains(&diagnostic_codes::AN_ELEMENT_ACCESS_EXPRESSION_SHOULD_TAKE_AN_ARGUMENT)
    );
    assert!(codes.contains(&diagnostic_codes::EXPECTED));
    assert!(!codes.contains(&diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED));
}

#[test]
fn call_expression_then_stray_paren_unchanged() {
    // Negative control: a completed call followed by a stray `)` already
    // reports TS1005 ';' expected (no TS1011 involved).
    let codes = expression_recovery_codes("invoke() )\n");
    assert!(codes.contains(&diagnostic_codes::EXPECTED));
    assert!(!codes.contains(&diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED));
}
