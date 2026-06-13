//! Regression tests for TS2322 over-suppression on `keyof <type parameter>`
//! targets.
//!
//! `should_suppress_assignability_diagnostic`'s "complex generic" carve-out
//! suppressed any target that merely *contained* a type parameter and was not
//! itself a bare type parameter. A deferred `keyof T` target satisfies that
//! shape, so a genuine key-space mismatch the solver had already rejected
//! (`keyof S <: keyof T` iff `T <: S`) was silently dropped — `tsc` reports
//! TS2322 there. A deferred `keyof T` is a structural key-space relation the
//! solver decides directly, so it must not take the complex-generic
//! suppression (concrete-key `keyof` targets are handled by the separate
//! literal-membership suppression).
//!
//! The rule is structural (`keyof` of a type-parameter-bearing operand), not
//! keyed on any identifier, so the renamed-binder variants below must behave
//! identically.

use tsz_checker::test_utils::check_source_codes;

const TYPE_NOT_ASSIGNABLE: u32 = 2322;

#[test]
fn keyof_of_distinct_type_params_reports_ts2322() {
    // `keyof X` is not assignable to `keyof A` for unrelated `A`, `X`
    // (contravariant: it would require `A <: X`). tsc reports TS2322.
    let source = r#"
function f<A, X>(t: keyof A, s: keyof X): void {
    t = s;
}
"#;
    let codes = check_source_codes(source);
    assert!(
        codes.contains(&TYPE_NOT_ASSIGNABLE),
        "keyof X assigned to keyof A (distinct params) must report TS2322; got {codes:?}"
    );
}

#[test]
fn keyof_of_distinct_type_params_renamed_binders_reports_ts2322() {
    // Same structural shape, different binder names: the decision must not be
    // name-keyed.
    let source = r#"
function relate<Schema, Other>(left: keyof Schema, right: keyof Other): void {
    left = right;
}
"#;
    let codes = check_source_codes(source);
    assert!(
        codes.contains(&TYPE_NOT_ASSIGNABLE),
        "renamed-binder keyof mismatch must still report TS2322; got {codes:?}"
    );
}

#[test]
fn keyof_variable_declaration_initializer_reports_ts2322() {
    // The same defect reached through a variable-declaration initializer.
    let source = r#"
function f<A, X>(s: keyof X): void {
    const t: keyof A = s;
}
"#;
    let codes = check_source_codes(source);
    assert!(
        codes.contains(&TYPE_NOT_ASSIGNABLE),
        "keyof X initializer for a keyof A binding must report TS2322; got {codes:?}"
    );
}

#[test]
fn keyof_of_indexed_access_type_params_reports_ts2322() {
    // The kysely `AnyColumn` shape: `keyof DB[TB]`. The deferred indexed-keyof
    // target must stay non-suppressible.
    let source = r#"
function f<A, BK extends keyof A, X, YK extends keyof X>(
    t: keyof A[BK],
    s: keyof X[YK],
): void {
    t = s;
}
"#;
    let codes = check_source_codes(source);
    assert!(
        codes.contains(&TYPE_NOT_ASSIGNABLE),
        "keyof X[YK] assigned to keyof A[BK] must report TS2322; got {codes:?}"
    );
}

#[test]
fn keyof_subset_override_direction_reports_ts2322() {
    // `A extends X` makes `keyof A` a *superset* of `keyof X`, so
    // `keyof A <: keyof X` is false (it would require `X <: A`). tsc reports
    // TS2322; the suppression must not hide it.
    let source = r#"
function f<X, A extends X>(t: keyof X, s: keyof A): void {
    t = s;
}
"#;
    let codes = check_source_codes(source);
    assert!(
        codes.contains(&TYPE_NOT_ASSIGNABLE),
        "keyof A assigned to keyof X (A extends X) must report TS2322; got {codes:?}"
    );
}

#[test]
fn keyof_self_assignment_stays_clean() {
    // Negative control: `keyof A <: keyof A` is trivially assignable. The fix
    // must not start manufacturing a spurious TS2322 here.
    let source = r#"
function f<A>(t: keyof A, s: keyof A): void {
    t = s;
}
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&TYPE_NOT_ASSIGNABLE),
        "keyof A assigned to keyof A must stay clean; got {codes:?}"
    );
}

#[test]
fn keyof_constraint_subtype_direction_stays_clean() {
    // Negative control / adjacent direction: `A extends X` means
    // `keyof X <: keyof A`, so assigning `keyof X` to `keyof A` is allowed.
    let source = r#"
function f<X, A extends X>(t: keyof A, s: keyof X): void {
    t = s;
}
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&TYPE_NOT_ASSIGNABLE),
        "keyof X assigned to keyof A (A extends X) must stay clean; got {codes:?}"
    );
}
