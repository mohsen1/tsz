//! Regression tests for `noUncheckedIndexedAccess` value-level access
//! through an `any`-typed index expression.
//!
//! Type-level `T[any]` resolves to `any` (TypeScript's idiomatic rule), but
//! value-level element access expressions like `obj[someAny]` are different:
//! tsc routes through the receiver's applicable index signature so NUIA
//! still widens reads to `T | undefined` and rejects writes of `undefined`
//! against the un-widened slot type.
//!
//! Source: `conformance/pedantic/noUncheckedIndexedAccess.ts`
//! (`const e14: boolean = strMap[null as any];` and
//!  `strMap[null as any] = undefined;`).
//!
//! Structural rule (one sentence): when the index expression of a value-level
//! element access has type `any`, the result is the receiver's applicable
//! index-signature value type with the standard NUIA read/write split, not
//! the type-level `any` short-circuit.

use tsz_common::options::checker::CheckerOptions;

fn diags_for_strict_nuia(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    let opts = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        no_unchecked_indexed_access: true,
        ..CheckerOptions::default()
    };
    crate::test_utils::check_source(source, "test.ts", opts)
}

/// Read-side: an `any`-typed index expression must still widen the access
/// type to `T | undefined`, otherwise assigning the result to a non-undefined
/// slot silently typechecks as `any`.
#[test]
fn nuia_read_with_any_index_widens_to_undefined_and_emits_ts2322() {
    let source = r#"
declare const strMap: { [s: string]: boolean };
const e: boolean = strMap[null as any];
"#;
    let diags = diags_for_strict_nuia(source);
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2322),
        "NUIA read with any-typed index must emit TS2322 (boolean|undefined → boolean). Got: {codes:?}",
    );
}

/// Anti-hardcoding: same shape, different identifier names and value type.
/// The fix is not keyed off `strMap`/`boolean`.
#[test]
fn nuia_read_with_any_index_renamed_identifier() {
    let source = r#"
declare const lookupTable: { [k: string]: number };
declare const idx: any;
const v: number = lookupTable[idx];
"#;
    let diags = diags_for_strict_nuia(source);
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2322),
        "Renamed: NUIA read with any-typed index must emit TS2322 (number|undefined → number). Got: {codes:?}",
    );
}

/// Write-side: writing `undefined` through an `any`-typed index expression
/// must still emit TS2322 because NUIA's `| undefined` widening only applies
/// to reads. The write target stays the un-widened slot type.
#[test]
fn nuia_write_undefined_with_any_index_emits_ts2322() {
    let source = r#"
declare const strMap: { [s: string]: boolean };
strMap[null as any] = undefined;
"#;
    let diags = diags_for_strict_nuia(source);
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2322),
        "NUIA write of undefined through any-index must emit TS2322. Got: {codes:?}",
    );
}

/// Write-side anti-hardcoding cover: different identifiers, different value
/// type. Confirms the rule is structural, not keyed to the test's names.
#[test]
fn nuia_write_undefined_with_any_index_renamed_identifier() {
    let source = r#"
declare const lookupTable: { [k: string]: number };
declare const idx: any;
lookupTable[idx] = undefined;
"#;
    let diags = diags_for_strict_nuia(source);
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2322),
        "Renamed: NUIA write of undefined through any-index must emit TS2322. Got: {codes:?}",
    );
}

/// Negative control 1: a `T | undefined` slot accepts the NUIA-widened read,
/// so no TS2322 should fire for `const x: T | undefined = obj[anyExpr]`.
#[test]
fn nuia_read_with_any_index_to_t_or_undefined_slot_does_not_emit_ts2322() {
    let source = r#"
declare const strMap: { [s: string]: boolean };
const x: boolean | undefined = strMap[null as any];
"#;
    let diags = diags_for_strict_nuia(source);
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2322),
        "NUIA-widened read should be assignable to `boolean | undefined`. Got: {codes:?}",
    );
}

/// Negative control 2: when the receiver has no applicable index signature,
/// `obj[any]` falls back to the standard `any` rule so the access still
/// typechecks. Otherwise we'd be emitting spurious TS2322 on plain `any`
/// reads.
#[test]
fn no_index_signature_with_any_index_falls_back_to_any() {
    let source = r#"
declare const obj: { a: boolean };
declare const idx: any;
const x: boolean = obj[idx];
"#;
    let diags = diags_for_strict_nuia(source);
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2322),
        "Receiver without index signature must keep the type-level `any` fallback for any-typed indices. Got: {codes:?}",
    );
}

/// Negative control 3: the type-level rule `T[any] = any` must still hold
/// for type aliases and constraint checks (verifies the value-level fix
/// doesn't leak into the type-level evaluator, which would create an extra
/// TS2344 when `any` is replaced by `T | undefined` in type position).
#[test]
fn type_level_t_any_does_not_widen_to_undefined_under_nuia() {
    let source = r#"
type CheckBooleanOnly<T extends boolean> = T;
declare const strMap: { [s: string]: boolean };
type T_OK = CheckBooleanOnly<(typeof strMap)[any]>;
"#;
    let diags = diags_for_strict_nuia(source);
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2344),
        "Type-level `T[any]` must remain `any` under NUIA (no extra TS2344). Got: {codes:?}",
    );
}

/// Generic-key write: when the index is a type parameter constrained by
/// `keyof <receiver>` and the receiver is concrete, tsc preserves the
/// deferred `Receiver[K]` form for the WRITE target so writes of `undefined`
/// still emit TS2322. Without preservation, the read-side NUIA widening
/// makes the LHS `T | undefined` and `undefined` slips through.
#[test]
fn nuia_generic_key_write_undefined_emits_ts2322() {
    let source = r#"
declare const myRecord: { a: string; b: string; [key: string]: string };
const fn = <Key extends keyof typeof myRecord>(key: Key) => {
    myRecord[key] = undefined;
};
"#;
    let diags = diags_for_strict_nuia(source);
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2322),
        "Generic-key write of undefined must emit TS2322. Got: {codes:?}",
    );
}

/// Anti-hardcoding cover for the generic-key write rule: different type
/// parameter name and identifier names confirm the rule is structural.
#[test]
fn nuia_generic_key_write_undefined_renamed_type_parameter() {
    let source = r#"
declare const lookupTable: { foo: number; bar: number; [key: string]: number };
const setter = <P extends keyof typeof lookupTable>(p: P) => {
    lookupTable[p] = undefined;
};
"#;
    let diags = diags_for_strict_nuia(source);
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2322),
        "Renamed: generic-key write of undefined must still emit TS2322. Got: {codes:?}",
    );
}

/// Negative control: the read side of a generic-key access keeps using the
/// resolved value type (with NUIA `| undefined`) so the standard variable
/// initialiser path (`const v: string = obj[key]`) still surfaces the
/// `string | undefined → string` mismatch — nothing about the write
/// preservation should change reads.
#[test]
fn nuia_generic_key_read_still_emits_ts2322_against_strict_slot() {
    let source = r#"
declare const myRecord: { a: string; b: string; [key: string]: string };
const fn = <Key extends keyof typeof myRecord>(key: Key) => {
    const v: string = myRecord[key];
    return v;
};
"#;
    let diags = diags_for_strict_nuia(source);
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2322),
        "Generic-key read should still emit TS2322 for `string | undefined → string`. Got: {codes:?}",
    );
}
