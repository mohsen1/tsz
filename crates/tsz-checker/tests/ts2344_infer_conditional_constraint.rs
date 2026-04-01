//! Tests for TS2344: type argument constraint validation for infer-result
//! conditional types used as type arguments.
//!
//! When a type like `GetProps<C>` (= `C extends ComponentType<infer P> ? P : never`)
//! is used as a type argument to a generic type with a non-trivial constraint,
//! TS2344 should be emitted because the infer result has base constraint `unknown`,
//! which does not satisfy any non-trivial constraint.
//!
//! This matches tsc's `getBaseConstraintOfType` for distributive conditional types,
//! which returns the union of base constraints of true/false branches. For an
//! unconstrained infer true branch and never false branch, this resolves to `unknown`.

use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn compile_and_get_diagnostics(source: &str) -> Vec<(u32, String)> {
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

    checker
        .ctx
        .diagnostics
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

/// Infer-result conditional used as type argument with self-referential constraint
/// should emit TS2344. This is the core pattern from
/// `circularlyConstrainedMappedTypeContainingConditionalNoInfiniteInstantiationDepth.ts`.
#[test]
fn test_infer_conditional_with_self_referential_constraint_emits_ts2344() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
interface Array<T> {}
interface Boolean {}
interface Function {}
interface IArguments {}
interface Number {}
interface Object {}
interface RegExp {}
interface String {}

type GetProps<C> = C extends { props: infer P } ? P : never;

type Shared<
    InjectedProps,
    DecorationTargetProps extends Shared<InjectedProps, DecorationTargetProps>
> = {
    [P in Extract<keyof InjectedProps, keyof DecorationTargetProps>]?: DecorationTargetProps[P];
};

type Result<TInjectedProps> =
    <C extends { props: any }>(
        component: C
    ) => Shared<TInjectedProps, GetProps<C>>;
"#,
    );

    let ts2344_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2344)
        .collect();
    assert!(
        !ts2344_errors.is_empty(),
        "Expected TS2344 for GetProps<C> not satisfying Shared constraint, got: {diagnostics:?}"
    );
    assert!(
        ts2344_errors
            .iter()
            .any(|(_, msg)| msg.contains("GetProps")),
        "Expected TS2344 message to mention 'GetProps', got: {ts2344_errors:?}"
    );
}

/// Infer-result conditional used with a concrete (fully resolved) constraint
/// defers to instantiation time. tsz currently only emits TS2344 for infer
/// conditionals when the constraint contains type parameters (e.g.,
/// self-referential constraints). Concrete constraints like `string` are
/// deferred because tsc can resolve them via restrictive instantiation.
#[test]
fn test_infer_conditional_with_concrete_constraint_defers() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
interface Array<T> {}
interface Boolean {}
interface Function {}
interface IArguments {}
interface Number {}
interface Object {}
interface RegExp {}
interface String {}

type ExtractName<T> = T extends { name: infer N } ? N : never;
type MustBeString<T extends string> = T;
type Test<T> = MustBeString<ExtractName<T>>;
"#,
    );

    // NOTE: tsc emits TS2344 here, but tsz defers for concrete constraints.
    // This is a known limitation — tsz doesn't implement restrictive
    // instantiation, so it can't distinguish cases where the conditional
    // resolves to a satisfying type from those where it doesn't.
    let ts2344_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2344)
        .collect();
    assert!(
        ts2344_errors.is_empty(),
        "Expected no TS2344 for concrete constraint (deferred to instantiation), got: {ts2344_errors:?}"
    );
}

/// When infer result's own constraint satisfies the required constraint,
/// no TS2344 should be emitted.
#[test]
fn test_constrained_infer_satisfying_constraint_no_ts2344() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
interface Array<T> {}
interface Boolean {}
interface Function {}
interface IArguments {}
interface Number {}
interface Object {}
interface RegExp {}
interface String {}

type ExtractString<T> = T extends { name: infer N extends string } ? N : never;
type MustBeString<T extends string> = T;
type Test<T> = MustBeString<ExtractString<T>>;
"#,
    );

    let ts2344_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2344)
        .collect();
    assert!(
        ts2344_errors.is_empty(),
        "Should NOT emit TS2344 when infer constraint satisfies required constraint, got: {ts2344_errors:?}"
    );
}
