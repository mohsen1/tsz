//! Regression tests for #13970: `this` inside an object-literal method /
//! accessor / `function`-expression property must resolve against the
//! **complete** object literal type — every data property, accessor, and
//! method — regardless of declaration order.
//!
//! Before the fix, the synthetic `this` was built from members processed *so
//! far* plus a pre-scan covering only **callable** members, so a later
//! non-function data property or accessor was invisible to an earlier member's
//! `this`, producing spurious `TS2339` (and the consequent `TS7023`).
//!
//! Tests vary binder names (anti-hardcoding) and assert both that the false
//! positives are gone and that genuine errors still surface.

use crate::test_utils::{check_source_codes, check_source_strict_codes};

fn assert_clean_strict(src: &str) {
    let codes = check_source_strict_codes(src);
    assert!(codes.is_empty(), "expected no diagnostics, got: {codes:?}");
}

// ---------------------------------------------------------------------------
// The reported witness: data member declared after the method.
// ---------------------------------------------------------------------------

#[test]
fn method_sees_later_data_property() {
    // `tsc`: clean. Before fix: TS7023 + TS2339.
    assert_clean_strict(
        "
const obj = { method() { return this.value; }, value: 42 };
const n: number = obj.method();
",
    );
}

#[test]
fn declaration_order_is_irrelevant_for_data_members() {
    // Data-before and data-after must produce identical results.
    let before = check_source_strict_codes("const o = { v: 1, m() { return this.v; } };");
    let after = check_source_strict_codes("const o = { m() { return this.v; }, v: 1 };");
    assert!(before.is_empty(), "data-before should be clean: {before:?}");
    assert_eq!(
        before, after,
        "declaration order must not change diagnostics"
    );
}

// ---------------------------------------------------------------------------
// Accessors declared after a method, and getters reading later data.
// ---------------------------------------------------------------------------

#[test]
fn method_sees_later_accessor() {
    assert_clean_strict(
        "
const handlers = { run() { return this.status; }, get status() { return 200; } };
const code: number = handlers.run();
",
    );
}

#[test]
fn getter_sees_later_data_property() {
    assert_clean_strict(
        "
const box = { get label() { return this.text; }, text: \"hi\" };
const s: string = box.label;
",
    );
}

// ---------------------------------------------------------------------------
// `function`-expression property sees later data.
// ---------------------------------------------------------------------------

#[test]
fn function_property_sees_later_data() {
    assert_clean_strict(
        "
const widget = { render: function () { return this.width; }, width: 7 };
const w: number = widget.render();
",
    );
}

// ---------------------------------------------------------------------------
// Renamed binders (anti-hardcoding): same structural shape, different names.
// ---------------------------------------------------------------------------

#[test]
fn renamed_binders_method_sees_later_data() {
    assert_clean_strict(
        "
const controller = { dispatch() { return this.payload; }, payload: { id: \"x\" } };
const out: { id: string } = controller.dispatch();
",
    );
}

#[test]
fn nested_access_through_later_data() {
    assert_clean_strict(
        "
const server = { boot() { return this.config.port; }, config: { port: 8080 } };
const p: number = server.boot();
",
    );
}

// ---------------------------------------------------------------------------
// Precise type preservation: a real `this.value` mismatch must still error.
// ---------------------------------------------------------------------------

#[test]
fn later_data_keeps_precise_type_for_mismatch() {
    // `this.amount` is `number`; returning it against `string` must emit TS2322.
    let codes = check_source_strict_codes(
        "const ledger = { total(): string { return this.amount; }, amount: 42 };",
    );
    assert!(
        codes.contains(&2322),
        "expected TS2322 for the real mismatch, got: {codes:?}"
    );
}

#[test]
fn genuinely_missing_member_still_errors() {
    // `this.missing` does not exist anywhere in the literal -> TS2339, in both
    // declaration orders (order-independence is the point of the fix).
    let after = check_source_strict_codes("const a = { m() { return this.missing; }, value: 1 };");
    let before = check_source_strict_codes("const a = { value: 1, m() { return this.missing; } };");
    assert!(
        after.contains(&2339),
        "expected TS2339 for the missing member, got: {after:?}"
    );
    assert_eq!(
        before, after,
        "missing-member diagnostics must be order-independent"
    );
}

// ---------------------------------------------------------------------------
// `as const`: later data literal is preserved (and readonly).
// ---------------------------------------------------------------------------

#[test]
fn const_assertion_preserves_later_data_literal() {
    assert_clean_strict(
        "
const frozen = { read() { return this.tag; }, tag: 7 } as const;
const t: 7 = frozen.read();
",
    );
}

// ---------------------------------------------------------------------------
// Regression guard: method-after-method visibility is unchanged.
// ---------------------------------------------------------------------------

#[test]
fn method_sees_later_method_unchanged() {
    assert_clean_strict(
        "
const calc = { outer() { return this.inner(); }, inner() { return 1; } };
const r: number = calc.outer();
",
    );
}

// ---------------------------------------------------------------------------
// Self-referential later initializer must not crash and must stay tolerant.
// ---------------------------------------------------------------------------

#[test]
fn self_referential_later_initializer_is_tolerated() {
    // `record.seed` references the object's own binding; the preview falls back
    // to `any` rather than evaluating out of order. The point is no panic and
    // no spurious order-dependent error on `this.seed`.
    let codes = check_source_codes(
        "const record: any = { m() { return this.seed; }, seed: (record as any).x };",
    );
    assert!(
        !codes.contains(&2339),
        "this.seed must resolve (no TS2339), got: {codes:?}"
    );
}
