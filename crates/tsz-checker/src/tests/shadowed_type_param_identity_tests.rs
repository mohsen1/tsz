//! Regression tests: two distinct type parameters that share a *name* must not
//! be treated as the same parameter when their constraints prove they are
//! distinct declarations.
//!
//! `TypeParamInfo` carries no declaration handle, so the subtype relation used a
//! shared name as a proxy for parameter identity. For an inner generic that
//! shadows an outer one with an *incompatible* constraint, that proxy is wrong:
//! the parameters are unrelated and `tsc` reports the assignment failure
//! (TS2719, "two different types with this name exist, but they are unrelated").
//! The fix suppresses the name-based reflexive shortcut when both constraints
//! are present and mutually non-assignable, so a real mismatch is reported.
//!
//! The names are varied across cases so the routing is exercised structurally,
//! not keyed on a particular identifier spelling.

use crate::context::CheckerOptions;
use crate::test_utils::check_source;

fn strict() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    }
}

fn codes(src: &str) -> Vec<u32> {
    check_source(src, "test.ts", strict())
        .iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn shadowed_param_incompatible_constraints_report_ts2719() {
    // Inner `T extends number` shadows outer `T extends string`; assigning the
    // inner-parameter value to the outer-parameter binding is unrelated.
    let c = codes(
        r#"
function outer<T extends string>(x: T) {
    function inner<T extends number>(y: T) {
        x = y;
    }
}
"#,
    );
    assert!(
        c.contains(&2719),
        "distinct same-named params with incompatible constraints must report TS2719, got {c:?}"
    );
}

#[test]
fn shadowed_param_incompatible_constraints_report_ts2719_renamed() {
    // Same structural scenario, different identifier spelling for the shared
    // name — proves the routing is structural, not a literal `T` check.
    let c = codes(
        r#"
function wrap<Elem extends boolean>(a: Elem) {
    function nested<Elem extends string>(b: Elem) {
        a = b;
    }
}
"#,
    );
    assert!(
        c.contains(&2719),
        "renamed distinct same-named params must still report TS2719, got {c:?}"
    );
}

#[test]
fn distinct_differently_named_params_still_report_ts2322() {
    // Guard: parameters that do not share a name keep routing through the
    // ordinary TS2322 path (display strings differ, so no TS2719).
    let c = codes(
        r#"
function host<A>(x: A) {
    function child<B>(y: B) {
        x = y;
    }
}
"#,
    );
    assert!(
        c.contains(&2322),
        "differently-named params must report TS2322, got {c:?}"
    );
    assert!(
        !c.contains(&2719),
        "differently-named params must not report TS2719, got {c:?}"
    );
}

#[test]
fn alpha_equivalent_generic_signatures_do_not_regress() {
    // Two generic function signatures that differ only in type-parameter name
    // are mutually assignable (alpha-equivalence). The reflexive shortcut must
    // remain for the unconstrained case so this keeps type-checking cleanly.
    let c = codes(
        r#"
declare let f: <T>(x: T) => T;
declare let g: <U>(x: U) => U;
f = g;
g = f;
"#,
    );
    assert!(
        !c.contains(&2322) && !c.contains(&2719),
        "alpha-equivalent generic signatures must not error, got {c:?}"
    );
}

#[test]
fn alpha_equivalent_constrained_signatures_do_not_regress() {
    // Same as above but with an identical constraint written on both sides.
    let c = codes(
        r#"
declare let f: <T extends object>(x: T) => T;
declare let g: <U extends object>(x: U) => U;
f = g;
g = f;
"#,
    );
    assert!(
        !c.contains(&2322) && !c.contains(&2719),
        "alpha-equivalent constrained signatures must not error, got {c:?}"
    );
}
