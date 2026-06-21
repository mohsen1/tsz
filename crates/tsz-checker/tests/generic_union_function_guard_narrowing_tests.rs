//! Regression tests for narrowing a generic union by a `value is Function`
//! (or built-in `Function`) type guard.
//!
//! tsc's `getNarrowedTypeWorker` keeps only the constituents structurally
//! related to the predicate target; the per-member `T & target` intersection
//! synthesis is a *fallback* reached only when no constituent is related. tsz
//! used to synthesize `V & Function` eagerly for a bare type-parameter member
//! and retain it next to the real function member, so the call site saw a
//! non-callable `V & Function` and reported a spurious TS2349. When the fallback
//! does legitimately apply (no function member in the union), the resulting
//! `V & Function` must itself be callable as an untyped call (tsc's
//! `isUntypedFunctionCall`).

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs, load_default_lib_files};
use tsz_common::diagnostics::Diagnostic;

fn check(source: &str) -> Vec<Diagnostic> {
    let libs = load_default_lib_files();
    check_source_with_libs(source, "test.ts", CheckerOptions::default(), &libs)
}

/// The reported repro (issue #14320, mined from radash): a generic union with
/// a bare type parameter `V` plus a function member, narrowed by a user
/// predicate `value is Function`. The function member is the only constituent
/// related to `Function`, so `V` is dropped and the call type-checks.
#[test]
fn user_function_guard_drops_bare_type_param_member() {
    let source = r#"
declare const isFunction: (value: any) => value is Function;

function pick<V>(values: V | ((idx: number) => V)): V {
  if (isFunction(values)) {
    return values(0);
  }
  return values;
}
"#;
    let diags = check(source);
    let not_callable: Vec<_> = diags.iter().filter(|d| d.code == 2349).collect();
    assert!(
        not_callable.is_empty(),
        "function guard must keep only the callable member of `V | (fn)`; got: {diags:#?}"
    );
}

/// Renamed binder + constrained type parameter must behave identically: the
/// fix is structural, not keyed on the parameter name `V` or an unconstrained
/// constraint.
#[test]
fn user_function_guard_renamed_and_constrained_type_param() {
    let source = r#"
declare const isFunction: (value: any) => value is Function;

function renamed<Elem>(vals: Elem | ((i: number) => Elem)): Elem {
  if (isFunction(vals)) {
    return vals(0);
  }
  return vals;
}

function constrained<W extends object>(values: W | ((idx: number) => W)): W {
  if (isFunction(values)) {
    return values(0);
  }
  return values;
}
"#;
    let diags = check(source);
    let not_callable: Vec<_> = diags.iter().filter(|d| d.code == 2349).collect();
    assert!(
        not_callable.is_empty(),
        "renamed/constrained type-param unions must narrow identically; got: {diags:#?}"
    );
}

/// When the union has no function member, the fallback legitimately produces
/// `V & Function`. tsc treats a callee assignable to the global `Function`
/// interface with no own signatures as an untyped call returning `any`, so the
/// call must type-check. (Bare `Function` and the intersection must both work.)
#[test]
fn function_guard_fallback_intersection_is_callable() {
    let source = r#"
declare const f: Function;
const a = f(1, 2);

declare const isFunction: (value: any) => value is Function;
function h<V>(values: V | string): unknown {
  if (isFunction(values)) {
    return values(0);
  }
  return "";
}
"#;
    let diags = check(source);
    let not_callable: Vec<_> = diags.iter().filter(|d| d.code == 2349).collect();
    assert!(
        not_callable.is_empty(),
        "`V & Function` and bare `Function` must be callable (untyped call); got: {diags:#?}"
    );
}

/// Guard against over-permitting: an intersection callee with no `Function`
/// constituent (and no call signatures) is still not callable.
#[test]
fn non_function_intersection_callee_still_not_callable() {
    let source = r#"
declare const n: number & { tag: 1 };
n();
"#;
    let diags = check(source);
    let not_callable: Vec<_> = diags.iter().filter(|d| d.code == 2349).collect();
    assert_eq!(
        not_callable.len(),
        1,
        "a non-Function intersection must remain non-callable; got: {diags:#?}"
    );
}
