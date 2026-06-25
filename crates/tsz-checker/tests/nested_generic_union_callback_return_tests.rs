//! Return-context inference for a generic call whose signature return is checked
//! against a *union* contextual type must combine its union arms, not pin the
//! type parameter to whichever arm matched first.
//!
//! Structural rule: when a generic call's return type (`U[]`) is matched against
//! a contextual union whose arms bind the call's type parameter differently
//! (`string[]` binds `U := string`, `string[][]` binds `U := string[]`), the
//! arms are genuinely ambiguous. `tsc` does not let the return context pin `U`
//! from a single arm; argument inference (the callback body) decides it. Taking
//! only the first arm pinned `U := string`, contextually typed a nested
//! callback's return as `string`, and spuriously rejected its body — and for a
//! `U | U[]` callback target it leaked the outer type parameter into the result
//! (`U[]`). Owner: the return-context substitution in the generic-call solver
//! boundary (and its checker counterpart), which now skips a parameter the
//! union arms disagree on.
//!
//! Witness family: the `ofetch` canary `withQuery` (issue #14731), and the lib
//! `Array.prototype.flatMap` shape `U | ReadonlyArray<U>`. Type-parameter names
//! are varied across cases so no fix can key on a specific binder name.

use tsz_checker::context::{CheckerOptions, ScriptTarget};
use tsz_checker::test_utils::{check_multi_file_with_libs, load_default_lib_files};
use tsz_common::diagnostics::Diagnostic;

fn ts_options() -> CheckerOptions {
    CheckerOptions {
        target: ScriptTarget::ES2017,
        ..CheckerOptions::default()
    }
}

fn check(files: &[(&str, &str)]) -> Vec<Diagnostic> {
    let libs = load_default_lib_files();
    check_multi_file_with_libs(files, files[0].0, ts_options(), &libs)
}

fn assert_clean(src: &str, context: &str) {
    let diags = check(&[("case.ts", src)]);
    assert!(
        diags.is_empty(),
        "{context}: expected no diagnostics, got: {:#?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

fn assert_reports_2322(src: &str, context: &str) {
    let diags = check(&[("case.ts", src)]);
    assert!(
        diags.iter().any(|d| d.code == 2322),
        "{context}: expected a TS2322, got: {:#?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

/// The minimal witness: a generic `.map` over an `any[]` receiver checked against
/// a concrete union contextual type. The inner callback returns `string[]`; with
/// the first-arm pin it was typed against `string` and rejected.
#[test]
fn map_over_any_under_concrete_union_context_is_clean() {
    assert_clean(
        r#"
declare const inner: any[];
const r: string[] | string[][] = inner.map((item) => [String(item)]);
"#,
        "map over any[] under string[] | string[][]",
    );
}

/// The receiver element type must not matter: a concrete `number[]` receiver
/// reproduces the same first-arm pin.
#[test]
fn map_over_number_under_concrete_union_context_is_clean() {
    assert_clean(
        r#"
declare const inner: number[];
const r: string[] | string[][] = inner.map((item) => [String(item)]);
"#,
        "map over number[] under string[] | string[][]",
    );
}

/// The issue's exact repro: a hand-written `cb: (x) => U | U[]` callback target
/// whose body is a nested generic call over an `any[]` receiver. The outer `U`
/// must be inferred as `string[]` (result `string[][]`), not leaked as `U[]`.
#[test]
fn hand_written_union_callback_target_does_not_leak_param() {
    assert_clean(
        r#"
declare function myFlatMap<U>(cb: (x: number) => U | U[]): U[];
const result = myFlatMap((value) => {
  const inner: any[] = [];
  return inner.map((item) => [String(item)]);
});
const probe: string[][] = result;
"#,
        "U | U[] callback target with nested any[] map",
    );
}

/// Same shape with a differently-named type parameter and the lib
/// `ReadonlyArray<E>` arm, proving the fix is neither name- nor
/// `Array`-literal-scoped.
#[test]
fn readonly_array_union_callback_target_does_not_leak_param() {
    assert_clean(
        r#"
declare function expand<E>(cb: (x: number) => E | ReadonlyArray<E>): E[];
const out = expand((value) => {
  const src: any[] = [];
  return src.map((item) => [String(item)]);
});
const probe: string[][] = out;
"#,
        "E | ReadonlyArray<E> callback target with nested any[] map",
    );
}

/// The lib `Array.prototype.flatMap` is the production witness (ofetch
/// `withQuery`): its callback return target is `U | ReadonlyArray<U>`.
#[test]
fn lib_flatmap_with_nested_any_map_is_clean() {
    assert_clean(
        r#"
const inner: any[] = [];
const result = [1, 2].flatMap((value) => {
  return inner.map((item) => [String(item)]);
});
const probe: string[][] = result;
"#,
        "Array.prototype.flatMap with nested any[] map",
    );
}

/// The result must be inferred as the *correct* `string[][]`, not merely be
/// error-free: assigning it to an incompatible annotation must still report
/// TS2322. This guards against a fix that simply suppresses the diagnostic.
#[test]
fn result_type_is_string_array_array_not_silenced() {
    assert_reports_2322(
        r#"
declare function myFlatMap<U>(cb: (x: number) => U | U[]): U[];
const result = myFlatMap((value) => {
  const inner: any[] = [];
  return inner.map((item) => [String(item)]);
});
const probe: number[] = result;
"#,
        "result string[][] is not assignable to number[]",
    );
}

/// When the contextual union arms agree on a single binding for the parameter,
/// the return context still pins it (no regression in the unambiguous case).
/// Here both arms drive `T := string`, and the callback's `() => "x"` body is
/// accepted by the pinned contextual return.
#[test]
fn agreeing_union_arms_still_pin_parameter() {
    assert_clean(
        r#"
declare function pickOne<T>(cb: () => T): T | T[];
const value = pickOne(() => "x");
const probe: string | string[] = value;
"#,
        "agreeing union arms keep pinning T",
    );
}
