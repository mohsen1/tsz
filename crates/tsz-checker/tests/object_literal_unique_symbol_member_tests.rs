//! Object-type-literal `unique symbol` members must keep their unique-symbol
//! identity when read back via `typeof`, matching interface members and tsc.
//!
//! Regression for the false-negative where a `unique symbol`-typed property in
//! an *object type literal* widened to plain `symbol` via `typeof obj.prop`
//! (or `(typeof obj)["prop"]`), so assigning a generic `symbol` was wrongly
//! accepted (missed TS2322). Per tsc's `getESSymbolLikeTypeForNode`, a
//! `readonly` property signature — interface *or* object-type-literal member —
//! owns a `unique symbol` keyed on the member's own declaration symbol.

use tsz_checker::context::CheckerOptions;

fn check_strict(source: &str) -> Vec<(u32, String)> {
    let options = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..Default::default()
    };
    tsz_checker::test_utils::check_source(source, "test.ts", options)
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

fn ts2322_count(diags: &[(u32, String)]) -> usize {
    diags.iter().filter(|(c, _)| *c == 2322).count()
}

#[test]
fn object_literal_member_via_property_access_emits_ts2322() {
    let source = r#"
declare const C: { readonly K: unique symbol };
declare const s: symbol;
const b1: typeof C.K = s;
"#;
    let diags = check_strict(source);
    assert_eq!(
        ts2322_count(&diags),
        1,
        "object-type-literal member read via `typeof C.K` must reject generic symbol: {diags:?}"
    );
}

#[test]
fn object_literal_member_via_indexed_access_emits_ts2322() {
    let source = r#"
declare const C: { readonly K: unique symbol };
declare const s: symbol;
const b2: (typeof C)["K"] = s;
"#;
    let diags = check_strict(source);
    assert_eq!(
        ts2322_count(&diags),
        1,
        "object-type-literal member read via `(typeof C)[\"K\"]` must reject generic symbol: {diags:?}"
    );
}

#[test]
fn matches_interface_and_const_variable_owners() {
    // All four binding forms behave identically and reject a generic symbol.
    let source = r#"
declare const C: { readonly K: unique symbol };
declare const s: symbol;
const b1: typeof C.K = s;
const b2: (typeof C)["K"] = s;
interface I { readonly K: unique symbol }
declare const D: I;
const b3: typeof D.K = s;
declare const E: unique symbol;
const b4: typeof E = s;
"#;
    let diags = check_strict(source);
    assert_eq!(
        ts2322_count(&diags),
        4,
        "interface, const, and object-literal forms must all emit TS2322: {diags:?}"
    );
}

#[test]
fn renamed_binders_behave_identically() {
    // §25: the fix must not depend on user-chosen identifier names.
    let source = r#"
declare const aDifferentName: { readonly RENAMED_KEY: unique symbol };
declare const someSym: symbol;
const x: typeof aDifferentName.RENAMED_KEY = someSym;
"#;
    let diags = check_strict(source);
    assert_eq!(
        ts2322_count(&diags),
        1,
        "renamed binders must still reject generic symbol: {diags:?}"
    );
}

#[test]
fn nested_object_literal_member() {
    let source = r#"
declare const C: { readonly inner: { readonly K: unique symbol } };
declare const s: symbol;
const x: typeof C.inner.K = s;
"#;
    let diags = check_strict(source);
    assert_eq!(
        ts2322_count(&diags),
        1,
        "nested object-literal member must reject generic symbol: {diags:?}"
    );
}

#[test]
fn parenthesized_unique_symbol_member() {
    let source = r#"
declare const C: { readonly K: (unique symbol) };
declare const s: symbol;
const x: typeof C.K = s;
"#;
    let diags = check_strict(source);
    assert_eq!(
        ts2322_count(&diags),
        1,
        "parenthesized `(unique symbol)` member must reject generic symbol: {diags:?}"
    );
}

#[test]
fn distinct_object_literals_have_distinct_unique_symbols() {
    // Two structurally identical object-type-literal members are distinct
    // unique symbols, so cross-assignment is rejected.
    let source = r#"
declare const A: { readonly K: unique symbol };
declare const B: { readonly K: unique symbol };
const x: typeof A.K = B.K;
"#;
    let diags = check_strict(source);
    assert_eq!(
        ts2322_count(&diags),
        1,
        "distinct object-literal members must be distinct unique symbols: {diags:?}"
    );
}

#[test]
fn same_member_unique_symbol_is_assignable_to_itself() {
    // The member's unique symbol must round-trip: assigning it to its own
    // `typeof` type must not error.
    let source = r#"
declare const C: { readonly K: unique symbol };
const ok: typeof C.K = C.K;
"#;
    let diags = check_strict(source);
    assert_eq!(
        ts2322_count(&diags),
        0,
        "a member's own unique symbol must be assignable to `typeof C.K`: {diags:?}"
    );
}
