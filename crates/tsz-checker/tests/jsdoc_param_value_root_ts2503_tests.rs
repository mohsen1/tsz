//! Regression coverage for a JSDoc `@param` qualifier rooted at a plain
//! runtime value.
//!
//! Structural rule: a bare dotted JSDoc type name `A.B` can only use `A` as
//! a namespace/type qualifier when `A` has namespace/type meaning (a
//! namespace/module, class, enum, interface, type alias, or import alias).
//! A plain runtime value (`var A = {}`) that only grew members via a JS
//! expando write (`A.B = class {}`) is not a namespace in type position;
//! `tsc` emits `TS2503` ("Cannot find namespace 'A'.") — oracle-verified
//! (typescript@7.0.2) against
//! `TypeScript/tests/cases/conformance/salsa/typeFromPropertyAssignment35.ts`.
//!
//! The check (`report_jsdoc_value_root_used_as_namespace`,
//! `crates/tsz-checker/src/jsdoc/lookup.rs`) already ran for the `@type` tag
//! path (`jsdoc_type_annotation_for_node`). It was never wired into the
//! `@param {type} name` path (`resolve_jsdoc_param_type_with_pos`,
//! `crates/tsz-checker/src/jsdoc/params_type_strings.rs`), so tsz silently
//! resolved the expando member and reported a downstream `TS2339` instead of
//! `TS2503` at the root.

use tsz_checker::test_utils::check_js_source_diagnostics;

/// The exact `typeFromPropertyAssignment35.ts` shape, reduced to a single
/// file: a plain-value root grows an expando member, referenced through a
/// JSDoc `@param` dotted type name.
#[test]
fn param_plain_value_root_expando_qualifier_reports_ts2503() {
    let diags = check_js_source_diagnostics(
        "var Emu = {}\nEmu.D = class {\n}\n/** @param {Emu.D} x */\nfunction f(x) {\n}\n",
    );
    let codes: Vec<_> = diags.iter().map(|d| d.code).collect();
    assert_eq!(codes, vec![2503], "expected TS2503, got {diags:?}");
    assert!(
        diags[0].message_text.contains("Emu"),
        "TS2503 should name the unresolved root, got {diags:?}"
    );
}

/// Renamed binder: the rule keys on symbol shape, not identifier spelling.
#[test]
fn param_plain_value_root_expando_qualifier_renamed_binder_reports_ts2503() {
    let diags = check_js_source_diagnostics(
        "var Widget = {}\nWidget.Part = class {\n}\n/** @param {Widget.Part} y */\nfunction g(y) {\n}\n",
    );
    let codes: Vec<_> = diags.iter().map(|d| d.code).collect();
    assert_eq!(codes, vec![2503], "got {diags:?}");
}

/// Negative control: a real namespace root keeps its meaning — declaring
/// `A` as a namespace with a member `B` must not trip the plain-value check.
#[test]
fn param_namespace_root_qualifier_stays_clean() {
    let diags = check_js_source_diagnostics(
        "namespace A {\n  export class B {}\n}\n/** @param {A.B} x */\nfunction f(x) {\n}\n",
    );
    let codes: Vec<_> = diags.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2503),
        "namespace root must not be treated as a plain value, got {diags:?}"
    );
}

/// Negative control: `@type` tag coverage (the pre-existing path) is
/// untouched by wiring the check into `@param` as well.
#[test]
fn type_tag_plain_value_root_expando_qualifier_still_reports_ts2503() {
    let diags = check_js_source_diagnostics(
        "var Emu = {}\nEmu.D = class {\n}\n/** @type {Emu.D} */\nvar x\n",
    );
    let codes: Vec<_> = diags.iter().map(|d| d.code).collect();
    assert_eq!(codes, vec![2503], "expected TS2503, got {diags:?}");
}

/// Optional-suffix (`{Type=}`) and rest (`{...Type}`) `@param` spellings
/// must resolve the same root position math as the bare form.
#[test]
fn param_plain_value_root_optional_suffix_reports_ts2503() {
    let diags = check_js_source_diagnostics(
        "var Emu = {}\nEmu.D = class {\n}\n/** @param {Emu.D=} x */\nfunction f(x) {\n}\n",
    );
    let codes: Vec<_> = diags.iter().map(|d| d.code).collect();
    assert_eq!(codes, vec![2503], "expected TS2503, got {diags:?}");
}

#[test]
fn param_plain_value_root_rest_reports_ts2503() {
    let diags = check_js_source_diagnostics(
        "var Emu = {}\nEmu.D = class {\n}\n/** @param {...Emu.D} x */\nfunction f(...x) {\n}\n",
    );
    let codes: Vec<_> = diags.iter().map(|d| d.code).collect();
    assert_eq!(codes, vec![2503], "expected TS2503, got {diags:?}");
}
