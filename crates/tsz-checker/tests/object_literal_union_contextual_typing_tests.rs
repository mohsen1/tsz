//! Tests for contextual typing of object literals against union-of-objects targets.
//!
//! When the declared type is a union whose members share a property at different
//! literal-vs-widened precision (e.g. `{ k: 2 } | { k: number; j: boolean }`), a
//! fresh object literal on the right-hand side must be contextually typed
//! per-property: property `k` sees `2 | number` as its contextual type, which
//! preserves the number literal `2` instead of widening it to `number`. Without
//! literal preservation the inferred `{ k: number }` matches neither arm (the
//! literal arm by value, the wider arm by arity) and tsz emitted a spurious
//! TS2322.
//!
//! Root cause: `contextual_object_literal_property_type` already builds the
//! per-property contextual type with `union_preserve_members` (`2 | number`),
//! mirroring tsc's `getTypeOfPropertyOfContextualType` under
//! `UnionReduction.None`. But `prefer_more_specific_contextual_property_type`
//! then collapsed that union back to its widened member `number` via the
//! "union contains the candidate" rule, dropping the literal arm.
//!
//! Fix: only collapse a union to a contained member when the member is a
//! *strict* subset. When the union reduces to exactly that member as a set
//! (`2 | number` denotes the same values as `number`), the un-reduced union is
//! the literal-preserving form and is kept.
//!
//! This is the object-literal counterpart of
//! `tuple_union_contextual_typing_tests.rs`.

use tsz_checker::test_utils::{check_source_codes, check_source_strict_codes};

// ---------------------------------------------------------------------------
// Core cases - differing-arity object unions, literal arm preserved
// ---------------------------------------------------------------------------

/// Assigning `{ k: 2 }` to `{ k: 2 } | { k: number; j: boolean }` must NOT emit
/// TS2322: the fresh `2` is preserved by the `2 | number` contextual type and
/// matches the literal arm.
#[test]
fn object_literal_assignable_to_number_literal_arm() {
    let codes = check_source_codes(
        r#"
const a: { k: number; j: boolean } | { k: 2 } = { k: 2 };
const b: { k: 2 } | { k: number; j: boolean } = { k: 2 };
"#,
    );
    assert!(!codes.contains(&2322), "expected no TS2322, got: {codes:?}");
}

/// Same shape with string literals.
#[test]
fn object_literal_assignable_to_string_literal_arm() {
    let codes = check_source_codes(
        r#"
const a: { a: string; b: number } | { a: "x" } = { a: "x" };
const b: { a: "x" } | { a: string; b: number } = { a: "x" };
"#,
    );
    assert!(!codes.contains(&2322), "expected no TS2322, got: {codes:?}");
}

/// Same shape with bigint literals.
#[test]
fn object_literal_assignable_to_bigint_literal_arm() {
    let codes = check_source_codes(
        r#"
const a: { n: bigint; extra: boolean } | { n: 2n } = { n: 2n };
"#,
    );
    assert!(!codes.contains(&2322), "expected no TS2322, got: {codes:?}");
}

/// Renamed binders (different property/alias names) must behave identically —
/// the rule is structural, not keyed on any identifier.
#[test]
fn object_literal_union_renamed_binders() {
    let codes = check_source_codes(
        r#"
type Wide = { value: number; flag: boolean };
type Narrow = { value: 7 };
const a: Wide | Narrow = { value: 7 };
const b: Narrow | Wide = { value: 7 };
"#,
    );
    assert!(!codes.contains(&2322), "expected no TS2322, got: {codes:?}");
}

// ---------------------------------------------------------------------------
// Through a type alias (the original large-ts-repo witness shape)
// ---------------------------------------------------------------------------

#[test]
fn object_literal_union_through_alias() {
    let codes = check_source_codes(
        r#"
type Lit = { k: 2 };
const a: Lit | { k: number; j: boolean } = { k: 2 };
"#,
    );
    assert!(!codes.contains(&2322), "expected no TS2322, got: {codes:?}");
}

// ---------------------------------------------------------------------------
// Nested object literals
// ---------------------------------------------------------------------------

#[test]
fn nested_object_literal_union_preserves_literal() {
    let codes = check_source_codes(
        r#"
const a: { o: { k: 2 } } | { o: { k: number; j: boolean } } = { o: { k: 2 } };
"#,
    );
    assert!(!codes.contains(&2322), "expected no TS2322, got: {codes:?}");
}

// ---------------------------------------------------------------------------
// Discriminated unions (same-key, same-arity) must remain unaffected
// ---------------------------------------------------------------------------

#[test]
fn discriminated_union_object_literal_ok() {
    let codes = check_source_codes(
        r#"
type Shape = { tag: "a"; v: number } | { tag: "b"; v: string };
const a: Shape = { tag: "b", v: "hi" };
const b: Shape = { tag: "a", v: 3 };
"#,
    );
    assert!(!codes.contains(&2322), "expected no TS2322, got: {codes:?}");
}

// ---------------------------------------------------------------------------
// Negative cases - genuine mismatches must STILL error
// ---------------------------------------------------------------------------

/// A literal that matches no arm by value or arity must still emit TS2322.
#[test]
fn object_literal_not_in_union_still_errors() {
    let codes = check_source_codes(
        r#"
const z: { k: 2 } | { m: string } = { k: 3 };
"#,
    );
    assert!(codes.contains(&2322), "expected TS2322, got: {codes:?}");
}

/// A string literal outside both arms' literal sets must still error.
#[test]
fn object_literal_wrong_string_literal_still_errors() {
    let codes = check_source_codes(
        r#"
const z: { a: "x" } | { a: "y"; b: number } = { a: "z" };
"#,
    );
    assert!(codes.contains(&2322), "expected TS2322, got: {codes:?}");
}

/// The literal arm is matched, but a missing required property in the only
/// arm whose discriminant fits must still error (no accidental over-acceptance).
#[test]
fn object_literal_missing_required_property_still_errors() {
    let codes = check_source_codes(
        r#"
const z: { k: 2; extra: string } | { k: number; j: boolean } = { k: 2 };
"#,
    );
    assert!(codes.contains(&2322), "expected TS2322, got: {codes:?}");
}

// ---------------------------------------------------------------------------
// Genuinely-wider union member must still be preferred (no over-fix)
// ---------------------------------------------------------------------------

/// When the union does NOT reduce to a single contained member — the two arms
/// carry disjoint primitive property types (`string` vs `number`) — the
/// per-property contextual type is a real `string | number` and a literal
/// argument is still accepted on the matching side.
#[test]
fn object_literal_disjoint_primitive_union_ok() {
    let codes = check_source_codes(
        r#"
const a: { v: string } | { v: number } = { v: "hello" };
const b: { v: string } | { v: number } = { v: 42 };
"#,
    );
    assert!(!codes.contains(&2322), "expected no TS2322, got: {codes:?}");
}

// ---------------------------------------------------------------------------
// Object-literal METHOD/property params contextually typed from a union
// annotation (jotai `atomWithStorage` / `createJSONStorage`).
//
// Two roots are exercised here:
//   1. The discriminant pre-scan of a union contextual type must not check
//      function/object/array initializers context-free — doing so committed a
//      premature TS7006 on the method's parameters before the real, contextually
//      typed pass ran.
//   2. The union contextual *parameter* extraction is signature-level: params are
//      contextually typed iff the callable members agree at every shared position
//      (tsc `getUnionSignatures`); if any position disagrees, every parameter is
//      implicit-any (#5840), not just the disagreeing one.
// ---------------------------------------------------------------------------

/// Repro: union members declare `getItem` with identical parameter types and
/// differ only in return type (`Promise<Value>` vs `Value`). tsc contextually
/// types `key`/`initialValue`; tsz used to emit spurious TS7006.
#[test]
fn union_member_identical_params_method_is_contextually_typed() {
    let codes = check_source_strict_codes(
        r#"
interface AsyncStorage<Value> { getItem: (key: string, initialValue: Value) => Promise<Value> }
interface SyncStorage<Value>  { getItem: (key: string, initialValue: Value) => Value }
function make<Value>() {
  const storage: AsyncStorage<Value> | SyncStorage<Value> = {
    getItem: (key, initialValue) => initialValue,
  };
  return storage;
}
"#,
    );
    assert!(!codes.contains(&7006), "expected no TS7006, got: {codes:?}");
}

/// Renamed binders + a second method (`setItem`) and a third union arm: the fix
/// must not depend on any particular property/identifier name.
#[test]
fn union_member_identical_params_multi_method_three_arms_contextual() {
    let codes = check_source_strict_codes(
        r#"
interface A<V> { read: (slot: string, seed: V) => Promise<V>; write: (slot: string, next: V) => void }
interface B<V> { read: (slot: string, seed: V) => V; write: (slot: string, next: V) => void }
interface C<V> { read: (slot: string, seed: V) => V | null; write: (slot: string, next: V) => void }
function make<V>() {
  const store: A<V> | B<V> | C<V> = {
    read: (slot, seed) => seed,
    write: (slot, next) => {},
  };
  return store;
}
"#,
    );
    assert!(!codes.contains(&7006), "expected no TS7006, got: {codes:?}");
}

/// Optional union member (`| undefined`) still contextually types the method.
#[test]
fn union_with_undefined_member_method_is_contextually_typed() {
    let codes = check_source_strict_codes(
        r#"
interface AsyncStorage<Value> { getItem: (key: string, initialValue: Value) => Promise<Value> }
interface SyncStorage<Value>  { getItem: (key: string, initialValue: Value) => Value }
function make<Value>() {
  const storage: AsyncStorage<Value> | SyncStorage<Value> | undefined = {
    getItem: (key, initialValue) => initialValue,
  };
  return storage;
}
"#,
    );
    assert!(!codes.contains(&7006), "expected no TS7006, got: {codes:?}");
}

/// Negative (#5840): when the union members disagree on a parameter type, the
/// union provides NO contextual signature, so EVERY parameter is implicit-any —
/// not just the one that differs.
#[test]
fn union_member_disagreeing_params_keeps_all_implicit_any() {
    let codes = check_source_strict_codes(
        r#"
interface A<V> { getItem: (key: string, v: V) => V }
interface B<V> { getItem: (key: number, v: V) => V }
function make<V>() {
  const s: A<V> | B<V> = { getItem: (key, v) => v };
  return s;
}
"#,
    );
    // Both `key` (disagrees) and `v` (agrees) must be implicit-any.
    let ts7006 = codes.iter().filter(|&&c| c == 7006).count();
    assert_eq!(ts7006, 2, "expected TS7006 on both params, got: {codes:?}");
}

/// Negative: first parameter agrees, second disagrees → still all implicit-any.
#[test]
fn union_member_later_param_disagrees_keeps_all_implicit_any() {
    let codes = check_source_strict_codes(
        r#"
interface A { f: (a: string, b: string) => void }
interface B { f: (a: string, b: number) => void }
const o: A | B = { f: (a, b) => {} };
"#,
    );
    let ts7006 = codes.iter().filter(|&&c| c == 7006).count();
    assert_eq!(ts7006, 2, "expected TS7006 on both params, got: {codes:?}");
}

/// Arity gap is not a disagreement: the longer member's extra parameter is
/// contextually typed from the member that declares it.
#[test]
fn union_member_arity_gap_still_contextually_types_extra_param() {
    let codes = check_source_strict_codes(
        r#"
interface A { f: (a: string, b: number) => void }
interface B { f: (a: string) => void }
const o: A | B = { f: (a, b) => { a.toUpperCase(); b.toFixed(); } };
"#,
    );
    assert!(!codes.contains(&7006), "expected no TS7006, got: {codes:?}");
    assert!(!codes.contains(&2339), "expected no TS2339, got: {codes:?}");
}
