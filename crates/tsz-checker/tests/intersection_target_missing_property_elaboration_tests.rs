//! Intersection-target missing-property elaboration (TS2322 + nested reason).
//!
//! Structural rule: when a value is assigned to an *intersection* target and a
//! required property of one intersection member is missing, tsc keeps the
//! top-level `Type 'S' is not assignable to type '<intersection>'` (TS2322) but
//! elaborates *which* member requires the missing property:
//!
//! ```text
//! Type 'S' is not assignable to type '<intersection>'.
//!   Property 'X' is missing in type 'S' but required in type '<member>'.
//! ```
//!
//! tsz previously emitted only the bare top-level TS2322 for intersection
//! targets (the "intersection fallback"), hiding the root property mismatch.
//! See issue #11480 (`checker intersection fallback hides root property mismatch
//! in mapped rows`).
//!
//! These tests vary the mapped-type iteration variable, the property names, and
//! the alias names so a fix keyed to a particular spelling would not satisfy
//! them. They assert structurally (the elaboration line names the missing
//! property and that it is *required in type*) rather than depending on the
//! exact member rendering, which is governed by the type printer.

use tsz_checker::test_utils::check_source_diagnostics;
use tsz_common::diagnostics::Diagnostic;

fn diagnostics(source: &str) -> Vec<Diagnostic> {
    check_source_diagnostics(source)
}

/// True when some TS2322 carries a nested "Property '<prop>' is missing ... but
/// required in type ..." elaboration line.
fn has_missing_member_elaboration(diags: &[Diagnostic], prop: &str) -> bool {
    let needle = format!("Property '{prop}' is missing");
    diags.iter().any(|d| {
        d.code == 2322
            && d.related_information.iter().any(|info| {
                info.message_text.contains(&needle)
                    && info.message_text.contains("but required in type")
            })
    })
}

/// Reported repro: a mapped member of the intersection requires the missing
/// property. The top-level stays TS2322; the nested line must name `b`.
#[test]
fn missing_property_in_mapped_member_emits_elaboration() {
    let diags = diagnostics(
        r#"
type Map1<T> = { [K in keyof T]: T[K] };
type Target = Map1<{ a: string; b: number }> & { c: boolean };
const v: Target = { a: "s", c: true };
"#,
    );
    assert!(
        has_missing_member_elaboration(&diags, "b"),
        "expected TS2322 with a `Property 'b' is missing ... required in type` \
         elaboration for the mapped intersection member; got {diags:?}"
    );
}

/// Same rule, different mapped-variable spelling (`P` instead of `K`),
/// different property names, and different alias names. A name-hardcoded fix
/// would miss this.
#[test]
fn missing_property_in_mapped_member_renamed_vars() {
    let diags = diagnostics(
        r#"
type Identity<U> = { [P in keyof U]: U[P] };
type Combined = Identity<{ first: string; second: number }> & { flag: boolean };
const w: Combined = { first: "x", flag: true };
"#,
    );
    assert!(
        has_missing_member_elaboration(&diags, "second"),
        "expected the elaboration regardless of mapped-variable / property / \
         alias spelling; got {diags:?}"
    );
}

/// The missing property lives in a *plain* (non-mapped) member of the
/// intersection. Here the member rendering is stable, so we can assert the
/// full elaboration text including the requiring member.
#[test]
fn missing_property_in_plain_member_names_member() {
    let diags = diagnostics(
        r#"
type Map1<T> = { [K in keyof T]: T[K] };
type Target = Map1<{ a: string }> & { b: number };
const v: Target = { a: "s" };
"#,
    );
    let matched = diags.iter().any(|d| {
        d.code == 2322
            && d.related_information.iter().any(|info| {
                info.message_text.contains("Property 'b' is missing")
                    && info
                        .message_text
                        .contains("required in type '{ b: number; }'")
            })
    });
    assert!(
        matched,
        "expected `Property 'b' is missing ... required in type '{{ b: number; }}'`; \
         got {diags:?}"
    );
}

/// A non-literal source (so the object-literal property elaboration cannot fire)
/// still produces the intersection member elaboration.
#[test]
fn missing_property_non_literal_source_emits_elaboration() {
    let diags = diagnostics(
        r#"
type Map1<T> = { [K in keyof T]: T[K] };
type Target = Map1<{ a: string; b: number }> & { c: boolean };
declare const src: { a: string; c: boolean };
const v: Target = src;
"#,
    );
    assert!(
        has_missing_member_elaboration(&diags, "b"),
        "expected the elaboration for a non-literal source assigned to an \
         intersection target; got {diags:?}"
    );
}

/// Negative / fallback: when the source satisfies every intersection member,
/// there is no assignability error at all — and therefore no missing-member
/// elaboration. Guards against a spurious diagnostic.
#[test]
fn complete_source_has_no_error() {
    let diags = diagnostics(
        r#"
type Map1<T> = { [K in keyof T]: T[K] };
type Target = Map1<{ a: string; b: number }> & { c: boolean };
const v: Target = { a: "s", b: 1, c: true };
"#,
    );
    assert!(
        !diags.iter().any(|d| d.code == 2322 || d.code == 2741),
        "a complete source must not produce an assignability error; got {diags:?}"
    );
}

/// Negative / fallback: a member property whose *type* mismatches (rather than
/// being missing) is reported at the property location, not as a missing-member
/// elaboration on the intersection chain. Guards against over-eager elaboration.
#[test]
fn member_property_type_mismatch_is_not_missing_elaboration() {
    let diags = diagnostics(
        r#"
type Map1<T> = { [K in keyof T]: T[K] };
type Target = Map1<{ a: string }> & { b: number };
const v: Target = { a: 123, b: 1 };
"#,
    );
    // The `a` mismatch is a value error, not a missing property.
    assert!(
        !has_missing_member_elaboration(&diags, "a"),
        "a property *type* mismatch must not be reported as a missing-member \
         elaboration; got {diags:?}"
    );
    assert!(
        diags.iter().any(|d| d.code == 2322),
        "expected a TS2322 for the `a` value mismatch; got {diags:?}"
    );
}

// ── Multiple-missing-property (MissingProperties) cases ──────────────────────

/// When two properties are missing and both live in the same mapped intersection
/// member, each missing property gets its own elaboration line.
#[test]
fn multiple_missing_in_mapped_member_elaborates_each() {
    let diags = diagnostics(
        r#"
type Map1<T> = { [K in keyof T]: T[K] };
type Target = Map1<{ a: string; b: number; c: boolean }> & { d: string };
const v: Target = { d: "x" };
"#,
    );
    assert!(
        has_missing_member_elaboration(&diags, "a"),
        "expected elaboration for missing `a`; got {diags:?}"
    );
    assert!(
        has_missing_member_elaboration(&diags, "b"),
        "expected elaboration for missing `b`; got {diags:?}"
    );
    assert!(
        has_missing_member_elaboration(&diags, "c"),
        "expected elaboration for missing `c`; got {diags:?}"
    );
}

/// Properties missing from *different* intersection members each get an
/// elaboration line pointing to their respective requiring member.
#[test]
fn multiple_missing_across_different_members_elaborates_each() {
    let diags = diagnostics(
        r#"
type Map1<T> = { [K in keyof T]: T[K] };
type Target = Map1<{ x: string }> & { y: number };
const v: Target = {};
"#,
    );
    assert!(
        has_missing_member_elaboration(&diags, "x"),
        "expected elaboration for missing `x` in mapped member; got {diags:?}"
    );
    assert!(
        has_missing_member_elaboration(&diags, "y"),
        "expected elaboration for missing `y` in plain member; got {diags:?}"
    );
}

/// Anti-hardcoding: different iteration-variable / property / alias spellings
/// must not affect whether elaboration fires for the multi-property case.
#[test]
fn multiple_missing_renamed_vars_elaborates() {
    let diags = diagnostics(
        r#"
type Wrap<U> = { [Q in keyof U]: U[Q] };
type Combined = Wrap<{ alpha: string; beta: number }> & { gamma: boolean };
const w: Combined = { gamma: true };
"#,
    );
    assert!(
        has_missing_member_elaboration(&diags, "alpha"),
        "expected elaboration for missing `alpha`; got {diags:?}"
    );
    assert!(
        has_missing_member_elaboration(&diags, "beta"),
        "expected elaboration for missing `beta`; got {diags:?}"
    );
}

/// A plain (non-mapped) intersection alias is normalised to an object type by
/// the solver before it reaches the error reporter, so multiple missing
/// properties produce TS2739 (the standard missing-properties message) rather
/// than the intersection-member elaboration.  This verifies we don't regress
/// the TS2739 path while the mapped-intersection elaboration is active.
#[test]
fn plain_intersection_alias_multiple_missing_gives_ts2739() {
    let diags = diagnostics(
        r#"
type Target = { a: string; b: number } & { c: boolean };
const v: Target = {};
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2739),
        "plain intersection with multiple missing properties must emit TS2739; got {diags:?}"
    );
}

/// Negative: a complete source must not trigger any missing-member elaboration,
/// even for multi-property intersection targets.
#[test]
fn multiple_missing_complete_source_no_error() {
    let diags = diagnostics(
        r#"
type Map1<T> = { [K in keyof T]: T[K] };
type Target = Map1<{ a: string; b: number }> & { c: boolean };
const v: Target = { a: "s", b: 1, c: true };
"#,
    );
    assert!(
        !diags.iter().any(|d| d.code == 2322 || d.code == 2741),
        "a complete source must not produce any assignability error; got {diags:?}"
    );
}
