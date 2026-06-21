//! Regression tests for false-positive TS2556 when a non-tuple array spread
//! fails to select a rest-parameter overload declared after a fixed-arity one.
//!
//! Root cause (mined from es-toolkit, `flowRight.ts` vs the overloaded `flow`):
//! when an overload set declares a fixed-arity overload before a rest-parameter
//! overload and the call spreads a non-tuple array, `tsc` tries every overload
//! and resolves to the rest overload, so no TS2556 is reported. tsz's overload
//! loop committed TS2556 against the first (fixed-arity) overload via an
//! `ArgumentTypeMismatch` -> TS2556 early-return and never fell back to the rest
//! overload.
//!
//! Fix: a spread-into-non-rest mismatch is a soft overload failure (try the next
//! overload) whenever any overload in the set has a rest parameter; TS2556 is
//! committed only when no overload could absorb the spread into a rest
//! parameter. The rule keys on the structural overload shapes, not identifiers.
//!
//! Issue: <https://github.com/tsz-org/tsz/issues/14319>

use std::sync::{Arc, OnceLock};
use tsz_binder::lib_loader::LibFile;

use crate::CheckerOptions;
use crate::test_utils::{check_source_with_libs_code_messages, load_default_lib_files};

/// The default lib bundle, parsed once and shared across this module.
fn default_libs() -> &'static [Arc<LibFile>] {
    static DEFAULT_LIBS: OnceLock<Vec<Arc<LibFile>>> = OnceLock::new();
    DEFAULT_LIBS.get_or_init(load_default_lib_files)
}

const TS2556: u32 = 2556;

fn check(src: &str) -> Vec<(u32, String)> {
    check_source_with_libs_code_messages(src, "test.ts", CheckerOptions::default(), default_libs())
}

fn ts2556(diags: &[(u32, String)]) -> Vec<&(u32, String)> {
    diags.iter().filter(|(code, _)| *code == TS2556).collect()
}

fn assert_no_ts2556(src: &str, context: &str) {
    let diags = check(src);
    assert!(
        ts2556(&diags).is_empty(),
        "expected no TS2556 ({context}), got: {diags:?}"
    );
}

/// The exact witness from issue #14319: a fixed-arity overload declared before a
/// rest-parameter overload, called with a non-tuple array spread, must resolve
/// to the rest overload with no TS2556.
#[test]
fn fixed_then_rest_overload_spread_no_ts2556() {
    assert_no_ts2556(
        r#"
declare function flow(f: () => any): any;
declare function flow(...funcs: Array<() => any>): any;
function f(arr: Array<() => any>): any { return flow(...arr); }
"#,
        "rest overload after a fixed-arity one",
    );
}

/// Same rule with renamed binders and a differently named function: the rule is
/// structural (overload shapes), not keyed on `flow`/`funcs`.
#[test]
fn fixed_then_rest_overload_spread_renamed_no_ts2556() {
    assert_no_ts2556(
        r#"
declare function compose(first: () => number): number;
declare function compose(...steps: Array<() => number>): number;
function run(pipeline: Array<() => number>): number { return compose(...pipeline); }
"#,
        "renamed fixed-then-rest overload set",
    );
}

/// The rest overload first, fixed overload second: still clean (the rest
/// overload accepts the spread regardless of declaration order).
#[test]
fn rest_then_fixed_overload_spread_no_ts2556() {
    assert_no_ts2556(
        r#"
declare function flow(...funcs: Array<() => any>): any;
declare function flow(f: () => any): any;
function f(arr: Array<() => any>): any { return flow(...arr); }
"#,
        "rest overload declared first",
    );
}

/// A single rest-parameter overload (no competing fixed overload) was already
/// clean; guard that it stays clean.
#[test]
fn rest_only_overload_spread_no_ts2556() {
    assert_no_ts2556(
        r#"
declare function flow(...funcs: Array<() => any>): any;
function f(arr: Array<() => any>): any { return flow(...arr); }
"#,
        "rest-only overload",
    );
}

/// Negative control: when *every* overload is fixed-arity, a non-tuple array
/// spread still overflows the parameter list, so TS2556 must still fire. The fix
/// must not blanket-suppress the diagnostic.
#[test]
fn all_fixed_arity_overloads_spread_still_emits_ts2556() {
    let diags = check(
        r#"
declare function flow(f: () => any): any;
declare function flow(f: () => any, g: () => any): any;
function f(arr: Array<() => any>): any { return flow(...arr); }
"#,
    );

    assert!(
        !ts2556(&diags).is_empty(),
        "expected TS2556 when no overload has a rest parameter, got: {diags:?}"
    );
}
