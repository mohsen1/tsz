//! Inferred return type aggregation for error-typed and recursive self-call
//! returns.
//!
//! tsc's `checkAndAggregateReturnExpressionTypes` keeps a genuine error-typed
//! return (an unresolved name's `errorType` carries the `Any` flag) and lets
//! `getUnionType` collapse the whole union to `error`/`any`; the ONLY return
//! form it omits is a *direct* recursive self-call (`return self(...)`), whose
//! circular provisional type must not poison the aggregate. tsz previously
//! blanket-dropped every error-typed contribution, which turned
//! `function g() { if (c) return globalThis; return global; }` (unresolved
//! `global`) into `typeof globalThis | undefined` instead of `any`, spuriously
//! flagging downstream property accesses (TS2339). See the class-transformer
//! `getGlobal()` false-positive cluster.

use crate::context::CheckerOptions;
use crate::test_utils::check_source;

fn check_strict(source: &str) -> Vec<u32> {
    let options = CheckerOptions {
        strict_null_checks: true,
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    check_source(source, "test.ts", options)
        .iter()
        .map(|d| d.code)
        .collect()
}

/// A function whose body mixes a concrete return with a genuine error-typed
/// return (here `obj.nonExistentProp`, whose result is the `error` type) infers
/// `any`, matching tsc's contagious `errorType`. The real error (TS2339 on the
/// bad property access) is still reported, but assigning the *result* to an
/// incompatible annotation does NOT report TS2322 — proving the inferred return
/// is `any`, not `{ z: 1 } | undefined`. Before this fix, the error-typed
/// contribution was blanket-dropped and the assignment reported a spurious
/// TS2322.
#[test]
fn error_typed_return_collapses_inferred_return_to_any() {
    let source = r#"
declare const cond: boolean;
declare const obj: { a: number };
function pick() {
  if (cond) return { z: 1 };
  return obj.nonExistentProp;
}
const s: string = pick();
"#;
    let codes = check_strict(source);
    assert!(
        codes.contains(&2339),
        "the genuine bad property access must still report TS2339; got: {codes:?}"
    );
    assert!(
        !codes.contains(&2322),
        "the error-typed return must collapse the inferred return to any, so \
         `string = pick()` reports NO TS2322; got: {codes:?}"
    );
}

/// Renamed-binder variant: the rule is structural, not tied to any identifier.
#[test]
fn error_typed_return_collapses_inferred_return_to_any_renamed() {
    let source = r#"
declare const flag: boolean;
declare const source: { count: number };
function resolveEnv() {
  if (flag) return { kind: "node" };
  return source.missingMember;
}
const env: number = resolveEnv();
"#;
    let codes = check_strict(source);
    assert!(
        codes.contains(&2339),
        "the genuine bad property access must still report TS2339; got: {codes:?}"
    );
    assert!(
        !codes.contains(&2322),
        "the error-typed return must collapse the inferred return to any; got: {codes:?}"
    );
}

/// A *direct* recursive self-call (`return countdown()`) is omitted from the
/// aggregate — matching tsc — so a `const` arrow still infers its base-case
/// type (`number`). Assigning it to `string` therefore reports TS2322; if the
/// self-call poisoned the union, the result would be `any` and no error would
/// fire.
#[test]
fn direct_self_call_const_arrow_keeps_base_case_type() {
    let source = r#"
declare const cond: boolean;
const countdown = () => {
  if (cond) return countdown();
  return 0;
};
const label: string = countdown();
"#;
    let codes = check_strict(source);
    assert!(
        codes.contains(&2322),
        "recursive const arrow must infer its base-case type (number), so \
         `string = countdown()` reports TS2322; got: {codes:?}"
    );
}

/// Same for a named function declaration self-call: the base case wins.
#[test]
fn direct_self_call_function_declaration_keeps_base_case_type() {
    let source = r#"
declare const cond: boolean;
function walk() {
  if (cond) return walk();
  return "leaf";
}
const size: number = walk();
"#;
    let codes = check_strict(source);
    assert!(
        codes.contains(&2322),
        "recursive function declaration must infer its base-case type (string), \
         so `number = walk()` reports TS2322; got: {codes:?}"
    );
}

/// A *wrapped* self-call (`return [bounce][0]()`) is NOT a direct self-call, so
/// — like tsc — its circular type is aggregated and the function degrades to the
/// implicit-`any` circular return (TS7023), rather than adopting the base case.
/// The assignment to `string` therefore does NOT report TS2322.
#[test]
fn wrapped_self_call_is_not_skipped() {
    let source = r#"
declare const cond: boolean;
function bounce() {
  if (cond) return [bounce][0]();
  return 0;
}
const t: string = bounce();
"#;
    let codes = check_strict(source);
    assert!(
        codes.contains(&7023),
        "a wrapped self-call must still report the circular implicit-any return \
         (TS7023); got: {codes:?}"
    );
    assert!(
        !codes.contains(&2322),
        "a wrapped self-call degrades to any, so no TS2322 fires on the \
         assignment; got: {codes:?}"
    );
}
