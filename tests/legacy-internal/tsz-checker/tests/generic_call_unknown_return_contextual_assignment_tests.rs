//! Contextual return type must not clobber an argument-inferred `unknown`.
//!
//! tsc resolves a generic call's type arguments from arguments
//! (`NakedTypeVariable` priority) strictly before the contextual return type
//! (`ReturnType` priority). So when a return type parameter also appears in a
//! parameter position, the argument-derived inference wins — even when that
//! inference is `unknown`. Calling `generic<T>(x: T): T` with an
//! `unknown`-typed argument infers `T = unknown`; assigning the `unknown`
//! result to a concrete annotation must still report `TS2322`. The contextual
//! annotation must not retroactively fill `T` (issue: false-negative solver /
//! checker — unconstrained generic call inferred to `unknown` skipped `TS2322`
//! at a concrete assignment target).
//!
//! A *return-only* type parameter (`f<T>(): T`) has no argument to constrain it,
//! so it is still legitimately filled from the contextual return type — those
//! cases must keep inferring the annotation and stay error-free.

use tsz_common::options::checker::CheckerOptions;

fn diags_strict(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    let opts = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    crate::test_utils::check_source(source, "test.ts", opts)
}

fn has_ts2322_to(diags: &[crate::diagnostics::Diagnostic], target: &str) -> bool {
    diags
        .iter()
        .any(|d| d.code == 2322 && d.message_text.contains(target))
}

#[test]
fn bare_return_type_param_unknown_arg_reports_ts2322() {
    let diags = diags_strict(
        r#"
function generic<T>(x: T): T { return x; }
declare const w: unknown;
const s: string = generic(w);
"#,
    );
    assert!(
        has_ts2322_to(&diags, "'string'")
            && diags
                .iter()
                .any(|d| d.code == 2322 && d.message_text.contains("'unknown'")),
        "Expected TS2322 unknown -> string for a bare-return generic call; got: {diags:?}"
    );
}

#[test]
fn bare_return_type_param_unknown_arg_reports_ts2322_for_literal_target() {
    let diags = diags_strict(
        r#"
function generic<T>(x: T): T { return x; }
declare const w: unknown;
const s: 42 = generic(w);
"#,
    );
    assert!(
        has_ts2322_to(&diags, "'42'"),
        "Expected TS2322 unknown -> 42; got: {diags:?}"
    );
}

#[test]
fn second_type_param_return_unknown_arg_reports_ts2322() {
    // The full inference path (two type parameters), not the trivial single
    // type-parameter fast path. `U` is argument-owned via `b: U`.
    let diags = diags_strict(
        r#"
function pick<T, U>(a: T, b: U): U { return b; }
declare const w: unknown;
const s: string = pick(1, w);
"#,
    );
    assert!(
        has_ts2322_to(&diags, "'string'"),
        "Expected TS2322 unknown -> string for a two-type-parameter generic call; got: {diags:?}"
    );
}

#[test]
fn explicit_unknown_constraint_unknown_arg_reports_ts2322() {
    let diags = diags_strict(
        r#"
function generic<T extends unknown>(x: T): T { return x; }
declare const w: unknown;
const s: string = generic(w);
"#,
    );
    assert!(
        has_ts2322_to(&diags, "'string'"),
        "Expected TS2322 unknown -> string for `<T extends unknown>`; got: {diags:?}"
    );
}

#[test]
fn varying_binder_names_still_report_ts2322() {
    // Anti-hardcoding: the rule is structural, not keyed to `generic`/`T`/`x`.
    let diags = diags_strict(
        r#"
function reflect<Elem>(value: Elem): Elem { return value; }
declare const anything: unknown;
const out: number = reflect(anything);
"#,
    );
    assert!(
        has_ts2322_to(&diags, "'number'"),
        "Expected TS2322 unknown -> number with renamed binders; got: {diags:?}"
    );
}

#[test]
fn concrete_arg_still_reports_mismatch_against_annotation() {
    // The argument-derived type still drives the assignment check for a
    // concrete (non-`unknown`) argument.
    let diags = diags_strict(
        r#"
function generic<T>(x: T): T { return x; }
const s: string = generic(42);
"#,
    );
    assert!(
        has_ts2322_to(&diags, "'string'")
            && diags
                .iter()
                .any(|d| d.code == 2322 && d.message_text.contains("'number'")),
        "Expected TS2322 number -> string; got: {diags:?}"
    );
}

#[test]
fn wrapped_return_unknown_arg_reports_ts2322() {
    let diags = diags_strict(
        r#"
function wrap<T>(x: T): T[] { return [x]; }
declare const w: unknown;
const s: string[] = wrap(w);
"#,
    );
    assert!(
        has_ts2322_to(&diags, "'string[]'"),
        "Expected TS2322 unknown[] -> string[]; got: {diags:?}"
    );
}

#[test]
fn return_only_type_param_still_filled_from_context() {
    // No parameter mentions `T`, so the contextual return type legitimately
    // fills it — tsc infers `T = string` and reports no error.
    let diags = diags_strict(
        r#"
declare function make<T>(): T;
const s: string = make();
"#,
    );
    assert!(
        !diags.iter().any(|d| d.code == 2322),
        "Return-only type parameter must still infer from context (no TS2322); got: {diags:?}"
    );
}

#[test]
fn return_only_type_param_array_still_filled_from_context() {
    let diags = diags_strict(
        r#"
declare function make<T>(): T[];
const s: string[] = make();
"#,
    );
    assert!(
        !diags.iter().any(|d| d.code == 2322),
        "Return-only type parameter (array) must still infer from context; got: {diags:?}"
    );
}

#[test]
fn identity_literal_preservation_unaffected() {
    // Concrete argument, contextual literal target: the argument-owned literal
    // preservation path must remain intact (no false TS2322).
    let diags = diags_strict(
        r#"
function id<T>(x: T): T { return x; }
const s: "a" = id("a");
"#,
    );
    assert!(
        !diags.iter().any(|d| d.code == 2322),
        "Literal preservation for `id(\"a\")` must not regress; got: {diags:?}"
    );
}

#[test]
fn concrete_matching_arg_has_no_error() {
    let diags = diags_strict(
        r#"
function generic<T>(x: T): T { return x; }
declare const str: string;
const s: string = generic(str);
"#,
    );
    assert!(
        !diags.iter().any(|d| d.code == 2322),
        "Matching concrete argument must not report TS2322; got: {diags:?}"
    );
}
