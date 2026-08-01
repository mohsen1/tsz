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

// ---------------------------------------------------------------------------
// Mechanism 3 — `unknown` operands are TS2407, in both strictness modes.
//
// tsc's for-in gate is `allTypesAssignableToKind(rightType, NonPrimitive |
// InstantiableNonPrimitive)`; `unknown` satisfies neither disjunct (only
// `any` is exempted separately, via `isTypeAny`). tsz previously treated
// `unknown` the same as `any` here, by analogy rather than by the actual
// rule, so it was silent where tsc reports. Unlike the nullable-operand
// mechanism above, this is not `strictNullChecks`-gated: `unknown` reports
// TS2407 the same way with the flag on or off.
// ---------------------------------------------------------------------------

#[test]
fn unknown_for_in_operand_is_ts2407_in_both_modes() {
    let source = "declare const u: unknown;\nfor (const k in u) { }\n";
    assert_has_2407(&non_strict(source), "unknown operand");
    assert_has_2407(&strict(source), "unknown operand, strict");
}

#[test]
fn unknown_for_in_operand_via_type_alias_is_still_ts2407() {
    // The gate must see through an alias to the same `unknown` leaf type, not
    // just a literal `unknown` annotation.
    let source =
        "type Maybe = unknown;\ndeclare const item: Maybe;\nfor (const prop in item) { }\n";
    assert_has_2407(&non_strict(source), "unknown via type alias");
    assert_has_2407(&strict(source), "unknown via type alias, strict");
}

#[test]
fn unknown_for_in_operand_var_loop_variable_is_still_ts2407() {
    // Renamed binder / `var` form control, distinct from the `const` case above.
    let source = "declare const payload: unknown;\nfor (var field in payload) { }\n";
    assert_has_2407(&non_strict(source), "unknown operand, var loop variable");
}

#[test]
fn any_for_in_operand_stays_clean_next_to_the_unknown_fix() {
    // Adjacent positive control: `any` is exempted by a separate tsc rule
    // (`isTypeAny`) and must not regress alongside removing `unknown`.
    let source = "declare const dyn: any;\nfor (const k in dyn) { }\n";
    assert_no_2407(&non_strict(source), "any operand");
    assert_no_2407(&strict(source), "any operand, strict");
}

#[test]
fn type_parameter_constrained_to_unknown_stays_clean() {
    // A type parameter is `InstantiableNonPrimitive` regardless of its
    // constraint, so this must not be swept up by the `unknown`-leaf fix:
    // the parameter itself, not its constraint, is what tsz inspects here.
    let source = "function f<T extends unknown>(x: T) {\n  for (const k in x) { }\n}\n";
    assert_no_2407(&non_strict(source), "type parameter constrained to unknown");
    assert_no_2407(
        &strict(source),
        "type parameter constrained to unknown, strict",
    );
}

// ---------------------------------------------------------------------------
// Mechanism 1, reporting half — the same circular loop head that clears the
// TS2407 gate is what `tsc` reports TS7022 on.
//
// #16138 removed the wrong TS2407 for this shape; `tsz` then said nothing at
// all, where `tsc` reports TS7022 (plus a TDZ TS2448 for `let`/`const`, which
// is a separate owner and deliberately not asserted here). Every expectation
// below is oracle-pinned against `tsc` 7.0.2, `--noEmit --pretty false`, run
// under `--strict`, `--strict false`, and `--strict --noImplicitAny false`.
// ---------------------------------------------------------------------------

const TS7022: u32 = 7022;

fn assert_has_7022(codes: &[u32], label: &str) {
    assert!(
        codes.contains(&TS7022),
        "{label}: expected TS7022 (tsc reports the circular loop variable), got codes: {codes:?}"
    );
}

fn assert_no_7022(codes: &[u32], label: &str) {
    assert!(
        !codes.contains(&TS7022),
        "{label}: expected no TS7022, got codes: {codes:?}"
    );
}

#[test]
fn for_in_operand_naming_its_own_var_loop_variable_reports_ts7022() {
    // Oracle: `for (var v in v) {}` under --strict is exactly `TS7022` — no
    // TS2448, because `var` is hoisted and has no TDZ.
    let source = "for (var scratch in scratch) { }\n";
    assert_has_7022(&strict(source), "var self-reference");
    // And the TS2407 half stays suppressed: one predicate drives both.
    assert_no_2407(&strict(source), "var self-reference keeps no TS2407");
}

#[test]
fn for_in_operand_naming_its_own_const_loop_variable_reports_ts7022() {
    // Oracle for `for (const k in k) {}` is `TS2448 + TS7022`. tsz does not yet
    // report the TDZ half (tracked separately in #16141); the implicit-any half
    // is what this path owns.
    let source = "for (const entry in entry) { }\n";
    assert_has_7022(&strict(source), "const self-reference");
}

#[test]
fn for_in_operand_naming_its_own_let_loop_variable_reports_ts7022() {
    let source = "for (let cursor in cursor) { }\n";
    assert_has_7022(&strict(source), "let self-reference");
}

#[test]
fn for_in_operand_reaching_its_own_loop_variable_through_a_property_reports_ts7022() {
    // Oracle: `for (var w in w.k) {}` is `TS7022`. The cycle is indirect — the
    // operand is a property access whose *object* is the loop variable.
    let source = "for (var holder in holder.inner) { }\n";
    assert_has_7022(&strict(source), "indirect self-reference through property");
}

#[test]
fn for_in_operand_reaching_its_own_loop_variable_through_a_call_reports_ts7022() {
    // Oracle: `for (const k in id(k)) {}` is `TS2448 + TS7022`.
    let source = "\
declare function passthrough(value: any): any;
for (const item in passthrough(item)) { }
";
    assert_has_7022(&strict(source), "indirect self-reference through call");
}

#[test]
fn a_loop_variable_shadowing_an_outer_object_binding_still_reports_ts7022() {
    // Oracle: an outer `k: object` does NOT rescue the inner loop head —
    // `declare const k: object; function f() { for (const k in k) {} }` is
    // still `TS2448 + TS7022`. The inner declaration shadows, so the operand
    // resolves to the loop variable and the cycle stands.
    let source = "\
declare const shadowed: object;
function walk() {
    for (const shadowed in shadowed) { }
}
";
    assert_has_7022(&strict(source), "loop variable shadowing an outer object");
}

#[test]
fn a_member_name_that_merely_spells_the_loop_variable_is_not_a_self_reference() {
    // Oracle: `declare const o: { v: object }; for (const v in o.v) {}` is
    // CLEAN. `v` occurs in the operand only as a *member name*, which is not a
    // reference to the loop variable — this is exactly the case an
    // identifier-spelling match would get wrong, and why this path resolves
    // binder symbols instead.
    let source = "\
declare const bag: { field: object };
for (const field in bag.field) { }
";
    assert_no_7022(&strict(source), "member name spelling the loop variable");
    assert_no_2407(&strict(source), "member name spelling the loop variable");
}

#[test]
fn an_unrelated_loop_variable_over_an_object_operand_reports_nothing() {
    // Negative control: no cycle, no diagnostic on either half.
    let source = "\
declare const source: object;
for (const key in source) { }
";
    assert_no_7022(&strict(source), "unrelated loop variable");
    assert_no_2407(&strict(source), "unrelated loop variable");
}

#[test]
fn an_annotated_circular_loop_variable_does_not_report_ts7022() {
    // Oracle: `for (const v: string in o) {}` reports only `TS2404` (a for-in
    // variable may not have a type annotation). An annotation means tsc reads
    // it instead of the operand, so there is no circular resolution to report.
    let source = "for (var labeled: string in labeled) { }\n";
    assert_no_7022(&strict(source), "annotated loop variable");
}

#[test]
fn for_in_self_reference_is_silent_without_no_implicit_any() {
    // Oracle: with `--noImplicitAny false`, `tsc`'s `reportCircularityError`
    // stays silent and the variable is quietly `any` — the same gating the
    // for-of twin already implements. Both `--strict false` and
    // `--strict --noImplicitAny false` drop TS7022 in the oracle; the
    // non-strict projection is the one the corpus rows carry.
    let source = "for (var scratch in scratch) { }\n";
    assert_no_7022(&non_strict(source), "var self-reference, noImplicitAny off");
    assert_no_2407(&non_strict(source), "var self-reference, noImplicitAny off");
}

#[test]
fn the_for_of_twin_is_unchanged_by_the_for_in_path() {
    // for-of already reported TS7022 for its own self-reference; the for-in
    // arm must not disturb it.
    let source = "for (var stream of stream) { }\n";
    assert_has_7022(&strict(source), "for-of self-reference still reports");
}

#[test]
fn an_element_access_indexed_by_the_loop_variable_is_a_self_reference() {
    // The mirror of the member-name case, and the reason the narrowing above is
    // property-access-only. Oracle: `declare const o: { [k: string]: object };
    // for (const v in o[v]) {}` is `TS2448 + TS7022` — an element access really
    // does read the loop variable.
    let source = "\
declare const table: { [k: string]: object };
for (const slot in table[slot]) { }
";
    assert_has_7022(
        &strict(source),
        "element access indexed by the loop variable",
    );
}
