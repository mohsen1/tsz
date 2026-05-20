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
use tsz_checker::test_utils::diagnostic_code_messages;
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

    diagnostic_code_messages(checker.ctx.diagnostics)
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

/// Infer-result conditional used with a concrete constraint should emit TS2344
/// when the true branch is an unconstrained infer variable. Tsc treats the
/// infer result's base constraint as `unknown`, which does not satisfy `string`.
#[test]
fn test_infer_conditional_with_concrete_constraint_emits_ts2344() {
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

    let ts2344_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2344)
        .collect();
    assert!(
        ts2344_errors.len() == 1,
        "Expected exactly one TS2344 for unconstrained infer result against concrete string constraint, got: {ts2344_errors:?}"
    );
    assert!(
        ts2344_errors
            .iter()
            .any(|(_, msg)| msg.contains("ExtractName")),
        "Expected TS2344 message to mention 'ExtractName', got: {ts2344_errors:?}"
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

/// A source constraint can make the inferred property type satisfy the concrete
/// target constraint. This mirrors the issue's accepted control:
/// `T extends { name: string }` proves `ExtractName<T>` satisfies `string`.
#[test]
fn test_source_constraint_satisfying_concrete_constraint_no_ts2344() {
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
type Test<T extends { name: string }> = MustBeString<ExtractName<T>>;
"#,
    );

    let ts2344_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2344)
        .collect();
    assert!(
        ts2344_errors.is_empty(),
        "Should NOT emit TS2344 when the conditional check type's source constraint proves the infer result, got: {ts2344_errors:?}"
    );
}

#[test]
fn test_tuple_rest_infer_satisfies_array_constraint_no_ts2344() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
interface Array<T> {
    length: number;
    [n: number]: T;
}
interface Boolean {}
interface Function {}
interface IArguments {}
interface Number {}
interface Object {}
interface RegExp {}
interface String {}

type UnshiftTuple<T extends [...any[]]> = T extends [T[0], ...infer Tail] ? Tail : never;
type UseArray<T extends any[]> = T;
type UseNestedArray<T extends Array<Array<any>>> = T;

type FromRest<T extends [...any[]]> = UseArray<UnshiftTuple<T>>;
type NestedTuple = UseNestedArray<[[]]>;
"#,
    );

    let ts2344_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2344)
        .collect();
    assert!(
        ts2344_errors.is_empty(),
        "Should NOT emit TS2344 for tuple-rest infer results or nested tuple array constraints, got: {ts2344_errors:?}"
    );
}

#[test]
fn test_fake_readonly_array_surface_does_not_satisfy_readonly_array_constraint() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
interface ReadonlyArray<T> {
    readonly length: number;
    readonly [n: number]: T;
    concat(...items: T[]): T[];
    slice(start?: number, end?: number): T[];
    join(separator?: string): string;
    indexOf(searchElement: T): number;
    lastIndexOf(searchElement: T): number;
    every(callbackfn: (value: T) => boolean): boolean;
}

interface Fake {
    readonly length: number;
    readonly [n: number]: string;
    concat: any;
    slice: any;
}

type Box<X extends readonly string[]> = X;
type A = Box<Fake>;
"#,
    );

    let ts2344_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2344)
        .collect();
    assert!(
        !ts2344_errors.is_empty(),
        "Fake array-like surfaces should not satisfy readonly array constraints. Got: {diagnostics:?}"
    );
}

#[test]
fn test_mapped_key_infer_subset_satisfies_keyof_constraint() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
type Pick<T, K extends keyof T> = { [P in K]: T[P] };
type KeysWithoutStringIndex<T> =
    { [K in keyof T]: string extends K ? never : K } extends { [_ in keyof T]: infer U }
    ? U
    : never;

export type RemoveIdxSgn<T> = Pick<T, KeysWithoutStringIndex<T>>;
"#,
    );

    let ts2344_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2344)
        .collect();
    assert!(
        ts2344_errors.is_empty(),
        "Mapped key infer result should be accepted as a keyof subset. Got: {ts2344_errors:?}"
    );
}

#[test]
fn test_react_component_props_with_ref_accepts_conditional_element_type() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
namespace JSX {
    export interface IntrinsicElements { div: any; }
}

namespace React {
    export type ComponentType<P = any> = (props: P) => any;
    export type ElementType<P = any> = keyof JSX.IntrinsicElements | ComponentType<P>;
    export type ComponentPropsWithRef<T extends ElementType> = any;
}

type IntrinsicElementsKeys = keyof JSX.IntrinsicElements;
type Props<C extends string | React.ComponentType<any>> =
    React.ComponentPropsWithRef<
        C extends IntrinsicElementsKeys | React.ComponentType<any> ? C : never
    >;
"#,
    );

    let ts2344_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2344)
        .collect();
    assert!(
        ts2344_errors.is_empty(),
        "Conditional intrinsic/component element types should satisfy ComponentPropsWithRef. Got: {ts2344_errors:?}"
    );
}

#[test]
fn test_styled_component_inner_component_constraint_errors_at_declaration_time() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
namespace JSX {
    export interface IntrinsicElements { div: any; }
}

namespace React {
    export type ComponentType<P = any> = (props: P) => any;
    export type ElementType<P = any> = keyof JSX.IntrinsicElements | ComponentType<P>;
    export type ComponentPropsWithRef<T extends ElementType> = any;
}

type StyledComponent<C extends keyof JSX.IntrinsicElements | React.ComponentType<any>> =
    string & React.ComponentType<any>;
type AnyStyledComponent = StyledComponent<any>;

interface StyledComponentBase {
    withComponent<WithC extends AnyStyledComponent>(): StyledComponent<
        StyledComponentInnerComponent<WithC>
    >;
}

type StyledComponentInnerComponent<C extends React.ComponentType<any>> =
    C extends StyledComponent<infer I> ? I : C;
type StyledComponentPropsWithRef<C extends keyof JSX.IntrinsicElements | React.ComponentType<any>> =
    C extends AnyStyledComponent
        ? React.ComponentPropsWithRef<StyledComponentInnerComponent<C>>
        : React.ComponentPropsWithRef<C>;
"#,
    );

    let ts2344_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2344)
        .collect();
    assert!(
        ts2344_errors
            .iter()
            .any(|(_, msg)| msg.contains("Type 'WithC' does not satisfy")),
        "Expected TS2344 for WithC not satisfying ComponentType<any>. Got: {ts2344_errors:?}"
    );
    assert!(
        ts2344_errors
            .iter()
            .any(|(_, msg)| msg.contains("Type 'AnyStyledComponent & C' does not satisfy")),
        "Expected TS2344 for narrowed C not satisfying ComponentType<any>. Got: {ts2344_errors:?}"
    );
}

#[test]
fn test_function_rest_infer_satisfies_array_constraint_no_ts2344() {
    // When `infer A` appears in a function rest-parameter position
    // (`...args: infer A`), `A` is implicitly constrained to `unknown[]`.
    // Using `A` as a type argument to a generic that requires `T extends unknown[]`
    // must NOT produce TS2344 — TSC defers the check to conditional type evaluation.
    // Regression test for https://github.com/mohsen1/tsz/issues/5796.
    let diagnostics = compile_and_get_diagnostics(
        r#"
interface Array<T> { length: number; [n: number]: T; }
interface Boolean {}
interface Function {}
interface IArguments {}
interface Number {}
interface Object {}
interface RegExp {}
interface String {}

type Reverse<T extends unknown[]> =
    T extends [infer First, ...infer Rest]
        ? [...Reverse<Rest>, First]
        : [];

type FlipArguments<T extends (...args: any) => any> =
    T extends (...args: infer A) => infer R
        ? (...args: Reverse<A>) => R
        : never;
"#,
    );

    let ts2344_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2344)
        .collect();
    assert!(
        ts2344_errors.is_empty(),
        "Should NOT emit TS2344 for infer from function rest parameter used as array-constrained type arg. Got: {ts2344_errors:?}"
    );
}

#[test]
fn test_function_rest_infer_multiple_names_no_ts2344() {
    // Same fix verified with different infer variable names (not just `A`).
    let diagnostics = compile_and_get_diagnostics(
        r#"
interface Array<T> { length: number; [n: number]: T; }
interface Boolean {}
interface Function {}
interface IArguments {}
interface Number {}
interface Object {}
interface RegExp {}
interface String {}

type Reverse<T extends unknown[]> =
    T extends [infer First, ...infer Rest]
        ? [...Reverse<Rest>, First]
        : [];

type FlipArguments1<T extends (...args: any) => any> =
    T extends (...args: infer Params) => infer Ret
        ? (...args: Reverse<Params>) => Ret
        : never;

type FlipArguments2<T extends (...args: any) => any> =
    T extends (...args: infer X) => infer Y
        ? (...args: Reverse<X>) => Y
        : never;
"#,
    );

    let ts2344_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2344)
        .collect();
    assert!(
        ts2344_errors.is_empty(),
        "Should NOT emit TS2344 regardless of infer variable name. Got: {ts2344_errors:?}"
    );
}
