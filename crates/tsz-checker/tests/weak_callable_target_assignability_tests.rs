//! Regression for #14749: a callable interface assigned to another callable
//! interface whose only non-call member is a property must NOT be rejected by
//! the weak-type "no properties in common" rule.
//!
//! Root cause: `check_callable_subtype` compares the two call signatures, then
//! decomposes each callable into an `ObjectShape` built from its property part
//! (call/construct signatures stripped) and runs `check_object_subtype`. That
//! object comparison applied the weak-type rule to the stripped shapes, so
//! `{ (): void; extra: number }` <: `{ (): void; name?: string }` failed: the
//! target looked weak (all optional) and the source shared no property name.
//! tsc's `isWeakType` returns false for any type carrying a call/construct
//! signature, so a callable target is never weak. The fix marks the callable
//! property-part comparison (`in_callable_property_check`) so the direct-level
//! weak rejection is suppressed; nested property-value comparisons re-enable it
//! through `in_property_check`.
//!
//! Binder names are varied so the fix cannot key on any identifier.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs_code_messages, load_default_lib_files};
use tsz_common::common::ScriptTarget;

fn codes(source: &str) -> Vec<(u32, String)> {
    let libs = load_default_lib_files();
    assert!(!libs.is_empty(), "default lib files must be available");
    check_source_with_libs_code_messages(
        source,
        "test.ts",
        CheckerOptions {
            target: ScriptTarget::ES2022,
            strict: true,
            ..CheckerOptions::default()
        },
        &libs,
    )
}

fn assert_no_code(source: &str, code: u32, label: &str) {
    let diags = codes(source);
    assert!(
        !diags.iter().any(|(c, _)| *c == code),
        "{label}: expected no TS{code}. Got: {diags:#?}"
    );
}

fn assert_has_code(source: &str, code: u32, label: &str) {
    let diags = codes(source);
    assert!(
        diags.iter().any(|(c, _)| *c == code),
        "{label}: expected TS{code}. Got: {diags:#?}"
    );
}

#[test]
fn callable_target_optional_prop_source_extra_no_ts2322() {
    // tsc: clean. The target is callable, so it is not a weak type.
    let src = r#"
        interface T1 { (): void; name?: string }
        interface S1 { (): void; extra: number }
        declare const s1: S1;
        const a1: T1 = s1;
    "#;
    assert_no_code(
        src,
        2322,
        "callable optional-prop target, source extra prop",
    );
}

#[test]
fn callable_target_symbol_keyed_source_no_ts2322() {
    // mobx witness: IReactionDisposer ([sym]: Reaction) -> Lambda (name?).
    let src = r#"
        declare const sym: unique symbol;
        interface Reaction { x: 1 }
        interface Lambda { (): void; name?: string }
        interface IReactionDisposer { (): void; [sym]: Reaction }
        declare const d: IReactionDisposer;
        const l: Lambda = d;
    "#;
    assert_no_code(src, 2322, "callable target, symbol-keyed source extra");
}

#[test]
fn callable_target_renamed_binders_no_ts2322() {
    // Same structure, unrelated binder names — fix must not key on identifiers.
    let src = r#"
        interface Disposer { (): void; label?: string }
        interface Worker { (): void; pid: number }
        declare const w: Worker;
        const job: Disposer = w;
    "#;
    assert_no_code(src, 2322, "renamed callable binders");
}

#[test]
fn callable_target_no_extra_prop_control_clean() {
    // Control: target has no property besides the call signature; always clean.
    let src = r#"
        interface T2 { (): void }
        interface S2 { (): void; extra: number }
        declare const s2: S2;
        const a2: T2 = s2;
    "#;
    let diags = codes(src);
    assert!(
        diags.is_empty(),
        "callable target without a property must be clean. Got: {diags:#?}"
    );
}

#[test]
fn genuine_weak_object_target_still_ts2559() {
    // Negative: target is a real weak object (no call signature), source shares
    // no property -> the weak-type rule must STILL fire. tsc: TS2559.
    let src = r#"
        interface Weak { a?: 1; b?: 2 }
        declare const noCommon: { z: 3 };
        const wk: Weak = noCommon;
    "#;
    assert_has_code(src, 2559, "genuine weak object target");
}

#[test]
fn callable_target_nested_weak_property_still_errors() {
    // Soundness: the suppression is direct-level only. A callable property whose
    // VALUE is a genuine weak object must still fail (in_property_check
    // re-enables the rule). tsc: TS2322 (property 'cfg' incompatible).
    let src = r#"
        interface Tn { (): void; cfg?: { a?: string } }
        interface Sn { (): void; cfg: { c: number } }
        declare const sn: Sn;
        const n: Tn = sn;
    "#;
    assert_has_code(src, 2322, "nested weak property value must still error");
}
