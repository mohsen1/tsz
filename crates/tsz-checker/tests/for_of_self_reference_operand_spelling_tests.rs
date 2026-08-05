//! Regression tests for TS7022 on `for-of` operands that only *spell* the loop
//! variable's name without referencing its binding.
//!
//! `tsc` decides for-of circularity by resolving the operand and watching
//! `pushTypeResolution` fail on the loop variable's own symbol — an identity
//! question. tsz's `check_for_of_self_reference_circularity` asked it twice:
//! once through binder symbol identity, and once through a raw walk that
//! matched any identifier in the operand subtree whose *text* equalled the loop
//! variable's name. The two were or-ed together, so the spelling walk decided
//! every row the identity walk declined.
//!
//! The identity walk was narrowed (for-in twin, #16144) to skip a property
//! access's member name, because `o.alpha` reads no binding called `alpha`.
//! The spelling walk was not, and because the two were or-ed that narrowing
//! could never bind on the for-of side.
//!
//! Every row below is oracle-verified against `tsc` 7.0.2
//! (`--noEmit --strict --pretty false --target es2015 --lib es2015`).
//!
//! The load-bearing asymmetry, pinned as a matched pair over one operand type:
//! `for (const alpha of holder.alpha)` is **clean** (a member name is not a
//! value reference) while `for (const alpha of holder[alpha])` **reports**
//! (an element access genuinely reads the binding). Both operands are
//! `number[]`; only the access kind differs.
//!
//! Binder names are varied across rows so nothing here can be satisfied by a
//! user-chosen identifier.

use crate::test_utils::check_source_strict_codes;

const TS7022: u32 = 7022;

fn assert_no_7022(source: &str, label: &str) {
    let codes = check_source_strict_codes(source);
    assert!(
        !codes.contains(&TS7022),
        "{label}: expected no TS7022 (tsc resolves this operand normally), got codes: {codes:?}"
    );
}

fn assert_has_7022(source: &str, label: &str) {
    let codes = check_source_strict_codes(source);
    assert!(
        codes.contains(&TS7022),
        "{label}: expected TS7022 (tsc reports a circular loop variable), got codes: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// The matched pair: same operand type, same holder, only the access kind moves.
// ---------------------------------------------------------------------------

#[test]
fn property_access_member_name_spelling_the_loop_variable_is_not_circular() {
    assert_no_7022(
        r"
declare const holder: { alpha: number[]; [k: string]: number[] };
for (const alpha of holder.alpha) { alpha; }
        ",
        "for (const alpha of holder.alpha)",
    );
}

#[test]
fn element_access_reading_the_loop_variable_is_still_circular() {
    assert_has_7022(
        r"
declare const holder: { alpha: number[]; [k: string]: number[] };
for (const alpha of holder[alpha]) { alpha; }
        ",
        "for (const alpha of holder[alpha])",
    );
}

// ---------------------------------------------------------------------------
// Negative rows — the operand only spells the name, never reads the binding.
// ---------------------------------------------------------------------------

#[test]
fn nested_property_access_member_names_are_not_circular() {
    assert_no_7022(
        r"
declare const outer: { inner: { beta: number[] } };
for (const beta of outer.inner.beta) { beta; }
        ",
        "for (const beta of outer.inner.beta)",
    );
}

#[test]
fn optional_property_access_member_name_is_not_circular() {
    assert_no_7022(
        r"
declare const maybe: { gamma: number[] } | undefined;
for (const gamma of maybe?.gamma ?? []) { gamma; }
        ",
        "for (const gamma of maybe?.gamma ?? [])",
    );
}

#[test]
fn object_literal_property_name_spelling_the_loop_variable_is_not_circular() {
    assert_no_7022(
        r"
declare function pick(source: { delta: number[] }): number[];
for (const delta of pick({ delta: [1] })) { delta; }
        ",
        "for (const delta of pick({ delta: [1] }))",
    );
}

#[test]
fn member_name_in_a_call_chain_is_not_circular() {
    assert_no_7022(
        r"
declare const bag: { epsilon: number[]; widen(seed: number[]): number[] };
for (const epsilon of bag.widen(bag.epsilon)) { epsilon; }
        ",
        "for (const epsilon of bag.widen(bag.epsilon))",
    );
}

#[test]
fn method_call_member_name_spelling_the_loop_variable_is_not_circular() {
    assert_no_7022(
        r"
declare const source: { zeta(): number[] };
for (const zeta of source.zeta()) { zeta; }
        ",
        "for (const zeta of source.zeta())",
    );
}

/// Renamed binder: the rule is about the access position, not about any
/// particular spelling, so a differently-named pair behaves identically.
#[test]
fn renamed_binder_member_name_is_not_circular() {
    assert_no_7022(
        r"
declare const registry: { qux: number[] };
for (const qux of registry.qux) { qux; }
        ",
        "for (const qux of registry.qux)",
    );
}

/// Two loops whose operands both spell `theta` as a member name: neither reads
/// a binding called `theta`, so the identity walk declines both.
#[test]
fn same_spelling_property_on_an_unrelated_object_is_not_circular() {
    assert_no_7022(
        r"
declare const first: { theta: number[] };
declare const second: { theta: number[] };
for (const theta of first.theta) { theta; }
for (const iota of second.theta) { iota; }
        ",
        "two loops over same-spelled members",
    );
}

// ---------------------------------------------------------------------------
// Positive rows — real value references must keep reporting.
// ---------------------------------------------------------------------------

#[test]
fn direct_self_reference_is_circular() {
    assert_has_7022(
        r"
for (var lambda of lambda) { lambda; }
        ",
        "for (var lambda of lambda)",
    );
}

#[test]
fn renamed_binder_direct_self_reference_is_circular() {
    assert_has_7022(
        r"
for (var omega of omega) { omega; }
        ",
        "for (var omega of omega)",
    );
}

#[test]
fn array_literal_containing_the_loop_variable_is_circular() {
    assert_has_7022(
        r"
for (var sigma of [sigma]) { sigma; }
        ",
        "for (var sigma of [sigma])",
    );
}

#[test]
fn loop_variable_read_on_the_object_side_of_a_property_access_is_circular() {
    assert_has_7022(
        r"
for (var kappa of kappa.items) { kappa; }
        ",
        "for (var kappa of kappa.items)",
    );
}

/// The *initializer* side of an object-literal property is a real expression
/// position — skipping the written name must not skip the value with it.
#[test]
fn object_literal_property_initializer_reading_the_loop_variable_is_circular() {
    assert_has_7022(
        r"
declare function pick(source: { other: number[] }): number[];
for (const eta of pick({ other: eta })) { eta; }
        ",
        "for (const eta of pick({ other: eta }))",
    );
}

/// A computed property name evaluates its expression, so it reads the binding
/// even though it sits in the same syntactic slot as the written name.
#[test]
fn computed_property_name_reading_the_loop_variable_is_circular() {
    assert_has_7022(
        r"
declare function pick(source: { [k: string]: number[] }): number[];
for (const rho of pick({ [rho]: [1] })) { rho; }
        ",
        "for (const rho of pick({ [rho]: [1] }))",
    );
}

/// A shorthand property's name *is* the reference — a different node kind from
/// the written name, and it must keep its default recursion.
#[test]
fn shorthand_property_reading_the_loop_variable_is_circular() {
    assert_has_7022(
        r"
declare function pick(source: { mu: number[] }): number[];
declare const mu: number[];
for (const mu of pick({ mu })) { mu; }
        ",
        "for (const mu of pick({ mu }))",
    );
}

/// The same walk backs the for-in twin (#16144), so the object-literal rule
/// lands on both loop forms at once.
#[test]
fn for_in_object_literal_property_name_is_not_circular() {
    assert_no_7022(
        r"
for (const nu in { nu: 1 }) { nu; }
        ",
        "for (const nu in { nu: 1 })",
    );
}

#[test]
fn for_in_property_access_member_name_is_not_circular() {
    assert_no_7022(
        r"
declare const box: { psi: { inner: number } };
for (const psi in box.psi) { psi; }
        ",
        "for (const psi in box.psi)",
    );
}

#[test]
fn header_shadowing_self_reference_is_circular() {
    assert_has_7022(
        r"
let tau = [1];
for (let tau of tau) { tau; }
        ",
        "for (let tau of tau) shadowing an outer binding",
    );
}
