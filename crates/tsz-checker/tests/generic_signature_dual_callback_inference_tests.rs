//! Inference over a generic call whose every argument is an inferentially
//! typed callback must not manufacture diagnostics.
//!
//! Structural rule: when every argument of a generic call is a
//! context-sensitive function whose parameter and return positions both
//! reference the same uninferred type parameter (`foo(a: (x: T) => T,
//! b: (x: T) => T)` called as `foo((x) => 1, (x) => '')`), `tsc` collects no
//! fixed candidates, leaves the parameter unresolved (`unknown`), and accepts
//! the call; tsz does this through the checker's return-contribution
//! aggregation keeping fresh literal contributions unwidened while the
//! contextual return type is not a resolved literal domain, so
//! inference-time widening of fresh candidates owns the final decision.
//!
//! Regression fences for the family PR #17709 (and #17693 before it)
//! regressed: widening a fresh literal return contribution at aggregation
//! time — while the callback's contextual return is still the uninferred
//! type parameter, or a prematurely concrete instantiation of it — hands
//! inference a pre-widened candidate, pins the type parameter early, and
//! manufactures a false `TS2322`/`TS2345` on the sibling callback
//! (`conformance/.../typeInference/genericCallWithGenericSignatureArguments.ts`,
//! the m4-attributed row on #17709, plus the CLI witnesses fenced here).
//!
//! Every expectation is oracle-pinned against the pinned conformance
//! typescript@7.0.2 via `scripts/conformance/oracle.sh`, strict and
//! non-strict, 2026-08-19.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_multi_file_with_libs, load_default_lib_files};
use tsz_common::diagnostics::Diagnostic;

fn check_with(source: &str, strict: bool) -> Vec<Diagnostic> {
    let libs = load_default_lib_files();
    check_multi_file_with_libs(
        &[("main.ts", source)],
        "main.ts",
        CheckerOptions {
            strict,
            strict_null_checks: strict,
            strict_function_types: strict,
            no_implicit_any: strict,
            ..CheckerOptions::default()
        },
        &libs,
    )
}

fn messages(diags: &[Diagnostic]) -> Vec<(u32, String)> {
    diags
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn assert_clean_both_modes(source: &str, context: &str) {
    for strict in [true, false] {
        let diags = check_with(source, strict);
        assert!(
            diags.is_empty(),
            "{context} (strict: {strict}): expected no diagnostics, got: {:#?}",
            messages(&diags)
        );
    }
}

/// The `genericCallWithGenericSignatureArguments.ts` `r1b` shape: two
/// unannotated arrow callbacks with cross-domain fresh literal returns.
/// Nothing fixes the type parameter, so the call is clean in both modes.
#[test]
fn dual_unannotated_arrow_callbacks_stay_clean() {
    assert_clean_both_modes(
        r#"
declare function foo<T>(a: (x: T) => T, b: (x: T) => T): (x: T) => T;
var r1b = foo((x) => 1, (x) => '');
"#,
        "dual unannotated arrows",
    );
}

/// Renamed binders with the literal domains reversed (string first, number
/// second) and a direct type-parameter result position.
#[test]
fn renamed_binders_reversed_domains_stay_clean() {
    assert_clean_both_modes(
        r#"
declare function bar<U>(a: (y: U) => U, b: (y: U) => U): U;
var q = bar((y) => 'a', (y) => 2);
"#,
        "renamed binders, reversed domains",
    );
}

/// Block-bodied function expressions route contributions through the
/// block-body aggregation path rather than the concise-body one; the rule is
/// the same.
#[test]
fn dual_function_expression_block_bodies_stay_clean() {
    assert_clean_both_modes(
        r#"
declare function foo<T>(a: (x: T) => T, b: (x: T) => T): (x: T) => T;
var r = foo(function (x) { return 1; }, function (x) { return ''; });
"#,
        "block-bodied function expressions",
    );
}

/// The fixture's `r2`/`r3` shapes: one annotated callback fixes the
/// parameter, the other returns a bare `null`. Under `strictNullChecks:
/// false` the oracle accepts both orders.
#[test]
fn annotated_callback_with_null_return_stays_clean_nonstrict() {
    for (source, context) in [
        (
            r#"
declare function foo<T>(a: (x: T) => T, b: (x: T) => T): (x: T) => T;
var r2 = foo((x: Object) => null, (x: string) => '');
"#,
            "Object/null then string",
        ),
        (
            r#"
declare function foo<T>(a: (x: T) => T, b: (x: T) => T): (x: T) => T;
var r3 = foo((x: number) => 1, (x: Object) => null);
"#,
            "number then Object/null",
        ),
    ] {
        let diags = check_with(source, false);
        assert!(
            diags.is_empty(),
            "{context}: expected no diagnostics, got: {:#?}",
            messages(&diags)
        );
    }
}

/// Negative control: annotated callbacks that genuinely disagree must keep
/// failing, with the oracle's exact head and parameter-incompatibility drill,
/// in both modes.
#[test]
fn mismatched_annotated_callbacks_report_ts2345() {
    for strict in [true, false] {
        let diags = check_with(
            r#"
declare function foo<T>(a: (x: T) => T, b: (x: T) => T): (x: T) => T;
var r1 = foo((x: number) => 1, (x: string) => '');
"#,
            strict,
        );
        assert_eq!(
            diags.len(),
            1,
            "negative control (strict: {strict}): expected exactly one diagnostic, got: {:#?}",
            messages(&diags)
        );
        assert_eq!(diags[0].code, 2345, "negative control (strict: {strict})");
        let mut chain = vec![diags[0].message_text.clone()];
        chain.extend(
            diags[0]
                .related_information
                .iter()
                .map(|related| related.message_text.clone()),
        );
        let combined = chain.join("\n");
        for needle in [
            "'(x: string) => string'",
            "'(x: number) => number'",
            "Types of parameters 'x' and 'x' are incompatible.",
            "Type 'number' is not assignable to type 'string'.",
        ] {
            assert!(
                combined.contains(needle),
                "negative control (strict: {strict}): missing `{needle}` in:\n{combined}"
            );
        }
    }
}

/// Living TODO (`#[ignore]`d, red on main): with only context-sensitive
/// callbacks the oracle infers `unknown` for the type parameter, so using the
/// call result where `string | number` is required is a `TS2322` (`Type
/// 'unknown' is not assignable to type 'string | number'.`) in both modes.
/// tsz currently infers an assignable type and accepts the program — a
/// pre-existing false negative, unrelated to the aggregation-widening family
/// fenced above. Drop the `#[ignore]` when unresolved-parameter inference
/// pins `unknown`.
#[test]
#[ignore]
fn context_sensitive_only_inference_pins_unknown() {
    for strict in [true, false] {
        let diags = check_with(
            r#"
declare function foo<T>(a: (x: T) => T, b: (x: T) => T): (x: T) => T;
var f = foo((x) => 1, (x) => '');
var n: number | string = f(2);
"#,
            strict,
        );
        assert_eq!(
            diags.len(),
            1,
            "unknown pin (strict: {strict}): expected exactly one diagnostic, got: {:#?}",
            messages(&diags)
        );
        assert_eq!(diags[0].code, 2322, "unknown pin (strict: {strict})");
        assert!(
            diags[0]
                .message_text
                .contains("Type 'unknown' is not assignable to type 'string | number'."),
            "unknown pin (strict: {strict}): got: {:#?}",
            messages(&diags)
        );
    }
}

/// Negative control for the retry-inference restoration (#17761): callbacks
/// with NO parameters are not context-sensitive, so `tsc` takes their fresh
/// literal returns as genuine inference candidates, widens them at
/// inference time, pins the parameter from the first (`number`), and reports
/// the second body against it — in both modes. The restoration in the
/// generic-call retry must not resurrect these literals: their widened form
/// is the real candidate.
#[test]
fn zero_param_callbacks_keep_widened_candidates_and_report_ts2322() {
    for strict in [true, false] {
        let diags = check_with(
            r#"
declare function bar<T>(a: () => T, b: () => T): T;
var x = bar(() => 1, () => '');
"#,
            strict,
        );
        assert_eq!(
            diags.len(),
            1,
            "zero-param control (strict: {strict}): expected exactly one diagnostic, got: {:#?}",
            messages(&diags)
        );
        assert_eq!(diags[0].code, 2322, "zero-param control (strict: {strict})");
        assert!(
            diags[0]
                .message_text
                .contains("Type 'string' is not assignable to type 'number'."),
            "zero-param control (strict: {strict}): got: {:#?}",
            messages(&diags)
        );
    }
}

/// The same dual context-sensitive shape routed through a construct
/// signature: a generic class whose constructor takes the two callbacks.
#[test]
fn new_expression_dual_unannotated_callbacks_stay_clean() {
    assert_clean_both_modes(
        r#"
declare class Box<V> { constructor(a: (v: V) => V, b: (v: V) => V); }
var boxed = new Box((v) => 3, (v) => 'x');
"#,
        "new-expression dual unannotated callbacks",
    );
}

/// A fresh enum-member return contribution widens to its parent enum through
/// `widen_enum_member_type` rather than the primitive literal widener; the
/// retry restoration must recognize that artifact the same way.
#[test]
fn enum_member_and_string_callbacks_stay_clean() {
    assert_clean_both_modes(
        r#"
enum Shade { Light, Dark }
declare function mix<S>(a: (v: S) => S, b: (v: S) => S): S;
var tone = mix((v) => Shade.Light, (v) => 'soft');
"#,
        "enum-member and string callbacks",
    );
}

/// Same-base-kind fresh literals in both callbacks: nothing conflicts, and
/// the call stays clean whether or not the contributions widen.
#[test]
fn same_kind_literal_callbacks_stay_clean() {
    assert_clean_both_modes(
        r#"
declare function join<W>(a: (v: W) => W, b: (v: W) => W): W;
var same = join((v) => 1, (v) => 2);
"#,
        "same-kind literal callbacks",
    );
}
