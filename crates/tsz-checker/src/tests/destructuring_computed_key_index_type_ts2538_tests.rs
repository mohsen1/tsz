//! Regression coverage for the computed-key destructuring TS2538 family.
//!
//! A computed binding key desugars to element access:
//! `const { [k]: v } = obj` is `v = obj[k]`. The index-type-validity decision
//! (`TS2538 "Type 'X' cannot be used as an index type"`) must therefore consult
//! the *source* object type exactly like element access does, not just the key:
//!
//! * An **error-typed source** (an unresolved reference) cascades like `any`;
//!   `tsc` runs no `isValidIndexType` check against the computed key, so no
//!   TS2538 piles on top of the errors it already reports (#17529).
//! * A **`symbol`/`unique symbol` key over a `symbol` index signature**
//!   (`{ [k: symbol]: V }`, `Record<symbol, V>`, `Record<PropertyKey, V>`)
//!   resolves through that signature to its value type `V` — no TS2538, and the
//!   binding is typed `V`, mirroring `obj[s]` (#17528).
//!
//! The negative controls (`any`/`unknown`/concrete sources, symbol-only string
//! index) keep `tsc`'s behavior. Binder names are varied so the rule stays
//! structural rather than keyed off any literal identifier.

use crate::CheckerOptions;
use crate::test_utils::{
    check_source_with_libs, diagnostic_codes, load_default_lib_files, non_strict_checker_options,
};

fn codes_with(source: &str, options: CheckerOptions) -> Vec<u32> {
    diagnostic_codes(&check_source_with_libs(
        source,
        "test.ts",
        options,
        &load_default_lib_files(),
    ))
}

fn codes(source: &str) -> Vec<u32> {
    codes_with(source, non_strict_checker_options())
}

fn assert_no_ts2538(source: &str) {
    let found = codes(source);
    assert!(
        !found.contains(&2538),
        "expected no TS2538, got {found:?} for source:\n{source}"
    );
}

fn assert_has_ts2538(source: &str) {
    let found = codes(source);
    assert!(
        found.contains(&2538),
        "expected TS2538, got {found:?} for source:\n{source}"
    );
}

// ---------------------------------------------------------------------------
// #17528 — a `symbol` key over a `symbol` index signature is valid.
// ---------------------------------------------------------------------------

/// `Record<symbol, V>` exposes a `symbol` index signature; a `unique symbol`
/// computed key resolves through it with no TS2538.
#[test]
fn symbol_key_over_record_symbol_index_is_clean() {
    assert_no_ts2538(
        r#"
const marker = Symbol();
type Bag = Record<symbol, number>;
const { [marker]: picked } = {} as Bag;
"#,
    );
}

/// An inline `{ [k: symbol]: V }` signature behaves identically — the rule is
/// structural, not tied to the `Record` alias spelling.
#[test]
fn symbol_key_over_inline_symbol_index_is_clean() {
    assert_no_ts2538(
        r#"
const tag = Symbol();
const { [tag]: value } = {} as { [entry: symbol]: string };
"#,
    );
}

/// A `PropertyKey` index signature (`string | number | symbol`) accepts a
/// symbol key too.
#[test]
fn symbol_key_over_property_key_index_is_clean() {
    assert_no_ts2538(
        r#"
const slot = Symbol();
const { [slot]: read } = {} as Record<PropertyKey, boolean>;
"#,
    );
}

/// A generic alias application that instantiates to a `symbol`-indexed object
/// still resolves the key structurally.
#[test]
fn symbol_key_over_generic_symbol_index_application_is_clean() {
    assert_no_ts2538(
        r#"
type Dict<V> = { [pos: symbol]: V };
const handle = Symbol();
const { [handle]: element } = {} as Dict<number>;
"#,
    );
}

/// A union source is symbol-indexable only when *every* member is; both member
/// orderings resolve cleanly.
#[test]
fn symbol_key_over_all_symbol_indexed_union_is_clean() {
    assert_no_ts2538(
        r#"
const key = Symbol();
type L = { [a: symbol]: number };
type R = { [b: symbol]: string };
const { [key]: left } = {} as L | R;
const { [key]: right } = {} as R | L;
"#,
    );
}

/// The resolved binding type is the signature's value type, not `any`:
/// assigning it to an incompatible annotation must still error (TS2322).
#[test]
fn symbol_index_binding_resolves_to_value_type() {
    let found = codes(
        r#"
const marker = Symbol();
const { [marker]: picked } = {} as Record<symbol, number>;
const widened: string = picked;
"#,
    );
    assert!(!found.contains(&2538), "expected no TS2538, got {found:?}");
    assert!(
        found.contains(&2322),
        "expected TS2322 proving the binding is `number`, not `any`; got {found:?}"
    );
}

// ---------------------------------------------------------------------------
// #17528 — negative controls: sources WITHOUT a symbol-accepting index keep TS2538.
// ---------------------------------------------------------------------------

/// No symbol member at all → TS2538 preserved.
#[test]
fn symbol_key_over_object_without_symbol_member_keeps_ts2538() {
    assert_has_ts2538(
        r#"
const marker = Symbol();
const { [marker]: v } = {} as { a: number };
"#,
    );
}

/// A bare `string` index signature cannot accept a symbol key → TS2538 preserved.
#[test]
fn symbol_key_over_string_index_only_keeps_ts2538() {
    assert_has_ts2538(
        r#"
const marker = Symbol();
const { [marker]: v } = {} as { [s: string]: number };
"#,
    );
}

// ---------------------------------------------------------------------------
// #17529 — an error-typed source cascades like `any`: no extra TS2538.
// ---------------------------------------------------------------------------

/// A `unique symbol` key over an error-typed source (an unresolved type
/// reference) reports only the source's own error (TS2304), never an extra
/// TS2538. The `unique symbol` key takes the property-not-found path, so this
/// exercises the error-source guard there.
#[test]
fn unique_symbol_key_over_error_source_suppresses_ts2538() {
    let found = codes(
        r#"
declare const src: Bogus;
const tag = Symbol();
const { [tag]: v } = src;
"#,
    );
    assert!(
        found.contains(&2304),
        "expected TS2304 for the unresolved source type, got {found:?}"
    );
    assert!(
        !found.contains(&2538),
        "an error-typed source must not add TS2538, got {found:?}"
    );
}

/// A bare `symbol` key (dynamic, property-name-less) over an error-typed source
/// is suppressed by the same source gate on the computed-key validity block.
#[test]
fn wide_symbol_key_over_error_source_suppresses_ts2538() {
    let found = codes(
        r#"
declare const src: Bogus;
declare const wide: symbol;
const { [wide]: v } = src;
"#,
    );
    assert!(
        found.contains(&2304),
        "expected TS2304 for the unresolved source type, got {found:?}"
    );
    assert!(
        !found.contains(&2538),
        "an error-typed source must not add TS2538, got {found:?}"
    );
}

// ---------------------------------------------------------------------------
// Source-shape controls that must stay unchanged.
// ---------------------------------------------------------------------------

/// A concrete source with no symbol index keeps TS2538 for a symbol key.
#[test]
fn symbol_key_over_concrete_source_keeps_ts2538() {
    assert_has_ts2538(
        r#"
const marker = Symbol();
declare const obj: { a: number };
const { [marker]: v } = obj;
"#,
    );
}

/// An `any` source never produces TS2538 (the caller short-circuits it).
#[test]
fn symbol_key_over_any_source_is_clean() {
    assert_no_ts2538(
        r#"
const marker = Symbol();
declare const anything: any;
const { [marker]: v } = anything;
"#,
    );
}
