use tsz_checker::test_utils::check_source_diagnostics;

fn diagnostic_codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|diag| diag.code)
        .collect()
}

#[test]
fn recursive_alias_body_resolution_terminates_without_definition_ts2344() {
    let codes = diagnostic_codes(
        r#"
type Has<U, U1> = [U1] extends [U] ? 1 : 0;
type If<B extends 0 | 1, Then, Else = never> = B extends 1 ? Then : Else;

type ComputeDeep<A, Seen = never> =
  A extends object ? If<Has<Seen, A>, A, {
    [K in keyof A]: ComputeDeep<A[K], A | Seen>
  }> : A;

type Compute<A> = ComputeDeep<A>;
type Use<T extends { value: object }> = Compute<T>;
"#,
    );

    assert!(
        !codes.contains(&2344),
        "recursive alias body lowering should not emit definition-site TS2344: {codes:?}"
    );
}

#[test]
fn object_alias_application_satisfies_object_constraint_after_lazy_readiness() {
    let codes = diagnostic_codes(
        r#"
type Key = string | number | symbol;
type List<A = any> = { readonly [n: number]: A; readonly length: number };
type Pick<T, K extends keyof T> = { [P in K]: T[P] };
type Exclude<U, M> = U extends M ? never : U;

type ObjectOf<O extends List> =
  O extends unknown ? Pick<O, Exclude<keyof O, keyof any[]>> : never;

type ObjectExclude<O extends object, O1 extends object> =
  Pick<O, Exclude<keyof O, keyof O1>>;

type ListOf<O extends object> =
  O extends unknown ? { [K in keyof O]: O[K] } : never;

type ListExclude<L extends List, L1 extends List> =
  ListOf<ObjectExclude<ObjectOf<L>, ObjectOf<L1>>>;
"#,
    );

    assert!(
        !codes.contains(&2344),
        "object-producing alias applications should satisfy object constraints: {codes:?}"
    );
}

#[test]
fn array_constrained_type_parameter_satisfies_readonly_list_alias_constraint() {
    let codes = diagnostic_codes(
        r#"
type List<A = any> = ReadonlyArray<A>;
type Function<P extends List = any, R = any> = (...args: P) => R;
type AwaitedValue<T> = T extends Promise<infer A> ? A : T;

type ComposeListAsync = {
  <R0, R1, R2, P extends any[]>(fns: [
    Function<[AwaitedValue<R1>], R2>,
    Function<[AwaitedValue<R0>], R1>,
    Function<P, R0>,
  ]): Function<P, Promise<AwaitedValue<R2>>>
};
"#,
    );

    assert!(
        !codes.contains(&2344),
        "`P extends any[]` should satisfy `Function<P extends List>`: {codes:?}"
    );
}

#[test]
fn indexed_access_alias_result_satisfies_tuple_alias_constraint() {
    let codes = diagnostic_codes(
        r#"
type Iteration = [
  value: number,
  sign: '-' | '0' | '+',
  prev: keyof IterationMap,
  next: keyof IterationMap,
  oppo: keyof IterationMap,
];

type IterationMap = {
  '__': [number, '-' | '0' | '+', '__', '__', '__'],
  '0': [0, '0', '-1', '1', '0'],
};

type IterationOf<N extends number> =
  `${N}` extends keyof IterationMap ? IterationMap[`${N}`] : IterationMap['__'];

type Step<I extends Iteration = IterationOf<0>> = I;
"#,
    );

    assert!(
        !codes.contains(&2344),
        "`IterationOf<0>` should satisfy the `Iteration` tuple constraint: {codes:?}"
    );
}

#[test]
fn recursive_defaulted_iteration_alias_accepts_iterationof_default() {
    let codes = diagnostic_codes(
        r#"
type List<A = any> = ReadonlyArray<A>;
type Iteration = [
  value: number,
  sign: '-' | '0' | '+',
  prev: keyof IterationMap,
  next: keyof IterationMap,
  oppo: keyof IterationMap,
];

type IterationMap = {
  '__': [number, '-' | '0' | '+', '__', '__', '__'],
  '0': [0, '0', '__', '1', '0'],
  '1': [1, '+', '0', '__', '-1'],
};

type IterationOf<N extends number> =
  `${N}` extends keyof IterationMap ? IterationMap[`${N}`] : IterationMap['__'];
type Pos<I extends Iteration> = I[0];
type Next<I extends Iteration> = IterationMap[I[3]];
type Tail<L extends List> = L extends readonly [any, ...infer R] ? R : [];
type Extends<A, B> = A extends B ? 1 : 0;

type Walk<
  L1 extends List,
  L2 extends List[],
  LN extends List = [],
  L2D extends List[] = L2,
  I extends Iteration = IterationOf<0>
> = {
  0: Walk<L1, L2, LN, Tail<L2D>, Next<I>>;
  1: LN;
}[Extends<Pos<I>, L1['length']>];
"#,
    );

    assert!(
        !codes.contains(&2344),
        "recursive defaulted iteration aliases should accept `IterationOf<0>`: {codes:?}"
    );
}

#[test]
fn list_constrained_type_parameter_satisfies_object_helper_constraint() {
    let codes = diagnostic_codes(
        r#"
type List<A = any> = ReadonlyArray<A>;
type BuiltIn = Function | Error | Date | RegExp;
type Cast<A, B> = A extends B ? A : B;

type ObjectMerge<
  O extends object,
  O1 extends object,
  depth extends 'flat' | 'deep' = 'flat',
  ignore extends object = BuiltIn,
  fill = undefined
> = O & O1;

type Merge<
  L extends List,
  L1 extends List,
  depth extends 'flat' | 'deep' = 'flat',
  ignore extends object = BuiltIn,
  fill = undefined
> = Cast<ObjectMerge<L, L1, depth, ignore, fill>, List>;
"#,
    );

    assert!(
        !codes.contains(&2344),
        "`L extends List` should satisfy object helper constraints: {codes:?}"
    );
}
