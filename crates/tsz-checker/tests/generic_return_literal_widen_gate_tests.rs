//! The literal-widening gate for generic-call inference and contextual return
//! contributions must mirror `tsc`.
//!
//! Structural rules (issue #17686):
//!
//! 1. When a call-signature type parameter occurs at the top level of the
//!    signature's return type, `tsc` does not widen fresh literal inference
//!    candidates when resolving that parameter (`getCovariantInference`'s
//!    `widenLiteralTypes` gate); tsz does this through the solver's inference
//!    gate (`mark_top_level_in_return_type_unfixed`), so both the round-2
//!    contextual relation and the final result see the literal.
//! 2. A contextual return type pins a fresh literal return contribution only
//!    when it admits that literal's domain (`isLiteralOfContextualType` via
//!    `getWidenedLiteralLikeTypeForContextualReturnTypeIfNeeded`); a
//!    literal context of a different base kind widens the contribution like
//!    the no-context case. tsz does this through the checker's return-type
//!    aggregation (`return_contribution_is_widenable`).
//!
//! All expectations below are oracle-pinned against `tsc` 6.0.2 (`--strict`).

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_multi_file_with_libs, load_default_lib_files};
use tsz_common::diagnostics::Diagnostic;

fn check(source: &str) -> Vec<Diagnostic> {
    let libs = load_default_lib_files();
    check_multi_file_with_libs(
        &[("main.ts", source)],
        "main.ts",
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            strict_function_types: true,
            no_implicit_any: true,
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

fn assert_clean(source: &str, context: &str) {
    let diags = check(source);
    assert!(
        diags.is_empty(),
        "{context}: expected no diagnostics, got: {:#?}",
        messages(&diags)
    );
}

fn assert_single_message_contains(source: &str, code: u32, needles: &[&str], context: &str) {
    let diags = check(source);
    assert_eq!(
        diags.len(),
        1,
        "{context}: expected exactly one diagnostic, got: {:#?}",
        messages(&diags)
    );
    assert_eq!(
        diags[0].code,
        code,
        "{context}: wrong code: {:#?}",
        messages(&diags)
    );
    // The nested elaboration lines live in `related_information`; search the
    // outer message and every related message so the fences pin the full chain.
    let full_chain = std::iter::once(diags[0].message_text.clone())
        .chain(
            diags[0]
                .related_information
                .iter()
                .map(|r| r.message_text.clone()),
        )
        .collect::<Vec<_>>()
        .join("\n");
    for needle in needles {
        assert!(
            full_chain.contains(needle),
            "{context}: message chain missing `{needle}`, got:\n{full_chain}"
        );
    }
}

// --- The #17686 witness: nested elaboration keeps the literal target and the
// --- widened (not fresh) source.

/// Generic-method form (`genericClassWithFunctionTypedMemberArguments` r12).
/// The parameter display and the nested elaboration must agree: `U := 1`
/// survives to the relation, and the callback's `''` return widens to
/// `string` under the number-literal context.
#[test]
fn method_callback_elaboration_keeps_literal_target_and_widened_source() {
    assert_single_message_contains(
        r#"
class C3<T> {
    foo3<U>(x: T, cb: (a: T) => U, y: U) {
        return cb(x);
    }
}
declare var c3: C3<number>;
var r12 = c3.foo3(1, function (a) { return '' }, 1);
"#,
        2345,
        &[
            "'(a: number) => string'",
            "'(a: number) => 1'",
            "Type 'string' is not assignable to type '1'.",
        ],
        "method form",
    );
}

/// Free-function form with renamed binders.
#[test]
fn free_function_callback_elaboration_keeps_literal_target() {
    assert_single_message_contains(
        r#"
declare function apply<In, Out>(seed: In, op: (v: In) => Out, probe: Out): Out;
var out = apply(1, function (v) { return '' }, 1);
"#,
        2345,
        &[
            "'(v: number) => string'",
            "'(v: number) => 1'",
            "Type 'string' is not assignable to type '1'.",
        ],
        "free-function form",
    );
}

// --- Literal preservation for top-level-in-return type parameters (the fix's
// --- positive space; each was a TS2322 false positive before).

/// The literal survives a mixed direct/callback signature into a
/// literal-typed binding.
#[test]
fn literal_survives_mixed_callback_signature() {
    assert_clean(
        r#"
declare function pair<T, U>(x: T, cb: (a: T) => U, y: U): U;
const c: 1 = pair(2, (a) => 1, 1);
"#,
        "mixed direct/callback",
    );
}

/// The literal survives when the callback references the parameter in its own
/// parameter list (the shape `tsc` fixes for contextual typing), in both
/// argument orders.
#[test]
fn literal_survives_callback_referencing_param() {
    assert_clean(
        r#"
declare function g1<U>(cb: (a: U) => void, y: U): U;
const r1: 1 = g1((a) => {}, 1);
declare function g2<Elem>(y: Elem, cb: (a: Elem) => void): Elem;
const r2: 1 = g2(1, (a) => {});
"#,
        "callback references param",
    );
}

/// Same shape with a non-context-sensitive callback.
#[test]
fn literal_survives_non_context_sensitive_callback() {
    assert_clean(
        r#"
declare function g3<U>(cb: (a: U) => void, y: U): U;
const r3: 1 = g3((a: number) => {}, 1);
"#,
        "non-context-sensitive callback",
    );
}

/// Identity-shaped callbacks (`(a) => a`) keep the literal in both orders.
#[test]
fn literal_survives_identity_callback() {
    assert_clean(
        r#"
declare function m2<U>(cb: (a: U) => U, y: U): U;
const q1: 1 = m2((a) => a, 1);
declare function m3<Val>(y: Val, cb: (a: Val) => Val): Val;
const q2: 1 = m3(1, (a) => a);
"#,
        "identity callback",
    );
}

/// A literal candidate arriving only through a callback return or an object
/// property is preserved for a top-level-in-return parameter.
#[test]
fn literal_survives_nested_position_candidates() {
    assert_clean(
        r#"
declare function m4<T, U>(x: T, cb: (a: T) => U): U;
const q3: 1 = m4('s', (a) => 1);
declare function m5<U>(cb: () => U): U;
const q4: 1 = m5(() => 1);
declare function m6<U>(y: { v: U }): U;
const q5: 1 = m6({ v: 1 });
"#,
        "nested-position candidates",
    );
}

// --- Negative space: widening that must survive the fix.

/// A parameter that appears only under a constructor in the return type still
/// widens its fresh literal candidate.
#[test]
fn constructor_wrapped_return_param_still_widens() {
    assert_single_message_contains(
        r#"
declare function h<U>(y: U): U[];
const t = h(1);
const t2: 1[] = t;
"#,
        2322,
        &["'number[]'", "'1[]'"],
        "array-wrapped return",
    );
}

// --- Contextual-return literal-domain rule (rule 2).

/// A number-literal contextual return widens a fresh string-literal
/// contribution: the reported source is `string`, never `""`.
#[test]
fn cross_domain_literal_context_widens_contribution() {
    assert_single_message_contains(
        r#"
const f1: (a: number) => 1 = (a) => '';
"#,
        2322,
        &["Type 'string' is not assignable to type '1'."],
        "cross-domain context",
    );
}

/// A same-domain literal contextual return keeps the fresh literal: the
/// reported source stays `""`.
#[test]
fn same_domain_literal_context_preserves_contribution() {
    assert_single_message_contains(
        r#"
const f2: (a: number) => 'x' = (a) => '';
"#,
        2322,
        &[r#"Type '""' is not assignable to type '"x"'."#],
        "same-domain context",
    );
}

/// Same-domain preservation through a call-argument contextual signature.
#[test]
fn same_domain_argument_context_preserves_contribution() {
    assert_single_message_contains(
        r#"
declare function take(cb: () => 2): void;
take(() => 3);
"#,
        2322,
        &["Type '3' is not assignable to type '2'."],
        "argument context",
    );
}
