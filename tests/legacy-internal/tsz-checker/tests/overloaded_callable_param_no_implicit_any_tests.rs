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
fn disagreeing_overload_params_stay_untyped_unused_type_param() {
    // Same fixture's `f2`: an unused `<T>` on each overload still counts as
    // "generic" by shape, but the arity-filtered `ParameterForCallExtractor`
    // path must apply the same `noImplicitAny` gate as the plain path —
    // previously it lacked the gate entirely and unioned `x: string | number`.
    no_errors(
        r#"
var f2: {
    <T>(x: string): string;
    <T>(x: number): number;
} = (x) => x;
"#,
    );
}

#[test]
fn disagreeing_overload_params_stay_untyped_shared_type_param_position() {
    // Same fixture's `f3`: both overloads use their own type parameter `T`
    // at the parameter position. `ParameterForCallExtractor` previously
    // unioned the two distinct `T` `TypeId`s into an undeduped `T | T` and
    // checked the arrow's returned `x` against a plain `string` return type,
    // producing a spurious TS2322.
    no_errors(
        r#"
var f3: {
    <T>(x: T): string;
    <T>(x: T): number;
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

// ── noImplicitAny ON: combinable generic overloads produce a *combined* generic
//    signature, so the arrow adopts one shared `<T>` and stays a generic identity.
//
// tsc's `getIntersectedSignatures` combines two-or-more arity-applicable
// signatures under `noImplicitAny` only when their type parameters are identical
// (`compareTypeParametersIdentical`), mapping each later signature's own type
// parameters onto the first's before unioning parameter positions
// (`combineIntersectionParameters`). Two overloads that each declare their own
// `<T>` collapse to a single `<T>` — not an undeduped `T | T` — and the function
// expression adopts that `<T>`, becoming `<T>(x: T) => T`, which every overload
// can instantiate. Oracle-verified against pinned `typescript@7.0.2` `--strict`
// for each case below.

#[test]
fn shared_type_param_position_combines_and_stays_clean() {
    // `functionLiteralForOverloads.ts` `f3`. The pre-fix bug: the arity-filtered
    // extractor unioned the two overloads' distinct-but-same-named `T` into an
    // undeduped `T | T` and left the arrow non-generic, producing a spurious
    // TS2322 (`Type 'T | T' is not assignable to type 'string'`). #16950.
    let diags = diagnostics_no_implicit_any(
        r#"
var f3: {
    <T>(x: T): string;
    <T>(x: T): number;
} = (x) => x;
"#,
    );
    assert!(
        diags.is_empty(),
        "combinable shared-`T` overloads must contextually type the arrow as a generic identity, got: {diags:#?}"
    );
}

#[test]
fn generic_return_overloads_stay_clean() {
    // `functionLiteralForOverloads.ts` `f4`: a generic return with concrete
    // parameters. The arrow adopts `<T>` and stays assignable to both overloads.
    let diags = diagnostics_no_implicit_any(
        r#"
var f4: {
    <T>(x: string): T;
    <T>(x: number): T;
} = (x) => x;
"#,
    );
    assert!(
        diags.is_empty(),
        "generic-return overloads must stay clean under noImplicitAny, got: {diags:#?}"
    );
}

#[test]
fn renamed_binders_shared_position_stay_clean() {
    // The combine maps type parameters *positionally*, so two overloads that name
    // their shared-position parameter differently (`<U>`/`<V>`) still collapse to
    // one type parameter. Guards against a name-string dependence in the merge.
    let diags = diagnostics_no_implicit_any(
        r#"
var h: {
    <U>(x: U): string;
    <V>(x: V): number;
} = (x) => x;
"#,
    );
    assert!(
        diags.is_empty(),
        "renamed shared-position binders must still combine, got: {diags:#?}"
    );
}

#[test]
fn matching_constraint_overloads_stay_clean() {
    // Identical constraints are combinable (`compareTypeParametersIdentical`).
    let diags = diagnostics_no_implicit_any(
        r#"
var h: {
    <T extends object>(x: T): string;
    <T extends object>(x: T): number;
} = (x) => x;
"#,
    );
    assert!(
        diags.is_empty(),
        "matching-constraint overloads must combine, got: {diags:#?}"
    );
}

#[test]
fn three_shared_type_param_overloads_stay_clean() {
    // The identity check is pairwise across the whole set, not just adjacent.
    let diags = diagnostics_no_implicit_any(
        r#"
var h: {
    <T>(x: T): string;
    <T>(x: T): number;
    <T>(x: T): boolean;
} = (x) => x;
"#,
    );
    assert!(
        diags.is_empty(),
        "three combinable shared-`T` overloads must stay clean, got: {diags:#?}"
    );
}

// ── noImplicitAny ON: non-combinable overloads combine to nothing, so the arrow
//    parameter falls back to implicit `any` (TS7006) — never a synthesized union.

#[test]
fn mismatched_constraint_overloads_report_ts7006() {
    // Differing constraints are NOT identical, so tsc yields no contextual
    // signature at all and the parameter is implicitly `any`.
    let diags = diagnostics_no_implicit_any(
        r#"
var h: {
    <T extends string>(x: T): string;
    <T extends number>(x: T): number;
} = (x) => x;
"#,
    );
    assert!(
        diags.iter().any(|(code, _)| *code == 7006),
        "mismatched-constraint overloads must not combine; expected TS7006, got: {diags:#?}"
    );
    assert!(
        !diags.iter().any(|(code, _)| *code == 2322),
        "mismatched-constraint overloads must not synthesize a unioned parameter (no TS2322), got: {diags:#?}"
    );
}

#[test]
fn mixed_generic_and_non_generic_overloads_report_ts7006() {
    // A generic/non-generic mix differs in type-parameter arity, so it is not
    // combinable and the parameter stays implicit `any`.
    let diags = diagnostics_no_implicit_any(
        r#"
var h: {
    (x: string): string;
    <T>(x: T): number;
} = (x) => x;
"#,
    );
    assert!(
        diags.iter().any(|(code, _)| *code == 7006),
        "generic/non-generic mix must not combine; expected TS7006, got: {diags:#?}"
    );
}

// ── noImplicitAny ON: combinable *non-generic* overloads with disagreeing
//    concrete positions still combine and still report the return mismatch.

#[test]
fn disagreeing_concrete_params_report_ts2322() {
    // `functionLiteralForOverloads.ts` `f`: non-generic overloads combine to
    // `(x: string | number) => string | number`, which — because the return is
    // concrete and cannot be re-instantiated — is not assignable to either
    // overload's `string`/`number` return. tsc reports TS2322 here.
    let diags = diagnostics_no_implicit_any(
        r#"
var f: {
    (x: string): string;
    (x: number): number;
} = (x) => x;
"#,
    );
    assert!(
        diags.iter().any(|(code, _)| *code == 2322),
        "disagreeing concrete-parameter overloads must still report TS2322, got: {diags:#?}"
    );
}

#[test]
fn unused_generic_disagreeing_concrete_params_report_ts2322() {
    // `functionLiteralForOverloads.ts` `f2`: an unused `<T>` on each overload
    // does not save the concrete `string`/`number` return positions from the
    // same TS2322 as `f`.
    let diags = diagnostics_no_implicit_any(
        r#"
var f2: {
    <T>(x: string): string;
    <T>(x: number): number;
} = (x) => x;
"#,
    );
    assert!(
        diags.iter().any(|(code, _)| *code == 2322),
        "unused-generic disagreeing overloads must still report TS2322, got: {diags:#?}"
    );
}
