//! Regression tests for #17157: inside an object-literal method body,
//! `this.<sibling>()` collapsed to `any` when the sibling was an unannotated
//! method declared *later* in the same literal, silently dropping any
//! diagnostic that depended on the call's result (TS2322, TS2345, ...).
//! Declaration order flipped the outcome — the signature of the bug.
//!
//! Fix: `object_literal_sibling_callable_signature` in
//! `types/computation/object_literal_circularity.rs` infers a forward sibling's
//! widened literal return type when its body is exactly `{ return <literal>; }`,
//! instead of hardcoding `any`. Anything else (multi-statement bodies, a
//! genuine `this`-return cycle, a truly missing member) is unaffected.

use tsz_checker::test_utils::check_source_codes;

const TS2322: u32 = 2322;
const TS2339: u32 = 2339;
const TS7023: u32 = 7023;

#[test]
fn forward_referenced_unannotated_sibling_return_type_is_inferred() {
    // The reported repro: `bar` is declared after `foo`, so tsz's incremental
    // synthetic-`this` builder hadn't seen it yet when `foo`'s body checked.
    let source = r#"
const obj = { foo() { return this.bar(); }, bar() { return 1; } };
const t: string = obj.foo();
"#;
    assert_eq!(check_source_codes(source), vec![TS2322]);
}

#[test]
fn backward_referenced_sibling_return_type_still_inferred() {
    // Control: `bar` before `foo` was already correct (already present in the
    // incremental `properties` map). Must keep working identically.
    let source = r#"
const obj = { bar() { return 1; }, foo() { return this.bar(); } };
const t: string = obj.foo();
"#;
    assert_eq!(check_source_codes(source), vec![TS2322]);
}

#[test]
fn forward_reference_with_renamed_binders() {
    let source = r#"
const registry = { first() { return this.second(); }, second() { return "hi"; } };
const n: number = registry.first();
"#;
    assert_eq!(check_source_codes(source), vec![TS2322]);
}

#[test]
fn forward_reference_through_function_expression_property() {
    let source = r#"
const obj = { foo: function () { return this.bar(); }, bar() { return 1; } };
const t: string = obj.foo();
"#;
    assert_eq!(check_source_codes(source), vec![TS2322]);
}

#[test]
fn genuine_cycle_still_reports_circular_return_not_a_loop() {
    let source = r#"
const obj = {
  foo() { return this.bar(); },
  bar() { return this.foo(); },
};
"#;
    let codes = check_source_codes(source);
    assert!(
        codes.contains(&TS7023),
        "expected circular-return TS7023, got {codes:?}"
    );
}

#[test]
fn forward_sibling_with_non_literal_body_stays_any_no_false_positive() {
    // Multi-statement body: outside the safe literal-only prescan, so the
    // sibling stays `any` (no diagnostic introduced, none dropped either since
    // there was never one to find here).
    let source = r#"
const obj = {
  foo() { return this.bar(); },
  bar() { const x = 1; return x; },
};
const t: string = obj.foo();
"#;
    assert_eq!(check_source_codes(source), Vec::<u32>::new());
}

#[test]
fn forward_sibling_already_any_return_introduces_no_new_error() {
    // `bar`'s single statement returns an uninitialized (implicit-any) local,
    // not a literal, so the prescan yields `None` and the sibling keeps its
    // pre-fix `any` return — no new diagnostic from this change.
    let source = r#"
const obj = {
  foo() { return this.bar(); },
  bar() { let y; return y; },
};
const t: string = obj.foo();
"#;
    assert_eq!(check_source_codes(source), Vec::<u32>::new());
}

#[test]
fn nonexistent_forward_member_still_reports_missing_property() {
    // TS7023 here is a pre-existing companion diagnostic, unrelated to this
    // fix and unaffected by it: it fires identically even with a single
    // method and no forward reference at all (`{ foo() { return
    // this.missing(); } }`), confirmed against pre-fix code too. The relevant
    // assertion for this test is that TS2339 still fires for the genuinely
    // missing member.
    let source = r#"
const obj = { foo() { return this.missing(); }, bar() { return 1; } };
"#;
    let codes = check_source_codes(source);
    assert!(
        codes.contains(&TS2339),
        "expected missing-property TS2339, got {codes:?}"
    );
}

#[test]
fn forward_reference_boolean_literal_return_widens() {
    let source = r#"
const obj = { foo() { return this.bar(); }, bar() { return true; } };
const s: string = obj.foo();
"#;
    assert_eq!(check_source_codes(source), vec![TS2322]);
}

#[test]
fn forward_reference_annotated_sibling_return_type_takes_precedence() {
    // An explicit annotation on the forward sibling must still win over the
    // literal-return prescan.
    let source = r#"
const obj = { foo() { return this.bar(); }, bar(): number { return 1; } };
const t: string = obj.foo();
"#;
    assert_eq!(check_source_codes(source), vec![TS2322]);
}
