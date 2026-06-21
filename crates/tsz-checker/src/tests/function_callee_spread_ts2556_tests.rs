//! Regression tests for false-positive TS2556 when spread-calling a value typed
//! as the global `Function` interface (or `Function`/`any` intrinsic).
//!
//! Root cause of the original report (mined from deepkit-type): a value typed as
//! the global `Function` interface is callable in TypeScript through the
//! implicit any-signature `(...args: any[]): any`, so a non-tuple spread
//! argument can never overflow a fixed-arity parameter list and `tsc` reports no
//! TS2556. tsz's contextual "does this non-tuple spread land on a rest/optional
//! tail position?" probe returned `false` for the object-shaped `Function`
//! interface, and the global-`Function`-to-`any` re-collection threaded a
//! `none()` callable context whose fallback also rejected the spread, so tsz
//! emitted a spurious TS2556.
//!
//! Fix: the contextual rest-position extractor treats the `Function`/`any`
//! intrinsic and the structurally-sniffed global `Function` interface object as
//! having an implicit `(...args: any[])` rest position, and the boxed-`Function`
//! call re-collection threads an `any` callable context (routed through the
//! `expected == any` short-circuit) instead of a no-callable context.
//!
//! Test integrity: every assertion loads the real default libs
//! ([`load_default_lib_files`]) so `Function`, `Map`, and the iterator protocol
//! resolve to their genuine lib shapes — the minimal unit-test lib does not
//! declare them, which would make these guards pass vacuously.
//!
//! Issue: <https://github.com/tsz-org/tsz/issues/14218>

use std::sync::{Arc, OnceLock};
use tsz_binder::lib_loader::LibFile;

use crate::CheckerOptions;
use crate::test_utils::{check_source_with_libs_code_messages, load_default_lib_files};

/// The default lib bundle, parsed once and shared across this module.
fn default_libs() -> &'static [Arc<LibFile>] {
    static DEFAULT_LIBS: OnceLock<Vec<Arc<LibFile>>> = OnceLock::new();
    DEFAULT_LIBS.get_or_init(load_default_lib_files)
}

fn check(src: &str) -> Vec<(u32, String)> {
    check_source_with_libs_code_messages(src, "test.ts", CheckerOptions::default(), default_libs())
}

const TS2556: u32 = 2556;

fn ts2556(diags: &[(u32, String)]) -> Vec<&(u32, String)> {
    diags.iter().filter(|(code, _)| *code == TS2556).collect()
}

/// The exact witness from issue #14218: spreading a non-tuple iterable
/// (`Map<...>.values()`) into a value typed as the global `Function` interface
/// is clean in `tsc`; tsz must not emit TS2556.
#[test]
fn spread_iterable_into_global_function_value_no_ts2556() {
    let diags = check(
        r#"
declare const f: Function;
declare const m: Map<string, any>;
const r = f(...m.values());
void r;
"#,
    );

    assert!(
        ts2556(&diags).is_empty(),
        "expected no TS2556 for spread into a global `Function` value, got: {diags:?}"
    );
}

/// The same call shaped over an array spread and a renamed binder: the rule
/// keys on the structural `Function` shape, not the identifier, so a differently
/// named `Function`-typed value must be equally clean.
#[test]
fn spread_array_into_renamed_function_value_no_ts2556() {
    let diags = check(
        r#"
declare const myCallable: Function;
declare const xs: number[];
const r = myCallable(...xs);
void r;
"#,
    );

    assert!(
        ts2556(&diags).is_empty(),
        "expected no TS2556 for array spread into a renamed `Function` value, got: {diags:?}"
    );
}

/// An `any`-typed callee shares the implicit any-signature: a non-tuple spread
/// is likewise clean.
#[test]
fn spread_into_any_callee_no_ts2556() {
    let diags = check(
        r#"
declare const g: any;
declare const xs: number[];
const r = g(...xs);
void r;
"#,
    );

    assert!(
        ts2556(&diags).is_empty(),
        "expected no TS2556 for spread into an `any` callee, got: {diags:?}"
    );
}

/// Negative control: spreading a non-tuple array into a callee with a fixed
/// arity and no rest parameter still overflows the parameter list, so TS2556
/// must still fire — the fix must not blanket-suppress the diagnostic.
#[test]
fn spread_array_into_fixed_arity_callee_still_emits_ts2556() {
    let diags = check(
        r#"
declare function fixed(x: number, y: number): void;
declare const xs: number[];
fixed(...xs);
"#,
    );

    assert!(
        !ts2556(&diags).is_empty(),
        "expected TS2556 for a non-tuple spread into a fixed-arity callee, got: {diags:?}"
    );
}
