//! Tests for parser improvements to reduce TS1005 and TS2300 false positives — prefix unary recovery.

use crate::parser::test_fixture::parse_source;
use tsz_common::diagnostics::diagnostic_codes;
use tsz_common::position::LineMap;

#[test]
fn test_prefix_unary_without_operand_emits_ts1109_after_prior_ts1005() {
    // `var a = q~;` — after parsing `var a = q`, the `~` triggers TS1005
    // (',' expected) in the variable declaration list. Recovery then re-enters
    // statement parsing and treats `~;` as a prefix-unary expression with a
    // missing operand. tsc emits TS1109 at the `;` even though TS1005 was just
    // reported one column earlier; our distance-based error suppression used
    // to swallow the TS1109 because the two positions are within three
    // characters. Verify both diagnostics are now emitted.
    let source = "var a = q~;\n";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let ts1005_count = diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::EXPECTED)
        .count();
    let ts1109_count = diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::EXPRESSION_EXPECTED)
        .count();

    assert_eq!(
        ts1005_count, 1,
        "Expected exactly one TS1005 (',' expected) for `var a = q~;`, got diagnostics: {diagnostics:?}"
    );
    assert_eq!(
        ts1109_count, 1,
        "Expected TS1109 (Expression expected) at the `;` after `~` for `var a = q~;`, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_prefix_unary_tilde_missing_operand_emits_ts1109_after_initializer() {
    // `var b =~;` — the initializer is parsed as `~` with a missing operand.
    // tsc emits TS1109 at the `;`. This path has no prior parser error so it
    // does not exercise the suppression-bypass, but it pins down the baseline
    // behaviour alongside the prior-error variant above.
    let source = "var b =~;\n";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let ts1109_count = diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::EXPRESSION_EXPECTED)
        .count();

    assert_eq!(
        ts1109_count, 1,
        "Expected exactly one TS1109 at the `;` for `var b =~;`, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_bitwise_not_invalid_operations_matches_tsc_diagnostics() {
    // Matches the conformance test
    // TypeScript/tests/cases/conformance/expressions/unaryOperators/
    // bitwiseNotOperator/bitwiseNotOperatorInvalidOperations.ts after the
    // test runner strips the `// @target:` directive. tsc emits exactly four
    // diagnostics:
    //   (5,10) TS1005 ',' expected.
    //   (5,11) TS1109 Expression expected.
    //   (8,27) TS1134 Variable declaration expected.
    //   (11,9) TS1109 Expression expected.
    let source = "\
// Unary operator ~
var q;

// operand before ~
var a = q~;  //expect error

// multiple operands after ~
var mul = ~[1, 2, \"abc\"], \"\";  //expect error

// miss an operand
var b =~;
";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let line_map = LineMap::build(source);

    let mut fingerprints: Vec<(u32, u32, u32)> = diagnostics
        .iter()
        .map(|d| {
            let pos = line_map.offset_to_position(d.start, source);
            (d.code, pos.line + 1, pos.character + 1)
        })
        .collect();
    fingerprints.sort();

    let mut expected = vec![
        (diagnostic_codes::EXPECTED, 5, 10),
        (diagnostic_codes::EXPRESSION_EXPECTED, 5, 11),
        (diagnostic_codes::VARIABLE_DECLARATION_EXPECTED, 8, 27),
        (diagnostic_codes::EXPRESSION_EXPECTED, 11, 9),
    ];
    expected.sort();

    assert_eq!(
        fingerprints, expected,
        "Diagnostic fingerprints must match tsc exactly, got: {diagnostics:?}"
    );
}
