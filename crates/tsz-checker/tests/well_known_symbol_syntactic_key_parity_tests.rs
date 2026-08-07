//! Parity matrix for the well-known-symbol *syntactic* key rule (#16307).
//!
//! `tsc` decides whether `Symbol.<member>` denotes the well-known symbol itself
//! from the SYNTAX (`isWellKnownSymbolSyntactically`) — reach to the global
//! `Symbol` value — never from `<member>`'s declared kind on the (possibly
//! user-augmented) `SymbolConstructor` interface. So a `declare global`
//! augmentation that types a member as plain `symbol` (xstate's
//! `Symbol.observable` interop convention) does NOT make `[Symbol.observable]`
//! fold into a symbol index signature: it stays its own named member, and a
//! wide `symbol` key cannot index the containing type.
//!
//! Every row below was oracle-verified against the pinned `typescript@7.0.2`
//! (`scripts/conformance/typescript-versions.json`) with
//! `--noEmit --strict --lib esnext --target es2022`.
//!
//! Coverage history, because this issue has repeatedly been mis-tracked: the
//! *assignability* half of this rule is pinned in
//! `wide_symbol_computed_member_index_signature_tests.rs`, but the ELEMENT
//! ACCESS half — the reads that must report `TS7053` — had no coverage at all,
//! which is why two board sessions carried it as "still open" for two days
//! after it had actually started passing. Same failure mode #16572 was opened
//! to fix for the two other legs of this issue.

use tsz_checker::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs, load_default_lib_files};

fn diagnostic_codes(source: &str) -> Vec<u32> {
    let libs = load_default_lib_files();
    check_source_with_libs(source, "test.ts", CheckerOptions::default(), &libs)
        .iter()
        .map(|d| d.code)
        .collect()
}

fn assert_reports_ts7053(label: &str, source: &str) {
    let codes = diagnostic_codes(source);
    assert!(
        codes.contains(&7053),
        "{label}: a wide `symbol` key cannot index a type with no symbol index \
         signature; tsc 7.0.2 reports TS7053 here, got: {codes:?}"
    );
}

#[test]
fn wide_symbol_read_against_a_well_known_keyed_interface_reports_ts7053() {
    // `[Symbol.observable]` is a NAMED member even though the augmentation
    // types `observable` as plain `symbol`, so `I` has no symbol index
    // signature and `i[other]` is an implicit-any element access.
    assert_reports_ts7053(
        "interface",
        r#"
declare global { interface SymbolConstructor { readonly observable: symbol } }
declare const other: symbol;
interface I { [Symbol.observable]: number }
declare const i: I;
export const a = i[other];
"#,
    );
}

#[test]
fn wide_symbol_read_against_a_well_known_keyed_class_reports_ts7053() {
    assert_reports_ts7053(
        "class",
        r#"
declare global { interface SymbolConstructor { readonly observable: symbol } }
declare const other: symbol;
class C { [Symbol.observable]() { return 1 } }
declare const c: C;
export const b = c[other];
"#,
    );
}

#[test]
fn wide_symbol_read_against_a_well_known_keyed_object_literal_reports_ts7053() {
    assert_reports_ts7053(
        "object literal",
        r#"
declare global { interface SymbolConstructor { readonly observable: symbol } }
declare const other: symbol;
const o = { [Symbol.observable]: 1 };
export const d = o[other];
"#,
    );
}

#[test]
fn wide_symbol_read_against_a_plain_named_interface_reports_ts7053() {
    // Negative control on the other axis: no symbols are involved in the
    // TARGET at all. The rule is about the receiver lacking a symbol index
    // signature, not about how its members were keyed — so a plain interface
    // must report the same TS7053, and a fix in this family must not be
    // narrowed to symbol-keyed receivers.
    assert_reports_ts7053(
        "plain named interface",
        r#"
declare const other: symbol;
interface P { m: number }
declare const p: P;
export const e = p[other];
"#,
    );
}

#[test]
fn wide_symbol_read_against_a_genuine_well_known_keyed_interface_reports_ts7053() {
    // `Symbol.iterator` is a real `unique symbol` on `SymbolConstructor`, so
    // this row must behave identically to the augmented-wide rows above —
    // proving the reported behaviour does not depend on the declared kind.
    assert_reports_ts7053(
        "Symbol.iterator",
        r#"
declare const other: symbol;
interface H { [Symbol.iterator](): number }
declare const h: H;
export const f = h[other];
"#,
    );
}

#[test]
fn well_known_keyed_interface_is_not_assignable_to_a_symbol_index_signature() {
    // The assignability consequence of the same rule, in the direction the
    // element-access rows above do not cover: because `[Symbol.observable]` is
    // a named member and not an index signature, `I` does not satisfy
    // `{ [k: symbol]: number }`. tsc 7.0.2 reports TS2322 with
    // "Index signature for type 'symbol' is missing in type 'I'".
    let codes = diagnostic_codes(
        r#"
declare global { interface SymbolConstructor { readonly observable: symbol } }
interface I { [Symbol.observable]: number }
declare const i: I;
export const t: { [k: symbol]: number } = i;
"#,
    );
    assert!(
        codes.contains(&2322),
        "a well-known-keyed interface supplies no symbol index signature, so \
         tsc reports TS2322; got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Known-failing rows (#16605), filed as their own issue rather than fixed here.
//
// The two rows below are `keyof` / indexed-access over a WELL-KNOWN symbol key,
// the `UniqueSymbol(SymbolRef)` <-> `[Symbol.xxx]` atom round-trip. The failure
// is NOT merely that `symbol_named_atom_from_unique_symbol_ref` is unapplied on
// these paths — applying it is necessary but not sufficient, because of two
// coupled defects the pins below the rows document and verify:
//
//   1. The `TypeLowering` fast path (`compute_type_of_symbol` ->
//      `precompute_symbol_named_computed_property_names`) leaves a well-known
//      member's `is_symbol_named` unset, disagreeing with the canonical
//      `is_symbol_property_name` path, so `keyof` projects the member's shape
//      atom as a plain string key. Flagging it symbol-named (or projecting a
//      `unique symbol` key in `keyof`) makes `keyof I` a symbol, but routes
//      well-known members through the ref-based key path a HOMOMORPHIC mapped
//      type materializes through, regressing `for..of`/`for await..of`/spread
//      over `DeepReadonly<Iterable<...>>` (TS2488/TS2504). The two are coupled.
//
//   2. `crates/tsz-lowering/src/lower/advanced.rs` mints a `unique symbol` ref
//      from the ARENA-LOCAL node index (`SymbolRef(node_idx.0)`), so well-known
//      members declared in different lib-file arenas collide on one shared ref
//      — `typeof Symbol.iterator` and `typeof Symbol.asyncIterator` are the
//      SAME type to tsz (pinned by `well_known_unique_symbols_are_conflated`).
//      Under that collision the name<->ref registry cannot address one
//      canonical `[Symbol.xxx]` atom, so the reverse lookup is ambiguous and
//      the round-trip misses regardless of where it is applied.
//
// The fix is to make `unique symbol` refs globally unique (keyed off the
// declaring member symbol, not an arena-local node index), after which flagging
// well-known members symbol-named projects the correct key in both `keyof` and
// indexed access without disturbing mapped-type materialization. Then the two
// `#[ignore]`d rows below un-ignore and `well_known_unique_symbols_are_conflated`
// flips to `contains(&2322)`.
//
// Pinned as asserting `#[ignore]`d tests carrying their oracle rows so they
// flip loudly when the round-trip is fixed, instead of staying a silent absence.
// ---------------------------------------------------------------------------

#[test]
#[ignore = "known failure: keyof over a well-known-symbol-keyed interface does not reduce to the well-known symbol (false TS2322)"]
fn keyof_a_well_known_symbol_keyed_interface_reduces_to_the_well_known_symbol() {
    // tsc 7.0.2: exit 0 — `keyof I` IS `typeof Symbol.iterator`.
    // tsz today: TS2322 "Type 'keyof I' is not assignable to type 'unique symbol'."
    //
    // The equivalent shape keyed by a user-authored `unique symbol` binding
    // (`declare const u: unique symbol; interface I { [u]: number }`) is
    // already clean, so the gap is specific to the well-known leg.
    let codes = diagnostic_codes(
        r#"
interface I { [Symbol.iterator]: number }
type K = keyof I;
declare const k: K;
export const a: typeof Symbol.iterator = k;
"#,
    );
    assert!(
        codes.is_empty(),
        "keyof over a well-known-symbol-keyed interface must reduce to that \
         symbol, so the assignment is clean like tsc; got: {codes:?}"
    );
}

#[test]
#[ignore = "known failure: indexed access by a well-known symbol type leaks the __unique_N placeholder (TS2339/TS2538)"]
fn indexed_access_by_a_well_known_symbol_type_resolves_the_member() {
    // tsc 7.0.2: exit 0 — `I[typeof Symbol.iterator]` is `number`.
    // tsz today, three diagnostics, the first of which leaks an internal key
    // into user-facing text exactly as #16307's title describes:
    //   TS2339 "Property '__unique_5' does not exist on type 'I'."
    //   TS2538 "Type 'unique symbol' cannot be used as an index type."
    //   TS2322 "Type 'undefined' is not assignable to type 'number'."
    let codes = diagnostic_codes(
        r#"
interface I { [Symbol.iterator]: number }
declare const i: I;
export const a: number = i[Symbol.iterator];
type V = I[typeof Symbol.iterator];
export const b: number = null as any as V;
"#,
    );
    assert!(
        codes.is_empty(),
        "an indexed access keyed by a well-known symbol type must resolve the \
         member stored under its canonical `[Symbol.xxx]` atom; got: {codes:?}"
    );
}

// Root-cause pin for the rows above (defect 2). tsz currently CONFLATES
// distinct well-known unique symbols: `typeof Symbol.iterator` and
// `typeof Symbol.asyncIterator` intern to the same `SymbolRef` (the arena-local
// node index of their `unique symbol` type-operator nodes collides across lib
// files), so assigning one to the other is wrongly accepted where tsc reports
// TS2322. This asserts the CURRENT (buggy) behaviour so it flips loudly when
// `unique symbol` refs are made globally unique — the prerequisite for
// un-ignoring the two rows above. Update it to `contains(&2322)` as part of
// that fix.
#[test]
fn well_known_unique_symbols_are_conflated() {
    let codes = diagnostic_codes(
        r#"
export const a: typeof Symbol.iterator = Symbol.asyncIterator;
"#,
    );
    assert!(
        !codes.contains(&2322),
        "PIN: tsz currently conflates distinct well-known unique symbols; when \
         this starts reporting TS2322 the ref-collision fix has landed and the \
         two #[ignore]d rows above should be un-ignored; got: {codes:?}"
    );
}
