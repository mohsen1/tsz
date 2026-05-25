//! Regression coverage for #9766: an equality guard `x === s` where `x` is the
//! wide `symbol` type and `s` is a `unique symbol` must NOT narrow `x` to the
//! `unique symbol` type.
//!
//! Structural rule: tsc's `replacePrimitivesWithLiterals` only substitutes the
//! wide `string`/`number`/`bigint` primitives with their literal/unit subtypes
//! during equality narrowing. A wide `symbol` is deliberately never collapsed
//! to a `unique symbol`, so `x: symbol` stays `symbol` inside `if (x === s)`
//! and a later `const y: typeof s = x` still reports TS2322. A union of
//! `unique symbol` members (a genuine singleton domain) must still narrow.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

fn ts2322_count(source: &str) -> usize {
    check_source(source, "test.ts", CheckerOptions::default())
        .iter()
        .filter(|d| d.code == 2322)
        .count()
}

/// Reported repro: guarded assignment after `x === s` still errors.
#[test]
fn wide_symbol_equality_guard_does_not_narrow_to_unique() {
    let source = r#"
declare const s: unique symbol;
function f(x: symbol) {
    if (x === s) {
        const y: typeof s = x;
    }
}
"#;
    assert_eq!(
        ts2322_count(source),
        1,
        "TS2322 must fire: `x === s` must not narrow wide symbol to unique symbol"
    );
}

/// Rename invariance: the fix is structural, not keyed on `x`/`s`.
#[test]
fn wide_symbol_equality_guard_renamed_still_errors() {
    let source = r#"
declare const marker: unique symbol;
function check(value: symbol) {
    if (value === marker) {
        const captured: typeof marker = value;
    }
}
"#;
    assert_eq!(
        ts2322_count(source),
        1,
        "renaming the identifiers must not change the diagnostic"
    );
}

/// `!==` negative (else) branch is the positive `===` guard, so it must also
/// refuse to narrow wide symbol to unique symbol.
#[test]
fn wide_symbol_inequality_else_branch_does_not_narrow() {
    let source = r#"
declare const s: unique symbol;
function f(x: symbol) {
    if (x !== s) {
    } else {
        const y: typeof s = x;
    }
}
"#;
    assert_eq!(
        ts2322_count(source),
        1,
        "else branch of `x !== s` must not narrow wide symbol to unique symbol"
    );
}

/// Legitimate narrowing of a union of `unique symbol` members must be
/// preserved — each member is a singleton the guard can partition.
#[test]
fn union_of_unique_symbols_still_narrows() {
    let source = r#"
declare const s1: unique symbol;
declare const s2: unique symbol;
function f(x: typeof s1 | typeof s2) {
    if (x === s1) {
        const a: typeof s1 = x;
    } else {
        const b: typeof s2 = x;
    }
}
"#;
    assert_eq!(
        ts2322_count(source),
        0,
        "narrowing a union of unique symbols by `=== s1` must keep working"
    );
}

/// Regression guard: the unguarded control already errors and must keep
/// erroring (the fix changes only the guarded narrowing, not assignability).
#[test]
fn unguarded_wide_symbol_assignment_still_errors() {
    let source = r#"
declare const s: unique symbol;
function f(x: symbol) {
    const y: typeof s = x;
}
"#;
    assert_eq!(
        ts2322_count(source),
        1,
        "wide symbol is not assignable to unique symbol without any guard"
    );
}

/// Positive control: a genuinely unique-symbol-typed source assigns fine,
/// both directly and inside the guard — the fix must not over-suppress.
#[test]
fn unique_symbol_source_assigns_without_error() {
    let source = r#"
declare const s: unique symbol;
function f(x: typeof s) {
    const direct: typeof s = x;
    if (x === s) {
        const guarded: typeof s = x;
    }
}
"#;
    assert_eq!(
        ts2322_count(source),
        0,
        "a unique-symbol source is assignable to its own unique-symbol type"
    );
}
