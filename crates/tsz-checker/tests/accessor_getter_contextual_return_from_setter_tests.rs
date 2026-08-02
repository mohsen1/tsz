//! An unannotated `get` accessor's body is contextually typed by the paired
//! `set` accessor's *annotated* parameter type (issue #16152).
//!
//! `tsc`'s `getContextualReturnType` special-cases
//! `isGetAccessorWithAnnotatedSetAccessor`: when a getter has no return
//! annotation and its paired setter's parameter IS annotated, the setter's
//! annotation becomes the contextual type for the getter's `return`
//! expression during checking. This is distinct from — and was already
//! partly handled by — the setter-parameter direction fixed in #16151; this
//! is the mirror direction, and unlike #16151's gap it was missing for
//! *both* object literals and classes (no mechanism implemented it at all).
//!
//! This only affects the check-time contextual/assignability type of the
//! `return` statement. It must NOT change the accessor pair's own declared
//! property type, which stays the getter's inferred return type (tsc's
//! `getTypeOfAccessors` fallback order runs getter annotation, then setter
//! annotation, then getter's inferred type — a separate concern from
//! `getContextualReturnType`, and out of scope here).
//!
//! The witness returns an arrow function so the *parameter*'s type is
//! observable: `var p: string; var p = t;` reports `TS2403` (subsequent
//! declarations must have the same type) unless `t` is contextually typed
//! as `string` from the setter's annotation.

use tsz_checker::test_utils::check_source_non_strict_codes;

/// `CheckerOptions::default()` is a strict run, and these shapes are all
/// `@strict: false` corpus rows, so every case goes through the non-strict
/// entry point.
fn assert_clean(src: &str) {
    let codes = check_source_non_strict_codes(src);
    assert!(codes.is_empty(), "expected no diagnostics, got {codes:?}");
}

fn assert_codes(src: &str, expected: &[u32]) {
    let codes = check_source_non_strict_codes(src);
    assert_eq!(codes, expected, "unexpected diagnostics");
}

// ── object literal ───────────────────────────────────────────────────────

#[test]
fn object_literal_getter_return_contextually_typed_by_setter_param() {
    assert_clean(
        "var o = { set n(v: (t: string) => void) { }, \
         get n() { return (t) => { var p: string; var p = t; } } };",
    );
}

/// Declaration order must not matter: getter-first also pairs.
#[test]
fn object_literal_getter_first_still_pairs_with_setter() {
    assert_clean(
        "var o = { get n() { return (t) => { var p: string; var p = t; } }, \
         set n(v: (t: string) => void) { } };",
    );
}

// ── class ─────────────────────────────────────────────────────────────────

#[test]
fn class_getter_return_contextually_typed_by_setter_param() {
    assert_clean(
        "class K { set n(v: (t: string) => void) { } \
         get n() { return (t) => { var p: string; var p = t; } } }",
    );
}

#[test]
fn class_getter_first_still_pairs_with_setter() {
    assert_clean(
        "class K { get n() { return (t) => { var p: string; var p = t; } } \
         set n(v: (t: string) => void) { } }",
    );
}

// ── negative controls ────────────────────────────────────────────────────

/// No paired setter: the arrow parameter stays implicitly `any`, and the
/// witness reports its usual `TS2403`.
#[test]
fn object_literal_getter_without_paired_setter_stays_uncontextualized() {
    assert_codes(
        "var o = { get n() { return (t) => { var p: string; var p = t; } } };",
        &[2403],
    );
}

/// The paired setter's parameter has NO annotation itself (only the
/// #16151 direction applies to it — inferred from the getter). Joining the
/// getter's return context to an unannotated setter parameter here would
/// recurse the pair back through the getter it is trying to type, so it
/// must not participate: the arrow parameter stays implicitly `any`.
#[test]
fn unannotated_setter_parameter_does_not_participate() {
    assert_codes(
        "var o = { set n(v) { }, \
         get n() { return (t) => { var p: string; var p = t; } } };",
        &[2403],
    );
}

#[test]
fn class_unannotated_setter_parameter_does_not_participate() {
    assert_codes(
        "class K { set n(v) { } \
         get n() { return (t) => { var p: string; var p = t; } } }",
        &[2403],
    );
}

/// A getter with its OWN return annotation wins over the setter's
/// parameter type (tsc's declared-type priority order, step 1 before 2).
#[test]
fn getters_own_annotation_still_wins_over_paired_setter() {
    assert_codes(
        "var o = { set n(v: (t: string) => void) { }, \
         get n(): (t: number) => void { return (t) => { var p: string; var p = t; } } };",
        &[2403],
    );
}
