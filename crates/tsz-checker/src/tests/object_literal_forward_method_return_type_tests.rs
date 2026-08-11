//! Regression tests for #17157: an object-literal method's `this.<sibling>()`
//! must carry the sibling's real inferred return type even when the sibling
//! is an unannotated method declared *later* in the same literal.
//!
//! Before the fix, `build_object_literal_method_synthetic_this_type` (and its
//! `function`-expression-property sibling) hardcoded `any` as the return type
//! of any forward-referenced, unannotated sibling — so `this.<later>()`
//! silently widened to `any` and dropped any diagnostic that depended on the
//! call's result. A sibling declared *before* the caller was unaffected (it
//! is already in the incrementally-built `properties` map with its real
//! type), so these tests specifically assign the call result to a narrower
//! type (`string`) to force a mismatch that `any` would mask.
//!
//! Tests vary binder names (anti-hardcoding), cover both declaration orders,
//! `function`-expression siblings, genuine return cycles (still `TS7023`),
//! an `any`-returning sibling (stays `any`, no new error), and a genuinely
//! missing member (still `TS2339`).

use crate::test_utils::check_source_strict_codes;

// ---------------------------------------------------------------------------
// The reported witness: forward-referenced unannotated method sibling.
// ---------------------------------------------------------------------------

#[test]
fn forward_referenced_method_return_type_is_precise() {
    // tsc: TS2322 (`number` not assignable to `string`). Before the fix: no
    // diagnostic, because `this.bar()` collapsed to `any`.
    let codes = check_source_strict_codes(
        "const obj = { foo() { return this.bar(); }, bar() { return 1; } };
const t: string = obj.foo();",
    );
    assert!(
        codes.contains(&2322),
        "expected TS2322 for the forward-referenced sibling's real return type, got: {codes:?}"
    );
}

#[test]
fn backward_referenced_method_return_type_is_precise_control() {
    // Control: sibling declared first must produce the identical diagnostic.
    let codes = check_source_strict_codes(
        "const obj = { bar() { return 1; }, foo() { return this.bar(); } };
const t: string = obj.foo();",
    );
    assert!(
        codes.contains(&2322),
        "expected TS2322 for the backward-referenced sibling (control), got: {codes:?}"
    );
}

#[test]
fn declaration_order_does_not_change_the_diagnostic_set() {
    let forward = check_source_strict_codes(
        "const obj = { foo() { return this.bar(); }, bar() { return 1; } };
const t: string = obj.foo();",
    );
    let backward = check_source_strict_codes(
        "const obj = { bar() { return 1; }, foo() { return this.bar(); } };
const t: string = obj.foo();",
    );
    assert_eq!(
        forward, backward,
        "forward vs backward sibling reference must produce identical diagnostics"
    );
}

// ---------------------------------------------------------------------------
// Renamed binders (anti-hardcoding): same structural shape, different names.
// ---------------------------------------------------------------------------

#[test]
fn renamed_binders_forward_reference_return_type_is_precise() {
    let codes = check_source_strict_codes(
        "const widget = { alpha() { return this.beta(); }, beta() { return 1; } };
const s: string = widget.alpha();",
    );
    assert!(
        codes.contains(&2322),
        "expected TS2322 with renamed binders, got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// `function`-expression property sibling (both the caller and the callee).
// ---------------------------------------------------------------------------

#[test]
fn function_expression_property_forward_reference_return_type_is_precise() {
    let codes = check_source_strict_codes(
        "const obj = { foo: function () { return this.bar(); }, bar() { return 1; } };
const t: string = obj.foo();",
    );
    assert!(
        codes.contains(&2322),
        "expected TS2322 through a function-expression-property caller, got: {codes:?}"
    );
}

#[test]
fn function_expression_property_sibling_forward_reference_return_type_is_precise() {
    let codes = check_source_strict_codes(
        "const obj = { foo() { return this.bar(); }, bar: function () { return 1; } };
const t: string = obj.foo();",
    );
    assert!(
        codes.contains(&2322),
        "expected TS2322 through a function-expression-property sibling, got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// A genuine return cycle must still be reported as TS7023, not silently
// resolved to a concrete type.
// ---------------------------------------------------------------------------

#[test]
fn genuine_return_cycle_still_reports_ts7023() {
    let codes = check_source_strict_codes(
        "const obj = { foo() { return this.bar(); }, bar() { return this.foo(); } };",
    );
    assert!(
        codes.contains(&7023),
        "expected TS7023 for the genuine foo<->bar return cycle, got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// A sibling whose real return type is itself `any` must stay `any` — no new
// diagnostic should appear just because the sibling is now inferred on demand.
// ---------------------------------------------------------------------------

#[test]
fn sibling_with_any_return_type_stays_any() {
    let codes = check_source_strict_codes(
        "const obj = { foo() { return this.bar(); }, bar() { return JSON.parse(\"{}\"); } };
const t: string = obj.foo();",
    );
    assert!(
        !codes.contains(&2322),
        "an any-returning sibling must not produce a new TS2322, got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Negative: a genuinely missing member must still be TS2339, in both
// declaration orders.
// ---------------------------------------------------------------------------

#[test]
fn genuinely_missing_member_still_reports_ts2339() {
    let codes = check_source_strict_codes("const obj = { foo() { return this.missing(); } };");
    assert!(
        codes.contains(&2339),
        "expected TS2339 for the genuinely missing member, got: {codes:?}"
    );
}
