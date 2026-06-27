//! TS2502 for self-referential class **property** type annotations (#14819).
//!
//! A class property whose declared type annotation references the same member —
//! via `typeof Class.m`, `typeof this.m`, or `typeof Class[k]` (including a
//! symbol-keyed computed member), directly or through an indirect cycle — is
//! circular and `tsc` reports TS2502 at the property name. A reference to a
//! *different* member is not circular.

use crate::context::CheckerOptions;
use crate::test_utils::check_source;

fn ts2502(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    check_source(source, "test.ts", CheckerOptions::default())
        .into_iter()
        .filter(|d| d.code == 2502)
        .collect()
}

fn all_codes(source: &str) -> Vec<u32> {
    let mut codes: Vec<u32> = check_source(source, "test.ts", CheckerOptions::default())
        .into_iter()
        .map(|d| d.code)
        .collect();
    codes.sort_unstable();
    codes
}

#[test]
fn symbol_keyed_static_member_typeof_index_self_reference() {
    let diags =
        ts2502("declare const s: unique symbol;\nclass C { static [s]: typeof C[typeof s]; }");
    assert_eq!(diags.len(), 1, "expected one TS2502: {diags:?}");
    assert!(
        diags[0].message_text.contains("'[s]'"),
        "symbol-keyed member should render as '[s]': {diags:?}"
    );
}

#[test]
fn string_keyed_static_member_typeof_dot_self_reference() {
    let diags = ts2502("class D { static x: typeof D.x; }");
    assert_eq!(diags.len(), 1, "expected one TS2502: {diags:?}");
    assert!(diags[0].message_text.contains("'x'"), "{diags:?}");
}

#[test]
fn instance_member_typeof_this_self_reference() {
    let diags = ts2502("class E { x: typeof this.x; }");
    assert_eq!(diags.len(), 1, "expected one TS2502: {diags:?}");
    assert!(diags[0].message_text.contains("'x'"), "{diags:?}");
}

#[test]
fn static_readonly_member_typeof_dot_self_reference() {
    let diags = ts2502("class F { static readonly y: typeof F.y = 0 as any; }");
    assert_eq!(diags.len(), 1, "expected one TS2502: {diags:?}");
    assert!(diags[0].message_text.contains("'y'"), "{diags:?}");
}

#[test]
fn instance_member_typeof_this_index_self_reference() {
    // `typeof this["x"]` indexes the instance side through the value receiver.
    let diags = ts2502("class K { x: typeof this[\"x\"]; }");
    assert_eq!(diags.len(), 1, "expected one TS2502: {diags:?}");
    assert!(diags[0].message_text.contains("'x'"), "{diags:?}");
}

#[test]
fn indirect_cycle_through_second_member_reports_both() {
    let diags = ts2502("class H { static a: typeof H.b; static b: typeof H.a; }");
    assert_eq!(diags.len(), 2, "expected TS2502 on both members: {diags:?}");
    assert!(
        diags.iter().any(|d| d.message_text.contains("'a'")),
        "{diags:?}"
    );
    assert!(
        diags.iter().any(|d| d.message_text.contains("'b'")),
        "{diags:?}"
    );
}

#[test]
fn renamed_binders_still_circular() {
    // The fix must not depend on any particular class/member spelling.
    let diags = ts2502("class Widget { static config: typeof Widget.config; }");
    assert_eq!(diags.len(), 1, "expected one TS2502: {diags:?}");
    assert!(diags[0].message_text.contains("'config'"), "{diags:?}");
}

#[test]
fn reference_to_different_member_is_not_circular() {
    // `b` references the *other* member `a`; no cycle, so no TS2502.
    let diags = ts2502("class I { a: number = 0; b: typeof I.a; }");
    assert!(diags.is_empty(), "unexpected TS2502: {diags:?}");
}

#[test]
fn different_side_member_is_not_circular() {
    // The instance `x` and the static `x` are distinct members; `typeof X.x`
    // (static side) referenced from the static member only is circular, the
    // instance member is independent.
    let diags = ts2502("class X { x: number = 0; static x: typeof X.x; }");
    assert_eq!(
        diags.len(),
        1,
        "only the static member is circular: {diags:?}"
    );
}

#[test]
fn instance_member_does_not_also_emit_ts2564() {
    // The witness must report ONLY TS2502, never a strict-init TS2564.
    let codes = all_codes("class E { x: typeof this.x; }");
    assert_eq!(codes, vec![2502], "expected only TS2502: {codes:?}");
}

#[test]
fn non_circular_typeof_other_value_is_unaffected() {
    // `typeof other` where `other` is an unrelated value must not fire.
    let diags = ts2502("const other = 0;\nclass C { x: typeof other; }");
    assert!(diags.is_empty(), "unexpected TS2502: {diags:?}");
}
