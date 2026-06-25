//! Regression tests: a function body that is a bare expression-statement calling
//! a never-returning function does not fall through, so its inferred return type
//! is `never` (like `throw` or `return die()`), not `void`.
//!
//! Witness: immer `proxy.ts` `defineProperty() { die(11) }`. tsz previously
//! inferred `void` for such a body and emitted a spurious TS2322 when it was
//! assigned to a non-`void` function type such as `() => boolean`.
//!
//! Structural rule: when a function body's only effect is a bare
//! expression-statement whose expression is a never-returning call, control flow
//! does not reach the end of the body, so the inferred return type is `never`.

use crate::test_utils::check_source_strict_codes as check_strict;

const TS2322: u32 = 2322;
/// TS2355: "A function whose declared type is neither 'undefined', 'void', nor
/// 'any' must return a value." tsc (and tsz, see `never_initializer_falls_through_tests`)
/// emit this for an annotated function with no value-returning path whose body
/// can fall through.
const TS2355: u32 = 2355;
/// TS2366: "Function lacks ending return statement and return type does not
/// include 'undefined'."
const TS2366: u32 = 2366;

/// The bug witness: a bare never-call expression-statement body assigned to a
/// `() => boolean` must infer `never` (assignable to `boolean`), not `void`.
#[test]
fn bare_never_call_body_infers_never_no_ts2322() {
    let source = r#"
declare function die(n: number): never;
const b: () => boolean = () => { die(1); };
"#;
    let codes = check_strict(source);
    assert!(
        !codes.contains(&TS2322),
        "bare never-call body should infer `never`, not `void`; got {codes:?}"
    );
}

/// `throw` form was already correct; keep it green as a sibling control.
#[test]
fn throw_body_infers_never_no_ts2322() {
    let source = r#"
const a: () => boolean = () => { throw new Error(); };
"#;
    let codes = check_strict(source);
    assert!(
        !codes.contains(&TS2322),
        "throw body should infer `never`; got {codes:?}"
    );
}

/// `return die()` form was already correct; keep it green as a sibling control.
#[test]
fn return_never_call_body_infers_never_no_ts2322() {
    let source = r#"
declare function die(n: number): never;
const c: () => boolean = () => { return die(1); };
"#;
    let codes = check_strict(source);
    assert!(
        !codes.contains(&TS2322),
        "`return die()` body should infer `never`; got {codes:?}"
    );
}

/// The fix must not depend on the binder name of the never function or the
/// const it is assigned to.
#[test]
fn bare_never_call_body_renamed_binders_no_ts2322() {
    let source = r#"
declare function abort(reason: string): never;
const handler: () => number = () => { abort("stop"); };
"#;
    let codes = check_strict(source);
    assert!(
        !codes.contains(&TS2322),
        "structural rule must not depend on binder names; got {codes:?}"
    );
}

/// NEGATIVE CONTROL: a body that genuinely falls through (a non-never call) must
/// still infer `void`, which is NOT assignable to `boolean` → TS2322 must fire,
/// exactly like tsc.
#[test]
fn fall_through_body_still_infers_void_emits_ts2322() {
    let source = r#"
declare function log(n: number): void;
const f: () => boolean = () => { log(1); };
"#;
    let codes = check_strict(source);
    assert!(
        codes.contains(&TS2322),
        "a falling-through body must still infer `void` (TS2322 vs `boolean`); got {codes:?}"
    );
}

/// NEGATIVE CONTROL: an empty body falls through to `void`; assigning to a
/// non-`void` function type must still emit TS2322.
#[test]
fn empty_body_still_infers_void_emits_ts2322() {
    let source = r#"
const f: () => boolean = () => {};
"#;
    let codes = check_strict(source);
    assert!(
        codes.contains(&TS2322),
        "empty body must still infer `void` (TS2322 vs `boolean`); got {codes:?}"
    );
}

/// NEGATIVE CONTROL: the return-completeness check must STILL fire where tsc
/// fires it — an annotated `: number` body that only terminates on the `flag`
/// branch and falls through on the implicit `else`. tsz (matching tsc and the
/// established `never_initializer_falls_through_tests` parity) reports this as
/// TS2355.
#[test]
fn mixed_branch_annotated_body_still_emits_return_completeness() {
    let source = r#"
declare function die(n: number): never;
function f(flag: boolean): number {
  if (flag) {
    die(1);
  }
}
"#;
    let codes = check_strict(source);
    assert!(
        codes.contains(&TS2355) || codes.contains(&TS2366),
        "a body that falls through on the no-`die` branch must still emit a \
         return-completeness diagnostic (TS2355/TS2366); got {codes:?}"
    );
}

/// NEGATIVE CONTROL: the return-completeness check must NOT fire when every
/// branch terminates — one branch returns a value, the other is a bare
/// never-call.
#[test]
fn mixed_branch_value_return_and_never_call_no_return_completeness() {
    let source = r#"
declare function die(n: number): never;
function f(flag: boolean): number {
  if (flag) {
    return 1;
  }
  die(2);
}
"#;
    let codes = check_strict(source);
    assert!(
        !codes.contains(&TS2355) && !codes.contains(&TS2366),
        "every-branch-terminating body (return + never-call) must not emit a \
         return-completeness diagnostic; got {codes:?}"
    );
}

/// An annotated `: number` function whose whole body is a bare never-call must
/// not be reported as lacking a return (parity with `throw`); the established
/// `never_initializer_falls_through_tests::bare_never_call_statement_suppresses_ts2355`
/// asserts the same for the TS2355 family.
#[test]
fn annotated_bare_never_call_body_no_return_completeness() {
    let source = r#"
declare function die(n: number): never;
function f(): number {
  die(1);
}
"#;
    let codes = check_strict(source);
    assert!(
        !codes.contains(&TS2355) && !codes.contains(&TS2366),
        "a bare never-call body terminates control flow; no return-completeness \
         diagnostic must fire; got {codes:?}"
    );
}
