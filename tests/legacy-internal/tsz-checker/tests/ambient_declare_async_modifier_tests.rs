//! `declare`/`async` interaction on a class member (issue #16291 follow-up,
//! 2026-08-07T23:05Z comment): `declare async p` on a property previously
//! reported tsz's semantic TS1042 (`'async' modifier cannot be used here.`)
//! instead of tsc's TS1040 (`'async' modifier cannot be used in an ambient
//! context.`).
//!
//! tsc's `checkGrammarModifiers` walks a member's modifiers in SOURCE ORDER
//! and reports exactly one diagnostic, so which code wins depends on order
//! and member kind:
//! - `async`/`override` before `declare`: the ambient conflict (TS1040) is
//!   known as soon as `declare` is reached, regardless of member kind (even
//!   a method or accessor reports TS1040, not TS1031).
//! - `declare` before `async`/`override`: `declare` is checked against the
//!   member kind immediately. On a method/accessor/constructor — which never
//!   allow `declare` at all — that is TS1031 and the walk stops there,
//!   *before* async/override is even reached. Only on a property (the one
//!   member kind `declare` is legal on) does the walk continue to report
//!   TS1040 at `async`.
//!
//! Every expectation is pinned against `typescript@7.0.2`
//! (`--noEmit --strict --target es2022 --lib es2022`). Binder names are varied
//! so the diagnostic is keyed on the modifier shape, not any identifier.

// TS1031/TS1040 are parser-emitted; TS1042 is checker-emitted. Only
// `check_source_codes_with_parse_health` sees both sides (see its doc comment
// in `test_utils.rs`) — the plain `check_source_diagnostics`/`check_source_codes`
// helpers never wire parser diagnostics into the result at all, which is
// exactly the family this suite needs to distinguish.
use crate::test_utils::check_source_codes_with_parse_health;

const TS1031: u32 = 1031; // 'declare' modifier cannot appear on class elements of this kind.
const TS1040: u32 = 1040; // 'async' modifier cannot be used in an ambient context.
const TS1042: u32 = 1042; // 'async' modifier cannot be used here.

/// Grammar codes this suite is about, filtered so assertions stay immune to
/// unrelated harness noise (e.g. a no-lib `Promise` return type also draws
/// TS1064/TS2583 that the real CLI, with the lib present, never emits).
const GRAMMAR_CODES: [u32; 3] = [TS1031, TS1040, TS1042];

fn codes(source: &str) -> Vec<u32> {
    let mut v: Vec<u32> = check_source_codes_with_parse_health(source)
        .into_iter()
        .filter(|c| GRAMMAR_CODES.contains(c))
        .collect();
    v.sort_unstable();
    v
}

// --- `declare` before `async`, on a property: TS1040, not TS1042 -----------

#[test]
fn declare_async_property_reports_ts1040_not_ts1042() {
    for name in ["p", "value", "data", "field"] {
        let source = format!("class C {{ declare async {name}: number; }}");
        assert_eq!(codes(&source), vec![TS1040], "source: {source}");
    }
}

#[test]
fn declare_async_property_reports_ts1040_independent_of_class_name() {
    for name in ["C", "Widget", "Repository", "Zzz"] {
        let source = format!("class {name} {{ declare async p: number; }}");
        assert_eq!(codes(&source), vec![TS1040], "source: {source}");
    }
}

// --- `async` before `declare`, any member kind: TS1040 -----------------------

#[test]
fn async_declare_property_reports_ts1040() {
    assert_eq!(codes("class C { async declare p: number; }"), vec![TS1040]);
}

#[test]
fn async_declare_method_reports_ts1040_not_ts1031() {
    // `async` came first, so the ambient conflict wins before the
    // declare-illegal-on-method check (TS1031) is even reached.
    assert_eq!(codes("class C { async declare m(): void; }"), vec![TS1040]);
}

#[test]
fn async_declare_accessor_reports_ts1040_not_ts1031() {
    assert_eq!(
        codes("class C { async declare get x(): number; }"),
        vec![TS1040]
    );
}

// --- `declare` before `async`, on a method/accessor: TS1031, not TS1040 ------
// `declare` is illegal on these member kinds outright; the walk stops at
// `declare` and never reaches `async`.

#[test]
fn declare_async_method_reports_ts1031_not_ts1040() {
    assert_eq!(codes("class C { declare async m(): void; }"), vec![TS1031]);
}

#[test]
fn declare_async_accessor_reports_ts1031_not_ts1040() {
    assert_eq!(
        codes("class C { declare async get x(): number; }"),
        vec![TS1031]
    );
    assert_eq!(
        codes("class C { declare async set x(v: number); }"),
        vec![TS1031]
    );
}

// --- an enclosing ambient context (no member-local `declare`) is unaffected --

#[test]
fn async_property_in_declare_class_reports_ts1040() {
    assert_eq!(codes("declare class C { async p: number; }"), vec![TS1040]);
}

#[test]
fn async_method_in_declare_namespace_class_reports_ts1040() {
    assert_eq!(
        codes("declare namespace N { class C { async m(): void; } }"),
        vec![TS1040]
    );
}

// --- member-local `declare` nested inside an enclosing ambient context still
// --- reports exactly one TS1040, not a duplicate --------------------------

#[test]
fn declare_async_property_in_declare_namespace_reports_single_ts1040() {
    assert_eq!(
        codes("declare namespace N { class C { declare async p: number; } }"),
        vec![TS1040]
    );
}

// --- combined with other modifiers: order between `declare`/`async` still
// --- decides the code, regardless of what else is present ------------------

#[test]
fn static_declare_async_property_reports_ts1040() {
    assert_eq!(
        codes("class C { static declare async p: number; }"),
        vec![TS1040]
    );
}

#[test]
fn readonly_declare_async_property_reports_ts1040() {
    assert_eq!(
        codes("class C { readonly declare async p: number; }"),
        vec![TS1040]
    );
}

// --- adjacent controls: unaffected by this change ---------------------------

#[test]
fn plain_declare_property_stays_clean() {
    assert!(codes("class C { declare p: number; }").is_empty());
}

#[test]
fn plain_declare_method_reports_ts1031() {
    assert_eq!(codes("class C { declare m(): void; }"), vec![TS1031]);
}

#[test]
fn plain_async_property_without_declare_still_reports_ts1042() {
    // No `declare` at all: the ordinary async-on-property check is untouched.
    assert_eq!(codes("class C { async p: number = 1; }"), vec![TS1042]);
}
