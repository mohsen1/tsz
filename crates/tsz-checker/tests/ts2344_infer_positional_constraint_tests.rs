//! Tests for TS2344: infer variable constraint propagation from declaring
//! type parameter positions (issue #6520).
//!
//! When `infer R` appears in a position where the enclosing generic type
//! declares `extends string` (e.g. `Result<T, Rest extends string>` → `infer R`
//! in position of `Rest`), `R` is implicitly constrained to `string`.
//! Using `R` as a type argument where `string` is required must NOT produce
//! TS2344 — tsc treats the positional constraint as the infer variable's
//! effective base constraint.
//!
//! The fix adds:
//! 1. Function/constructor type traversal in `collect_infer_constraints_from_extends_type`
//!    so the infer variable is discovered inside function return/param types.
//! 2. A positional-constraint check in constraint_validation.rs for bare Infer
//!    type arguments with no explicit constraint.

use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_checker::test_utils::diagnostic_code_messages;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn compile(source: &str) -> Vec<(u32, String)> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );
    checker.check_source_file(root);
    diagnostic_code_messages(checker.ctx.diagnostics)
}

fn ts2344(diags: &[(u32, String)]) -> Vec<&(u32, String)> {
    diags.iter().filter(|(c, _)| *c == 2344).collect()
}

// ---------------------------------------------------------------------------
// Core scenario from issue #6520
// ---------------------------------------------------------------------------

/// `infer R` in function return type — R's positional constraint is `string`.
/// Using R as the second arg of Result<any, R> must NOT emit TS2344.
#[test]
fn infer_in_fn_return_constrained_position_no_ts2344() {
    let diags = compile(
        r#"
type Result<T, Rest extends string> = { value: T; rest: Rest };

type UseResult<P> =
    P extends (s: string) => Result<any, infer R>
        ? Result<any, R>
        : never;
"#,
    );
    assert!(
        ts2344(&diags).is_empty(),
        "infer R in constrained return type position must not produce TS2344; got: {diags:?}"
    );
}

/// Vary the infer variable name to X — must still be handled generically.
#[test]
fn infer_in_fn_return_constrained_position_different_name_no_ts2344() {
    let diags = compile(
        r#"
type Result<T, Rest extends string> = { value: T; rest: Rest };

type UseResult<P> =
    P extends (s: string) => Result<any, infer X>
        ? Result<any, X>
        : never;
"#,
    );
    assert!(
        ts2344(&diags).is_empty(),
        "infer X (renamed from R) in constrained return type position must not produce TS2344; got: {diags:?}"
    );
}

/// Third type parameter is constrained; infer targets it by position.
#[test]
fn infer_third_type_param_constrained_no_ts2344() {
    let diags = compile(
        r#"
type Triple<A, B, C extends number> = { a: A; b: B; c: C };

type ExtractC<P> =
    P extends () => Triple<any, any, infer N>
        ? Triple<any, any, N>
        : never;
"#,
    );
    assert!(
        ts2344(&diags).is_empty(),
        "infer N in third constrained position must not produce TS2344; got: {diags:?}"
    );
}

/// When infer appears in a PARAMETER position of the function type in the
/// extends clause, the constraint from the parameter type should be propagated.
#[test]
fn infer_in_fn_param_constrained_position_no_ts2344() {
    let diags = compile(
        r#"
type Wrapper<K extends string> = { key: K };

type ExtractKey<F> =
    F extends (k: Wrapper<infer K>) => void
        ? Wrapper<K>
        : never;
"#,
    );
    assert!(
        ts2344(&diags).is_empty(),
        "infer K in constrained parameter position must not produce TS2344; got: {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// Explicit-constraint infer should still work (existing behavior preserved)
// ---------------------------------------------------------------------------

/// `infer R extends string` — explicit constraint, must not produce TS2344.
#[test]
fn explicit_infer_constraint_satisfying_no_ts2344() {
    let diags = compile(
        r#"
type Result<T, Rest extends string> = { value: T; rest: Rest };

type UseResult<P> =
    P extends (s: string) => Result<any, infer R extends string>
        ? Result<any, R>
        : never;
"#,
    );
    assert!(
        ts2344(&diags).is_empty(),
        "explicit infer R extends string must not produce TS2344; got: {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// Unconstrained infer — existing TS2344 behaviour must be preserved
// ---------------------------------------------------------------------------

/// `infer N` extracted from an object shape (not in constrained position)
/// used where `string` is required — still TS2344.
#[test]
fn unconstrained_infer_in_object_position_emits_ts2344() {
    let diags = compile(
        r#"
type ExtractName<T> = T extends { name: infer N } ? N : never;
type MustBeString<T extends string> = T;
type Test<T> = MustBeString<ExtractName<T>>;
"#,
    );
    assert!(
        !ts2344(&diags).is_empty(),
        "Unconstrained infer in object position used against string constraint must emit TS2344; got: {diags:?}"
    );
}

/// Positional constraint is `number`, required constraint is `string` →
/// incompatible, TS2344 should fire.
#[test]
fn positional_constraint_incompatible_with_required_emits_ts2344() {
    let diags = compile(
        r#"
type Pair<A extends number, B> = { a: A; b: B };

type MustBeString<T extends string> = T;

type Extract<P> =
    P extends () => Pair<infer N, any>
        ? MustBeString<N>
        : never;
"#,
    );
    assert!(
        !ts2344(&diags).is_empty(),
        "number positional constraint cannot satisfy string requirement — must emit TS2344; got: {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// Broader patterns: constructor types and nested generics
// ---------------------------------------------------------------------------

/// Constructor type in extends clause — same traversal as function types.
#[test]
fn infer_in_constructor_return_constrained_no_ts2344() {
    let diags = compile(
        r#"
type Box<T extends string> = { val: T };

type FromCtor<C> =
    C extends new () => Box<infer S>
        ? Box<S>
        : never;
"#,
    );
    assert!(
        ts2344(&diags).is_empty(),
        "infer S in constructor return constrained position must not produce TS2344; got: {diags:?}"
    );
}

/// Infer nested two levels deep inside the function return type — still works.
#[test]
fn infer_nested_in_generic_in_fn_return_constrained_no_ts2344() {
    let diags = compile(
        r#"
type Outer<T extends string> = { inner: T };
type Wrapper<X> = { outer: X };

type Extract<F> =
    F extends () => Wrapper<Outer<infer S>>
        ? Outer<S>
        : never;
"#,
    );
    assert!(
        ts2344(&diags).is_empty(),
        "infer S nested inside Outer<infer S> inside Wrapper in return type must not produce TS2344; got: {diags:?}"
    );
}

/// Two independent infer variables: one constrained to `string`, one to `number`.
/// Both must be usable without TS2344 in their respective constrained positions.
#[test]
fn two_infer_vars_each_constrained_independently_no_ts2344() {
    let diags = compile(
        r#"
type Pair<S extends string, N extends number> = { s: S; n: N };

type Extract<F> =
    F extends () => Pair<infer S, infer N>
        ? Pair<S, N>
        : never;
"#,
    );
    assert!(
        ts2344(&diags).is_empty(),
        "Two independently constrained infer vars must both be usable without TS2344; got: {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// Diagnostic: simpler patterns without function wrapper to isolate failure
// ---------------------------------------------------------------------------

/// Direct extends clause (no function wrapper) — R is in `Result<any, infer R>`.
/// This tests the base case of hidden_conditional_infer_constraint_type.
#[test]
fn infer_in_direct_constrained_position_no_fn_wrapper_no_ts2344() {
    let diags = compile(
        r#"
type Result<T, Rest extends string> = { value: T; rest: Rest };

type UseResult<P> =
    P extends Result<any, infer R>
        ? Result<any, R>
        : never;
"#,
    );
    assert!(
        ts2344(&diags).is_empty(),
        "infer R in direct constrained position (no function wrapper) must not produce TS2344; got: {diags:?}"
    );
}
