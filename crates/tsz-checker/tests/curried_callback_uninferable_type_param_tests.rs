//! Regression tests for issue #15461: an uninferable type parameter reachable
//! only through a nested / curried callback parameter must resolve to its
//! constraint (or `unknown`) and must never leak an internal `__infer_N`
//! inference placeholder into the finalized call-result type.
//!
//! `zipWith<T, S, U>(a: T[], f: (x: T) => (y: S) => U): U[]` called with a
//! curried `pair` whose inner parameter `y: S` occupies only a contravariant
//! slot: `S` receives no inference candidate. `tsc`'s `getInferredType`
//! resolves it to `unknown` (or its constraint), so the result is
//! `{ x: number; y: unknown; }[]` — not `{ x: number; y: __infer_3; }[]`.

use tsz_checker::diagnostics::diagnostic_codes;
use tsz_checker::test_utils::{check_source_diagnostics, diagnostic_codes as codes_of};

/// The finalized result type of a curried-callback generic call must never
/// carry an internal inference placeholder, regardless of which diagnostic
/// (if any) fires.
fn assert_no_leaked_placeholder(source: &str) {
    for diag in check_source_diagnostics(source) {
        assert!(
            !diag.message_text.contains("__infer"),
            "diagnostic leaked an inference placeholder: {diag:?}"
        );
    }
}

/// The original witness: an unconstrained uninferable inner parameter `S`
/// defaults to `unknown`, so `r` is `{ x: number; y: unknown; }[]`.
#[test]
fn curried_uninferable_param_defaults_to_unknown() {
    let source = r#"
declare var zipWith: <T, S, U>(a: T[], f: (x: T) => (y: S) => U) => U[];
declare var pair: <T, S>(x: T) => (y: S) => { x: T; y: S; };
const r = zipWith([1], pair);
const bad: null = r;
"#;
    let diagnostics = check_source_diagnostics(source);
    let codes = codes_of(&diagnostics);
    assert!(
        codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "expected TS2322 for the `null` mismatch: {codes:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|d| d.message_text.contains("y: unknown")),
        "result must default the uninferable inner param to `unknown`: {diagnostics:?}"
    );
    assert_no_leaked_placeholder(source);
}

/// A constrained uninferable inner parameter (`S extends string`) resolves to
/// its constraint (`string`), not `unknown` and not a placeholder.
#[test]
fn curried_uninferable_constrained_param_defaults_to_constraint() {
    let source = r#"
declare var zipWith: <T, S extends string, U>(a: T[], f: (x: T) => (y: S) => U) => U[];
declare var pair: <T, S extends string>(x: T) => (y: S) => { x: T; y: S; };
const r = zipWith([1], pair);
const bad: null = r;
"#;
    let diagnostics = check_source_diagnostics(source);
    assert!(
        diagnostics
            .iter()
            .any(|d| d.message_text.contains("y: string")),
        "constrained uninferable inner param must resolve to its constraint `string`: {diagnostics:?}"
    );
    assert_no_leaked_placeholder(source);
}

/// The fix is structural, not keyed on the `T`/`S`/`U`/`zipWith`/`pair`
/// spelling: renamed binders behave identically.
#[test]
fn curried_uninferable_param_is_structural_across_renamed_binders() {
    let source = r#"
declare var combine: <Elem, Ignored, Out>(a: Elem[], build: (x: Elem) => (y: Ignored) => Out) => Out[];
declare var make: <Elem, Ignored>(x: Elem) => (y: Ignored) => { x: Elem; y: Ignored; };
const r = combine([1], make);
const bad: null = r;
"#;
    let diagnostics = check_source_diagnostics(source);
    assert!(
        diagnostics
            .iter()
            .any(|d| d.message_text.contains("y: unknown")),
        "renamed binders must default the uninferable inner param to `unknown`: {diagnostics:?}"
    );
    assert_no_leaked_placeholder(source);
}

/// A simple single-level uninferable type parameter already defaults to
/// `unknown`; the fix must not disturb it.
#[test]
fn simple_single_level_uninferred_param_stays_unknown() {
    let source = r#"
declare function make<T>(x: number): T;
const r = make(1);
const bad: null = r;
"#;
    let diagnostics = check_source_diagnostics(source);
    assert!(
        diagnostics
            .iter()
            .any(|d| d.message_text.contains("Type 'unknown'")),
        "single-level uninferred type parameter must stay `unknown`: {diagnostics:?}"
    );
    assert_no_leaked_placeholder(source);
}

/// When the inner parameter IS inferable (a sibling `b: S[]` argument fixes
/// `S`), the curried result must use the inferred type — the leak fix must not
/// widen an inferable parameter to `unknown`. Mirrors the upstream
/// `inferentialTypingWithFunctionTypeZip` fixture shape.
#[test]
fn curried_inferable_param_keeps_inferred_type() {
    let source = r#"
declare var zipWith: <T, S, U>(a: T[], b: S[], f: (x: T) => (y: S) => U) => U[];
declare var pair: <T, S>(x: T) => (y: S) => { x: T; y: S; };
const r = zipWith([1], ["a"], pair);
const bad: null = r;
"#;
    let diagnostics = check_source_diagnostics(source);
    assert!(
        diagnostics
            .iter()
            .any(|d| d.message_text.contains("y: string")),
        "an inferable inner param must keep its inferred type (`string`): {diagnostics:?}"
    );
    assert_no_leaked_placeholder(source);
}
