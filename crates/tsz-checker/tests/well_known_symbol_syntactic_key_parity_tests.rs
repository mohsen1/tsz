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
// Well-known-symbol key round-trip (#16605): `keyof` / indexed-access over a
// WELL-KNOWN symbol key, the `UniqueSymbol(SymbolRef)` <-> `[Symbol.xxx]` atom
// round-trip. Both directions now hold; the history that shaped these rows:
//
//   1. `keyof` (Row 1) projected the member's shape atom as a plain string key
//      because a well-known-keyed member is a *named* member (`is_symbol_named`
//      is false by the #16307 rule). `property_name_to_key_type` now recovers
//      the `UniqueSymbol` key for a well-known `[Symbol.xxx]` named key via the
//      forward registry, without disturbing mapped-type materialization.
//
//   2. `unique symbol` refs were minted from the ARENA-LOCAL node index
//      (`SymbolRef(node_idx.0)`), so well-known members declared in different
//      lib-file arenas collided on one shared ref — `typeof Symbol.iterator` and
//      `typeof Symbol.asyncIterator` were the SAME type to tsz. Under that
//      collision the name<->ref registry could not address one canonical
//      `[Symbol.xxx]` atom, so the indexed-access (Row 2) reverse lookup was
//      ambiguous. The mint now folds the arena's source file name in
//      (`unique_symbol_ref_from_source_span`), giving each well-known symbol a
//      distinct, globally-unique ref; the type-position indexed-access
//      diagnostics then reverse-resolve it through the eagerly-seeded registry.
// ---------------------------------------------------------------------------

#[test]
fn keyof_a_well_known_symbol_keyed_interface_reduces_to_the_well_known_symbol() {
    // tsc 7.0.2: exit 0 — `keyof I` IS `typeof Symbol.iterator`.
    //
    // A well-known-symbol-keyed member is a *named* member (`is_symbol_named`
    // is false) stored under its canonical `[Symbol.iterator]` atom, so `keyof`
    // took the literal-key branch and produced the string literal
    // `"[Symbol.iterator]"` instead of `typeof Symbol.iterator`. `keyof` now
    // recovers the unique-symbol key for such a named key through the
    // well-known-symbol registry, which is seeded from the lib `SymbolConstructor`
    // members up front so the registry is populated before the alias is
    // evaluated. The equivalent shape keyed by a user-authored `unique symbol`
    // binding (`declare const u: unique symbol; interface I { [u]: number }`)
    // was already clean; this closes the well-known leg.
    //
    // First landed as #16628, reverted as #16764 because seeding the registry
    // eagerly (via `collect_properties` over `SymbolConstructor`'s resolved
    // type) cost the `SymbolConstructor` display name on unrelated `TS2339`
    // property-lookup failures — see
    // `well_known_symbol_property_lookup_failure_keeps_symbol_constructor_name`
    // below, the regression this reland must not reintroduce (#16765).
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

// Pins #16765's acceptance criterion 4: a `TS2339` property-lookup failure on
// `Symbol` (typed `SymbolConstructor`) must keep showing the alias name, not
// the structural intersection `SymbolConstructor`'s per-file lib declarations
// merge into. `seed_well_known_symbol_names` (called from
// `prepare_source_file_for_checking`, ahead of the normal environment build)
// resolves and collects `SymbolConstructor`'s properties eagerly; without
// explicitly re-recording that resolution's display-alias provenance, whichever
// caller reaches `SymbolConstructor` first "wins" the alias registration, and
// losing that race prints ~400 characters of merged interface shape instead of
// one word. This is a rendered-message assertion, deliberately narrower than
// `diagnostic_codes` — the corpus regression in #16628/#16764 had an identical
// diagnostic *code* set on both sides of the bug, so a code-only assertion here
// would not have caught it.
#[test]
fn well_known_symbol_property_lookup_failure_keeps_symbol_constructor_name() {
    use tsz_checker::test_utils::check_source_with_libs_code_messages;
    let libs = load_default_lib_files();
    let diags = check_source_with_libs_code_messages(
        r#"
Symbol.nonsense;
"#,
        "test.ts",
        CheckerOptions::default(),
        &libs,
    );
    let messages: Vec<&str> = diags.iter().map(|(_, m)| m.as_str()).collect();
    assert!(
        messages
            .iter()
            .any(|m| m.contains("does not exist on type 'SymbolConstructor'")),
        "TS2339 on `Symbol.<unknown>` must name 'SymbolConstructor', not its \
         expanded structural shape; got: {messages:?}"
    );
    assert!(
        !messages.iter().any(|m| m.contains("readonly iterator")),
        "must not leak SymbolConstructor's merged structural shape into the \
         message; got: {messages:?}"
    );
}

#[test]
fn indexed_access_by_a_well_known_symbol_type_resolves_the_member() {
    // tsc 7.0.2: exit 0 — `I[typeof Symbol.iterator]` is `number`.
    //
    // This previously produced three diagnostics, the first of which leaked an
    // internal key into user-facing text exactly as #16307's title describes:
    //   TS2339 "Property '__unique_5' does not exist on type 'I'."
    //   TS2538 "Type 'unique symbol' cannot be used as an index type."
    //   TS2322 "Type 'undefined' is not assignable to type 'number'."
    //
    // The reverse direction (`typeof Symbol.iterator` = `UniqueSymbol(ref)` back
    // to the `[Symbol.iterator]` atom to find the member) failed because several
    // well-known members shared one `SymbolRef` — their `unique symbol` nodes
    // collided on one arena-local node index across lib files — so the reverse
    // lookup was ambiguous. The mint now folds the source file name in, giving
    // each well-known symbol a distinct ref; the eagerly-seeded registry then
    // reverse-resolves it, and the type-position indexed-access diagnostics
    // (TS2339 via `literal_index_keys`, TS2538 via the concrete-index guard)
    // consult that registry instead of the `__unique_N` placeholder.
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

// Root-cause guard for the row above (defect 2). Distinct well-known unique
// symbols must have distinct identities: `typeof Symbol.iterator` and
// `typeof Symbol.asyncIterator` denote different `unique symbol`s, so assigning
// one to the other is a TS2322 in tsc. Previously tsz conflated them — their
// `unique symbol` type-operator nodes sat at the same arena-local node index in
// different lib files, and the mint keyed the `SymbolRef` off that index alone,
// so the two interned to one type. The mint now folds the source file name in
// (`unique_symbol_ref_from_source_span`), so the two are distinct and the
// mismatch is reported.
#[test]
fn distinct_well_known_unique_symbols_are_not_conflated() {
    let codes = diagnostic_codes(
        r#"
export const a: typeof Symbol.iterator = Symbol.asyncIterator;
"#,
    );
    assert!(
        codes.contains(&2322),
        "distinct well-known unique symbols must not be conflated; assigning \
         `Symbol.asyncIterator` to a `typeof Symbol.iterator` is a TS2322 in \
         tsc; got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// #17720 — type-position `M[typeof Symbol.iterator]` on a well-known-symbol
// METHOD member.
//
// The property-form row above (`indexed_access_by_a_well_known_symbol_type_...`)
// warmed the name<->ref registry as a side effect of evaluating its own
// property-form alias, which masked a distinct residual: a well-known symbol is
// denoted by SEVERAL distinct `SymbolRef`s across the pipeline (the lib
// `SymbolConstructor` member ref the eager seed records, the ref a use-site
// `typeof Symbol.iterator` resolves to, and the ref recovered from an
// interface's own computed-name expression). The forward `name -> ref` registry
// keeps only one ref per name (last write wins), so a later registration for
// one spelling CLOBBERED the seed's use-site ref and the reverse `ref -> name`
// lookup then missed for a ref that legitimately denotes the same symbol. The
// property-form path happened to re-register the use-site ref last and so
// recovered; the method-form path never did, leaving the member unresolved and
// firing a spurious TS2538. The registry now accumulates every ref per name in
// a reverse map (`ref -> name` is a true function), so the reverse lookup
// recognizes the name regardless of registration order and spelling.
// ---------------------------------------------------------------------------

#[test]
fn type_position_indexed_access_by_a_well_known_symbol_method_member_resolves() {
    // tsc 6.0.2 / 7.0.2: exit 0 — `M[typeof Symbol.iterator]` is `() => number`.
    // In ISOLATION (no property-form alias earlier in the file to warm the
    // registry), so the method member must resolve on its own.
    let codes = diagnostic_codes(
        r#"
interface M { [Symbol.iterator](): number }
type VM = M[typeof Symbol.iterator];
export const bm: () => number = null as any as VM;
"#,
    );
    assert!(
        codes.is_empty(),
        "a type-position indexed access keyed by a well-known symbol on a METHOD \
         member must resolve the member and stay clean like tsc; got: {codes:?}"
    );
}

#[test]
fn class_and_generic_well_known_symbol_method_members_resolve() {
    // Adjacency: a class method form and a generic interface method form, both
    // clean in tsc, both previously firing the same spurious TS2538.
    let codes = diagnostic_codes(
        r#"
class C { [Symbol.iterator](): number { return 1; } }
type VC = C[typeof Symbol.iterator];
export const bc: () => number = null as any as VC;

interface G<X> { [Symbol.iterator](): X }
type VG = G<string>[typeof Symbol.iterator];
export const bg: () => string = null as any as VG;
"#,
    );
    assert!(
        codes.is_empty(),
        "class and generic well-known-symbol method members must resolve in \
         type position like tsc; got: {codes:?}"
    );
}

#[test]
fn missing_well_known_symbol_key_reports_ts2339_not_ts2538() {
    // tsc 6.0.2 / 7.0.2: a well-known-symbol key the object lacks is a *named*
    // member miss — a single TS2339 ("Property '[Symbol.asyncIterator]' does not
    // exist"), never TS2538. tsz previously double-emitted TS2339 + TS2538 for
    // both the property and method forms of the containing member; the
    // concrete-index guard now defers a well-known named key to the
    // resolver-aware TS2339 path instead of emitting a spurious TS2538.
    for member in ["[Symbol.iterator]: number", "[Symbol.iterator](): number"] {
        let source = format!(
            r#"
interface I {{ {member} }}
type T = I[typeof Symbol.asyncIterator];
"#,
        );
        let codes = diagnostic_codes(&source);
        assert!(
            codes.contains(&2339),
            "a missing well-known-symbol key must report TS2339; member `{member}`, \
             got: {codes:?}"
        );
        assert!(
            !codes.contains(&2538),
            "a missing well-known-symbol NAMED key must not also report the \
             spurious TS2538; member `{member}`, got: {codes:?}"
        );
    }
}
