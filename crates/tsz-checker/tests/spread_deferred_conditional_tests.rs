//! Regression for #14946: spreading a deferred conditional type as a call/rest
//! argument (`f(...v)` / `arr.push(...v)` where `v: T extends U ? X : Y`) must
//! reduce the conditional to its branch-union base constraint (`X | Y`, tsc's
//! `getBaseConstraintOfType` of a conditional) before extracting the iteration
//! element type. Otherwise the bare deferred conditional is related to the
//! parameter, producing a false `TS2345`.
//!
//! Scope note: this handles the branch-union-compatible family (the remeda
//! witness). A conditional whose check is decidable from the check-type's
//! constraint (e.g. `Q extends number ? Q[] : string[]` with `Q extends
//! number`, where tsc reduces to the true branch `Q[]`) is a separate, deeper
//! conditional-reduction gap (tsz does not reduce constraint-satisfiable
//! deferred conditionals — see `evaluate_rules/conditional.rs`) and is not
//! addressed here.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs_code_messages, load_default_lib_files};
use tsz_common::common::ScriptTarget;

fn codes(source: &str) -> Vec<(u32, String)> {
    let libs = load_default_lib_files();
    assert!(!libs.is_empty(), "default lib files must be available");
    check_source_with_libs_code_messages(
        source,
        "test.ts",
        CheckerOptions {
            target: ScriptTarget::ES2022,
            strict: true,
            ..CheckerOptions::default()
        },
        &libs,
    )
}

#[test]
fn spread_deferred_conditional_compatible_branches_no_ts2345() {
    let diags = codes(
        r#"
export {};
type Cond<P> = P extends string ? [P] : (string | number)[];
function g<P extends string>(p: P) {
    const result: (string | number)[] = [];
    const v: Cond<P> = null as any;
    result.push(...v);
}
"#,
    );
    assert!(
        !diags.iter().any(|(code, _)| *code == 2345),
        "deferred-conditional spread with element-compatible branches must not error TS2345. \
         Got: {diags:#?}"
    );
}

#[test]
fn spread_deferred_conditional_renamed_binders_no_ts2345() {
    // Same shape, different binder names — the fix must be structural.
    let diags = codes(
        r#"
export {};
type Pick<Q> = Q extends string ? Q[] : string[];
function h<Q extends string>(q: Q) {
    const out: string[] = [];
    const z: Pick<Q> = null as any;
    out.push(...z);
}
"#,
    );
    assert!(
        !diags.iter().any(|(code, _)| *code == 2345),
        "renamed-binder deferred-conditional spread must not error TS2345. Got: {diags:#?}"
    );
}

#[test]
fn spread_deferred_conditional_incompatible_branch_still_errors() {
    // Guard: both branches yield `boolean` elements, not assignable to
    // `(string | number)[]`, so the spread must still report TS2345.
    let diags = codes(
        r#"
export {};
type Cond<P> = P extends string ? boolean[] : boolean[];
function g<P extends string>(p: P) {
    const result: (string | number)[] = [];
    const v: Cond<P> = null as any;
    result.push(...v);
}
"#,
    );
    assert!(
        diags.iter().any(|(code, _)| *code == 2345),
        "deferred-conditional spread with an incompatible element type must still error TS2345. \
         Got: {diags:#?}"
    );
}

#[test]
fn spread_non_conditional_generic_array_control_no_ts2345() {
    // Control: a non-conditional generic array alias spread is unaffected
    // (the conditional reduction is a no-op) and stays clean.
    let diags = codes(
        r#"
export {};
type Arr<T> = T[];
function g<T extends number>(t: T) {
    const out: number[] = [];
    const z: Arr<T> = null as any;
    out.push(...z);
}
"#,
    );
    assert!(
        !diags.iter().any(|(code, _)| *code == 2345),
        "non-conditional generic array spread must stay clean. Got: {diags:#?}"
    );
}
