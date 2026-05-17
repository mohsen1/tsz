//! Tests for conditional type evaluation with infer patterns

use tsz_checker::diagnostics::Diagnostic;

fn assert_no_ts2322(source: &str, label: &str) {
    let diags = tsz_checker::test_utils::check_source_strict(source);
    let errors: Vec<&Diagnostic> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        errors.is_empty(),
        "[{label}] expected no TS2322, got:\n{:#?}",
        diags
            .iter()
            .map(|d| (d.code, d.start, d.message_text.as_str()))
            .collect::<Vec<_>>()
    );
}

fn check_source_strict_with_default_libs(source: &str) -> Vec<Diagnostic> {
    let libs = tsz_checker::test_utils::load_default_lib_files();
    tsz_checker::test_utils::check_source_with_libs(
        source,
        "test.ts",
        tsz_checker::context::CheckerOptions {
            strict: true,
            ..Default::default()
        },
        &libs,
    )
}

#[test]
fn equal_any_unknown_is_false_and_later_identity_stays_true() {
    let source = r#"
type Equal<X, Y> =
  (<T>() => T extends X ? 1 : 2) extends
  (<T>() => T extends Y ? 1 : 2) ? true : false;

type EQ_any = Equal<any, unknown>;
type EQ5 = Equal<{ a: 1 }, { a: 1 }>;

const eq_any: EQ_any = false;
const eq5: EQ5 = true;

export {};
"#;

    let diagnostics = tsz_checker::test_utils::check_source_strict(source);
    let ts2322_errors: Vec<&Diagnostic> = diagnostics.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322_errors.is_empty(),
        "Equal<any, unknown> must be false and must not affect a later identical structural Equal instantiation. Actual diagnostics: {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, d.start, d.length, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

// =============================================================================
// Equal<X, Y> identity pattern — `any` boundary tests (issues #6777 / #6742)
// =============================================================================
//
// Structural rule: when two generic functions with conditional return types are
// compared for subtyping (the type-challenges `Equal<X, Y>` trick), `any` must
// not be treated as a universal wildcard inside the conditional `extends` clause.
// `Equal<any, non-any>` and `Equal<non-any, any>` both evaluate to `false`;
// only `Equal<any, any>` evaluates to `true`.

#[test]
fn equal_any_literal_is_false() {
    assert_no_ts2322(
        r#"
type Equal<X, Y> =
  (<T>() => T extends X ? 1 : 2) extends
  (<T>() => T extends Y ? 1 : 2) ? true : false;

type E = Equal<any, 1>;
const e: E = false;

export {};
"#,
        "Equal<any, 1> = false",
    );
}

#[test]
fn equal_unknown_any_is_false() {
    assert_no_ts2322(
        r#"
type Equal<X, Y> =
  (<T>() => T extends X ? 1 : 2) extends
  (<T>() => T extends Y ? 1 : 2) ? true : false;

type E = Equal<unknown, any>;
const e: E = false;

export {};
"#,
        "Equal<unknown, any> = false",
    );
}

#[test]
fn equal_any_any_is_true() {
    assert_no_ts2322(
        r#"
type Equal<X, Y> =
  (<T>() => T extends X ? 1 : 2) extends
  (<T>() => T extends Y ? 1 : 2) ? true : false;

type E = Equal<any, any>;
const e: E = true;

export {};
"#,
        "Equal<any, any> = true",
    );
}

/// Full matrix: false-cases followed by true-cases in one file.
/// Regression gate for issue #6742 (cache corruption from `any` evaluations).
#[test]
fn equal_any_matrix_no_cache_corruption() {
    assert_no_ts2322(
        r#"
type Equal<X, Y> =
  (<T>() => T extends X ? 1 : 2) extends
  (<T>() => T extends Y ? 1 : 2) ? true : false;

// --- should be false ---
type F1 = Equal<any, 1>;       const f1: F1 = false;
type F2 = Equal<any, string>;  const f2: F2 = false;
type F3 = Equal<any, unknown>; const f3: F3 = false;
type F4 = Equal<unknown, any>; const f4: F4 = false;
type F5 = Equal<1, any>;       const f5: F5 = false;
type F6 = Equal<string, any>;  const f6: F6 = false;
type F7 = Equal<any, never>;   const f7: F7 = false;
type F8 = Equal<never, any>;   const f8: F8 = false;

// --- should be true (must not be corrupted by the any-cases above) ---
type T1 = Equal<string, string>;     const t1: T1 = true;
type T2 = Equal<number, number>;     const t2: T2 = true;
type T3 = Equal<{ a: 1 }, { a: 1 }>; const t3: T3 = true;
type T4 = Equal<any, any>;           const t4: T4 = true;

export {};
"#,
        "equal any matrix / cache corruption",
    );
}

#[test]
fn equal_nested_any_object_property_is_false() {
    assert_no_ts2322(
        r#"
type Equal<X, Y> =
  (<T>() => T extends X ? 1 : 2) extends
  (<T>() => T extends Y ? 1 : 2) ? true : false;

type F = Equal<{ a: any }, { a: string }>;   const f: F = false;
type G = Equal<{ a: string }, { a: any }>;   const g: G = false;
type T = Equal<{ a: string }, { a: string }>; const t: T = true;

export {};
"#,
        "Equal with nested any in object property",
    );
}

#[test]
fn equal_any_array_element_is_false() {
    assert_no_ts2322(
        r#"
type Equal<X, Y> =
  (<T>() => T extends X ? 1 : 2) extends
  (<T>() => T extends Y ? 1 : 2) ? true : false;

type F1 = Equal<any[], string[]>;  const f1: F1 = false;
type F2 = Equal<string[], any[]>;  const f2: F2 = false;
type T1 = Equal<string[], string[]>; const t1: T1 = true;

export {};
"#,
        "Equal with any in array element position",
    );
}

/// Naming of the type-parameter (`T`, `P`, `K`, `Item`) must not affect the result.
#[test]
fn equal_any_identity_independent_of_param_name() {
    assert_no_ts2322(
        r#"
type EqualP<X, Y>    = (<P>()    => P    extends X ? 1 : 2) extends (<P>()    => P    extends Y ? 1 : 2) ? true : false;
type EqualK<X, Y>    = (<K>()    => K    extends X ? 1 : 2) extends (<K>()    => K    extends Y ? 1 : 2) ? true : false;
type EqualItem<X, Y> = (<Item>() => Item extends X ? 1 : 2) extends (<Item>() => Item extends Y ? 1 : 2) ? true : false;

const fp: EqualP<any, 1>    = false;
const fk: EqualK<any, 1>    = false;
const fi: EqualItem<any, 1> = false;

const tp: EqualP<string, string>    = true;
const tk: EqualK<string, string>    = true;
const ti: EqualItem<string, string> = true;

export {};
"#,
        "Equal any identity independent of param name",
    );
}

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

#[test]
fn recursive_template_literal_with_string_intrinsics_resolves_to_literal() {
    let source = r#"
type CamelCase<S extends string> = S extends `${infer L}_${infer R}`
  ? `${Lowercase<L>}${CamelCase<Capitalize<R>>}`
  : Lowercase<S>;

type CC1 = CamelCase<"hello_world">;
const rejected: CC1 = "anything";
"#;

    let diagnostics = tsz_checker::test_utils::check_source_strict(source);
    let ts2322_errors: Vec<&Diagnostic> = diagnostics.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322_errors.len(),
        1,
        "recursive CamelCase should resolve to the literal \"helloworld\" and reject arbitrary strings. Actual diagnostics: {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn chained_infer_extends_preserves_numeric_literal() {
    let source = r#"
type GetPromiseValue<T> = T extends Promise<infer V extends string>
  ? V
  : T extends Promise<infer V extends number>
    ? V
    : never;

type P2 = GetPromiseValue<Promise<42>>;

const p2: P2 = 42;
"#;

    let diagnostics = check_source_strict_with_default_libs(source);
    let ts2322_errors: Vec<&Diagnostic> = diagnostics.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322_errors.is_empty(),
        "GetPromiseValue<Promise<42>> should preserve literal 42 in the second constrained infer branch; got: {diagnostics:#?}"
    );
}

#[test]
fn keyof_mapped_application_uses_instantiated_constraint() {
    let source = r#"
type MyPick<T, K extends keyof T> = { [P in K]: T[P] };
type PickedKeys = keyof MyPick<{ a: string; b: number }, "a">;

const accepted: PickedKeys = "a";
const rejected: PickedKeys = "b";
"#;

    let diagnostics = tsz_checker::test_utils::check_source_strict(source);
    let ts2322_errors: Vec<&Diagnostic> = diagnostics.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322_errors.len(),
        1,
        "expected only the non-picked key assignment to fail; all diagnostics: {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn infer_props_equality_preserves_pick_alias_body() {
    let source = r#"
type MyExclude<T, U> = T extends U ? never : T;
type MyPick<T, K extends keyof T> = { [P in K]: T[P] };
type MyPartial<T> = { [P in keyof T]?: T[P] };
type IsOptional<T> =
    undefined | null extends T ? true :
    undefined extends T ? true :
    null extends T ? true :
    false;

interface Validator<T> {
    __type: T;
}

type InferType<V> = V extends Validator<infer T> ? T : never;
type RequiredKeys<V> = {
    [K in keyof V]-?:
        MyExclude<V[K], undefined> extends Validator<infer T>
            ? IsOptional<T> extends true ? never : K
            : never
}[keyof V];
type OptionalKeys<V> = MyExclude<keyof V, RequiredKeys<V>>;
type InferPropsInner<V> = { [K in keyof V]-?: InferType<V[K]> };
type InferProps<V> =
    InferPropsInner<MyPick<V, RequiredKeys<V>>> &
    MyPartial<InferPropsInner<MyPick<V, OptionalKeys<V>>>>;

declare const stringValidator: Validator<string>;
declare const maybeValidator: Validator<string | null | undefined>;

const propTypes: {
    name: Validator<string>;
    maybe?: Validator<string | null | undefined>;
} = {
    name: stringValidator,
    maybe: maybeValidator,
};
const propTypesWithoutAnnotation = {
    name: stringValidator,
    maybe: maybeValidator,
};

type ExtractedProps = InferProps<typeof propTypes>;
type ExtractedPropsWithoutAnnotation = InferProps<typeof propTypesWithoutAnnotation>;
type ExtractPropsMatch =
    ExtractedProps extends ExtractedPropsWithoutAnnotation ? true : false;

const matched: true = null as any as ExtractPropsMatch;
"#;

    let diagnostics = check_source_strict_with_default_libs(source);
    assert!(
        diagnostics.iter().all(|d| d.code != 2322),
        "expected InferProps equality to hold; all diagnostics: {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

/// Conditional source is a wrapper Application (e.g. `Exclude<X<T> | undefined,
/// undefined>`) whose base does not match the pattern's base, but whose
/// evaluated form does. The evaluator must reduce through the wrapper so the
/// Application-vs-Application infer match can bind type arguments.
///
/// Without the wrapper-base reduction, `match_infer_pattern` falls through to
/// structural pattern expansion that cannot bind infer arguments through a
/// `Callable` pattern that also carries properties — the common shape for
/// validator-style interfaces (call signature + tagged property).
#[test]
fn exclude_wrapped_source_application_binds_infer_arg() {
    fn check(label: &str, source: &str) {
        let diagnostics = check_source_strict_with_default_libs(source);
        assert!(
            diagnostics.iter().all(|d| d.code != 2322),
            "[{label}] expected infer to bind through Exclude wrapper; all diagnostics: {:?}",
            diagnostics
                .iter()
                .map(|d| (d.code, d.message_text.clone()))
                .collect::<Vec<_>>()
        );
    }

    // 1) Callable+property interface — the original PropTypes-style shape.
    check(
        "callable+property",
        r#"
declare const tag: unique symbol;
interface Validator<T> {
    (props: object): Error | null;
    [tag]?: T;
}
type R = Exclude<Validator<number> | undefined, undefined> extends Validator<infer X> ? X : "no";
const r: number = (null as any as R);
"#,
    );

    // 2) Plain property interface — same rule, different shape.
    check(
        "property-only",
        r#"
interface Box<T> {
    value: T;
}
type R = Exclude<Box<number> | undefined, undefined> extends Box<infer X> ? X : "no";
const r: number = (null as any as R);
"#,
    );

    // 3) Renamed type parameter — rule is structural, not name-based.
    check(
        "renamed-param",
        r#"
interface Wrap<Value> {
    payload: Value;
}
type R = Exclude<Wrap<string> | undefined, undefined> extends Wrap<infer Y> ? Y : "no";
const r: string = (null as any as R);
"#,
    );

    // 4) Builtin NonNullable wrapper — same shape as Exclude<T, null | undefined>.
    check(
        "nonnullable-wrapper",
        r#"
interface Box<T> {
    value: T;
}
type R = NonNullable<Box<number> | null | undefined> extends Box<infer X> ? X : "no";
const r: number = (null as any as R);
"#,
    );

    // 5) Generic conditional consumes the wrapped Application correctly.
    check(
        "generic-context",
        r#"
interface Box<T> { value: T; }
type Unbox<X> = Exclude<X, undefined> extends Box<infer U> ? U : never;
type R = Unbox<Box<number> | undefined>;
const r: number = (null as any as R);
"#,
    );
}

#[test]
fn exclude_wrapped_source_application_does_not_bind_unrelated_base() {
    let diagnostics = check_source_strict_with_default_libs(
        r#"
interface Box<T> { value: T; }
interface Other<T> { other: T; }
type R = Exclude<Box<number> | undefined, undefined> extends Other<infer X> ? X : "no";
const r: "no" = (null as any as R);
"#,
    );
    assert!(
        diagnostics.iter().all(|d| d.code != 2322),
        "unrelated application bases must not bind through wrapper recovery; diagnostics: {:?}",
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
fn distributive_identity_conditional_target_accepts_type_parameter() {
    let source = r#"
type Deferred<T> = T extends unknown ? T : never;

function withDeferred<T>(x: T): Deferred<T> {
    return x;
}

type DeferredAny<T> = T extends any ? T : never;

function withDeferredAny<T>(x: T): DeferredAny<T> {
    return x;
}
"#;

    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    assert!(
        diagnostics.iter().all(|diag| diag.code != 2322),
        "transparent identity conditionals should not emit TS2322. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn recursive_promise_chain_keeps_only_ts1062_without_self_assignment_ts2322() {
    let source = r#"
type PromiseChain<T> = Promise<T | PromiseChain<T>>;

async function unwrapChain<T>(chain: PromiseChain<T>): Promise<T> {
  const result = await chain;
  if (result instanceof Promise) {
    return unwrapChain(result as PromiseChain<T>);
  }
  return result as T;
}
"#;

    let diagnostics = check_source_strict_with_default_libs(source);
    let ts1062_count = diagnostics.iter().filter(|diag| diag.code == 1062).count();
    let false_self_ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|diag| {
            diag.code == 2322
                && diag
                    .message_text
                    .contains("Type 'T' is not assignable to type 'T'")
        })
        .collect();
    assert_eq!(
        ts1062_count, 1,
        "expected exactly one TS1062 for recursive Promise chain; diagnostics: {diagnostics:#?}"
    );
    assert!(
        false_self_ts2322.is_empty(),
        "recursive Promise chain should not emit self-assignment TS2322; diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn extract_like_conditional_target_still_rejects_unconstrained_type_parameter() {
    let source = r#"
type OnlyObjects<T> = T extends object ? T : never;

function withObject<T>(x: T): OnlyObjects<T> {
    return x;
}
"#;

    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    assert!(
        diagnostics.iter().any(|diag| diag.code == 2322),
        "non-transparent Extract-like conditional must still reject unconstrained T. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn recursive_conditional_index_access_does_not_report_property_missing() {
    let source = r#"
type Flatten<T extends readonly unknown[]> = T extends unknown[] ? _Flatten<T>[] : readonly _Flatten<T>[];
type _Flatten<T> = T extends readonly (infer U)[] ? _Flatten<U> : T;

type InfiniteArray<T> = InfiniteArray<T>[];

type B2 = Flatten<InfiniteArray<string>>;
type B3 = B2[0];
"#;

    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    let ts2589_count = diagnostics.iter().filter(|diag| diag.code == 2589).count();
    assert_eq!(
        ts2589_count, 1,
        "recursive indexed access should emit one TS2589 at the index site. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        diagnostics.iter().all(|diag| diag.code != 2339),
        "recursive indexed access must not cascade into TS2339. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn recursive_array_application_infer_flatten_resolves_to_leaf() {
    let source = r#"
type Flatten<T> = T extends Array<infer U> ? Flatten<U> : T;

type F0 = Flatten<number[]>;
type F1 = Flatten<number[][]>;
type F2 = Flatten<number[][][]>;
type F3 = Flatten<Array<Array<number>>>;

const f0: F0 = 42;
const f1: F1 = 42;
const f2: F2 = 42;
const f3: F3 = 42;
"#;

    let diagnostics = tsz_checker::test_utils::check_source_strict(source);
    assert!(
        diagnostics.iter().all(|diag| diag.code != 2322),
        "recursive Array<infer U> flatten should accept leaf numbers. Actual diagnostics: {diagnostics:#?}"
    );

    let rejection_source = r#"
type Flatten<T> = T extends Array<infer U> ? Flatten<U> : T;

type F0 = Flatten<number[]>;
type F1 = Flatten<number[][]>;
type F2 = Flatten<number[][][]>;
type F3 = Flatten<Array<Array<number>>>;

const bad0: F0 = [42];
const bad1: F1 = [[42]];
const bad2: F2 = [[[42]]];
const bad3: F3 = [[42]];
"#;

    let diagnostics = check_source_strict_with_default_libs(rejection_source);
    let ts2322_count = diagnostics.iter().filter(|diag| diag.code == 2322).count();
    assert_eq!(
        ts2322_count, 4,
        "recursive Array<infer U> flatten should reject nested arrays after resolving to number. Actual diagnostics: {diagnostics:#?}"
    );
}

/// Issue #6307 anti-hardcoding gate. The recursive `Array<infer ?>` flatten
/// rule is *structural* — it must not depend on the user choosing the name
/// `U` for the inferred element type, nor on a specific recursion depth that
/// happens to match a fixture. Vary both: rename the infer variable, vary
/// the element type, vary the depth, and exercise a negative case where the
/// leaf is not an array so the conditional terminates without firing the
/// recursive branch.
#[test]
fn recursive_array_application_infer_flatten_rule_is_structural() {
    let source = r#"
// Rename the infer variable: U -> X. The rule is "T extends Array<infer ?>",
// the name must not matter.
type FlattenX<T> = T extends Array<infer X> ? FlattenX<X> : T;

// String element, deeper recursion than the reported repro.
type FS5 = FlattenX<string[][][][][]>;
const fs5: FS5 = "leaf";

// Object element terminates the recursion at depth 1.
type FO1 = FlattenX<{ tag: number }[]>;
const fo1: FO1 = { tag: 1 };

// Non-array input: the conditional's false branch returns T unchanged.
type FN0 = FlattenX<number>;
const fn0: FN0 = 42;

// Different infer name choice on a sibling alias still resolves.
type FlattenE<S> = S extends Array<infer E> ? FlattenE<E> : S;
type FE2 = FlattenE<boolean[][][]>;
const fe2: FE2 = true;
"#;

    let diagnostics = tsz_checker::test_utils::check_source_strict(source);
    assert!(
        diagnostics.iter().all(|diag| diag.code != 2322),
        "recursive Array<infer ?> flatten rule must be name- and depth-independent. Actual diagnostics: {diagnostics:#?}"
    );

    let rejection_source = r#"
type FlattenX<T> = T extends Array<infer X> ? FlattenX<X> : T;

type FS5 = FlattenX<string[][][][][]>;
type FE2 = FlattenX<boolean[][][]>;
type FN0 = FlattenX<number>;

const bad_fs5: FS5 = ["still", "an", "array"];
const bad_fe2: FE2 = [true, false];
const bad_fn0: FN0 = [1];
"#;

    let diagnostics = check_source_strict_with_default_libs(rejection_source);
    let ts2322_count = diagnostics.iter().filter(|diag| diag.code == 2322).count();
    assert_eq!(
        ts2322_count, 3,
        "renamed/deeper Array<infer ?> flatten must still reject array assignments to the resolved leaf. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn recursive_awaited_application_emits_ts2589_at_outer_alias() {
    let source = r#"
interface Promise<T> {
    then(onfulfilled: (value: T) => any): any;
}

type __Awaited<T> = T extends null | undefined ? T :
    T extends object & { then(onfulfilled: infer F): any } ?
        F extends ((value: infer V) => any) ? __Awaited<V> : never :
    T;

type DeeplyNested<T> = Promise<DeeplyNested<T>>;
type A = __Awaited<DeeplyNested<number>>;
"#;

    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    let ts2589_count = diagnostics.iter().filter(|diag| diag.code == 2589).count();
    assert_eq!(
        ts2589_count, 1,
        "recursive __Awaited alias application should emit exactly one TS2589. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn recursive_awaited_type_parameter_assignment_keeps_type_parameter_display() {
    let source = r#"
type __Awaited<T> =
    T extends null | undefined ? T :
    T extends PromiseLike<infer U> ? __Awaited<U> :
    T;

type MyPromise<T> = {
    then<U>(f: ((value: T) => U | PromiseLike<U>) | null | undefined): MyPromise<U>;
}
type InfinitePromise<T> = Promise<InfinitePromise<T>>;

type P0 = __Awaited<Promise<string | Promise<MyPromise<number> | null> | undefined>>;
type P1 = __Awaited<any>;
type P2 = __Awaited<InfinitePromise<number>>;

function f11<T, U extends T>(tx: T, ta: __Awaited<T>, ux: U, ua: __Awaited<U>) {
    ta = ua;
    ua = ta;
    ta = tx;
    tx = ta;
}
"#;

    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    assert!(
        diagnostics.iter().any(|diag| {
            diag.code == 2322
                && diag
                    .message_text
                    .contains("Type 'T' is not assignable to type '__Awaited<T>'")
        }),
        "expected T -> __Awaited<T> display, got: {diagnostics:#?}"
    );
    assert!(
        diagnostics.iter().any(|diag| {
            diag.code == 2322
                && diag
                    .message_text
                    .contains("Type '__Awaited<T>' is not assignable to type 'T'")
        }),
        "expected __Awaited<T> -> T display, got: {diagnostics:#?}"
    );
    assert!(
        diagnostics.iter().all(|diag| {
            !diag.message_text.contains(
                "__Awaited<Promise<string | Promise<MyPromise<number> | null> | undefined>>",
            )
        }),
        "concrete __Awaited alias must not repaint scoped type parameter diagnostics: {diagnostics:#?}"
    );
}

fn assert_no_ts2589(source: &str) {
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    assert!(
        diagnostics.iter().all(|diag| diag.code != 2589),
        "bounded recursive conditional alias should not emit TS2589. \
         Actual diagnostics: {diagnostics:#?}\nSource:\n{source}"
    );
}

#[test]
fn bounded_recursive_conditional_alias_array_branch_no_ts2589() {
    assert_no_ts2589(
        r#"
type Test<T> = [T] extends [any[]] ? { array: Test<T[0]> } : { notArray: T };
declare const x: Test<number[]>;
const y: { array: { notArray: number } } = x;
"#,
    );
}

#[test]
fn bounded_recursive_conditional_alias_action_payload_no_ts2589() {
    assert_no_ts2589(
        r#"
type Action<T, P> = P extends void ? { type: T } : { type: T, payload: P };

enum ActionType {
    Foo,
    Batch
}

type ReducerAction =
    | Action<ActionType.Foo, string>
    | Action<ActionType.Batch, ReducerAction[]>;
"#,
    );
}

#[test]
fn bounded_recursive_conditional_alias_infer_recursive_box_no_ts2589() {
    assert_no_ts2589(
        r#"
interface Box<T> {
    __: T;
}

type Recursive<T> =
    | T
    | Box<Recursive<T>>;

type InferRecursive<T> = T extends Recursive<infer R> ? R : "never!";
type t1 = Box<string | Box<number | boolean>>;
type t2 = InferRecursive<t1>;
type t3 = InferRecursive<Box<string | Box<number | boolean>>>;
"#,
    );
}

#[test]
fn bounded_recursive_conditional_alias_abstract_class_infer_no_ts2589() {
    assert_no_ts2589(
        r#"
abstract class SomeAbstractClass<C, M, R> {
    foo!: (r?: R) => void;
    bar!: (r?: any) => void;
    abstract baz(c: C): M;
}

declare class SomeClass extends SomeAbstractClass<number, string, boolean> {}

type RType<T> = T extends SomeAbstractClass<any, any, infer R> ? R : never;
type SomeClassR = RType<SomeClass>;
declare const r: SomeClassR;
const ok: boolean = r;
"#,
    );
}

#[test]
fn bounded_recursive_conditional_alias_jsonify_no_ts2589() {
    assert_no_ts2589(
        r#"
type JsonifiedObject<T extends object> = { [K in keyof T]: Jsonified<T[K]> };

type Jsonified<T> =
    T extends string | number | boolean | null ? T
    : T extends undefined | Function ? never
    : T extends { toJSON(): infer R } ? R
    : T extends object ? JsonifiedObject<T>
    : "what is this";

declare class MyClass {
    toJSON(): "correct";
}

type Example = {
    customClass: MyClass,
    obj: {
        nested: { attr: MyClass }
    },
};

type JsonifiedExample = Jsonified<Example>;
declare let ex: JsonifiedExample;
const z1: "correct" = ex.customClass;
"#,
    );
}

#[test]
fn nested_tuple_rest_infer_result_satisfies_array_constraint() {
    let source = r#"
interface Array<T> {
    length: number;
}

type _PrependNextNum<A extends unknown[]> = A['length'] extends infer T
    ? [T, ...A] extends [...infer X]
        ? X
        : never
    : never;

type _Enumerate<A extends unknown[], N extends number> = N extends A['length']
    ? A
    : _Enumerate<_PrependNextNum<A>, N> & number;

type Enumerate<N extends number> = number extends N
    ? number
    : _Enumerate<[], N> extends (infer E)[]
    ? E
    : never;

function foo2<T extends unknown[]>(value: T): Enumerate<T['length']> {
    return value.length;
}
"#;

    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    assert!(
        diagnostics.iter().all(|diag| diag.code != 2344),
        "tuple-rest infer result should satisfy Array<unknown> and not emit TS2344. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        diagnostics
            .iter()
            .all(|diag| diag.code != 2339 && diag.code != 2536),
        "array-like length access should not cascade into TS2339/TS2536. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        diagnostics.iter().any(|diag| {
            diag.code == 2322
                && diag
                    .message_text
                    .contains("Type 'number' is not assignable to type 'Enumerate<T[\"length\"]>'")
        }),
        "generic tuple length return should preserve Enumerate<T[\"length\"]> in TS2322. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn type_challenges_json_parser_alias_result_satisfies_array_constraint() {
    let source = r#"
type Token = any;
type ParseResult<Value, Rest extends Token[]> = [Value, Rest];
type Tokenize<Input extends string, State extends Token[] = []> = Token[];
type ParseLiteral<Rest extends Token[]> = ParseResult<any, Rest>;

type Parse<Input extends string> = ParseLiteral<Tokenize<Input>>[0];
"#;

    let diagnostics = check_source_strict_with_default_libs(source);
    assert!(
        diagnostics.iter().all(|diag| diag.code != 2344),
        "alias result with a declared array constraint should not emit TS2344. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn type_challenges_json_parser_mapped_wrapper_preserves_alias_array_constraint() {
    let source = r#"
type Pure<T> = {
    [Key in keyof T]: T[Key] extends object ? Pure<T[Key]> : T[Key]
};

type SetProperty<T, Key extends PropertyKey, Value> = {
    [Prop in (keyof T) | Key]: Prop extends Key ? Value : Prop extends keyof T ? T[Prop] : never
};

type Token = any;
type ParseResult<T, K extends Token[]> = [T, K];
type Tokenize<T extends string, S extends Token[] = []> = Token[];
type ParseLiteral<T extends Token[]> = ParseResult<any, T>;

type Parse<T extends string> = Pure<ParseLiteral<Tokenize<T>>[0]>;
"#;

    let diagnostics = check_source_strict_with_default_libs(source);
    assert!(
        diagnostics.iter().all(|diag| diag.code != 2344),
        "mapped wrapper around alias result should preserve the declared array constraint. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn renamed_alias_type_parameter_constraint_satisfies_array_argument_slot() {
    let source = r#"
type Item = any;
type Pair<Head, Tail extends Item[]> = [Head, Tail];
type Produce<Name extends string> = Item[];
type Consume<Queue extends Item[]> = Pair<unknown, Queue>;

type Result<Name extends string> = Consume<Produce<Name>>;
"#;

    let diagnostics = check_source_strict_with_default_libs(source);
    assert!(
        diagnostics.iter().all(|diag| diag.code != 2344),
        "renamed alias result with a declared array constraint should not emit TS2344. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn renamed_generic_tuple_length_return_uses_structural_conditional_indexed_shape() {
    let source = r#"
interface Array<T> {
    length: number;
}

type _PrependNextNum<A extends unknown[]> = A['length'] extends infer T
    ? [T, ...A] extends [...infer X]
        ? X
        : never
    : never;

type _Range<A extends unknown[], N extends number> = N extends A['length']
    ? A
    : _Range<_PrependNextNum<A>, N> & number;

type CountFromLength<N extends number> = number extends N
    ? number
    : _Range<[], N> extends (infer E)[]
    ? E
    : never;

function foo2<T extends unknown[]>(value: T): CountFromLength<T['length']> {
    return value.length;
}
"#;

    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    assert!(
        diagnostics.iter().any(|diag| {
            diag.code == 2322
                && diag.message_text.contains(
                    "Type 'number' is not assignable to type 'CountFromLength<T[\"length\"]>'",
                )
        }),
        "generic tuple length return should be reported from conditional/indexed-access shape, not an alias spelling. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn recursive_tuple_spread_length_index_access_is_valid() {
    let source = r#"
type NTuple<N extends number, Tup extends unknown[] = []> =
    Tup['length'] extends N ? Tup : NTuple<N, [...Tup, unknown]>;

type Add<A extends number, B extends number> =
    [...NTuple<A>, ...NTuple<B>]['length'];

let five: Add<2, 3>;
"#;

    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    assert!(
        diagnostics.iter().all(|diag| diag.code != 2536),
        "tuple spread length indexed access should not emit TS2536. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn structurally_identical_recursive_conditionals_are_assignable() {
    let source = r#"
type Unpack1<T> = T extends (infer U)[] ? Unpack1<U> : T;
type Unpack2<T> = T extends (infer U)[] ? Unpack2<U> : T;

function f20<T>(x: Unpack1<T>, y: Unpack2<T>) {
    x = y;
    y = x;
}
"#;

    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    assert!(
        diagnostics.iter().all(|diag| diag.code != 2322),
        "structurally identical recursive conditional aliases should be mutually assignable. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn recursive_tuple_alias_assignment_reports_both_directions() {
    let source = r#"
type TupleOf<T, N extends number> =
    N extends N ? number extends N ? T[] : _TupleOf<T, N, []> : never;
type _TupleOf<T, N extends number, R extends unknown[]> =
    R['length'] extends N ? R : _TupleOf<T, N, [T, ...R]>;

function f22<N extends number, M extends N>(tn: TupleOf<number, N>, tm: TupleOf<number, M>) {
    tn = tm;
    tm = tn;
}
"#;

    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    assert!(
        diagnostics.iter().any(|diag| {
            diag.code == 2322
                && diag.message_text.contains(
                    "Type 'TupleOf<number, M>' is not assignable to type 'TupleOf<number, N>'",
                )
        }),
        "expected TupleOf<number, M> to TupleOf<number, N> assignment error. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        diagnostics.iter().any(|diag| {
            diag.code == 2322
                && diag.message_text.contains(
                    "Type 'TupleOf<number, N>' is not assignable to type 'TupleOf<number, M>'",
                )
        }),
        "expected TupleOf<number, N> to TupleOf<number, M> assignment error. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn interface_tupleof_assignment_uses_constraint_directionality() {
    let source = r#"
interface TupleOf<T, N extends number> {
    value: T;
    size: N;
}

function f22<N extends number, M extends N>(tn: TupleOf<number, N>, tm: TupleOf<number, M>) {
    tn = tm;
    tm = tn;
}
"#;

    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    let ts2322 = diagnostics
        .iter()
        .filter(|diag| diag.code == 2322)
        .collect::<Vec<_>>();

    assert_eq!(
        ts2322.len(),
        1,
        "interface TupleOf should only reject the N -> M assignment direction, got: {diagnostics:#?}"
    );
    assert!(
        ts2322[0]
            .message_text
            .contains("Type 'TupleOf<number, N>' is not assignable to type 'TupleOf<number, M>'"),
        "expected only the N -> M assignment error for interface TupleOf, got: {diagnostics:#?}"
    );
}

#[test]
fn recursive_conditional_call_parameter_keeps_alias_display() {
    let source = r#"
type Grow1<T extends unknown[], N extends number> =
    T['length'] extends N ? T : Grow1<[number, ...T], N>;
type Grow2<T extends unknown[], N extends number> =
    T['length'] extends N ? T : Grow2<[string, ...T], N>;

function f21<T extends number>(x: Grow1<[], T>, y: Grow2<[], T>) {
    f21(y, x);
}
"#;

    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    assert!(
        diagnostics.iter().any(|diag| {
            diag.code == 2345
                && diag
                    .message_text
                    .contains("parameter of type 'Grow1<[], T>'")
        }),
        "recursive conditional parameter diagnostics should preserve the alias target. Actual diagnostics: {diagnostics:#?}"
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

#[test]
fn test_distributive_conditional_identity_accepts_type_parameter_source() {
    let source = r#"
type ExtractWithDefault<T, U, D = never> = T extends U ? T : D;
type TemplatedConditional<TCheck, TExtends, TTrue, TFalse> =
    TCheck extends TExtends ? TTrue : TFalse;

function extractBuiltin<T>(x: Extract<T, T>) {
    const y: T = x;
    x = y;
}

function extractWithDefault<T>(x: ExtractWithDefault<T, T>) {
    const y: T = x;
    x = y;
}

function templated<T>(x: TemplatedConditional<T, T, T, never>) {
    const y: T = x;
    x = y;
}
"#;
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    let ts2322_errors: Vec<&Diagnostic> = diagnostics.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322_errors.len(),
        0,
        "`T extends T ? T : never` aliases must simplify to T in target position; got diagnostics: {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn conditional_keyof_pick_identity_assignable_to_type_parameter() {
    let source = r#"
const fn1 = <Params>(
    params: Pick<Params, Exclude<keyof Params, never>>,
): Params => params;

type ExtractWithDefault<T, U, D = never> = T extends U ? T : D;
type ExcludeWithDefault<T, U, D = never> = T extends U ? D : T;
type TemplatedConditional<TCheck, TExtends, TTrue, TFalse> =
    TCheck extends TExtends ? TTrue : TFalse;

const fn3 = <Params>(
    params: Pick<Params, Extract<keyof Params, keyof Params>>,
): Params => params;

const fn5 = <Params>(
    params: Pick<Params, ExcludeWithDefault<keyof Params, never>>,
): Params => params;

const fn7 = <Params>(
    params: Pick<Params, ExtractWithDefault<keyof Params, keyof Params>>,
): Params => params;

const fn9 = <Params>(
    params: Pick<Params, TemplatedConditional<keyof Params, never, never, keyof Params>>,
): Params => params;

const fn11 = <Params>(
    params: Pick<Params, TemplatedConditional<keyof Params, keyof Params, keyof Params, never>>,
): Params => params;
"#;
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "keyof Pick identity conditionals should be assignable to the original type parameter with no extra diagnostics; got diagnostics: {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn conditional_keyof_variance_assignability_matches_tsc() {
    let source = r#"
interface Covariant<T> {
    foo: T extends string ? T : number;
}

interface Contravariant<T> {
    foo: T extends string ? keyof T : number;
}

interface Invariant<T> {
    foo: T extends string ? keyof T : T;
}

interface CovariantFalse<T> {
    foo: T extends string ? number : T;
}

interface ContravariantFalse<T> {
    foo: T extends string ? number : keyof T;
}

interface InvariantFalse<T> {
    foo: T extends string ? T : keyof T;
}

function f1<A, B extends A>(a: Covariant<A>, b: Covariant<B>) {
    a = b;
    b = a;  // Error
}

function f2<A, B extends A>(a: Contravariant<A>, b: Contravariant<B>) {
    a = b;  // Error
    b = a;
}

function f3<A, B extends A>(a: Invariant<A>, b: Invariant<B>) {
    a = b;  // Error
    b = a;  // Error
}

function f4<A, B extends A>(a: CovariantFalse<A>, b: CovariantFalse<B>) {
    a = b;
    b = a;  // Error
}

function f5<A, B extends A>(a: ContravariantFalse<A>, b: ContravariantFalse<B>) {
    a = b;  // Error
    b = a;
}

function f6<A, B extends A>(a: InvariantFalse<A>, b: InvariantFalse<B>) {
    a = b;  // Error
    b = a;  // Error
}
"#;
    let diagnostics = tsz_checker::test_utils::check_source_strict(source);
    let ts2322_errors: Vec<&Diagnostic> = diagnostics.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322_errors.len(),
        8,
        "conditional keyof wrapper variance should produce only the tsc-expected assignments; all diagnostics: {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        diagnostics.len(),
        8,
        "conditional keyof variance fixture should not emit extra non-TS2322 diagnostics: {:?}",
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
fn test_conditional_infer_matches_explicit_this_parameters() {
    let source = r#"
type MyThis<T> = T extends (this: infer U, ...args: any[]) => any ? U : never;
type MyOmitThis<T> = T extends (this: any, ...args: infer A) => infer R ? (...args: A) => R : T;

type FnType = (this: { x: number }, y: string) => boolean;

type ThisFromFnType = MyThis<FnType>;
let badFromFnType: ThisFromFnType = { x: "wrong" };

type NoThisFromFnType = MyOmitThis<FnType>;
declare const noThisFromFnType: NoThisFromFnType;
let boolFromFnType: boolean = noThisFromFnType("ok");
noThisFromFnType.call({ x: 1 }, "ok");

function withThis(this: { x: number }, y: string): boolean {
  return this.x > y.length;
}

type ThisParameterType<T> = T extends (this: infer U, ...args: any[]) => any ? U : unknown;
type OmitThisParameter<T> = T extends (this: any, ...args: infer A) => infer R ? (...args: A) => R : T;

type BuiltinThis = ThisParameterType<typeof withThis>;
let badBuiltinThis: BuiltinThis = { x: "wrong" };

type BuiltinNoThis = OmitThisParameter<typeof withThis>;
declare const builtinNoThis: BuiltinNoThis;
let boolFromBuiltin: boolean = builtinNoThis("ok");
builtinNoThis.call({ x: 1 }, "ok");

type FirstParam<T> = T extends (arg: infer A) => any ? A : never;
let badFirstParam: FirstParam<(arg: string) => void> = 123;
"#;
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    let ts2322_count = codes.iter().filter(|&&code| code == 2322).count();
    assert_eq!(
        ts2322_count,
        3,
        "Expected two inferred-this assignment errors plus ordinary parameter control, got diagnostics: {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
    assert!(
        !codes.contains(&2684),
        "Omitted-this function types should not retain the original this parameter, got diagnostics: {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
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
fn test_build_tree_terminal_property_receiver_displays_evaluated_leaf_type() {
    let element_source = r#"
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
declare const grandUser: GrandUser;
grandUser.children[0].children[0].children[0];
"#;
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(element_source);
    let ts2339: Vec<_> = diagnostics.iter().filter(|d| d.code == 2339).collect();
    assert_eq!(ts2339.len(), 1, "Expected one TS2339, got: {diagnostics:?}");

    let message = &ts2339[0].message_text;
    assert!(
        message.contains("type 'User'"),
        "terminal recursive conditional receiver should display the evaluated leaf type, got: {message:?}"
    );
    assert!(
        !message.contains("BuildTree<"),
        "property receiver display should not preserve the recursive helper alias at the terminal leaf, got: {message:?}"
    );

    let renamed_element_source = r#"
type Length<T extends any[]> = T["length"];
type PushFront<V, T extends any[]> = ((head: V, ...args: T) => void) extends (
  ...args: infer R
) => void
  ? R
  : any;

type TreeAt<T, N extends number = -1, I extends any[] = []> = {
  1: T;
  0: T & { kids: TreeAt<T, N, PushFront<any, I>>[] };
}[Length<I> extends N ? 1 : 0];

interface Person {
  id: string;
}

type Family = TreeAt<Person, 1>;
declare const family: Family;
family.kids[0].kids[0];
"#;
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(renamed_element_source);
    let ts2339: Vec<_> = diagnostics.iter().filter(|d| d.code == 2339).collect();
    assert_eq!(ts2339.len(), 1, "Expected one TS2339, got: {diagnostics:?}");

    let message = &ts2339[0].message_text;
    assert!(
        message.contains("type 'Person'"),
        "renamed terminal recursive conditional receiver should display the evaluated leaf type, got: {message:?}"
    );
    assert!(
        !message.contains("TreeAt<"),
        "renamed property receiver display should not preserve the recursive helper alias at the terminal leaf, got: {message:?}"
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

#[test]
fn test_infer_inside_type_predicate_target_resolves_in_true_branch() {
    // `target is infer X` exposes `X` to the conditional's true branch the
    // same way return-position `infer X` does.
    let source = r#"
type X<F> = F extends (x: any) => x is infer N ? N : never;
type Test = X<(x: any) => x is string>;
"#;
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "Expected `infer` inside type predicate to bind in conditional true branch, got: {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_infer_inside_asserts_predicate_resolves_in_true_branch() {
    let source = r#"
type AssertedType<F> = F extends (target: any) => asserts target is infer N ? N : never;
type Test = AssertedType<(target: any) => asserts target is number>;
"#;
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "Expected `infer` inside asserts predicate to bind, got: {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

// Tests for fix: try_expand_application must evaluate its instantiated result
// so that distributive conditionals (K extends K ? [K, ...] : never) are resolved
// to concrete union-of-tuples before the structural subtype check proceeds.
//
// Structural rule: "When an Application type's expanded body contains conditional
// types from distributive instantiation over a union type parameter, the expanded
// result must be fully evaluated before subtype comparison."

#[test]
fn test_permutation_type_with_default_param_and_distribution() {
    // Case 1 (reported repro): T/K naming, numeric literals
    let source = r#"
type MyExclude<T, U> = T extends U ? never : T;

type Permutation<T, K = T> =
  [T] extends [never]
    ? []
    : K extends K
      ? [K, ...Permutation<MyExclude<T, K>>]
      : never;

type P = Permutation<1 | 2>;

const p1: P = [1, 2];
const p2: P = [2, 1];
"#;
    let diags = tsz_checker::test_utils::check_source_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Expected no errors for Permutation<1|2> assignments, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_permutation_type_renamed_params() {
    // Case 2: same rule, different type parameter names (A/B instead of T/K)
    // proves the fix is structural, not dependent on parameter name spelling
    let source = r#"
type Rem<A, B> = A extends B ? never : A;

type Perm<A, B = A> =
  [A] extends [never]
    ? []
    : B extends B
      ? [B, ...Perm<Rem<A, B>>]
      : never;

type Q = Perm<"x" | "y">;

const q1: Q = ["x", "y"];
const q2: Q = ["y", "x"];
"#;
    let diags = tsz_checker::test_utils::check_source_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Expected no errors for Perm<'x'|'y'> with renamed params, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_permutation_type_with_builtin_exclude() {
    let source = r#"
type Permutation<T, K = T> =
  [T] extends [never]
    ? []
    : K extends K
      ? [K, ...Permutation<Exclude<T, K>>]
      : never;

type P = Permutation<"A" | "B">;

const p1: P = ["A", "B"];
const p2: P = ["B", "A"];
"#;
    let diags = check_source_strict_with_default_libs(source);
    assert!(
        diags.is_empty(),
        "Expected no errors for Permutation<'A'|'B'> through built-in Exclude, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_permutation_type_three_element_union() {
    // Case 3: larger union (3 members) — ensures distribution over >2 members works
    let source = r#"
type MyExclude<T, U> = T extends U ? never : T;

type Permutation<T, K = T> =
  [T] extends [never]
    ? []
    : K extends K
      ? [K, ...Permutation<MyExclude<T, K>>]
      : never;

type P3 = Permutation<1 | 2 | 3>;

const pa: P3 = [1, 2, 3];
const pb: P3 = [2, 1, 3];
const pc: P3 = [3, 1, 2];
"#;
    let diags = tsz_checker::test_utils::check_source_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Expected no errors for Permutation<1|2|3> assignments, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_permutation_type_invalid_assignment_still_rejected() {
    // Case 4 (negative): invalid value must still be rejected — the fix must not
    // loosen type safety for non-permutation tuples
    let source = r#"
type MyExclude<T, U> = T extends U ? never : T;

type Permutation<T, K = T> =
  [T] extends [never]
    ? []
    : K extends K
      ? [K, ...Permutation<MyExclude<T, K>>]
      : never;

type P = Permutation<1 | 2>;

const bad: P = [3, 1];
"#;
    let diags = tsz_checker::test_utils::check_source_diagnostics(source);
    assert!(
        diags.iter().any(|d| d.code == 2322),
        "Expected TS2322 for invalid permutation [3, 1]: P, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

// Rule: when `infer R` matches a generic function's return type, free type parameters
// must be erased to their upper bounds (constraint or `unknown`) before `R` is bound.

const GET_RET_DEF: &str = "type GetRet<F> = F extends (...args: any[]) => infer R ? R : never;\n";

#[test]
fn return_type_of_unconstrained_generic_fn_erases_to_unknown() {
    let source = format!(
        r#"{GET_RET_DEF}
function generic<T>(x: T): T[] {{
    return [x];
}}
type GR = GetRet<typeof generic>;
const gr: GR = ["test"];
"#
    );
    let diags = tsz_checker::test_utils::check_source_diagnostics(&source);
    assert!(
        !diags.iter().any(|d| d.code == 2322),
        "GetRet of generic<T>: T[] should erase T to unknown, making string[] assignable. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn return_type_of_unconstrained_generic_fn_different_param_name() {
    // Same rule, different type-parameter name (U instead of T) — proves the fix
    // is not hardcoded to any specific name.
    let source = format!(
        r#"{GET_RET_DEF}
function wrap<U>(val: U): U[] {{
    return [val];
}}
type WR = GetRet<typeof wrap>;
const wr: WR = [42];
"#
    );
    let diags = tsz_checker::test_utils::check_source_diagnostics(&source);
    assert!(
        !diags.iter().any(|d| d.code == 2322),
        "GetRet of wrap<U>: U[] should erase U to unknown. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn return_type_of_constrained_generic_fn_erases_to_constraint() {
    let source = format!(
        r#"{GET_RET_DEF}
function constrained<T extends string>(x: T): T[] {{
    return [x];
}}
type CR = GetRet<typeof constrained>;
const cr: CR = ["hello"];
"#
    );
    let diags = tsz_checker::test_utils::check_source_diagnostics(&source);
    assert!(
        !diags.iter().any(|d| d.code == 2322),
        "GetRet of constrained<T extends string> erases T to string. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn return_infer_only_pattern_erases_single_type_param() {
    let source = format!(
        r#"{GET_RET_DEF}
function identity<K>(x: K): K {{
    return x;
}}
type IR = GetRet<typeof identity>;
const accepted: IR = "any value";
const accepted2: IR = 42;
"#
    );
    let diags = tsz_checker::test_utils::check_source_diagnostics(&source);
    assert!(
        !diags.iter().any(|d| d.code == 2322),
        "GetRet of identity<K>: K should produce unknown. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn return_type_of_multi_param_generic_fn_erases_all_type_params() {
    let source = format!(
        r#"{GET_RET_DEF}
function pair<A, B>(a: A, b: B): [A, B] {{
    return [a, b];
}}
type PR = GetRet<typeof pair>;
const pr: PR = ["x", 1];
"#
    );
    let diags = tsz_checker::test_utils::check_source_diagnostics(&source);
    assert!(
        !diags.iter().any(|d| d.code == 2322),
        "GetRet of pair<A,B> should erase both to unknown. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn return_type_of_non_generic_fn_still_precise() {
    let source = format!(
        r#"{GET_RET_DEF}
function nums(): number[] {{
    return [1, 2, 3];
}}
type NR = GetRet<typeof nums>;
const bad: NR = ["oops"];
"#
    );
    let diags = tsz_checker::test_utils::check_source_diagnostics(&source);
    assert!(
        diags.iter().any(|d| d.code == 2322),
        "GetRet of non-generic nums(): number[] should stay number[]. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn infer_first_param_with_unknown_rest_resolves_to_string() {
    // issue #6253: fixed source params must match the element type of a non-infer rest param
    let source = r#"
type FirstArg<T> = T extends (x: infer A, ...args: unknown[]) => unknown ? A : never;
type A1 = FirstArg<(a: string, b: number) => void>;
const a1: A1 = "test";
const bad: A1 = 42;
"#;
    let diags = tsz_checker::test_utils::check_source_diagnostics(source);
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "Only `bad: A1 = 42` should error (A1 = string). Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn infer_first_param_different_names_resolves_correctly() {
    // different type-param and rest-param names — fix must be structural, not name-keyed
    let source = r#"
type FirstArg<T> = T extends (first: infer S, ...rest: unknown[]) => unknown ? S : never;
type F1 = FirstArg<(x: number, y: string) => void>;
const ok: F1 = 1;
const bad: F1 = "nope";
"#;
    let diags = tsz_checker::test_utils::check_source_diagnostics(source);
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "Only `bad: F1 = \"nope\"` should error (F1 = number). Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn infer_first_param_multiple_extra_fixed_params() {
    let source = r#"
type FirstArg<T> = T extends (x: infer A, ...args: unknown[]) => unknown ? A : never;
type F3 = FirstArg<(a: string, b: number, c: boolean) => void>;
const ok: F3 = "hi";
const bad: F3 = 1;
"#;
    let diags = tsz_checker::test_utils::check_source_diagnostics(source);
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "Only numeric assignment should fail (F3 = string). Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn infer_first_param_rest_elem_constraint_fails_for_incompatible_extra_param() {
    let source = r#"
type FirstArg<T> = T extends (x: infer A, ...args: string[]) => unknown ? A : never;
type F = FirstArg<(a: string, b: object) => void>;
const accepted: F = "x" as never;
"#;
    let diags = tsz_checker::test_utils::check_source_diagnostics(source);
    assert!(
        diags.is_empty(),
        "F should be `never`; assigning `never` is valid (no errors). Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn infer_first_param_with_return_infer_and_unknown_rest() {
    let source = r#"
type FirstArgAndRet<T> =
  T extends (x: infer A, ...args: unknown[]) => infer R ? [A, R] : never;
type FR = FirstArgAndRet<(a: string, b: number) => boolean>;
const ok: FR = ["hi", true];
const bad: FR = [1, true];
"#;
    let diags = tsz_checker::test_utils::check_source_diagnostics(source);
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "FR = [string, boolean]; only `[1, true]` should fail. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn infer_first_param_source_rest_param_against_unknown_rest_pattern() {
    // source rest param compared array-to-array (not array vs element) against pattern rest
    let source = r#"
type FirstArg<T> = T extends (x: infer A, ...args: unknown[]) => unknown ? A : never;
type FR = FirstArg<(a: string, ...rest: number[]) => void>;
const ok: FR = "hi";
const bad: FR = 99;
"#;
    let diags = tsz_checker::test_utils::check_source_diagnostics(source);
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "FR = string; only numeric assignment should fail. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn infer_first_param_source_with_no_extra_params() {
    let source = r#"
type FirstArg<T> = T extends (x: infer A, ...args: unknown[]) => unknown ? A : never;
type F0 = FirstArg<(a: number) => void>;
const ok: F0 = 1;
const bad: F0 = "nope";
"#;
    let diags = tsz_checker::test_utils::check_source_diagnostics(source);
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "F0 = number; only string assignment should fail. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

// ── Tuple rest/last infer spread flattening (issue #6671) ──────────────────

/// `[...infer R, infer L]` captures R as a tuple. The true branch `[L, ...R]`
/// must spread R's elements into the result, not wrap them in a nested tuple.
#[test]
fn test_rotate_right_tuple_infer_rest_last_spreads_correctly() {
    let source = r#"
type RotateRight<T extends unknown[]> =
  T extends [...infer R, infer L] ? [L, ...R] : T;

type RR1 = RotateRight<[1, 2, 3]>;
type RR2 = RotateRight<[string, number]>;

const ok1: RR1 = [3, 1, 2];
const ok2: RR2 = [42, "hello"];
"#;
    let diags = check_source_strict_with_default_libs(source);
    assert!(
        diags.is_empty(),
        "RotateRight should produce no errors. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

/// `RotateRight` should reject assignments that don't match the rotated type.
#[test]
fn test_rotate_right_rejects_wrong_assignment() {
    let source = r#"
type RotateRight<T extends unknown[]> =
  T extends [...infer R, infer L] ? [L, ...R] : T;

type RR1 = RotateRight<[1, 2, 3]>;
const bad: RR1 = [1, 2, 3];
"#;
    let diags = check_source_strict_with_default_libs(source);
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        !ts2322.is_empty(),
        "Assigning [1,2,3] to RotateRight<[1,2,3]>=[3,1,2] should error."
    );
}

/// `RotateLeft` `[infer F, ...infer R]` followed by `[...R, F]` must also spread R.
#[test]
fn test_rotate_left_tuple_infer_first_rest_spreads_correctly() {
    let source = r#"
type RotateLeft<T extends unknown[]> =
  T extends [infer F, ...infer R] ? [...R, F] : T;

type RL1 = RotateLeft<[1, 2, 3]>;
type RL2 = RotateLeft<[string, number, boolean]>;

const ok1: RL1 = [2, 3, 1];
const ok2: RL2 = [42, true, "hello"];
"#;
    let diags = check_source_strict_with_default_libs(source);
    assert!(
        diags.is_empty(),
        "RotateLeft should produce no errors. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

/// `Reverse<T>` uses `[...infer Init, infer Last]` recursively; every recursive
/// step must spread the init portion correctly.
#[test]
fn test_recursive_reverse_tuple_spreads_correctly() {
    let source = r#"
type Reverse<T extends unknown[]> =
  T extends [...infer Init, infer Last] ? [Last, ...Reverse<Init>] : T;

type Rev1 = Reverse<[1, 2, 3]>;
type Rev2 = Reverse<[string, number]>;

const ok1: Rev1 = [3, 2, 1];
const ok2: Rev2 = [42, "hello"];
"#;
    let diags = check_source_strict_with_default_libs(source);
    assert!(
        diags.is_empty(),
        "Reverse should produce no errors. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

// ============================================================================
// Tests for issue #6657: infer bindings in complex extends clauses
//
// Structural rule: when the extends clause of a conditional type contains
// `infer X` (regardless of surrounding expression complexity), references to
// `X` in the true/false branches must resolve without TS2304.
// ============================================================================

fn no_ts2304(diags: &[tsz_checker::diagnostics::Diagnostic], ctx: &str) {
    let errors: Vec<_> = diags.iter().filter(|d| d.code == 2304).collect();
    assert!(
        errors.is_empty(),
        "{ctx}: expected no TS2304, got: {:?}",
        errors
            .iter()
            .map(|d| d.message_text.clone())
            .collect::<Vec<_>>()
    );
}

fn ts2304_count(diags: &[tsz_checker::diagnostics::Diagnostic]) -> usize {
    diags.iter().filter(|d| d.code == 2304).count()
}

/// Utility type in check position: `Pick<T, K> extends infer R`
/// — `R` must be visible inside the true branch mapped type.
#[test]
fn infer_binding_visible_when_check_type_is_utility_application() {
    let source = r#"
type Flatten<T, K extends keyof T> =
    Pick<T, K> extends infer R ? { [P in keyof R]: R[P] } : never;
interface Obj { a: string; b: number }
type T1 = Flatten<Obj, "a">;
"#;
    let diags = check_source_strict_with_default_libs(source);
    no_ts2304(&diags, "Pick<T,K> extends infer R");
}

/// Intersection in check position: `T & U extends infer V`
/// — `V` must be visible in the true branch.
#[test]
fn infer_binding_visible_when_check_type_is_intersection() {
    let source = r#"
type Merge<A, B> =
    A & B extends infer V ? { [K in keyof V]: V[K] } : never;
type T2 = Merge<{ x: number }, { y: string }>;
"#;
    let diags = check_source_strict_with_default_libs(source);
    no_ts2304(&diags, "A & B extends infer V");
}

/// Real-world `RequiredByKeys` pattern with Omit & Required<Pick>.
#[test]
fn infer_binding_visible_in_required_by_keys_pattern() {
    let source = r#"
type RequiredByKeys<T, K extends keyof T = keyof T> =
    Omit<T, K> & Required<Pick<T, K>> extends infer X
        ? { [P in keyof X]: X[P] }
        : never;
interface User { name?: string; age?: number }
type T3 = RequiredByKeys<User, "name">;
"#;
    let diags = check_source_strict_with_default_libs(source);
    no_ts2304(&diags, "Omit<T,K> & Required<Pick<T,K>> extends infer X");
}

/// Multiple infer bindings in the extends clause.
/// All bound names must be visible in the branches.
#[test]
fn multiple_infer_bindings_all_visible_in_branches() {
    let source = r#"
type Unpack<T> =
    T extends { first: infer A; second: infer B }
        ? { [K in keyof A | keyof B]: K extends keyof A ? A[K] : B[K extends keyof B ? K : never] }
        : never;
"#;
    let diags = check_source_strict_with_default_libs(source);
    no_ts2304(&diags, "multiple infer A, B");
}

/// Renamed infer variable (Q instead of R) — proves the fix is not
/// tied to any specific identifier spelling.
#[test]
fn infer_binding_works_with_any_variable_name() {
    let source = r#"
type Spread<T, K extends keyof T> =
    Pick<T, K> extends infer Q ? { [P in keyof Q]: Q[P] } : never;
interface Data { x: number; y: string }
type T4 = Spread<Data, "x">;
"#;
    let diags = check_source_strict_with_default_libs(source);
    no_ts2304(&diags, "Pick<T,K> extends infer Q (renamed variable)");
}

/// The simple case (check type is a direct type parameter) must still work:
/// no regression for `T extends infer R`.
#[test]
fn infer_binding_simple_type_param_check_no_regression() {
    let source = r#"
type Identity<T> = T extends infer R ? { [P in keyof R]: R[P] } : never;
type T5 = Identity<{ a: string }>;
"#;
    let diags = check_source_strict_with_default_libs(source);
    no_ts2304(&diags, "T extends infer R (simple case regression)");
}

/// Infer bindings are scoped to the true branch only.
/// The false branch must still report unknown-name errors for `R`.
#[test]
fn infer_binding_not_visible_in_false_branch() {
    let source = r#"
type Bad<T> =
    T extends { a: infer R }
        ? string
        : { [K in keyof R]: R[K] };
"#;
    let diags = check_source_strict_with_default_libs(source);
    assert_eq!(
        ts2304_count(&diags),
        2,
        "false branch infer references should remain unbound. Actual diagnostics: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn infer_extends_constraint_on_tuple_length() {
    let source = r#"
type GetLength<T> = T extends { length: infer L extends number } ? L : never;

type Len1 = GetLength<[1, 2, 3]>;
const l1: Len1 = 3;
const l1bad: Len1 = 4;

type LenArr = GetLength<number[]>;
const larr: LenArr = 42;

type LenObj = GetLength<{ length: 5 }>;
const lobj: LenObj = 5;
const lobjbad: LenObj = 6;
"#;
    let diagnostics = tsz_checker::test_utils::check_source_strict(source);
    let codes: Vec<_> = diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.as_str()))
        .collect();
    assert_eq!(
        diagnostics.iter().filter(|d| d.code == 2322).count(),
        2,
        "Expected exactly 2 TS2322 errors (l1bad and lobjbad); got: {codes:?}"
    );
}

#[test]
fn infer_extends_constraint_on_string_literal_length() {
    // String types have `length: number` from the String interface.
    // tsc infers `number` (not a literal count) for string literal length.
    // The bug: tsz returned `never` because string types weren't handled in
    // the conditional infer property resolver.
    let source = r#"
type GetStringLength<T> = T extends { length: infer L extends number } ? L : never;

type L1 = GetStringLength<"hi">;
type L2 = GetStringLength<"hello">;
type L3 = GetStringLength<"">;
type L4 = GetStringLength<string>;

const l1: L1 = 2;
const l2: L2 = 5;
const l3: L3 = 0;
const l4: L4 = 42;

const l1bad: L1 = "oops";
const l4bad: L4 = "also_oops";
"#;
    let diagnostics = tsz_checker::test_utils::check_source_strict(source);
    let codes: Vec<_> = diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.as_str()))
        .collect();
    assert_eq!(
        diagnostics.iter().filter(|d| d.code == 2322).count(),
        2,
        "Expected exactly 2 TS2322 errors (l1bad and l4bad — string to number); got: {codes:?}"
    );
}

#[test]
fn infer_extends_constraint_on_string_length_with_infer_name_variants() {
    // The fix must work regardless of the infer variable name.
    // Structural rule: string sources match { length: infer X extends number }
    // for any bound name X or Y.
    let source_x = r#"
type GetLen<T> = T extends { length: infer X extends number } ? X : never;
type R1 = GetLen<"abc">;
const ok: R1 = 3;
const bad: R1 = "nope";
"#;
    let source_y = r#"
type GetLen<T> = T extends { length: infer Y extends number } ? Y : never;
type R1 = GetLen<"abc">;
const ok: R1 = 3;
const bad: R1 = "nope";
"#;
    for (name, src) in [("X-bound", source_x), ("Y-bound", source_y)] {
        let diagnostics = tsz_checker::test_utils::check_source_strict(src);
        let ts2322: Vec<_> = diagnostics.iter().filter(|d| d.code == 2322).collect();
        assert_eq!(
            ts2322.len(),
            1,
            "{name}: expected exactly 1 TS2322 (string-to-number); got: {:?}",
            diagnostics
                .iter()
                .map(|d| (d.code, d.message_text.as_str()))
                .collect::<Vec<_>>()
        );
    }
}

// =============================================================================
// IsUnion<T> supplement — cases not covered by distributive_conditional_default_tests.rs
// =============================================================================

const IS_UNION_PRELUDE: &str = r#"
type IsUnion<T, U = T> = T extends U
  ? [U] extends [T]
    ? false
    : true
  : never;
"#;

#[test]
fn is_union_of_primitives_evaluates_to_true() {
    assert_no_ts2322(
        &format!(
            "{IS_UNION_PRELUDE}\n\
            type R = IsUnion<string | number>;\n\
            const r: R = true;\n"
        ),
        "IsUnion<string | number> = true",
    );
}

#[test]
fn is_union_diagnostic_shows_evaluated_literal_not_alias() {
    let source = format!(
        "{IS_UNION_PRELUDE}\n\
        type R = IsUnion<\"a\" | \"b\">;\n\
        const r: R = false;\n"
    );
    let diags = tsz_checker::test_utils::check_source_strict(&source);
    let msgs = tsz_checker::test_utils::diagnostic_messages_with_code(&diags, 2322);
    assert_eq!(
        msgs.len(),
        1,
        "Expected exactly one TS2322; got: {diags:#?}"
    );
    let msg = msgs[0];
    assert!(
        msg.contains("'false'") && msg.contains("'true'"),
        "Expected evaluated literal types in message; got: {msg:?}"
    );
    assert!(
        !msg.contains("IsUnion"),
        "Diagnostic must not show unevaluated alias 'IsUnion'; got: {msg:?}"
    );
}

// =============================================================================
// Constructor return type infer — typeof Class patterns (issue #6157)
// Rule: when a constructor pattern `new (...) => infer I` checks a `typeof C`
// expression, the check type must be fully evaluated from the TypeQuery before
// pattern matching, and construct signatures must be selected (not call signatures)
// when the source Callable carries both.
// =============================================================================

fn assert_no_ts2322_with_libs(source: &str, label: &str) {
    let diags = check_source_strict_with_default_libs(source);
    let errors: Vec<&Diagnostic> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        errors.is_empty(),
        "[{label}] expected no TS2322, got:\n{:#?}",
        diags
            .iter()
            .map(|d| (d.code, d.start, d.message_text.as_str()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn constructor_infer_typeof_date_returns_date_not_never() {
    // `typeof Date` has both call (returns string) and construct (returns Date) sigs.
    // The constructor pattern must use construct signatures → I = Date.
    let source = r#"
type InstanceOf<T> = T extends new (...args: any[]) => infer I ? I : never;
type IO = InstanceOf<typeof Date>;
const io: IO = new Date();
export {};
"#;
    assert_no_ts2322_with_libs(source, "InstanceOf<typeof Date> = Date");
}

#[test]
fn constructor_infer_user_class_without_libs() {
    // User-defined class `typeof Cls` must also resolve correctly.
    // Tests that visit_type_query deep-evaluates Lazy types.
    assert_no_ts2322(
        r#"
class Cls { x: number = 1; }
type InstanceOf<T> = T extends new (...args: any[]) => infer I ? I : never;
type R = InstanceOf<typeof Cls>;
const r: R = new Cls();
export {};
"#,
        "InstanceOf<typeof Cls> = Cls",
    );
}

#[test]
fn constructor_infer_renamed_type_param_k_user_class() {
    // Renamed outer and infer params must not change the result — the fix must be structural.
    assert_no_ts2322(
        r#"
class Widget { name: string = ""; }
type ConstructedBy<K> = K extends new (...args: any[]) => infer Result ? Result : never;
type W = ConstructedBy<typeof Widget>;
const w: W = new Widget();
export {};
"#,
        "ConstructedBy<typeof Widget> = Widget",
    );
}

#[test]
fn constructor_infer_typeof_map_returns_map_not_never() {
    // Map also has both call and construct sigs — confirms construct-sig selection is general.
    let source = r#"
type InstanceOf<T> = T extends new (...args: any[]) => infer I ? I : never;
type M = InstanceOf<typeof Map>;
const m: M = new Map();
export {};
"#;
    assert_no_ts2322_with_libs(source, "InstanceOf<typeof Map> = Map");
}

#[test]
fn constructor_infer_non_constructable_yields_never() {
    // A plain call-only function type must not match a construct pattern → `never`.
    assert_no_ts2322(
        r#"
type InstanceOf<T> = T extends new (...args: any[]) => infer I ? I : never;
type R = InstanceOf<() => string>;
type IsNever = [R] extends [never] ? true : false;
const check: IsNever = true;
export {};
"#,
        "InstanceOf<() => string> = never",
    );
}

const EQUAL_PRELUDE: &str = r#"type Equal<X, Y> =
  (<T>() => T extends X ? 1 : 2) extends
  (<T>() => T extends Y ? 1 : 2) ? true : false;
"#;

#[test]
fn equal_any_then_is_union_no_cross_contamination() {
    // Equal<any, X> evaluations must not corrupt subsequent IsUnion evaluations.
    assert_no_ts2322(
        &format!(
            "{EQUAL_PRELUDE}\n\
            {IS_UNION_PRELUDE}\n\
            type E1 = Equal<any, string>;  const e1: E1 = false;\n\
            type E2 = Equal<any, number>;  const e2: E2 = false;\n\
            type E3 = Equal<string, any>;  const e3: E3 = false;\n\
            type U1 = IsUnion<\"a\" | \"b\">; const u1: U1 = true;\n\
            type F1 = IsUnion<string>;     const f1: F1 = false;\n\
            type U2 = IsUnion<1 | 2>;      const u2: U2 = true;\n\
            type F2 = IsUnion<number>;     const f2: F2 = false;\n\
            type E4 = Equal<string, string>; const e4: E4 = true;\n\
            type E5 = Equal<{{a: 1}}, {{a: 1}}>; const e5: E5 = true;\n\
            export {{}};\n"
        ),
        "Equal<any,X> then IsUnion cross-contamination",
    );
}
