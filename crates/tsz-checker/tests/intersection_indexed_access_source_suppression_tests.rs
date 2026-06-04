//! Regression coverage for issue #11581 (case family `checker-22-20`):
//! "intersection fallback hides root property mismatch".
//!
//! Structural rule: when the *source* of an assignment is an intersection whose
//! members include an unresolved indexed-access type (`T[K]` with free type
//! parameters), tsz must still run the structural property relation against the
//! target. The solver resolves `T[K]` through its constraint/apparent type, so a
//! blanket "intersection-with-indexed-access source" suppression only hides
//! genuine property mismatches that `tsc` reports.
//!
//! Before this fix `should_suppress_assignability_diagnostic` carried a clause
//! `source_is_intersection_with_indexed_access()` that suppressed TS2322/TS2741
//! for *any* such source whose target was not already a structural type. That hid
//! real errors for plain-object and intersection targets, e.g.
//! `T[K] & { tag: string }` assigned to `{ id: string }`.
//!
//! These run under the lib-less checker harness, so they use user-defined mapped
//! types instead of the lib `Partial`/`Record` (issue #9648 notes the rule is not
//! `Record`-specific). Anti-hardcoding (§25): binder/type-parameter names vary
//! across cases and positive controls bracket every negative.

use tsz_checker::test_utils::{
    check_source_diagnostics, diagnostic_code_message_refs, has_any_diagnostic_code,
};

/// The structural assignability families this regression watches: `TS2322`
/// (value mismatch) and `TS2741` (missing required property). `tsc` picks either
/// depending on whether the failure is a missing key or a value clash, so the
/// assertions key on the *family* to stay robust to elaboration choice.
const ASSIGNABILITY_CODES: &[u32] = &[2322, 2741];

fn assert_error(source: &str) {
    let diags = check_source_diagnostics(source);
    assert!(
        has_any_diagnostic_code(&diags, ASSIGNABILITY_CODES),
        "expected TS2322/TS2741 for index-access intersection mismatch; got: {:?}",
        diagnostic_code_message_refs(&diags)
    );
}

fn assert_no_error(source: &str) {
    let diags = check_source_diagnostics(source);
    assert!(
        !has_any_diagnostic_code(&diags, ASSIGNABILITY_CODES),
        "expected no assignability error; got: {:?}",
        diagnostic_code_message_refs(&diags)
    );
}

// ---------------------------------------------------------------------------
// Negative cases: tsc reports a structural mismatch and tsz must too.
// ---------------------------------------------------------------------------

/// `T[K] & { tag: string }` lacks `id`, so assigning to `{ id: string }` must
/// report the missing/invalid property (tsc: TS2741).
#[test]
fn index_access_intersection_source_missing_property_reports_error() {
    assert_error(
        r#"
        function f<T extends { id: number }, K extends keyof T>(x: T[K] & { tag: string }) {
            const y: { id: string } = x;
        }
        "#,
    );
}

/// The intersection carries `id: string`; the target wants `id: number`, so the
/// value types clash (tsc: TS2322).
#[test]
fn index_access_intersection_source_mismatched_property_reports_error() {
    assert_error(
        r#"
        function f<T extends { id: number }, K extends keyof T>(x: T[K] & { id: string }) {
            const y: { id: number } = x;
        }
        "#,
    );
}

/// User-defined `MyPartial<T>[K] & ({} | null)` — the exact shape the old code
/// comment claimed "should actually be assignable", which tsc nonetheless
/// rejects against a concrete object target.
#[test]
fn partial_like_index_access_intersection_source_reports_error() {
    assert_error(
        r#"
        type MyPartial<U> = { [P in keyof U]?: U[P] };
        function g<S extends { a: number }, Q extends keyof S>(x: MyPartial<S>[Q] & ({} | null)) {
            const y: { a: number } = x;
        }
        "#,
    );
}

/// Mapped-type target must also see the mismatch (control that the
/// already-shipped `target_is_structural` carve-out still works alongside the
/// broadened rule).
#[test]
fn index_access_intersection_source_mapped_target_reports_error() {
    assert_error(
        r#"
        type Box<U> = { [P in keyof U]: U[P] };
        function f<T extends { id: number }, K extends keyof T>(x: T[K] & { tag: string }) {
            const y: Box<{ id: string }> = x;
        }
        "#,
    );
}

/// Intersection target (a second structural shape) must surface the mismatch.
#[test]
fn index_access_intersection_source_intersection_target_reports_error() {
    assert_error(
        r#"
        function f<T extends { id: number }, K extends keyof T>(x: T[K] & { tag: string }) {
            const y: { id: string } & { extra?: 1 } = x;
        }
        "#,
    );
}

/// Anti-hardcoding: renamed binders/type parameters must behave identically.
#[test]
fn index_access_intersection_source_renamed_binders_reports_error() {
    assert_error(
        r#"
        function transform<Elem extends { id: number }, Prop extends keyof Elem>(
            value: Elem[Prop] & { tag: string },
        ) {
            const out: { id: string } = value;
        }
        "#,
    );
}

// ---------------------------------------------------------------------------
// Positive controls: tsc accepts; the broadened rule must NOT introduce false
// positives. These are the cases the suppression was nominally protecting.
// ---------------------------------------------------------------------------

/// Identity assignment to the same index-access intersection stays assignable.
#[test]
fn index_access_intersection_source_identity_target_is_assignable() {
    assert_no_error(
        r#"
        function f<T extends { a: number }, K extends keyof T>(x: T[K] & { tag: string }) {
            const y: T[K] & { tag: string } = x;
        }
        "#,
    );
}

/// Assigning to the bare index-access component stays assignable.
#[test]
fn index_access_intersection_source_bare_index_target_is_assignable() {
    assert_no_error(
        r#"
        function f<T extends { a: number }, K extends keyof T>(x: T[K] & { tag: string }) {
            const y: T[K] = x;
        }
        "#,
    );
}

/// Assigning to the empty object type stays assignable.
#[test]
fn index_access_intersection_source_empty_object_target_is_assignable() {
    assert_no_error(
        r#"
        function f<T extends { a: number }, K extends keyof T>(x: T[K] & { tag: string }) {
            const y: {} = x;
        }
        "#,
    );
}

/// Assigning to the literal-only sibling member stays assignable.
#[test]
fn index_access_intersection_source_sibling_member_target_is_assignable() {
    assert_no_error(
        r#"
        function f<T extends { a: number }, K extends keyof T>(x: T[K] & { tag: string }) {
            const y: { tag: string } = x;
        }
        "#,
    );
}

/// Compatible concrete object: the contributed value matches, so no error.
#[test]
fn index_access_intersection_source_compatible_object_target_is_assignable() {
    assert_no_error(
        r#"
        function f<T extends { id: number }, K extends keyof T>(x: T[K] & { id: number }) {
            const y: { id: number } = x;
        }
        "#,
    );
}
