//! Round-2 contextual typing must not capture an enclosing signature's type
//! parameter that shares a name with an unfixed callee type parameter.
//!
//! Substitutions are name-keyed (#14345 migration debt). The round-2
//! contextual pass instantiates a context-sensitive callback's contextual
//! type with the round-1 candidates, then defaults still-unfixed callee
//! parameters. When a round-1 candidate legitimately mentions an *enclosing*
//! function's type parameter that shares a name with one of those unfixed
//! callee parameters (`map<F, R, A, B>` called inside `use<F, R, A, B>` with
//! `A := B_outer`), the defaulting pass used to rewrite the foreign
//! occurrence to `unknown` as well, producing a tsz-only `TS2345`
//! (`Argument of type 'unknown' is not assignable to parameter of type 'B'`).
//! `tsc` keeps the outer parameter. Guarded in
//! `query_boundaries/inference.rs::complete_contextual_type_param_plan`.
//!
//! Every expectation below is oracle-pinned against `tsc` 6.0.2 (`--strict`).

use tsz_checker::context::CheckerOptions;

fn check(source: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_source(source, "test.ts", CheckerOptions::default())
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

const HKT_PRELUDE: &str = r#"
interface TypeLambda {
    readonly In: unknown;
    readonly Out: unknown;
}
type Kind<F extends TypeLambda, In, Target> = F extends { readonly type: unknown }
    ? (F & { readonly In: In; readonly Target: Target })["type"]
    : { readonly F: F; readonly In: (_: In) => void; readonly Target: (_: Target) => Target };

declare const map: <F extends TypeLambda, R, A, B>(
    self: Kind<F, R, A>,
    f: (a: A) => B
) => Kind<F, R, B>;
"#;

fn check_hkt(body: &str) -> Vec<(u32, String)> {
    check(&format!("{HKT_PRELUDE}\n{body}"))
}

/// Bare colliding outer param: `use`'s `B` is the candidate for `map`'s `A`
/// while `map`'s own `B` is still unfixed. tsc: clean.
#[test]
fn outer_param_sharing_unfixed_callee_name_survives_round2() {
    let diags = check_hkt(
        r#"
function use<F extends TypeLambda, R, A, B>(
    self: Kind<F, R, B>,
    f: (b: B) => string
): Kind<F, R, string> {
    return map(self, (b) => f(b));
}
"#,
    );
    assert!(diags.is_empty(), "tsc reports no diagnostics: {diags:#?}");
}

/// Tuple of colliding outer params through a nested generic call — the
/// original `conditional_alias_first_arg_context_types_binding_pattern_callback`
/// witness shape. tsc: clean.
#[test]
fn tuple_of_colliding_outer_params_through_nested_call() {
    let diags = check_hkt(
        r#"
declare const pair: <F extends TypeLambda, R, A, B>(
    left: Kind<F, R, A>,
    right: Kind<F, R, B>
) => Kind<F, R, [A, B]>;
function use<F extends TypeLambda, R, A, B>(
    left: Kind<F, R, A>,
    right: Kind<F, R, B>,
    f: (a: A, b: B) => string
): Kind<F, R, string> {
    return map(pair(left, right), ([a, b]) => f(a, b));
}
"#,
    );
    assert!(diags.is_empty(), "tsc reports no diagnostics: {diags:#?}");
}

/// Renamed outer binders never collided and must stay clean (control).
#[test]
fn renamed_outer_binders_control() {
    let diags = check_hkt(
        r#"
function use<G extends TypeLambda, S, X, Y>(
    self: Kind<G, S, Y>,
    f: (b: Y) => string
): Kind<G, S, string> {
    return map(self, (b) => f(b));
}
"#,
    );
    assert!(diags.is_empty(), "tsc reports no diagnostics: {diags:#?}");
}

/// The colliding outer param wrapped in a generic interface. tsc: clean.
#[test]
fn wrapped_colliding_outer_param() {
    let diags = check_hkt(
        r#"
interface Box<T> { readonly value: T; }
function use<F extends TypeLambda, R, A, B>(
    self: Kind<F, R, Box<B>>,
    f: (b: Box<B>) => string
): Kind<F, R, string> {
    return map(self, (b) => f(b));
}
"#,
    );
    assert!(diags.is_empty(), "tsc reports no diagnostics: {diags:#?}");
}

/// Same collision without any return context (statement position). tsc: clean.
#[test]
fn collision_without_return_context() {
    let diags = check_hkt(
        r#"
function use<F extends TypeLambda, R, A, B>(
    self: Kind<F, R, B>,
    f: (b: B) => string
): void {
    const r = map(self, (b) => f(b));
}
"#,
    );
    assert!(diags.is_empty(), "tsc reports no diagnostics: {diags:#?}");
}

/// Negative control: a genuine mismatch in the callback body still errors,
/// and the argument renders as the preserved outer `B`, not `unknown`.
#[test]
fn genuine_mismatch_still_reports_outer_param() {
    let diags = check_hkt(
        r#"
function use<F extends TypeLambda, R, A, B>(
    self: Kind<F, R, B>,
    f: (n: number) => string
): Kind<F, R, string> {
    return map(self, (b) => f(b));
}
"#,
    );
    let ts2345: Vec<_> = diags.iter().filter(|(code, _)| *code == 2345).collect();
    assert_eq!(ts2345.len(), 1, "exactly one TS2345 like tsc: {diags:#?}");
    assert_eq!(
        ts2345[0].1, "Argument of type 'B' is not assignable to parameter of type 'number'.",
        "tsc renders the preserved outer parameter, not unknown"
    );
}

/// The defaulting the guard bypasses must still fire when no other candidate
/// mentions the unfixed name: `T` fixes to `unknown` and the callback param is
/// usable as `unknown`. tsc: clean.
#[test]
fn unfixed_param_without_colliding_candidate_still_defaults() {
    let diags = check(
        r#"
declare function make<T>(cb: (v: T) => void): T;
const out = make((v) => {
    const u: unknown = v;
});
"#,
    );
    assert!(diags.is_empty(), "tsc reports no diagnostics: {diags:#?}");
}

/// Concrete instantiation control: normal inference through the deferred
/// conditional alias still works outside any generic body. tsc: clean.
#[test]
fn concrete_instantiation_control() {
    let diags = check_hkt(
        r#"
declare const val: Kind<{ readonly In: unknown; readonly Out: unknown }, number, string>;
const mapped = map(val, (s) => s.length);
"#,
    );
    assert!(diags.is_empty(), "tsc reports no diagnostics: {diags:#?}");
}
