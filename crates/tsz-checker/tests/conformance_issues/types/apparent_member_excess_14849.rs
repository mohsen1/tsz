//! Fresh-object-literal excess-property checking (TS2353) must not be
//! suppressed when the target's property happens to share a name with an
//! `Object.prototype` / `Function.prototype` member (#14849).
//!
//! Root cause: `query_boundaries::assignability::is_global_object_or_function_shape`
//! recognized the global `Object`/`Function` interface with a *subset* check
//! over a merged prototype-member name list (`length`, `toString`, `valueOf`,
//! `constructor`, `toLocaleString`, `hasOwnProperty`, `isPrototypeOf`,
//! `propertyIsEnumerable`). A user object type whose sole property was one of
//! those names — `{ length: number }`, `{ toString: number }` — was therefore
//! misclassified as the global interface and had its excess-property check
//! suppressed (reported TS2345 over the whole argument, or nothing on the
//! assignment path, instead of tsc's TS2353 at the offending property).
//!
//! The boundary now delegates to the canonical `global_interfaces` sniff, which
//! requires the *full* distinguishing member set under a property-count cap, so
//! these user types are no longer mistaken for the global interface.

use super::super::core::*;

/// Every name in the historical trigger set, when it is the sole property of a
/// plain object target, must still flag a fresh-literal excess sibling as
/// TS2353 — exactly like an ordinary property name does.
#[test]
fn fresh_literal_excess_against_apparent_member_target_reports_ts2353() {
    // Apparent-member names (the bug's exact trigger set) paired with a varied
    // excess sibling name, so the assertion is structural rather than keyed to a
    // single fixture string.
    let cases = [
        ("length", "extra"),
        ("toString", "zzz"),
        ("valueOf", "other"),
        ("constructor", "spurious"),
        ("toLocaleString", "leftover"),
        ("hasOwnProperty", "stray"),
        ("isPrototypeOf", "bonus"),
        ("propertyIsEnumerable", "surplus"),
    ];
    for (member, excess) in cases {
        let source = format!(
            "declare function accept(value: {{ {member}: number }}): void;\n\
             accept({{ {member}: 1, {excess}: 3 }});\n"
        );
        let diagnostics = compile_and_get_diagnostics(&source);
        assert!(
            has_error(&diagnostics, 2353),
            "expected TS2353 for excess `{excess}` against `{{ {member}: number }}`, \
             got {diagnostics:#?}"
        );
        assert!(
            !has_error(&diagnostics, 2345),
            "excess against an apparent-member target must elaborate to TS2353, \
             not a whole-argument TS2345 (member `{member}`): {diagnostics:#?}"
        );
        let message = diagnostic_message(&diagnostics, 2353).unwrap_or_default();
        assert!(
            message.contains(excess),
            "TS2353 must name the offending excess property `{excess}`: {message}"
        );
    }
}

/// The same divergence on the assignment path: previously the apparent-member
/// target produced no diagnostic at all (the excess check was skipped after a
/// structurally-successful relation). It must now report TS2353.
#[test]
fn assignment_to_apparent_member_target_reports_excess_ts2353() {
    let diagnostics = compile_and_get_diagnostics(
        "const widget: { length: number } = { length: 1, caption: 'x' };\n",
    );
    assert!(
        has_error(&diagnostics, 2353),
        "fresh literal assigned to `{{ length: number }}` must flag excess `caption`: {diagnostics:#?}"
    );
}

/// Control: a non-apparent property name has always reported TS2353 — it must
/// keep doing so (the fix changes only the misclassified apparent-member case).
#[test]
fn fresh_literal_excess_against_ordinary_target_still_reports_ts2353() {
    let diagnostics = compile_and_get_diagnostics(
        "declare function accept(value: { id: number }): void;\n\
         accept({ id: 1, zzz: 3 });\n",
    );
    assert!(has_error(&diagnostics, 2353), "{diagnostics:#?}");
}

/// Control: the relation itself is fine. Routing the same value through a
/// non-fresh variable must not produce an excess error — confirming the fix is
/// scoped to the fresh-literal excess path, not the underlying subtype check.
#[test]
fn non_fresh_variable_with_apparent_member_target_has_no_excess_error() {
    let diagnostics = compile_and_get_diagnostics(
        "declare function accept(value: { length: number }): void;\n\
         const source = { length: 1, zzz: 3 };\n\
         accept(source);\n",
    );
    assert!(
        !has_error(&diagnostics, 2353) && !has_error(&diagnostics, 2345),
        "a non-fresh source is structurally assignable; no excess/relation error: {diagnostics:#?}"
    );
}

/// Adjacent: a second, non-apparent property alongside the apparent one always
/// reported TS2353 (the subset misclassification could not trigger). Guard that
/// it is unchanged.
#[test]
fn apparent_member_with_extra_named_property_still_reports_excess() {
    let diagnostics = compile_and_get_diagnostics(
        "declare function accept(value: { length: number; label: string }): void;\n\
         accept({ length: 1, label: 'a', zzz: 3 });\n",
    );
    assert!(has_error(&diagnostics, 2353), "{diagnostics:#?}");
}

/// Regression guard: the *real* global `Object` interface must still suppress
/// excess-property checking — `Object` accepts any object literal, so passing
/// `{ zzz: 3 }` to a `value: Object` parameter is not an error.
#[test]
fn real_global_object_target_still_skips_excess() {
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_and_get_diagnostics_with_lib(
        "declare function accept(value: Object): void;\n\
         accept({ zzz: 3 });\n",
    );
    assert!(
        !has_error(&diagnostics, 2353),
        "the global `Object` interface accepts arbitrary object literals (no excess check): {diagnostics:#?}"
    );
}
