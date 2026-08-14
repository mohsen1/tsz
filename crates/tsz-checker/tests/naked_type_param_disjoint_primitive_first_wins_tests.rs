//! Regression tests for issue #17484.
//!
//! When a type parameter `T` receives multiple naked-argument inference
//! candidates that are disjoint primitives with no common supertype, tsc's
//! `getCommonSupertype` keeps the leftmost candidate (`reduceLeft`) and reports
//! every later, conflicting argument as `TS2345`. It never unions. tsz's Step 5
//! fallback (`get_common_supertype_for_inference`) previously gated its
//! first-wins behaviour on an array-literal-element provenance, so a plain
//! `f<T>(a: T, b: T)` called `f(1, "a")` silently unioned `T = number | string`
//! and reported nothing — a false negative.
//!
//! The fix scopes the union fallback to *structural* candidates (where the BCT
//! `is_subtype` is unreliable) and first-wins for all primitive-like candidate
//! sets (where `is_subtype` is exact), matching tsc regardless of whether a
//! candidate came from an array element or a naked argument. These cases are
//! all oracle-verified against `typescript@7.0.2` (and `@6.0.2`), non-strict.

use tsz_checker::test_utils::check_source_code_messages as compile_and_get_diagnostics;

fn ts2345_count(source: &str) -> usize {
    compile_and_get_diagnostics(source)
        .iter()
        .filter(|(code, _)| *code == 2345)
        .count()
}

#[test]
fn two_naked_disjoint_literals_report_ts2345() {
    // The reported minimal repro: T fixes to the leftmost `1`, `"a"` conflicts.
    // oracle: error TS2345: Argument of type '"a"' is not assignable to
    // parameter of type '1'.
    let source = r#"
declare function f<T>(a: T, b: T): T;
f(1, "a");
"#;
    assert_eq!(
        ts2345_count(source),
        1,
        "two disjoint naked primitive candidates must fix T to the leftmost and report the conflict"
    );
}

#[test]
fn two_naked_disjoint_literals_swapped_report_ts2345() {
    // Swapped order: T fixes to `"a"`, the `1` conflicts. Not order-symmetric
    // by accident — leftmost genuinely wins in both directions.
    let source = r#"
declare function f<T>(a: T, b: T): T;
f("a", 1);
"#;
    assert_eq!(
        ts2345_count(source),
        1,
        "leftmost-wins must hold in the swapped order too"
    );
}

#[test]
fn boolean_and_number_naked_candidates_report_ts2345() {
    // A different disjoint primitive pair (boolean literal vs number literal).
    let source = r#"
declare function f<T>(a: T, b: T): T;
f(true, 3);
"#;
    assert_eq!(
        ts2345_count(source),
        1,
        "disjoint boolean/number candidates must first-win, not union"
    );
}

#[test]
fn renamed_type_parameter_and_params_report_ts2345() {
    // The rule is structural, not tied to the identifier names `T`/`a`/`b`.
    let source = r#"
declare function combine<Elem>(x: Elem, y: Elem): Elem;
combine(true, 1);
"#;
    assert_eq!(
        ts2345_count(source),
        1,
        "the fix is structural and must not depend on the type-parameter/param names"
    );
}

#[test]
fn three_naked_disjoint_candidates_report_single_ts2345() {
    // T fixes to the leftmost `""`; the first conflicting argument (`0`) is
    // reported. tsc reports exactly one TS2345 here (verified against the
    // oracle), not one per remaining argument.
    let source = r#"
declare function h<T>(a: T, b: T, c: T): T;
h("", 0, false);
"#;
    assert_eq!(
        ts2345_count(source),
        1,
        "three disjoint candidates must fix T to the leftmost and report a single conflict, matching tsc"
    );
}

// ---------------------------------------------------------------------------
// Negative controls: cases that must KEEP unioning / stay clean.
// ---------------------------------------------------------------------------

#[test]
fn homogeneous_number_candidates_stay_clean() {
    // Same-base literal candidates widen to `number` and unify — no conflict.
    let source = r#"
declare function f<T>(a: T, b: T): T;
f(1, 2);
"#;
    assert_eq!(
        ts2345_count(source),
        0,
        "same-base primitive candidates must not report a conflict"
    );
}

#[test]
fn fresh_object_literal_candidates_still_union() {
    // Fresh object-literal candidates are a genuinely different (structural)
    // path that must keep unioning (`{x:number}|{y:string}`), matching tsc.
    // The primitive-first-wins fix must not touch this.
    let source = r#"
declare function f<T>(a: T, b: T): T;
f({ x: 1 }, { y: "s" });
"#;
    assert_eq!(
        ts2345_count(source),
        0,
        "fresh object-literal candidates must keep unioning, not first-win"
    );
}
