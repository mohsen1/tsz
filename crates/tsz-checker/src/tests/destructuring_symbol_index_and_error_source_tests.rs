//! Regression coverage for two false-positive `TS2538` families in the
//! computed-key object-destructuring path
//! (`get_binding_element_type_with_request`), both oracle-verified against
//! `typescript@7.0.2`.
//!
//! - #17528: a `symbol` / `unique symbol` computed key that names no declared
//!   property must resolve through the source's `symbol` index signature —
//!   exactly like the `obj[s]` element access `{ [s]: v } = obj` desugars to —
//!   yielding that signature's value type and *no* `TS2538`. tsz previously fell
//!   through to the property-not-found path and reported a spurious `TS2538`.
//! - #17529: an error-typed object source behaves like `any` for destructuring;
//!   the computed-key index-type check must not cascade a `TS2538` on top of the
//!   errors tsc already reports. `unknown` is intentionally excluded (tsc keeps
//!   `TS2538` for an invalid computed key over an `unknown` source).
//!
//! Binder names (symbol bindings, source aliases, value bindings) are varied so
//! no identifier spelling is load-bearing.

use crate::CheckerOptions;
use crate::test_utils::{check_source_with_libs, diagnostic_codes, load_default_lib_files};

const TS2538: u32 = 2538;
const TS2322: u32 = 2322;

fn codes(source: &str) -> Vec<u32> {
    diagnostic_codes(&check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions::default(),
        &load_default_lib_files(),
    ))
}

fn assert_no_ts2538(source: &str) {
    let found = codes(source);
    assert!(
        !found.contains(&TS2538),
        "expected no TS2538, got {found:?} for source:\n{source}"
    );
}

fn assert_has_ts2538(source: &str) {
    let found = codes(source);
    assert!(
        found.contains(&TS2538),
        "expected TS2538, got {found:?} for source:\n{source}"
    );
}

// ---------------------------------------------------------------------------
// #17528 — a `symbol` key resolves through a `symbol` index signature.
// ---------------------------------------------------------------------------

/// The reported repro: a `Record<symbol, V>` source (a `symbol` index
/// signature) accepts a `unique symbol` computed key with no `TS2538`.
#[test]
fn unique_symbol_key_over_record_symbol_index_is_clean() {
    assert_no_ts2538(
        r#"
declare const marker: unique symbol;
type Store = Record<symbol, number>;
const { [marker]: picked } = {} as Store;
"#,
    );
}

/// An inline `{ [k: symbol]: V }` signature is equivalent to the `Record` form.
#[test]
fn unique_symbol_key_over_inline_symbol_index_is_clean() {
    assert_no_ts2538(
        r#"
declare const tag: unique symbol;
declare const bag: { [entry: symbol]: string };
const { [tag]: chosen } = bag;
"#,
    );
}

/// A *bare* `symbol` key (not a `unique symbol`) is equally valid against a
/// `symbol` index signature — the desugared `obj[symKey]` element access
/// resolves through it.
#[test]
fn bare_symbol_key_over_symbol_index_is_clean() {
    assert_no_ts2538(
        r#"
declare const wide: symbol;
declare const dict: { [slot: symbol]: boolean };
const { [wide]: flag } = dict;
"#,
    );
}

/// The resolved binding value type is the index signature's value type, not
/// `any`: assigning it to an incompatible annotation reports `TS2322`. Guards
/// against a future "suppress by widening to `any`" regression.
#[test]
fn symbol_index_binding_resolves_to_the_signature_value_type() {
    let found = codes(
        r#"
declare const key: unique symbol;
type Numbers = Record<symbol, number>;
const { [key]: value } = {} as Numbers;
const mistyped: string = value;
"#,
    );
    assert!(
        !found.contains(&TS2538),
        "no TS2538 for a symbol index source, got {found:?}"
    );
    assert!(
        found.contains(&TS2322),
        "the binding must resolve to `number` (TS2322 on the bad assignment), got {found:?}"
    );
}

/// A union source where *every* member carries a `symbol` index signature
/// accepts the key and binds the union of the members' value types.
#[test]
fn union_all_members_symbol_indexed_is_clean() {
    assert_no_ts2538(
        r#"
declare const id: unique symbol;
type Left = Record<symbol, number>;
type Right = { [k: symbol]: string };
declare const either: Left | Right;
const { [id]: v } = either;
"#,
    );
}

/// Positive control (regression guard): a source with a matching declared
/// `symbol`-keyed property already resolved cleanly and must stay clean — that
/// path predates this fix and must not be disturbed.
#[test]
fn matching_declared_symbol_property_stays_clean() {
    assert_no_ts2538(
        r#"
declare const field: unique symbol;
const holder = { [field]: 42 };
const { [field]: value } = holder;
"#,
    );
}

// ---------------------------------------------------------------------------
// #17528 negatives — a `symbol` key with no `symbol` index still errors.
// ---------------------------------------------------------------------------

/// No index signature at all: a `symbol` key is a genuine `TS2538`.
#[test]
fn symbol_key_over_no_index_source_reports_ts2538() {
    assert_has_ts2538(
        r#"
declare const sole: unique symbol;
declare const plain: { alpha: number };
const { [sole]: v } = plain;
"#,
    );
}

/// A `string` index signature cannot accept a `symbol` key: `TS2538` stays.
#[test]
fn symbol_key_over_string_index_only_reports_ts2538() {
    assert_has_ts2538(
        r#"
declare const onlySym: unique symbol;
declare const stringDict: { [name: string]: number };
const { [onlySym]: v } = stringDict;
"#,
    );
}

/// A union where a member lacks a `symbol` index signature is not uniformly
/// symbol-indexable: `TS2538` stays (mirrors `resolve_symbol_index`'s
/// all-members requirement).
#[test]
fn union_with_a_non_symbol_indexed_member_reports_ts2538() {
    assert_has_ts2538(
        r#"
declare const mark: unique symbol;
type Indexed = Record<symbol, number>;
declare const maybe: Indexed | { beta: number };
const { [mark]: v } = maybe;
"#,
    );
}

// ---------------------------------------------------------------------------
// #17529 — an error-typed source suppresses the computed-key index check.
// ---------------------------------------------------------------------------

/// Both the key and the source are unresolved: the source is error-typed, so
/// the computed-key index check must not add a `TS2538` on top of the
/// unresolved-name errors tsc already reports.
#[test]
fn error_source_with_error_key_has_no_ts2538() {
    let found = codes(
        r#"
const { [missingKey]: bound } = missingSource;
"#,
    );
    assert!(
        !found.contains(&TS2538),
        "an error-typed source must not cascade TS2538, got {found:?}"
    );
}

/// An error-typed *annotated* source (unresolved type reference) with a genuine
/// `symbol` key: still no `TS2538` — the source, not the key, is the error. This
/// is the reliable witness for the `parent_type == ERROR` early-out (the
/// annotation forces a genuine error type rather than an implicit-any source).
#[test]
fn error_annotated_source_with_symbol_key_has_no_ts2538() {
    let found = codes(
        r#"
declare const stamp: unique symbol;
declare const broken: NoSuchType;
const { [stamp]: bound } = broken;
"#,
    );
    assert!(
        !found.contains(&TS2538),
        "an error-typed source must not cascade TS2538, got {found:?}"
    );
}

/// Control: an `unknown` source is *not* caught by the error-source early-out —
/// it still reports the "Object is of type 'unknown'" family (`TS2571`) rather
/// than being silently accepted like an `any`/error source. Guards that the
/// `parent_type == ERROR` suppression does not leak to `unknown`.
#[test]
fn unknown_source_is_not_treated_like_error_source() {
    const TS2571: u32 = 2571;
    let found = codes(
        r#"
declare const anything: unknown;
const { [absent]: bound } = anything;
"#,
    );
    assert!(
        found.contains(&TS2571) || found.contains(&TS2538),
        "an `unknown` source must still be diagnosed (not suppressed like an error source), got {found:?}"
    );
}

/// Control: a concrete source with no index signature keeps `TS2538` for a
/// `symbol` key — only an *error* source suppresses it.
#[test]
fn concrete_source_with_symbol_key_keeps_ts2538() {
    assert_has_ts2538(
        r#"
declare const badge: unique symbol;
declare const concrete: { gamma: number };
const { [badge]: bound } = concrete;
"#,
    );
}
