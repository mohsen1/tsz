//! Tests for issue #16725: a per-property `as const` inside a fresh object
//! literal passed as a generic call argument must keep its literal type
//! through inference, not widen to the primitive base.
//!
//! `pick({ v: "x" as const })` for `declare function pick<T>(o: { v: T }): T`
//! should infer `T = "x"`, matching tsc's `getRegularTypeOfLiteralType`
//! (the property's own const assertion strips freshness regardless of
//! whether the enclosing object literal is itself fresh). Without the fix,
//! `constrain_properties` only propagated the *object's* `FRESH_LITERAL`
//! flag into inference-candidate freshness and ignored the property's own
//! `PropertyInfo::non_widening`, so the candidate was treated as a fresh
//! literal and widened to `string`/`number`.

use crate::test_utils::check_source_diagnostics;

#[test]
fn const_asserted_property_string_literal_preserved_through_generic_inference() {
    let diags = check_source_diagnostics(
        r#"
declare function pick<T>(o: { v: T }): T;
const r = pick({ v: "x" as const });
const ok: "x" = r;
"#,
    );

    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "Expected T to infer as \"x\", not widen to string, got: {:?}",
        ts2322.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

#[test]
fn const_asserted_property_number_literal_preserved_through_generic_inference() {
    let diags = check_source_diagnostics(
        r#"
declare function pick<T>(o: { v: T }): T;
const r = pick({ v: 1 as const });
const ok: 1 = r;
"#,
    );

    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "Expected T to infer as 1, not widen to number, got: {:?}",
        ts2322.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

#[test]
fn const_asserted_property_preserved_with_renamed_binders() {
    // Anti-hardcoding: vary the function, property, and value-binder names.
    let diags = check_source_diagnostics(
        r#"
declare function unwrap<Value>(box: { payload: Value }): Value;
const outcome = unwrap({ payload: "done" as const });
const done: "done" = outcome;
"#,
    );

    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "Expected name-independent const-assert preservation through inference, got: {:?}",
        ts2322.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

#[test]
fn non_asserted_property_still_widens_through_generic_inference() {
    // Regression guard: a plain (non-const-asserted) property in the same
    // fresh-object-argument shape must still widen — only the property's own
    // `as const` should suppress widening, not the mere presence of a
    // sibling const assertion or the call being generic at all.
    let diags = check_source_diagnostics(
        r#"
declare function pick<T>(o: { v: T }): T;
const r = pick({ v: "x" });
const bad: "x" = r;
"#,
    );

    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "Expected exactly one TS2322 — a plain literal property must still widen to string, got: {:?}",
        ts2322.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

#[test]
fn whole_object_const_assertion_still_preserved() {
    // Positive control from the issue's adjacent-case matrix: asserting the
    // whole object literal (rather than one property) was already correct
    // before this fix and must stay correct.
    let diags = check_source_diagnostics(
        r#"
declare function unbox<T>(o: { v: T }): T;
const r = unbox({ v: 1 } as const);
const ok: 1 = r;
"#,
    );

    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "Expected whole-object const assertion to keep preserving the literal, got: {:?}",
        ts2322.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

#[test]
fn primitive_constrained_type_param_still_preserved_without_const() {
    // Positive control: a type parameter constrained to a primitive already
    // preserves the literal without any `as const` (the constraint path is a
    // separate mechanism from the freshness fix here) and must stay that way.
    let diags = check_source_diagnostics(
        r#"
declare function unbox<T extends number>(o: { v: T }): T;
const r = unbox({ v: 1 as const });
const ok: 1 = r;
"#,
    );

    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "Expected constrained type parameter to keep preserving the literal, got: {:?}",
        ts2322.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

#[test]
fn mixed_asserted_and_plain_sibling_properties_infer_independently() {
    // Two independently-inferred type parameters on sibling properties of the
    // same fresh object literal: only the const-asserted one keeps its
    // literal, the plain sibling still widens.
    let diags = check_source_diagnostics(
        r#"
declare function pair<A, B>(o: { a: A; b: B }): [A, B];
const [a, b] = pair({ a: "x" as const, b: "y" });
const okA: "x" = a;
const okB: "y" = b;
"#,
    );

    let a_ok: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        a_ok.len(),
        1,
        "Expected exactly one TS2322 — the const-asserted `a` stays literal, only the plain `b` widens and fails, got: {:?}",
        a_ok.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}
