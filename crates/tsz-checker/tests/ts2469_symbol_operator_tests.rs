//! Tests for TS2469: The '{0}' operator cannot be applied to type 'symbol'.
//!
//! tsc emits TS2469 when certain operators are used with symbol-typed operands:
//! - Unary +, -, ~ on symbol types
//! - Binary + / += when one side is symbol and the other is string or any
//! - Relational operators (<, >, <=, >=) with symbol operands

use crate::context::CheckerOptions;
use crate::state::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn get_diagnostics(source: &str) -> Vec<(u32, String)> {
    let full_source =
        format!("declare var s: symbol;\ndeclare var str: string;\ndeclare var a: any;\n{source}");
    let mut parser = ParserState::new("test.ts".to_string(), full_source);
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );

    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn has_error(source: &str, code: u32) -> bool {
    get_diagnostics(source).iter().any(|d| d.0 == code)
}

// =============================================================================
// Unary operators on symbol: TS2469
// =============================================================================

#[test]
fn unary_plus_on_symbol_emits_ts2469() {
    assert!(
        has_error("+s;", 2469),
        "unary + on symbol should emit TS2469"
    );
}

#[test]
fn unary_minus_on_symbol_emits_ts2469() {
    assert!(
        has_error("-s;", 2469),
        "unary - on symbol should emit TS2469"
    );
}

#[test]
fn unary_tilde_on_symbol_emits_ts2469() {
    assert!(
        has_error("~s;", 2469),
        "unary ~ on symbol should emit TS2469"
    );
}

#[test]
fn unary_bang_on_symbol_does_not_emit_ts2469() {
    // ! is always valid (returns boolean)
    assert!(
        !has_error("!s;", 2469),
        "unary ! on symbol should NOT emit TS2469"
    );
}

// =============================================================================
// Binary + with symbol: TS2469 vs TS2365
// =============================================================================

#[test]
fn binary_plus_symbol_and_string_emits_ts2469() {
    // s + "" → TS2469 on left operand (symbol), not TS2365
    let diags = get_diagnostics("s + \"\";");
    assert!(
        diags.iter().any(|d| d.0 == 2469),
        "s + \"\" should emit TS2469"
    );
}

#[test]
fn binary_plus_string_and_symbol_emits_ts2469() {
    // "" + s → TS2469 on right operand (symbol)
    let diags = get_diagnostics("\"\" + s;");
    assert!(
        diags.iter().any(|d| d.0 == 2469),
        "\"\" + s should emit TS2469"
    );
}

#[test]
fn binary_plus_symbol_and_any_emits_ts2469() {
    // s + a → TS2469 on symbol operand
    let diags = get_diagnostics("s + a;");
    assert!(
        diags.iter().any(|d| d.0 == 2469),
        "s + a should emit TS2469"
    );
}

// =============================================================================
// Relational operators with symbol: TS2469
// =============================================================================

#[test]
fn relational_less_than_on_symbol_emits_ts2469() {
    assert!(has_error("s < s;", 2469), "s < s should emit TS2469");
}

#[test]
fn relational_greater_than_on_symbol_emits_ts2469() {
    assert!(has_error("s > 0;", 2469), "s > 0 should emit TS2469");
}

// =============================================================================
// Compound += with symbol: TS2469
// =============================================================================

#[test]
fn compound_plus_equals_symbol_string_emits_ts2469() {
    // s += "" → TS2469 on left (symbol)
    assert!(
        has_error("s += \"\";", 2469),
        "s += \"\" should emit TS2469"
    );
}

#[test]
fn compound_plus_equals_string_symbol_emits_ts2469() {
    // str += s → TS2469 on right (symbol)
    assert!(has_error("str += s;", 2469), "str += s should emit TS2469");
}

// =============================================================================
// TS2469 message text verification
// =============================================================================

#[test]
fn ts2469_message_contains_operator_and_symbol() {
    let diags = get_diagnostics("+s;");
    let ts2469_diags: Vec<_> = diags.iter().filter(|d| d.0 == 2469).collect();
    assert!(!ts2469_diags.is_empty(), "should have TS2469");
    assert!(
        ts2469_diags[0].1.contains("symbol"),
        "TS2469 message should mention 'symbol': {}",
        ts2469_diags[0].1
    );
    assert!(
        ts2469_diags[0].1.contains("+"),
        "TS2469 message should mention the operator: {}",
        ts2469_diags[0].1
    );
}
