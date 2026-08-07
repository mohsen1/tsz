//! An overloaded (non-generic) callable target contextually types a function
//! expression's parameter only under `noImplicitAny`.
//!
//! Structural rule (`typescript-go` `internal/checker/checker.go`,
//! `getContextualCallSignature` / `getIntersectedSignatures`): when a
//! contextual type's callable shape carries more than one arity-applicable
//! call signature, tsc returns a *combined* signature only when
//! `noImplicitAny` is on:
//!
//! ```go
//! func (c *Checker) getIntersectedSignatures(signatures []*Signature) *Signature {
//!     if !c.noImplicitAny {
//!         return nil
//!     }
//!     ...
//! }
//! ```
//!
//! Under a non-strict program (`noImplicitAny` off), an overloaded callable
//! target — even one whose signatures agree at every position — yields NO
//! contextual signature at all, so the parameter falls back to implicit
//! `any` and the body free-checks. tsz's `ParameterExtractor::visit_callable`
//! (`tsz-solver/src/contextual/extractors.rs`) previously combined multiple
//! signatures unconditionally, unioning disagreeing positions or reusing an
//! agreed-upon type — correct only for the `noImplicitAny` branch, wrongly
//! also applied when it is off. Confirmed against pinned `typescript@7.0.2`
//! oracle for every case below.

use crate::context::CheckerOptions;

// `CheckerOptions::default()` sets `no_implicit_any: true` (a checker-test
// convenience default, unrelated to tsc's own project default), so the
// non-strict cases below must explicitly turn it off.
fn diagnostics(source: &str) -> Vec<(u32, String)> {
    crate::test_utils::check_source(
        source,
        "test.ts",
        CheckerOptions {
            no_implicit_any: false,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|d| (d.code, d.message_text))
    .collect()
}

fn diagnostics_no_implicit_any(source: &str) -> Vec<(u32, String)> {
    crate::test_utils::check_source(source, "test.ts", CheckerOptions::default())
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

fn no_errors(source: &str) {
    let diags = diagnostics(source);
    assert!(
        diags.is_empty(),
        "expected no errors under default (non-strict) options, got: {diags:#?}\nsource:\n{source}"
    );
}

// ── Positive: same-arity overloads with disagreeing parameter types ────────

#[test]
fn disagreeing_overload_params_stay_untyped_without_no_implicit_any() {
    // TypeScript/tests/cases/compiler/functionLiteralForOverloads.ts (trimmed
    // to the `f` case). tsc reports nothing.
    no_errors(
        r#"
var f: {
    (x: string): string;
    (x: number): number;
} = (x) => x;
"#,
    );
}

#[test]
fn disagreeing_overload_params_stay_untyped_generic_return() {
    // Same fixture's `f4`: generic return type on both overloads.
    no_errors(
        r#"
var f4: {
    <T>(x: string): T;
    <T>(x: number): T;
} = (x) => x;
"#,
    );
}

#[test]
fn reassigned_overloaded_target_property_access_no_error() {
    // TypeScript/tests/cases/compiler/contextualTypingOfLambdaWithMultipleSignatures2.ts
    // `a` must stay implicit `any`, so `a.asdf` does not report TS2339.
    no_errors(
        r#"
var f: {
    (x: string): string;
    (x: number): string
};

f = (a) => { return a.asdf }
"#,
    );
}

#[test]
fn overloaded_callback_argument_no_error() {
    // TypeScript/tests/cases/conformance/types/typeRelationships/typeInference/genericCallWithOverloadedFunctionTypedArguments.ts
    // (`NonGenericParameter` block). `x => x` must not be checked against a
    // synthesized union parameter type.
    no_errors(
        r#"
namespace NonGenericParameter {
    var a: {
        (x: boolean): boolean;
        (x: string): string;
    }

    function foo4(cb: typeof a) {
        return cb;
    }

    var r4 = foo4(x => x);
}
"#,
    );
}

// ── Positive: identical same-position overload parameter types ─────────────

#[test]
fn identical_overload_params_still_stay_untyped_without_no_implicit_any() {
    // tsc gives NO contextual signature at all once there are 2+
    // arity-applicable signatures, even when they happen to agree —
    // `getIntersectedSignatures` bails on `!noImplicitAny` before comparing
    // shapes. Oracle-verified: `x.bogusProp` reports nothing under
    // `--strict false`.
    no_errors(
        r#"
var f: {
    (x: string): void;
    (x: string): void;
} = (x) => { x.bogusProp; };
"#,
    );
}

// ── Renamed binder control ──────────────────────────────────────────────────

#[test]
fn disagreeing_overload_params_stay_untyped_renamed_binder() {
    no_errors(
        r#"
var handler: {
    (value: string): string;
    (value: number): number;
} = (value) => value;
"#,
    );
}

// ── Negative control: noImplicitAny still combines and still reports ───────

#[test]
fn no_implicit_any_still_unions_disagreeing_overload_params() {
    // Same shape as `identical_overload_params_still_stay_untyped_...` but
    // with disagreeing positions and `noImplicitAny` on: tsc DOES combine
    // (`x: string | number`) and DOES report the member missing on one arm.
    let diags = diagnostics_no_implicit_any(
        r#"
var f: {
    (x: string): void;
    (x: number): void;
} = (x) => { x.length; };
"#,
    );
    assert!(
        diags.iter().any(|(code, _)| *code == 2339),
        "expected TS2339 for `.length` on the unioned `string | number` param under noImplicitAny, got: {diags:#?}"
    );
}

#[test]
fn no_implicit_any_allows_common_member_on_unioned_overload_params() {
    let diags = diagnostics_no_implicit_any(
        r#"
var f: {
    (x: string): void;
    (x: number): void;
} = (x) => { x.toString(); };
"#,
    );
    assert!(
        diags.is_empty(),
        "`.toString()` exists on both string and number, expected no errors, got: {diags:#?}"
    );
}

// ── Negative control: a genuinely single-signature target still contextually types ──

#[test]
fn single_signature_target_still_contextually_types_without_no_implicit_any() {
    let diags = diagnostics(
        r#"
var f: (x: string) => void = (x) => { x.bogusProp; };
"#,
    );
    assert!(
        diags.iter().any(|(code, _)| *code == 2339),
        "a single-signature target must still contextually type its parameter, got: {diags:#?}"
    );
}
