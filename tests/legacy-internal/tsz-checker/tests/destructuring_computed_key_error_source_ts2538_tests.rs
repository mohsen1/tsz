//! An unresolved (error-typed) destructuring source must not cascade a TS2538
//! "Type '…' cannot be used as an index type" onto a computed binding key.
//!
//! Structural rule, oracled against `typescript@7.0.2`
//! (`--noEmit --strict --target es2015`): `tsc` runs the computed-key
//! index-type check (TS2538, and its `unique symbol` / `isValidIndexType`
//! siblings) only when the object being destructured is a *genuinely
//! indexable* type — a real object, a primitive, or `unknown`. When the source
//! object type is `any` or an **error** type (here, a `const` annotated with an
//! unresolved type reference), the check is skipped: an error source is treated
//! like `any`, so an invalid computed key over it reports only the errors it
//! deserves on its own, never an extra TS2538 blamed on the unresolved source.
//!
//! tsz already short-circuited an `any` source in the caller
//! (`check_binding_element` skips the whole lookup for an `any` parent) and the
//! matching-index-signature check already guarded error/any/unknown parents,
//! but the index-type-*validity* check did not — so an error source produced a
//! false extra TS2538. The guard now mirrors the caller and the sibling check.
//!
//! `unknown` is deliberately NOT suppressed: `tsc` still reports TS2538 for an
//! invalid key over an `unknown` source, so this fix leaves that path untouched.
//! The negative controls below keep TS2538 for a concrete object source with an
//! invalid key, pinning that the suppression is scoped to the error source, not
//! to the invalid key. Binder/type names are varied across rows so no
//! identifier string is load-bearing.

use crate::test_utils::check_source_strict_codes;

fn codes(source: &str) -> Vec<u32> {
    let mut codes = check_source_strict_codes(source);
    codes.sort_unstable();
    codes
}

const TS2304: u32 = 2304; // Cannot find name '…'
const TS2464: u32 = 2464; // A computed property name must be of type string/number/symbol/any
const TS2538: u32 = 2538; // Type '…' cannot be used as an index type

// ---------------------------------------------------------------------------
// The bug: an error-typed source suppresses TS2538 (matches tsc). Each row
// pairs with a concrete-source control below that keeps TS2538, so the pair
// fails if the guard is dropped OR widened to swallow the concrete case.
// ---------------------------------------------------------------------------

#[test]
fn object_key_over_error_source_drops_ts2538() {
    // `Bogus` is unresolved → `src` is an error type; `bag` is an object, an
    // invalid computed key (TS2464). tsc does not add TS2538 for the error
    // source. TS2304 is for the unresolved `Bogus`.
    let source = "declare const src: Bogus; const bag = {}; const { [bag]: two } = src;";
    assert_eq!(codes(source), vec![TS2304, TS2464]);
}

#[test]
fn string_key_over_error_source_reports_only_the_unresolved_type() {
    // A plain string key over an error source: nothing to report but the
    // unresolved `Missing` type itself.
    let source = "declare const holder: Missing; declare const label: string; const { [label]: one } = holder;";
    assert_eq!(codes(source), vec![TS2304]);
}

#[test]
fn unique_symbol_key_over_error_source_drops_ts2538() {
    // A `unique symbol` key over an error source is likewise not an index-type
    // error to report against the unresolved source.
    let source =
        "declare const bag: Nope; declare const sym: unique symbol; const { [sym]: v } = bag;";
    assert_eq!(codes(source), vec![TS2304]);
}

// ---------------------------------------------------------------------------
// Negative controls: a real indexable source still reports TS2538.
// ---------------------------------------------------------------------------

#[test]
fn object_key_over_concrete_source_keeps_ts2538() {
    // Same object key against a concrete object source keeps both TS2464 and
    // TS2538 — the suppression is scoped to the error source, not the key.
    let source =
        "declare const src: { a: number }; const bucket = {}; const { [bucket]: two } = src;";
    assert_eq!(codes(source), vec![TS2464, TS2538]);
}

#[test]
fn unique_symbol_key_over_concrete_source_keeps_ts2538() {
    // A `unique symbol` key with no matching property on a concrete source is
    // an index-type error; the source is indexable, so TS2538 survives.
    let source = "declare const holder: { a: number }; declare const sym: unique symbol; const { [sym]: v } = holder;";
    assert_eq!(codes(source), vec![TS2538]);
}
