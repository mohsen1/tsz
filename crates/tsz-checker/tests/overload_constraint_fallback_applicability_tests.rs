//! Regression tests for overload candidate applicability when a (const or
//! plain) type parameter falls back to its constraint.
//!
//! Structural rule: when an overload's generic type parameter supplies no valid
//! lower bound from the argument and therefore falls back to its *constraint*,
//! the candidate must be re-validated — if the original argument is not
//! assignable to the constraint-instantiated parameter, the signature is
//! non-applicable and a later overload may win. Previously a bare `const` type
//! parameter unconditionally suppressed the final argument-type mismatch
//! (assuming "const params are inferred directly from the argument, so always
//! assignable"), which is false on the constraint-fallback path: the argument
//! was never inferred from, it was discarded in favor of the constraint.
//!
//! See issue #14510.

use tsz_checker::test_utils::{check_source_strict, check_source_strict_codes};

fn ts_codes(source: &str) -> Vec<u32> {
    let mut codes = check_source_strict_codes(source);
    codes.sort_unstable();
    codes
}

fn clean(source: &str) -> bool {
    check_source_strict(source).is_empty()
}

/// Two `const` overloads distinguished only by their type-parameter constraint;
/// the argument matches the *second*. The first overload's `T extends string`
/// gets no lower bound from `5`, falls back to `string`, and must be rejected so
/// overload 2 (`T extends number`, `T = 5`) wins → `r = { n: 5 }`.
#[test]
fn const_overload_constraint_fallback_picks_matching_signature() {
    let src = "\
        declare function f<const T extends string>(x: T): [T];\n\
        declare function f<const T extends number>(x: T): { n: T };\n\
        const r = f(5);\n\
        const ok: { n: 5 } = r;\n";
    assert!(
        clean(src),
        "argument 5 must select the `number`-constrained overload (r = {{ n: 5 }}), not the \
         `string`-constrained one falling back to its constraint; got {:?}",
        check_source_strict(src)
    );
}

/// Same shape, renamed function and type-parameter binders — the fix is
/// structural, not keyed on the spelling `f`/`T`.
#[test]
fn const_overload_constraint_fallback_renamed_binders() {
    let src = "\
        declare function widget<const Elem extends string>(input: Elem): [Elem];\n\
        declare function widget<const Elem extends number>(input: Elem): { n: Elem };\n\
        const z = widget(7);\n\
        const ok: { n: 7 } = z;\n";
    assert!(
        clean(src),
        "renamed binders must resolve identically; got {:?}",
        check_source_strict(src)
    );
}

/// The argument matches overload **1**; it must still be selected (the
/// constraint-fallback rejection only fires for the genuinely non-matching
/// candidate, never for the matching one).
#[test]
fn const_overload_matching_first_signature_still_selected() {
    let src = "\
        declare function f<const T extends string>(x: T): [T];\n\
        declare function f<const T extends number>(x: T): { n: T };\n\
        const r = f(\"a\");\n\
        const ok: [\"a\"] = r;\n";
    assert!(
        clean(src),
        "argument \"a\" must select the `string`-constrained overload (r = [\"a\"]); got {:?}",
        check_source_strict(src)
    );
}

/// Three-way `const` overload set; the argument matches the *third*.
#[test]
fn const_three_way_overload_set_picks_third() {
    let src = "\
        declare function f<const T extends string>(x: T): [T];\n\
        declare function f<const T extends number>(x: T): { n: T };\n\
        declare function f<const T extends boolean>(x: T): { b: T };\n\
        const r = f(true);\n\
        const ok: { b: true } = r;\n";
    assert!(
        clean(src),
        "argument true must select the `boolean`-constrained overload (r = {{ b: true }}); got {:?}",
        check_source_strict(src)
    );
}

/// The argument matches *no* overload's constraint — the genuine no-overload
/// error (TS2769) must still surface; the constraint-fallback rejection must not
/// silently swallow it.
#[test]
fn const_overload_no_matching_constraint_reports_ts2769() {
    let src = "\
        declare function f<const T extends string>(x: T): [T];\n\
        declare function f<const T extends number>(x: T): { n: T };\n\
        const r = f(true);\n";
    assert_eq!(
        ts_codes(src),
        vec![2769],
        "an argument assignable to no overload constraint must report TS2769"
    );
}

/// Plain (non-`const`) constrained overloads exhibit the same selection and were
/// already correct — guard against regressions from the shared finalize path.
#[test]
fn plain_overload_constraint_fallback_picks_matching_signature() {
    let src = "\
        declare function f<T extends string>(x: T): [T];\n\
        declare function f<T extends number>(x: T): { n: T };\n\
        const r = f(5);\n\
        const ok: { n: 5 } = r;\n";
    assert!(
        clean(src),
        "plain constrained overloads must select the `number` overload; got {:?}",
        check_source_strict(src)
    );
}

/// A single-signature `const` generic called with a non-matching argument must
/// STILL report TS2345 — the constraint fallback is legitimate outside overload
/// resolution and must keep erroring on the argument.
#[test]
fn single_signature_const_constraint_fallback_still_errors() {
    let src = "\
        declare function g<const T extends string>(x: T): [T];\n\
        const s = g(5);\n";
    assert_eq!(
        ts_codes(src),
        vec![2345],
        "a single-signature const generic must still report TS2345 on a non-assignable argument"
    );
}

/// The legitimate bare-`const` skip this fix narrows must be preserved: a const
/// type parameter genuinely inferred from the argument (no constraint fallback)
/// must not draw a spurious mismatch from the `in_const_assertion` TypeId
/// divergence.
#[test]
fn bare_const_genuine_inference_remains_clean() {
    let src = "\
        declare function h<const T extends readonly unknown[]>(x: T): T;\n\
        const r = h([1, 2, 3]);\n\
        const ok: readonly [1, 2, 3] = r;\n";
    assert!(
        clean(src),
        "a const param genuinely inferred from the argument must stay clean; got {:?}",
        check_source_strict(src)
    );
}
