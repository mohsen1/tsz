//! Regression tests for false-positive TS2322 when assigning `T[K]` to a
//! write target of the form `Partial<Pick<T,K>>[K]` or any similar utility
//! type chain that expands to a mapped type whose constraint, when evaluated,
//! equals the index type parameter K.
//!
//! Structural rule: when indexing `{ [P in C]: F(P) }` with type parameter K
//! and `evaluate(C) == evaluate(K)`, K is a valid key and the result is
//! `F(K)` (with optional-modifier semantics applied). The fix lives in
//! `tsz_solver::evaluation::evaluate_rules::index_access` — both the
//! pre-visitor fast path (`try_mapped_type_param_substitution`) and the
//! visitor `can_substitute` check now also match when `evaluate(K) ==
//! evaluate(mapped.constraint)`.

use crate::test_utils::check_source_codes;

// ── helpers ──────────────────────────────────────────────────────────────────

fn codes(source: &str) -> Vec<u32> {
    check_source_codes(source)
}

// ── Partial<Pick<T, K>> — the reported repro ─────────────────────────────────

/// The exact pattern from issue #6510: copying properties from `obj: T`
/// into `result: Partial<Pick<T, K>>` using the key `key: K`.
///
/// tsc does not emit TS2322 here; tsz was emitting a false positive.
#[test]
fn partial_pick_write_does_not_emit_ts2322() {
    let source = r#"
function copyProps<T, K extends keyof T>(obj: T, result: Partial<Pick<T, K>>, key: K): void {
    result[key] = obj[key];
}
"#;
    let diags = codes(source);
    assert!(
        !diags.contains(&2322),
        "Partial<Pick<T,K>>[K] = T[K] must not emit TS2322, got: {diags:?}"
    );
}

/// Anti-hardcoding: same logic with renamed type parameters (`U`/`V`).
/// Proves the fix is keyed on structural semantics, not type-parameter names.
#[test]
fn partial_pick_write_renamed_type_params_does_not_emit_ts2322() {
    let source = r#"
function copyProps<U, V extends keyof U>(obj: U, result: Partial<Pick<U, V>>, key: V): void {
    result[key] = obj[key];
}
"#;
    let diags = codes(source);
    assert!(
        !diags.contains(&2322),
        "Renamed type params: Partial<Pick<U,V>>[V] = U[V] must not emit TS2322, got: {diags:?}"
    );
}

/// Anti-hardcoding: subset of keys — `Pick<T, 'a' | 'b'>` — still works.
#[test]
fn partial_pick_literal_union_key_write_does_not_emit_ts2322() {
    let source = r#"
interface Rec { a: number; b: string; c: boolean; }
function copyAB(src: Rec, result: Partial<Pick<Rec, 'a' | 'b'>>, key: 'a' | 'b'): void {
    result[key] = src[key];
}
"#;
    let diags = codes(source);
    assert!(
        !diags.contains(&2322),
        "Partial<Pick<Rec,'a'|'b'>>[key] = Rec[key] must not emit TS2322, got: {diags:?}"
    );
}

// ── Negative controls: genuine type mismatches must still emit ───────────────

/// Assigning a `string` literal to a generic slot that has nothing to do with
/// Partial/Pick must still fail. Confirms the fix is not globally suppressing
/// TS2322 in generic contexts.
#[test]
fn direct_string_to_number_generic_slot_emits_ts2322() {
    let source = r#"
function bad<T extends { a: number }>(obj: T): void {
    (obj as { a: number }).a = "not a number";
}
"#;
    let diags = codes(source);
    assert!(
        diags.contains(&2322),
        "Direct string→number assignment must still emit TS2322, got: {diags:?}"
    );
}

/// A simple number → string assignment not involving Partial/Pick confirms
/// that TS2322 is still emitted for basic incompatible assignments (the fix
/// does not globally suppress TS2322).
#[test]
fn simple_incompatible_assignment_still_emits_ts2322() {
    let source = r#"
let x: number = "not a number";
"#;
    let diags = codes(source);
    assert!(
        diags.contains(&2322),
        "string→number direct assignment must still emit TS2322, got: {diags:?}"
    );
}
