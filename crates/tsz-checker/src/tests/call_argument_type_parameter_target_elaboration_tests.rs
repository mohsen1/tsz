//! Tests that `tsc`'s bare-type-parameter-target elaboration
//! (`TS5082`/`TS5075`) is also attached to the `TS2345` call-argument
//! surface, not only the `TS2322` assignment surface covered by
//! `nested_type_parameter_target_elaboration_tests.rs`.
//!
//! Structural rule: `tsc`'s `reportRelationError` attaches the
//! type-parameter note whenever a concrete source fails to relate to a bare
//! type-parameter target, regardless of whether the failing relation is an
//! assignment (`TS2322`) or a call argument (`TS2345`). A call whose callee
//! signature's own type parameter is itself a bare, uninstantiated type
//! parameter of the *caller* (e.g. `g<T>(x) { f<T>(null) }`) reaches this
//! shape without any overload set: `f`'s single, non-generic-from-the-
//! call-site signature resolves through the direct call path
//! (`error_argument_not_assignable_preserving_param_display` in
//! `types/computation/call_result.rs`), which built its message directly and
//! never consulted the checker's `unrelated_type_parameter_target_related_info`
//! helper that the `TS2322` path already used.

use crate::diagnostics::Diagnostic;
use crate::test_utils::check_source_diagnostics;

/// `TS5082` — "`'{T}'` could be instantiated with an arbitrary type …".
const COULD_BE_INSTANTIATED_ARBITRARY: u32 = 5082;
/// `TS5075` — "… could be instantiated with a different subtype of constraint …".
const COULD_BE_INSTANTIATED_DIFFERENT_SUBTYPE: u32 = 5075;

fn ts2345(source: &str) -> Diagnostic {
    check_source_diagnostics(source)
        .into_iter()
        .find(|d| d.code == 2345)
        .unwrap_or_else(|| panic!("expected a TS2345 for source:\n{source}"))
}

fn has_note(diag: &Diagnostic, code: u32) -> bool {
    diag.related_information.iter().any(|r| r.code == code)
}

#[test]
fn unconstrained_caller_type_param_target_gets_arbitrary_type_note() {
    // `g`'s own `T` has no `extends` clause, so `f`'s bare-type-parameter
    // target is unconstrained — `null` cannot satisfy an unconstrained
    // parameter, so tsc reports TS5082, not TS5075.
    let diag = ts2345(
        r#"
function f<T extends unknown>(x: T): void {}
function g<T>() {
    f<T>(null);
}
"#,
    );
    assert!(
        has_note(&diag, COULD_BE_INSTANTIATED_ARBITRARY),
        "expected TS5082 arbitrary-type note; got: {:?}",
        diag.related_information
    );
    assert!(!has_note(&diag, COULD_BE_INSTANTIATED_DIFFERENT_SUBTYPE));
}

#[test]
fn constrained_caller_type_param_target_gets_different_subtype_note() {
    // `g`'s own `T extends string` is satisfied by the `string` argument, so
    // tsc reports TS5075 ("assignable to the constraint... but could be
    // instantiated with a different subtype"), not TS5082.
    let diag = ts2345(
        r#"
function f<T extends unknown>(x: T): void {}
function g<T extends string>(y: string) {
    f<T>(y);
}
"#,
    );
    assert!(
        has_note(&diag, COULD_BE_INSTANTIATED_DIFFERENT_SUBTYPE),
        "expected TS5075 different-subtype note; got: {:?}",
        diag.related_information
    );
    assert!(!has_note(&diag, COULD_BE_INSTANTIATED_ARBITRARY));
}

#[test]
fn concrete_target_gets_no_type_parameter_note() {
    // A plain concrete parameter type is not a bare type parameter at all —
    // no elaboration note should be attached, matching the ordinary TS2345.
    let diag = ts2345(
        r#"
function f(x: number): void {}
f("str");
"#,
    );
    assert!(!has_note(&diag, COULD_BE_INSTANTIATED_ARBITRARY));
    assert!(!has_note(&diag, COULD_BE_INSTANTIATED_DIFFERENT_SUBTYPE));
}

#[test]
fn renamed_binders_still_elaborate() {
    // Anti-hardcoding: the same shape under different identifier names must
    // still elaborate — the decision is structural (is the target a bare
    // type parameter?), not keyed on any particular binder name.
    let diag = ts2345(
        r#"
function acceptsParam<Param extends unknown>(value: Param): void {}
function caller<Caller>() {
    acceptsParam<Caller>(null);
}
"#,
    );
    assert!(
        has_note(&diag, COULD_BE_INSTANTIATED_ARBITRARY),
        "expected TS5082 arbitrary-type note under renamed binders; got: {:?}",
        diag.related_information
    );
}

#[test]
fn direct_assignment_ts2322_path_unaffected() {
    // Regression guard: the pre-existing TS2322 direct-assignment path
    // (`error_reporter/render_failure/type_mismatch.rs`) must keep producing
    // its own elaboration unchanged by this TS2345 fix.
    let diag = check_source_diagnostics(
        r#"
function f<T extends unknown>(x: T): T {
    return null;
}
"#,
    )
    .into_iter()
    .find(|d| d.code == 2322)
    .unwrap_or_else(|| panic!("expected a TS2322"));
    assert!(has_note(&diag, COULD_BE_INSTANTIATED_DIFFERENT_SUBTYPE));
}
