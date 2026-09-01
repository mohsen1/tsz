//! Regression coverage for the `symbol` arm of an index-signature key space.
//!
//! When an object's index-signature key type spans `symbol` — directly
//! (`{ [k: symbol]: V }`), through a `PropertyKey` / `string | number | symbol`
//! alias (`{ [k: PropertyKey]: V }`), or via `Record<PropertyKey, V>` — `tsc`'s
//! `keyof` includes `symbol`, so reading or writing the object by any
//! symbol-bearing key is valid. tsz previously dropped the `symbol` arm at three
//! layers (mapped-type lowering, `keyof` key-kind classification, and the
//! indexed-access symbol-slot routing), producing spurious TS2536 (read),
//! TS2862 (write), and TS7053 (value-position implicit any). See #14315.
//!
//! Binder-name invariance: the witnesses vary the alias/parameter/type names so
//! the rule is structural, not keyed off a literal `PropertyKey` spelling.

use crate::CheckerOptions;
use crate::test_utils::{check_source_with_libs, diagnostic_codes, load_default_lib_files};

fn codes(source: &str) -> Vec<u32> {
    diagnostic_codes(&check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions::default(),
        &load_default_lib_files(),
    ))
}

fn assert_clean(source: &str) {
    let found = codes(source);
    assert!(
        found.is_empty(),
        "expected no diagnostics, got {found:?} for source:\n{source}"
    );
}

/// `Record<PropertyKey, V>` — the reported es-toolkit witness. Indexing by
/// `PropertyKey`, `symbol`, or `string | number | symbol` (type position) is
/// clean.
#[test]
fn record_property_key_type_position_index_is_clean() {
    assert_clean(
        r#"
type R = Record<PropertyKey, number>;
type A = R[PropertyKey];
type B = R[symbol];
type C = R[string | number | symbol];
type D = R[string];
type E = R[number];
"#,
    );
}

/// `keyof Record<PropertyKey, V>` must include `symbol`, so a `symbol` value is
/// assignable to it.
#[test]
fn keyof_record_property_key_includes_symbol() {
    assert_clean(
        r#"
type R = Record<PropertyKey, number>;
const k: keyof R = Symbol() as symbol;
"#,
    );
}

/// Value-position read and write by both a broad `symbol` and a `unique symbol`
/// key resolve through the index signature (no TS7053 / TS2862).
#[test]
fn record_property_key_value_position_symbol_access_is_clean() {
    assert_clean(
        r#"
type R = Record<PropertyKey, number>;
declare const r: R;
declare const broad: symbol;
declare const uniq: unique symbol;
const a: number = r[broad];
const b: number = r[uniq];
r[broad] = 1;
r[uniq] = 2;
"#,
    );
}

/// `Record<symbol, V>` (pure symbol key space) — `keyof` is `symbol`, and
/// symbol-keyed access is valid.
#[test]
fn record_symbol_key_space_is_clean() {
    assert_clean(
        r#"
type RS = Record<symbol, boolean>;
type V = RS[symbol];
declare const rs: RS;
declare const s: unique symbol;
const x: boolean = rs[s];
"#,
    );
}

/// An explicit `{ [k: PropertyKey]: V }` written as an interface, a type alias,
/// and an inline type literal — every container form — accepts symbol access in
/// both type and value position. The key parameter name is varied to keep the
/// rule structural.
#[test]
fn explicit_property_key_index_signature_symbol_access_is_clean() {
    assert_clean(
        r#"
interface Bag { [prop: PropertyKey]: string }
type AliasBag = { [entry: PropertyKey]: string };
declare const bag: Bag;
declare const alias: AliasBag;
declare const inline: { [member: PropertyKey]: string };
declare const sym: unique symbol;
type T1 = Bag[symbol];
type T2 = AliasBag[symbol];
const a: string = bag[sym];
const b: string = alias[sym];
const c: string = inline[sym];
bag[sym] = "x";
"#,
    );
}

/// A bare `{ [k: symbol]: V }` index signature accepts symbol access and rejects
/// string access.
#[test]
fn explicit_symbol_index_signature_keyspace() {
    assert_clean(
        r#"
interface SymBag { [k: symbol]: number }
declare const sb: SymBag;
declare const s: unique symbol;
const a: number = sb[s];
type V = SymBag[symbol];
"#,
    );
    // A string key into a symbol-only signature is an error (the fix must not
    // over-accept). tsz routes this to a "cannot index" diagnostic (TS2536 /
    // TS2537); the exact code is an unrelated parity detail.
    let found = codes(
        r#"
interface SymBag { [k: symbol]: number }
type Bad = SymBag[string];
"#,
    );
    assert!(
        found.contains(&2536) || found.contains(&2537),
        "string index into a symbol-only signature must error: {found:?}"
    );
}

/// A generic constrained by `Record<PropertyKey, V>` indexed by `keyof S` stays
/// clean (the constraint's key space carries `symbol`).
#[test]
fn generic_constrained_by_record_property_key_is_clean() {
    assert_clean(
        r#"
function read<S extends Record<PropertyKey, unknown>>(s: S): void {
  type X = S[keyof S];
  void (null as unknown as X);
}
void read;
"#,
    );
}

/// Negative parity: a string-only index signature indexed by `symbol` is still a
/// TS2536, and a number-only one likewise. The fix must not over-accept.
#[test]
fn string_or_number_only_index_by_symbol_still_errors() {
    let str_only = codes(
        r#"
type StrOnly = { [k: string]: number };
type Bad = StrOnly[symbol];
"#,
    );
    assert!(
        str_only.contains(&2538),
        "string-only signature indexed by symbol must emit TS2538 \
         (Type 'symbol' cannot be used as an index type): {str_only:?}"
    );

    let num_only = codes(
        r#"
type NumOnly = { [k: number]: number };
type Bad = NumOnly[symbol];
"#,
    );
    assert!(
        num_only.contains(&2538),
        "number-only signature indexed by symbol must emit TS2538 \
         (Type 'symbol' cannot be used as an index type): {num_only:?}"
    );
}

/// Union value-position writes by a unique symbol require every union member to
/// provide either that exact symbol property or a symbol-bearing index
/// signature. A string/number index signature on another arm is not enough, so
/// tsc reports TS7053 rather than checking assignment against the explicit
/// symbol property's value type.
#[test]
fn union_unique_symbol_write_requires_symbol_surface_on_every_member() {
    let found = codes(
        r#"
const marker = Symbol();
type Both =
  | { [marker]: boolean }
  | { [n: number]: number; [s: string]: string | number };
declare let both: Both;
both[marker] = "not ok";
"#,
    );
    assert!(
        found.contains(&7053) && !found.contains(&2322),
        "unique-symbol write into partial union surface must report TS7053, not TS2322: {found:?}"
    );

    assert_clean(
        r#"
const other = Symbol();
type Both =
  | { [other]: boolean }
  | Record<PropertyKey, boolean>;
declare let both: Both;
both[other] = true;
"#,
    );
}
