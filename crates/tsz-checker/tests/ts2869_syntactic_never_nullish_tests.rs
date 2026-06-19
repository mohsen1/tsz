//! Tests for TS2869 ("Right operand of ?? is unreachable because the left
//! operand is never nullish.") across the full set of syntactic forms tsc
//! classifies as *never nullish*.
//!
//! Structural rule (port of tsc's `getSyntacticNullishnessSemantics` over
//! `skipOuterExpressions(left, All)`):
//!
//! > The `??` left-operand check is purely syntactic. After looking through
//! > parentheses, type assertions, `satisfies`, and non-null assertions, an
//! > operand is *never nullish* (→ TS2869) unless its syntactic kind makes it
//! > *possibly* nullish (identifier reference, `this`, call / element &
//! > property access / `new` / tagged-template / `await` / `yield` /
//! > meta-property, `||`/`&&`) or *always* nullish (`null` / `undefined`,
//! > → TS2871). Object/array literals, function/arrow/class expressions, regex
//! > literals, `void`/`typeof`/unary expressions, and every non-logical binary
//! > operator are never nullish.
//!
//! The companion files `ts2871_always_nullish_tests.rs` and
//! `ts2871_through_assertions_tests.rs` cover the *always*-nullish arm.
//!
//! Per CLAUDE.md §25 the decision is over expression *shape*, not the static
//! type or any identifier text, so the regression guards below rename binders.

use tsz_checker::test_utils::check_source_strict_codes as check;

fn count(source: &str, code: u32) -> usize {
    check(source).iter().filter(|&&c| c == code).count()
}

// =========================================================================
// Literal value forms previously missed by tsz — TS2869 must fire
// =========================================================================

/// Object and array literals are never nullish; both must emit TS2869.
#[test]
fn object_and_array_literals_emit_ts2869() {
    assert_eq!(
        count("const a = ({}) ?? 1;\n", 2869),
        1,
        "object literal left operand of ?? must emit TS2869",
    );
    assert_eq!(
        count("const b = [] ?? 1;\n", 2869),
        1,
        "array literal left operand of ?? must emit TS2869",
    );
}

/// Function, arrow, and class expressions are never nullish.
#[test]
fn function_arrow_class_expressions_emit_ts2869() {
    assert!(check("const f = (function () {}) ?? 1;\n").contains(&2869));
    assert!(check("const g = (() => 1) ?? 1;\n").contains(&2869));
    assert!(check("const h = (class {}) ?? 1;\n").contains(&2869));
}

/// A regular-expression literal is never nullish.
#[test]
fn regex_literal_emits_ts2869() {
    assert!(check("const r = /abc/ ?? 1;\n").contains(&2869));
}

/// `void`, `typeof`, and unary arithmetic operators all produce a non-nullish
/// value (tsc classifies them via the `Never` default).
#[test]
fn void_typeof_and_unary_emit_ts2869() {
    assert!(check("const a = (void 0) ?? 1;\n").contains(&2869));
    assert!(check("declare const n: number;\nconst b = (typeof n) ?? 1;\n").contains(&2869));
    assert!(check("declare const n: number;\nconst c = (-n) ?? 1;\n").contains(&2869));
    assert!(check("declare const n: number;\nconst d = (!n) ?? 1;\n").contains(&2869));
}

// =========================================================================
// Binary / conditional / comma / assignment classification
// =========================================================================

/// Every non-logical binary operator yields a never-nullish value, so the
/// `??` right operand is unreachable.
#[test]
fn arithmetic_and_comparison_binaries_emit_ts2869() {
    assert!(check("const a = (1 + 2) ?? 3;\n").contains(&2869));
    assert!(
        check("declare const x: number, y: number;\nconst b = (x * y) ?? 3;\n").contains(&2869)
    );
    assert!(
        check("declare const x: number, y: number;\nconst c = (x === y) ?? 3;\n").contains(&2869)
    );
}

/// `||` and `&&` may surface a nullish operand, so the left operand is *not*
/// classified never-nullish — neither TS2869 nor TS2871 fires.
#[test]
fn logical_binaries_do_not_emit_nullish_diags() {
    let diags =
        check("declare const x: number | undefined, y: number;\nconst a = (x || y) ?? 3;\n");
    assert!(
        !diags.contains(&2869),
        "|| left operand must not emit TS2869: {diags:?}"
    );
    assert!(
        !diags.contains(&2871),
        "|| left operand must not emit TS2871: {diags:?}"
    );
    let diags =
        check("declare const x: number | undefined, y: number;\nconst b = (x && y) ?? 3;\n");
    assert!(
        !diags.contains(&2869),
        "&& left operand must not emit TS2869: {diags:?}"
    );
}

/// A conditional is never nullish only when *both* branches are; one
/// possibly-nullish branch makes it `Sometimes`.
#[test]
fn conditional_branches_union_their_semantics() {
    // both branches literal → never nullish → TS2869
    assert!(check("declare const c: boolean;\nconst a = (c ? 1 : 2) ?? 3;\n").contains(&2869));
    // one branch an identifier (possibly nullish) → no diagnostic
    let diags =
        check("declare const c: boolean, m: number | undefined;\nconst b = (c ? m : 1) ?? 3;\n");
    assert!(
        !diags.contains(&2869),
        "mixed conditional must not emit TS2869: {diags:?}"
    );
    assert!(!diags.contains(&2871));
    // both branches always-nullish → TS2871
    assert!(
        check("declare const c: boolean;\nconst d = (c ? null : null) ?? 3;\n").contains(&2871)
    );
}

/// Comma and assignment expressions defer to their right operand.
#[test]
fn comma_and_assignment_defer_to_right_operand() {
    // right operand is a literal → never nullish
    assert!(check("declare const p: number;\nconst a = (p, 1) ?? 3;\n").contains(&2869));
    assert!(check("let q: number;\nconst b = (q = 1) ?? 3;\n").contains(&2869));
}

// =========================================================================
// skipOuterExpressions: assertions / non-null / parens reach the literal
// =========================================================================

/// A type assertion, `satisfies`, or non-null assertion wrapping a literal is
/// transparent — tsc reaches the inner literal and reports never-nullish.
#[test]
fn assertions_are_transparent_for_never_nullish() {
    assert!(check("const a = (1 as unknown) ?? 2;\n").contains(&2869));
    assert!(check("const b = (1 satisfies number) ?? 2;\n").contains(&2869));
    assert!(check("const c = (1)! ?? 2;\n").contains(&2869));
    assert!(check("const d = (<number>1) ?? 2;\n").contains(&2869));
}

// =========================================================================
// Regression guards — possibly-nullish forms must stay silent
// =========================================================================

/// Plain identifier references, member/element access, calls, `new`, and
/// `this` are all possibly-nullish syntactic forms: no diagnostic regardless
/// of their static type. Binder names are varied to prove the rule is over
/// shape, not text.
#[test]
fn possibly_nullish_forms_emit_no_diagnostic() {
    // identifier whose type is never nullish — still no TS2869 (tsc parity).
    let diags = check("declare const widget: number;\nconst a = widget ?? 1;\n");
    assert!(
        !diags.contains(&2869),
        "identifier must not emit TS2869: {diags:?}"
    );

    let diags = check("declare const payload: { count: number };\nconst b = payload.count ?? 1;\n");
    assert!(
        !diags.contains(&2869),
        "property access must not emit TS2869: {diags:?}"
    );

    let diags = check("declare function produce(): number;\nconst c = produce() ?? 1;\n");
    assert!(
        !diags.contains(&2869),
        "call must not emit TS2869: {diags:?}"
    );

    let diags = check("class Gadget {}\nconst d = new Gadget() ?? 1;\n");
    assert!(
        !diags.contains(&2869),
        "new expression must not emit TS2869: {diags:?}"
    );

    let diags = check("class Holder {\n  read() {\n    return this ?? 1;\n  }\n}\n");
    assert!(
        !diags.contains(&2869),
        "this must not emit TS2869: {diags:?}"
    );
}

/// `await` is possibly nullish (the awaited value may be null/undefined).
#[test]
fn await_expression_emits_no_diagnostic() {
    let diags = check(
        "async function run(task: Promise<number | undefined>) {\n  return (await task) ?? 1;\n}\n",
    );
    assert!(
        !diags.contains(&2869),
        "await must not emit TS2869: {diags:?}"
    );
    assert!(!diags.contains(&2871));
}

/// An assertion wrapping a *possibly-nullish* inner form (an identifier) is
/// still possibly nullish — the transparency must not invent a diagnostic.
#[test]
fn assertion_over_identifier_stays_silent() {
    let diags =
        check("declare const source: number;\nconst a = (source as unknown as number) ?? 1;\n");
    assert!(
        !diags.contains(&2869),
        "assertion over identifier must not emit TS2869: {diags:?}"
    );
}
