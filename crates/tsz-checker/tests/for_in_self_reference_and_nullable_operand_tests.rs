//! Regression tests for TS2407 on `for-in` operands that `tsc` accepts because
//! the operand never reaches the object-type gate with a real type.
//!
//! Two independent mechanisms, both oracle-verified against `tsc` 7.0.2
//! (`--noEmit --target es2015 --pretty false`, `--strict` both ways):
//!
//! 1. **Circular self-reference.** `for (var v in v) {}` asks for the type of
//!    `v` while computing the type of `v` — `tsc` breaks that cycle by handing
//!    back `any` (and, under `noImplicitAny` only, reporting TS7022). `any` is
//!    a valid for-in operand, so TS2407 never fires. tsz resolved the loop
//!    variable to its for-in key type instead and reported a false TS2407.
//!    This is the `for (var of in of) {}` witness behind
//!    `parserForOfStatement19.ts` / `parserES5ForOfStatement19.ts`, and the
//!    `for (let v in v) {}` line of `recursiveLetConst.ts`.
//!
//! 2. **Nullable operands without `strictNullChecks`.** With the flag off,
//!    `null` and `undefined` are members of every type, so `null` satisfies the
//!    object-type gate and `tsc` is silent; with the flag on, the operand is
//!    stripped to `never` and TS2407 *is* reported. tsz reported TS2407 in both
//!    modes. This is the `for (var a in null) {}` line of `widenedTypes.ts`.
//!
//! `never` itself stays an error in both modes (oracle-confirmed), so the
//! nullable case is genuinely about flag-dependent assignability and not about
//! bottom types being acceptable operands.
//!
//! Binder names are varied across cases so nothing here can be satisfied by a
//! user-chosen identifier.

use crate::test_utils::{check_source_non_strict_codes, check_source_strict_codes};

const TS2407: u32 = 2407;

/// All four conformance rows behind this suite carry `// @strict: false`, and
/// `CheckerOptions::default()` is a *strict* run in TypeScript 7 — so the
/// non-strict projection is the one that reproduces them.
fn non_strict(source: &str) -> Vec<u32> {
    check_source_non_strict_codes(source)
}

fn strict(source: &str) -> Vec<u32> {
    check_source_strict_codes(source)
}

fn assert_no_2407(codes: &[u32], label: &str) {
    assert!(
        !codes.contains(&TS2407),
        "{label}: expected no TS2407 (tsc accepts this operand), got codes: {codes:?}"
    );
}

fn assert_has_2407(codes: &[u32], label: &str) {
    assert!(
        codes.contains(&TS2407),
        "{label}: expected TS2407 (tsc rejects this operand), got codes: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Mechanism 1 — circular self-reference resolves to `any`, so no TS2407.
// ---------------------------------------------------------------------------

#[test]
fn for_in_operand_naming_its_own_var_loop_variable_is_not_ts2407() {
    // `parserForOfStatement19.ts` / `parserES5ForOfStatement19.ts`: the loop
    // variable is literally named `of`, which is a for-in over itself.
    let source = "for (var of in of) { }\n";
    assert_no_2407(&non_strict(source), "var self-reference (`of`)");
    assert_no_2407(&strict(source), "var self-reference (`of`), strict");
}

#[test]
fn for_in_operand_naming_its_own_let_loop_variable_is_not_ts2407() {
    // `recursiveLetConst.ts`: tsc reports TS2448 for the TDZ use and nothing
    // else — in particular no TS2407.
    let source = "for (let v in v) { }\n";
    assert_no_2407(&non_strict(source), "let self-reference");
    assert_no_2407(&strict(source), "let self-reference, strict");
}

#[test]
fn renamed_binder_self_referential_for_in_operand_is_not_ts2407() {
    let source = "for (var payload in payload) { }\n";
    assert_no_2407(&non_strict(source), "renamed binder self-reference");
}

#[test]
fn indirect_self_referential_for_in_operand_is_not_ts2407() {
    // The cycle runs through a property access: `w.k` still needs `w`'s type.
    let source = "for (var w in w.k) { }\n";
    assert_no_2407(&non_strict(source), "indirect self-reference");
}

#[test]
fn const_self_referential_for_in_operand_is_not_ts2407() {
    let source = "for (const c in c) { }\n";
    assert_no_2407(&non_strict(source), "const self-reference");
}

#[test]
fn self_referential_for_in_operand_inside_a_function_body_is_not_ts2407() {
    let source = "function outer() { for (var s in s) { } }\n";
    assert_no_2407(&non_strict(source), "nested self-reference");
}

// ---------------------------------------------------------------------------
// Mechanism 2 — nullable operands are flag-dependent.
// ---------------------------------------------------------------------------

#[test]
fn null_for_in_operand_without_strict_null_checks_is_not_ts2407() {
    // `widenedTypes.ts` (`// @strict: false`).
    let source = "for (var a in null) { }\n";
    assert_no_2407(&non_strict(source), "null operand, non-strict");
}

#[test]
fn undefined_for_in_operand_without_strict_null_checks_is_not_ts2407() {
    let source = "for (var b in undefined) { }\n";
    assert_no_2407(&non_strict(source), "undefined operand, non-strict");
}

#[test]
fn null_for_in_operand_under_strict_null_checks_is_still_ts2407() {
    // Same source, opposite verdict: with the flag on tsc strips the operand to
    // `never` and reports TS2407. The non-strict rows above must not be bought
    // by making nullable operands universally valid.
    let source = "for (var a in null) { }\n";
    assert_has_2407(&strict(source), "null operand, strict");
}

#[test]
fn undefined_for_in_operand_under_strict_null_checks_is_still_ts2407() {
    let source = "for (var b in undefined) { }\n";
    assert_has_2407(&strict(source), "undefined operand, strict");
}

// ---------------------------------------------------------------------------
// Controls — the gate still rejects what tsc rejects.
// ---------------------------------------------------------------------------

#[test]
fn string_literal_for_in_operand_is_still_ts2407() {
    let source = "for (var k in \"abc\") { }\n";
    assert_has_2407(&non_strict(source), "string literal operand");
    assert_has_2407(&strict(source), "string literal operand, strict");
}

#[test]
fn declared_never_for_in_operand_is_still_ts2407_in_both_modes() {
    let source = "declare const nothing: never;\nfor (var a in nothing) { }\n";
    assert_has_2407(&non_strict(source), "never operand");
    assert_has_2407(&strict(source), "never operand, strict");
}

#[test]
fn number_variable_for_in_operand_is_still_ts2407() {
    let source = "declare const count: number;\nfor (var idx in count) { }\n";
    assert_has_2407(&non_strict(source), "number operand");
}

#[test]
fn ordinary_object_for_in_operands_stay_clean() {
    let source = "\
var src = { a: 1 };
for (var kk in src) { }
let obj = { b: 2 };
for (let k2 in obj) { }
";
    assert_no_2407(&non_strict(source), "ordinary object operands");
    assert_no_2407(&strict(source), "ordinary object operands, strict");
}

#[test]
fn a_distinct_outer_binding_of_the_same_name_is_not_treated_as_self_reference() {
    // `holder` here is an ordinary object in scope; the loop variable shadows
    // it. The operand still resolves to the *outer* binding for tsc (the inner
    // declaration is not yet initialized), so this must not become an error —
    // and, just as importantly, the self-reference exemption must not be the
    // reason it is clean for a non-object outer binding, which the next case
    // pins.
    let source = "\
var holder = { a: 1 };
for (var holder2 in holder) { }
";
    assert_no_2407(&non_strict(source), "shadowing outer object");
}

#[test]
fn a_non_object_outer_binding_still_reports_even_when_a_loop_variable_shadows_it() {
    // The exemption is keyed on the operand's *own* circular resolution, not on
    // name equality: a differently-named loop variable over a `string` binding
    // must still report.
    let source = "\
var text = \"abc\";
for (var textKey in text) { }
";
    assert_has_2407(&non_strict(source), "non-object outer binding");
}
