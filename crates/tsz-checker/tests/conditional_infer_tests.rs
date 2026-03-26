//! Tests for conditional type evaluation with infer patterns

use crate::state::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

/// Test that conditional types with `infer V` pattern resolve to concrete types
/// when the check type is a concrete application of the same generic interface.
///
/// TSC resolves `SyntheticDestination<number, Synthetic<number, number>>` to `number`.
/// We must match this behavior - the `infer V` should bind to `number`, not remain
/// as an uninstantiated type parameter `T`.
#[test]
fn test_conditional_infer_resolves_to_concrete_type() {
    let source = r#"
interface Synthetic<A, B extends A> {}
type SyntheticDestination<T, U> = U extends Synthetic<T, infer V> ? V : never;
type TestSynthetic = SyntheticDestination<number, Synthetic<number, number>>;
const z: TestSynthetic = '3'; // Should error TS2322: string not assignable to number
"#;
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
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // tsc emits TS2322 because TestSynthetic resolves to `number`,
    // and '3' (string) is not assignable to number.
    let ts2322_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .collect();
    assert_eq!(
        ts2322_errors.len(),
        1,
        "Expected exactly 1 TS2322 error (string not assignable to number), got {} errors. All diagnostics: {:?}",
        ts2322_errors.len(),
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

/// Test that conditional types with constrained type parameters don't emit false TS2322.
///
/// `UnrollOnHover<S>` is `S extends object ? { [K in keyof S]: S[K] } : never`.
/// When S is constrained by `Schema` (which extends `object`), the conditional's
/// constraint should simplify to `{ [K in keyof S]: S[K] }` (identity mapped type),
/// and `Table<S>` should be assignable to `Table<UnrollOnHover<S>>`.
#[test]
fn test_no_false_ts2322_conditional_type_constraint_target() {
    let source = r#"
type UnrollOnHover<O extends object> = O extends object ?
    { [K in keyof O]: O[K]; } :
    never;

type Schema = Record<string, unknown>;
class Table<S extends Schema> {
    __schema!: S;
}
class ColumnSelectViewImp<S extends Schema> extends Table<S> { }

const ColumnSelectView1: new <S extends Schema>() => Table<UnrollOnHover<S>> = ColumnSelectViewImp;
const ColumnSelectView2: new <S extends Schema>() => Table<UnrollOnHover<S>> = Table;
"#;
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
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // tsc emits no errors for this code. The conditional type's constraint
    // allows S to be assignable to UnrollOnHover<S>.
    let ts2322_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .collect();
    assert_eq!(
        ts2322_errors.len(),
        0,
        "Expected no TS2322 errors, got {} errors. All diagnostics: {:?}",
        ts2322_errors.len(),
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_conditional_object_multi_infer_resolves_true_branch() {
    let source = r#"
type PickMeta<T> = T extends { defaultProps: infer D; propTypes: infer P } ? [D, P] : never;
type Result = PickMeta<{
    defaultProps: { foo: string };
    propTypes: { bar: number };
}>;

const ok: Result = [{ foo: "x" }, { bar: 1 }];
const bad: Result = [{ foo: 1 }, { bar: "x" }];
"#;
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
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    let ts2322_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .collect();
    assert_eq!(
        ts2322_errors.len(),
        2,
        "Expected tuple element assignment errors from resolved multi-infer conditional, got diagnostics: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

/// Test that indexed access types in conditional contexts work correctly.
#[test]
fn test_indexed_access_in_conditional_context() {
    let source = r#"
type First<T extends any[]> = T extends [infer F, ...any[]] ? F : never;
type R1 = First<[string, number]>; // should be string
const x: R1 = 42; // should error: number not assignable to string
"#;
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
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    let ts2322_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .collect();
    assert_eq!(
        ts2322_errors.len(),
        1,
        "Expected exactly 1 TS2322 error (number not assignable to string), got {} errors. All diagnostics: {:?}",
        ts2322_errors.len(),
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_utility_types_function_keys_generic_pick_has_no_false_diagnostics() {
    let source = r#"
type NonUndefined<A> = A extends undefined ? never : A;
type FunctionKeys<T extends object> = {
  [K in keyof T]-?: NonUndefined<T[K]> extends (...args: any[]) => unknown ? K : never;
}[keyof T];
type FunctionProps<T extends object> = Pick<T, FunctionKeys<T>>;
"#;
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
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Expected utility-types FunctionKeys/Pick pattern to check cleanly, got: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}
