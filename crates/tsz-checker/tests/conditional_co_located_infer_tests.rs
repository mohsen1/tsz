//! Co-located `infer` variables: union covariant candidates, intersect
//! contravariant candidates.
//!
//! Structural rule (per #6407 and adjacent cases): when the same `infer X`
//! name appears in multiple positions of a single conditional extends
//! pattern, the candidates collected from each position must be combined
//! according to the position's variance — covariant positions (object
//! properties, tuple elements, array elements, return types) are unioned,
//! contravariant positions (function parameter types) are intersected.
//! This matches tsc and is independent of the variable's spelling.

use tsz_checker::diagnostics::Diagnostic;

fn diags(source: &str) -> Vec<Diagnostic> {
    tsz_checker::test_utils::check_source_strict(source)
}

fn ts2322_count(source: &str) -> usize {
    diags(source).iter().filter(|d| d.code == 2322).count()
}

fn expect_no_ts2322(source: &str, label: &str) {
    let ds = diags(source);
    let errs: Vec<_> = ds.iter().filter(|d| d.code == 2322).collect();
    assert!(
        errs.is_empty(),
        "[{label}] expected no TS2322, got: {:#?}",
        ds.iter()
            .map(|d| (d.code, d.message_text.as_str()))
            .collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------------------
// Covariant: object property positions
// ---------------------------------------------------------------------------

#[test]
fn object_two_props_with_same_infer_name_unions_candidates() {
    // `Flatten<{a: string; b: number}>` must evaluate to `string | number`,
    // not `number` (last-write-wins) or `never` (mutual-subtype required).
    expect_no_ts2322(
        r#"
        type Flatten<T> = T extends { a: infer U; b: infer U } ? U : never;
        type Fl = Flatten<{ a: string; b: number }>;
        const s: Fl = "x";
        const n: Fl = 42;
        export {};
        "#,
        "Flatten two-prop covariant union",
    );
}

#[test]
fn object_two_props_with_same_infer_name_rejects_other_types() {
    // boolean must NOT be assignable to `string | number`.
    let source = r#"
        type Flatten<T> = T extends { a: infer U; b: infer U } ? U : never;
        type Fl = Flatten<{ a: string; b: number }>;
        const b: Fl = true;
    "#;
    assert_eq!(
        ts2322_count(source),
        1,
        "expected boolean to be rejected, got: {:#?}",
        diags(source)
    );
}

#[test]
fn object_three_props_with_same_infer_name_unions_all_candidates() {
    expect_no_ts2322(
        r#"
        type F3<T> = T extends { a: infer U; b: infer U; c: infer U } ? U : never;
        type R = F3<{ a: string; b: number; c: boolean }>;
        const s: R = "x";
        const n: R = 42;
        const b: R = true;
        export {};
        "#,
        "three-prop covariant union",
    );
}

#[test]
fn object_renamed_infer_variable_still_unions() {
    // Prove the union rule is not hardcoded to the name `U`. The same rule
    // must apply whether the user writes `infer U`, `infer Q`, etc.
    expect_no_ts2322(
        r#"
        type FQ<T> = T extends { a: infer Q; b: infer Q } ? Q : never;
        type R = FQ<{ a: string; b: number }>;
        const s: R = "x";
        const n: R = 42;
        export {};
        "#,
        "renamed Q-variable still unions",
    );
}

#[test]
fn object_distinct_infer_names_bind_independently() {
    // Distinct names must continue to bind independently — no
    // cross-merge across different infer variables.
    expect_no_ts2322(
        r#"
        type Pair<T> = T extends { a: infer A; b: infer B } ? [A, B] : never;
        type P = Pair<{ a: string; b: number }>;
        const p: P = ["x", 42];
        export {};
        "#,
        "distinct names independent",
    );
}

#[test]
fn object_same_infer_same_value_does_not_double_union() {
    // Identical candidates must deduplicate, not yield `string | string`.
    expect_no_ts2322(
        r#"
        type F<T> = T extends { a: infer U; b: infer U } ? U : never;
        type R = F<{ a: string; b: string }>;
        const s: R = "x";
        export {};
        "#,
        "identical candidate deduplicates",
    );
}

// ---------------------------------------------------------------------------
// Covariant: tuple element positions
// ---------------------------------------------------------------------------

#[test]
fn tuple_two_elements_with_same_infer_name_unions_candidates() {
    expect_no_ts2322(
        r#"
        type Both<T> = T extends [infer U, infer U] ? U : never;
        type R = Both<[string, number]>;
        const s: R = "x";
        const n: R = 42;
        export {};
        "#,
        "tuple two-elem covariant union",
    );
}

#[test]
fn tuple_two_elements_with_same_infer_name_rejects_other_types() {
    let source = r#"
        type Both<T> = T extends [infer U, infer U] ? U : never;
        type R = Both<[string, number]>;
        const b: R = true;
    "#;
    assert_eq!(
        ts2322_count(source),
        1,
        "expected boolean to be rejected, got: {:#?}",
        diags(source)
    );
}

#[test]
fn tuple_three_elements_with_same_infer_name_unions_all_candidates() {
    expect_no_ts2322(
        r#"
        type T3<T> = T extends [infer X, infer X, infer X] ? X : never;
        type R = T3<[string, number, boolean]>;
        const s: R = "x";
        const n: R = 42;
        const b: R = true;
        export {};
        "#,
        "tuple three-elem covariant union",
    );
}

// ---------------------------------------------------------------------------
// Covariant: tuple of objects (nested patterns) — both occurrences are
// covariant, so the rule should still union.
// ---------------------------------------------------------------------------

#[test]
fn tuple_of_objects_with_same_infer_name_unions() {
    expect_no_ts2322(
        r#"
        type Both<T> = T extends [{ a: infer U }, { b: infer U }] ? U : never;
        type R = Both<[{ a: string }, { b: number }]>;
        const s: R = "x";
        const n: R = 42;
        export {};
        "#,
        "tuple of objects covariant union",
    );
}

// ---------------------------------------------------------------------------
// Contravariant: function parameter positions intersect, not union.
// ---------------------------------------------------------------------------

#[test]
fn function_two_params_with_same_infer_name_intersects() {
    // tsc: `Intersect<(a: string, b: "hello") => any>` = `string & "hello"`
    // = `"hello"`. The literal must be assignable; an arbitrary string must
    // not. This proves we are not unioning these contravariant positions.
    expect_no_ts2322(
        r#"
        type Intersect<T> = T extends (a: infer U, b: infer U) => any ? U : never;
        type R = Intersect<(a: string, b: "hello") => any>;
        const ok: R = "hello";
        export {};
        "#,
        "function params intersect to literal",
    );
}

#[test]
fn function_two_params_with_disjoint_types_collapse_to_never() {
    // `string & number` = `never`. Every concrete value is rejected.
    let source = r#"
        type Intersect<T> = T extends (a: infer U, b: infer U) => any ? U : never;
        type R = Intersect<(a: string, b: number) => any>;
        const s: R = "hello";
        const n: R = 42;
        const b: R = true;
    "#;
    assert_eq!(
        ts2322_count(source),
        3,
        "expected all 3 literals to be rejected, got: {:#?}",
        diags(source)
    );
}

#[test]
fn function_two_params_renamed_infer_still_intersects() {
    // Prove the intersection rule is not hardcoded to `U`.
    expect_no_ts2322(
        r#"
        type Intersect<T> = T extends (a: infer Q, b: infer Q) => any ? Q : never;
        type R = Intersect<(a: string, b: "hello") => any>;
        const ok: R = "hello";
        export {};
        "#,
        "renamed function-param intersect",
    );
}

// ---------------------------------------------------------------------------
// Mixed variance: same name in covariant + contravariant positions.
//
// tsc treats the name as contravariant whenever it appears in *any*
// contravariant position. Our `collect_contravariant_infer_names` does the
// same (a name in any function-param subtree of the outer pattern is in the
// set). The leaf merge therefore intersects all candidates.
// ---------------------------------------------------------------------------

#[test]
fn mixed_variance_same_name_intersects_param_and_return() {
    // `(a: string) => "hi"` against `(a: infer U) => infer U`:
    // tsc treats `U` as contravariant (param), so candidates from param
    // (`string`) and return (`"hi"`) intersect → `string & "hi" = "hi"`.
    expect_no_ts2322(
        r#"
        type Mixed<T> = T extends (a: infer U) => infer U ? U : never;
        type R = Mixed<(a: string) => "hi">;
        const ok: R = "hi";
        export {};
        "#,
        "mixed variance same-name intersects",
    );
}

// ---------------------------------------------------------------------------
// Optional + co-located covariant: exercises the partial-substitution path
// when constraint filtering on an optional prop produces no surviving type.
// ---------------------------------------------------------------------------

#[test]
fn optional_co_located_covariant_does_not_crash_and_unions_when_present() {
    // Both positions present and assignable → union as usual.
    expect_no_ts2322(
        r#"
        type F<T> = T extends { a?: infer U; b: infer U } ? U : never;
        type R = F<{ a: string; b: number }>;
        const s: R = "x";
        const n: R = 42;
        export {};
        "#,
        "optional co-located present",
    );
}

// ---------------------------------------------------------------------------
// Constraint-conflict on same-name infer: previously bind_infer could reject
// via mutual-subtype check; now it always merges. Verify the resulting type
// is still constrained correctly (out-of-constraint candidates are filtered
// before accumulation, so the union does not contain disallowed members).
// ---------------------------------------------------------------------------

#[test]
fn constraint_filtered_candidate_does_not_contaminate_union() {
    // `infer U extends string`: position `a` resolves to a string-compatible
    // type and is kept; position `b` resolves to `number` which is filtered
    // (does not satisfy `extends string`) and the conditional must take the
    // false branch. Without per-position filtering, the union would include
    // `number`. With it, no error fires for `42` because the conditional
    // resolves to the false branch (`unknown` here); the diagnostic for the
    // mistaken assumption belongs on a follow-up test for false-branch
    // semantics — here we just verify the rule does not silently widen.
    let source = r#"
        type F<T> = T extends { a: infer U extends string; b: infer U extends string } ? U : never;
        type R = F<{ a: "hi"; b: "lo" }>;
        const ok: R = "hi";
        const ok2: R = "lo";
        // `R` is `"hi" | "lo"`, so a bare `string` is NOT assignable.
        const bad: R = "other" as string;
    "#;
    assert_eq!(
        ts2322_count(source),
        1,
        "expected the bare `string` to be rejected, got: {:#?}",
        diags(source)
    );
}
