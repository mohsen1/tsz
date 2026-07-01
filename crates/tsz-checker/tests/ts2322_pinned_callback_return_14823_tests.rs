//! Regression tests for issue #14823 (goal: hold).
//!
//! When a contextually-typed `.map(cb)` (or a generic `f(x, cb)`) is returned or
//! assigned directly and the callback `cb` fixes its return type with an EXPLICIT
//! return annotation or a concise-body `as`/`satisfies`/type-assertion that is
//! WIDER than the target element type (e.g. the callback returns
//! `string | undefined` where the target is `string[]`), `tsc` ranks the
//! callback's authoritative explicit return above the contextual return
//! (`InferencePriority.ReturnType`). It fixes `U := string | undefined`, then the
//! ordinary assignment/return relation reports `(string | undefined)[]` is not
//! assignable to `string[]` — TS2322.
//!
//! tsz previously let the contextual `string[]` clamp `U` back down to `string`,
//! silently coercing the result and DROPPING the result-level relation — a
//! false-negative. The fix suppresses the contextual return only for pinned
//! callbacks (`suppress_generic_return_context_for_pinned_callback_return` /
//! `callback_return_is_explicitly_pinned` in
//! `crates/tsz-checker/src/checkers/call_context.rs`) and skips the
//! assignability-recovery retry that would re-seed the contextual return, so the
//! residual mismatch surfaces once at the assignment/return site.
//!
//! Anti-hardcoding: the rule is structural ("the callback's explicit/asserted
//! return pins the call's return type parameter"), so the tests vary binder and
//! parameter names and include positive controls, rather than matching any
//! specific identifier or rendered message.

use std::sync::{Arc, OnceLock};

use tsz_binder::lib_loader::LibFile;
use tsz_checker::test_utils::{
    check_source_strict_codes, check_source_with_libs, diagnostic_codes, load_default_lib_files,
    strict_checker_options,
};

/// The default lib bundle, loaded from disk once and shared across every
/// lib-based case (`load_default_lib_files` re-reads and re-parses the whole
/// bundle on each call; the `Arc`s make reuse cheap).
fn default_libs() -> &'static [Arc<LibFile>] {
    static LIBS: OnceLock<Vec<Arc<LibFile>>> = OnceLock::new();
    LIBS.get_or_init(load_default_lib_files)
}

/// Diagnostic codes for `source` type-checked in strict mode WITH the default
/// libs wired in (so `Array.prototype.map` and its generic signature resolve).
fn strict_codes_with_libs(source: &str) -> Vec<u32> {
    diagnostic_codes(&check_source_with_libs(
        source,
        "t.ts",
        strict_checker_options(),
        default_libs(),
    ))
}

/// Assert the pinned-callback-return mismatch surfaces as TS2322 (lib-based
/// `.map` cases). Checks once and reuses the codes in the failure message.
fn assert_reports_ts2322_with_libs(source: &str) {
    let codes = strict_codes_with_libs(source);
    assert!(codes.contains(&2322), "expected TS2322, got {codes:?}");
}

/// Assert a lib-based case is clean (positive control): a pinned return that
/// matches the target must not over-report TS2322.
fn assert_no_ts2322_with_libs(source: &str) {
    let codes = strict_codes_with_libs(source);
    assert!(!codes.contains(&2322), "unexpected TS2322, got {codes:?}");
}

/// Assert TS2322 for a self-contained (lib-free) generic-call case.
fn assert_reports_ts2322(source: &str) {
    let codes = check_source_strict_codes(source);
    assert!(codes.contains(&2322), "expected TS2322, got {codes:?}");
}

/// Assert a self-contained generic-call case is clean (positive control).
fn assert_no_ts2322(source: &str) {
    let codes = check_source_strict_codes(source);
    assert!(!codes.contains(&2322), "unexpected TS2322, got {codes:?}");
}

// ---------------------------------------------------------------------------
// Reported repros: pinned callback return wider than the contextual target
// ---------------------------------------------------------------------------

/// `xs.map((x): string | undefined => x)` returned directly into `string[]`:
/// the annotated `string | undefined` return pins `U`, so the
/// `(string | undefined)[]` result must fail against `string[]` with TS2322.
#[test]
fn map_callback_annotated_wider_return_reports_ts2322() {
    assert_reports_ts2322_with_libs(
        r#"
function fn2(xs: string[]): string[] {
  return xs.map((x): string | undefined => x);
}
"#,
    );
}

/// Same defect reached through a concise-body `as`-cast instead of an
/// annotation: `[1].map((x) => undefined as string | undefined)` into `string[]`.
#[test]
fn map_callback_as_cast_wider_return_reports_ts2322() {
    assert_reports_ts2322_with_libs(
        r#"
function fn3(): string[] {
  return [1].map((x) => undefined as string | undefined);
}
"#,
    );
}

/// A concise-body `satisfies`-cast pins the return type the same way an `as`-cast
/// does (both are recognized by `callback_return_is_explicitly_pinned`).
#[test]
fn map_callback_satisfies_wider_return_reports_ts2322() {
    assert_reports_ts2322_with_libs(
        r#"
function fn5(): string[] {
  return [1].map((x) => (undefined satisfies unknown) as string | undefined);
}
"#,
    );
}

/// The non-array generic-call form of the same family: `call<T, U>(x, cb)` where
/// `cb`'s `as`-cast pins `U = string | undefined`, assigned into `string`. This
/// case is self-contained (no lib needed).
#[test]
fn generic_call_as_cast_wider_return_reports_ts2322() {
    assert_reports_ts2322(
        r#"
declare function call<T, U>(x: T, fn: (x: T) => U): U;
const bad: string = call(1, (x) => undefined as string | undefined);
"#,
    );
}

/// Anti-hardcoding: renamed binders/parameters must behave identically — the
/// suppression keys on structure (the callback's pinned return references the
/// call's return type parameter), not on any specific name.
#[test]
fn binder_name_invariant_reports_ts2322() {
    for (fn_name, type_param, elem, param) in [
        ("collect", "R", "string", "value"),
        ("gather", "Widened", "string", "elem"),
        ("run", "Out", "string", "item"),
    ] {
        assert_reports_ts2322(&format!(
            r#"
declare function {fn_name}<T, {type_param}>(x: T, cb: (x: T) => {type_param}): {type_param};
const out: {elem} = {fn_name}(1, ({param}) => undefined as {elem} | undefined);
"#,
        ));
    }
}

// ---------------------------------------------------------------------------
// Positive controls: a pinned return that MATCHES the target must stay clean
// ---------------------------------------------------------------------------

/// The annotated callback return equals the target element type — no clamp, no
/// mismatch, so no TS2322. Guards against over-reporting from the suppression.
#[test]
fn map_callback_matching_annotation_is_clean() {
    assert_no_ts2322_with_libs(
        r#"
function ok(xs: string[]): string[] {
  return xs.map((x): string => x);
}
"#,
    );
}

/// The generic-call analogue: an `as`-cast whose type matches the target stays
/// clean.
#[test]
fn generic_call_matching_as_cast_is_clean() {
    assert_no_ts2322(
        r#"
declare function call<T, U>(x: T, fn: (x: T) => U): U;
const good: string = call(1, (x) => "" as string);
"#,
    );
}
