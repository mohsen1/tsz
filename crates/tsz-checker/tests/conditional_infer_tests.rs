//! Tests for conditional type evaluation with infer patterns

use tsz_checker::diagnostics::Diagnostic;

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
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    let ts2322_errors: Vec<&Diagnostic> = diagnostics.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322_errors.len(),
        1,
        "Expected exactly 1 TS2322 error (string not assignable to number), got {} errors. All diagnostics: {:?}",
        ts2322_errors.len(),
        diagnostics
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
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    let ts2322_errors: Vec<&Diagnostic> = diagnostics.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322_errors.len(),
        0,
        "Expected no TS2322 errors, got {} errors. All diagnostics: {:?}",
        ts2322_errors.len(),
        diagnostics
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
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    let ts2322_errors: Vec<&Diagnostic> = diagnostics.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322_errors.len(),
        2,
        "Expected tuple element assignment errors from resolved multi-infer conditional, got diagnostics: {:?}",
        diagnostics
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
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    let ts2322_errors: Vec<&Diagnostic> = diagnostics.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322_errors.len(),
        1,
        "Expected exactly 1 TS2322 error (number not assignable to string), got {} errors. All diagnostics: {:?}",
        ts2322_errors.len(),
        diagnostics
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

/// Regression test: `Prepend<V, T>` infers R = `[V, ...T]` from
/// `(head: V, ...args: T) extends (...args: infer R)`.
///
/// Previously `match_rest_infer_tuple` returned `false` when source params had
/// both fixed and rest elements (mixed case), causing `Prepend` to evaluate to
/// `any` (false branch) instead of the correct prepended tuple type.
#[test]
fn test_prepend_infer_rest_from_mixed_params() {
    // Prepend<V, T> infers R = [V, ...T] from (head: V, ...args: T) => void
    // BuildTree uses Prepend to count depth: terminates when Length<I> == N.
    let source = r#"
type Length<T extends any[]> = T["length"];
type Prepend<V, T extends any[]> = ((head: V, ...args: T) => void) extends (
  ...args: infer R
) => void
  ? R
  : any;

// Prepend<any, []> must be [any] (length 1), not any.
type P0 = Prepend<any, []>;
type L0 = Length<P0>;
const l0: L0 = 1; // Must not error

// Prepend<any, [any]> must be [any, any] (length 2).
type P1 = Prepend<any, [any]>;
type L1 = Length<P1>;
const l1: L1 = 2; // Must not error
"#;
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "Expected Prepend infer pattern to check cleanly, got: {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_build_tree_depth_two_selects_terminal_branch() {
    let source = r#"
type Length<T extends any[]> = T["length"];
type Prepend<V, T extends any[]> = ((head: V, ...args: T) => void) extends (
  ...args: infer R
) => void
  ? R
  : any;

type PickDepth<T, N extends number, I extends any[]> = {
  1: T;
  0: T & { children: PickDepth<T, N, Prepend<any, I>>[] };
}[Length<I> extends N ? 1 : 0];

interface User {
  name: string;
}

type DepthTwo = PickDepth<User, 2, [any, any]>;
const user: DepthTwo = { name: "Grandson" };
"#;
    let codes = tsz_checker::test_utils::check_source_codes(source);
    assert!(
        !codes.contains(&2741),
        "Depth-two BuildTree index should select terminal branch, got: {codes:?}"
    );
}

#[test]
fn test_tuple_length_conditional_key_resolves_to_true_literal() {
    let source = r#"
type Length<T extends any[]> = T["length"];
type Key<I extends any[], N extends number> = Length<I> extends N ? 1 : 0;
const key: Key<[any, any], 2> = 1;
const bad: Key<[any, any], 2> = 0;
"#;
    let codes = tsz_checker::test_utils::check_source_codes(source);
    assert!(
        codes == vec![2322],
        "Tuple length conditional key should resolve to literal 1, got: {codes:?}"
    );
}

#[test]
fn test_object_indexed_by_tuple_length_conditional_key() {
    let source = r#"
type Length<T extends any[]> = T["length"];
type Select<I extends any[], N extends number> = {
  1: { name: string };
  0: { name: string; children: unknown[] };
}[Length<I> extends N ? 1 : 0];

const user: Select<[any, any], 2> = { name: "Grandson" };
"#;
    let codes = tsz_checker::test_utils::check_source_codes(source);
    assert!(
        !codes.contains(&2741),
        "Object indexed by tuple-length conditional key should select branch 1, got: {codes:?}"
    );
}

/// Downstream check: `BuildTree` recursive conditional type should terminate
/// at depth N now that `Prepend<V, T>` infers correctly for mixed
/// fixed+rest params.
///
/// Without the `match_rest_infer_tuple` fix, `Prepend<any, I>` collapsed
/// to `any` and `BuildTree` never terminated, producing a false TS2741.
/// With the fix, the unit-level Prepend behaviour above is correct and the
/// instantiated indexed-access key is deferred until the resolver can expand
/// aliases like `Length<I>`.
#[test]
fn test_build_tree_no_false_ts2741() {
    // Without the fix, Prepend evaluated to `any`, causing BuildTree never to
    // terminate and emitting TS2741 (required property `children` missing).
    let source = r#"
type Length<T extends any[]> = T["length"];
type Prepend<V, T extends any[]> = ((head: V, ...args: T) => void) extends (
  ...args: infer R
) => void
  ? R
  : any;

type BuildTree<T, N extends number = -1, I extends any[] = []> = {
  1: T;
  0: T & { children: BuildTree<T, N, Prepend<any, I>>[] };
}[Length<I> extends N ? 1 : 0];

interface User {
  name: string;
}

type GrandUser = BuildTree<User, 2>;

// A correctly-typed assignment — depth-2 tree has no `children` requirement
// at depth 2, so the object literal should be valid.
const grandUser: GrandUser = {
  name: "Grand User",
  children: [
    { name: "Son", children: [{ name: "Grandson" }] }
  ]
};
"#;
    let codes = tsz_checker::test_utils::check_source_codes(source);
    assert!(
        !codes.contains(&2741),
        "Must NOT emit TS2741 — BuildTree must terminate at depth 2 without false property-missing errors, got: {codes:?}"
    );
}

#[test]
fn test_conditional_key_selects_depth_terminal_branch() {
    let source = r#"
type Length<T extends any[]> = T["length"];
type PickDepth<T, N extends number, I extends any[]> = {
  1: T;
  0: T & { children: any[] };
}[Length<I> extends N ? 1 : 0];

interface User {
  name: string;
}

type Depth2 = PickDepth<User, 2, [any, any]>;
const user: Depth2 = { name: "Grandson" };
"#;
    let codes = tsz_checker::test_utils::check_source_codes(source);
    assert!(
        !codes.contains(&2741),
        "Concrete depth selector must choose terminal branch without children, got: {codes:?}"
    );
}

#[test]
fn test_tuple_length_conditional_with_numeric_literal() {
    let source = r#"
type Length<T extends any[]> = T["length"];
type IsTwo = Length<[any, any]> extends 2 ? "yes" : "no";
const value: IsTwo = "yes";
"#;
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "Tuple length conditional should resolve to true branch, got: {diagnostics:?}"
    );
}

#[test]
fn test_object_index_with_tuple_length_conditional_key() {
    let source = r#"
type Length<T extends any[]> = T["length"];
type Selected = {
  1: "terminal";
  0: { children: any[] };
}[Length<[any, any]> extends 2 ? 1 : 0];
const value: Selected = "terminal";
"#;
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "Object index should use evaluated conditional key, got: {diagnostics:?}"
    );
}

#[test]
fn test_generic_object_index_with_numeric_literal_key() {
    let source = r#"
type Selected<T> = {
  1: T;
  0: T & { children: any[] };
}[1];

interface User {
  name: string;
}

type Depth2 = Selected<User>;
const user: Depth2 = { name: "Grandson" };
"#;
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "Generic object index should select numeric literal key, got: {diagnostics:?}"
    );
}

#[test]
fn test_generic_object_index_with_instantiated_conditional_key() {
    let source = r#"
type Length<T extends any[]> = T["length"];
type Selected<N extends number, I extends any[]> = {
  1: "terminal";
  0: { children: any[] };
}[Length<I> extends N ? 1 : 0];

type Depth2 = Selected<2, [any, any]>;
const value: Depth2 = "terminal";
"#;
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "Generic object index should use instantiated conditional key, got: {diagnostics:?}"
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
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "Expected utility-types FunctionKeys/Pick pattern to check cleanly, got: {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}
