//! Regression coverage for defaulted recursive aliases whose recursive branch
//! is detected as excessively deep.
//!
//! Tracks the solver-29-20 issue family. The structural rule is:
//! when a defaulted generic alias has already emitted TS2589 at the recursive
//! alias boundary, later aliases that merely reference the failed instantiation
//! should recover through the error type instead of cascading another TS2589 at
//! the reference site.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{
    check_all_multi_file_with_global_index, check_multi_file_with_global_index,
    check_source_strict, diagnostic_codes, diagnostics_without_codes,
};

fn semantic_codes(source: &str) -> Vec<u32> {
    diagnostic_codes(&diagnostics_without_codes(
        &check_source_strict(source),
        &[2318],
    ))
}

#[test]
fn defaulted_recursive_alias_does_not_cascade_ts2589_to_use_alias() {
    let codes = semantic_codes(
        r#"
type Link29<T, D extends number = 4> = [D] extends [0] ? T : Link29<T[]>;
type Solve29<T> = Link29<T> extends infer U ? U : never;
type A = Solve29<string>;
"#,
    );

    assert_eq!(
        codes,
        vec![2589, 2589],
        "tsc reports the recursive alias boundary and wrapper alias only; got {codes:?}"
    );
}

#[test]
fn defaulted_recursive_alias_wrapper_declaration_reports_ts2589_without_use_site() {
    let codes = semantic_codes(
        r#"
type Link28<T, D extends number = 4> = [D] extends [0] ? T : Link28<T[]>;
type Solve28<T> = Link28<T> extends infer U ? U : never;
"#,
    );

    assert_eq!(
        codes,
        vec![2589, 2589],
        "tsc reports the recursive alias boundary and wrapper alias even without a concrete use; got {codes:?}"
    );
}

#[test]
fn renamed_defaulted_recursive_alias_wrapper_declaration_reports_ts2589_without_use_site() {
    let codes = semantic_codes(
        r#"
type Path<X, N extends number = 4> = [N] extends [0] ? X : Path<X[]>;
type Resolve<X> = Path<X> extends infer Y ? Y : never;
"#,
    );

    assert_eq!(
        codes,
        vec![2589, 2589],
        "renamed aliases and params should report declaration-time TS2589 without a use site; got {codes:?}"
    );
}

#[test]
fn renamed_defaulted_recursive_alias_does_not_cascade_ts2589_to_use_alias() {
    let codes = semantic_codes(
        r#"
type Route<X, N extends number = 4> = [N] extends [0] ? X : Route<X[]>;
type Project<X> = Route<X> extends infer Y ? Y : never;
type Q = Project<number>;
"#,
    );

    assert_eq!(
        codes,
        vec![2589, 2589],
        "renamed aliases and params should follow the same TS2589 recovery rule; got {codes:?}"
    );
}

#[test]
fn direct_recursive_alias_use_still_reports_ts2589() {
    let codes = semantic_codes(
        r#"
type Foo<T extends "true", B> = { "true": Foo<T, Foo<T, B>> }[T];
let f1: Foo<"true", {}>;
"#,
    );

    assert_eq!(
        codes,
        vec![2589],
        "direct recursive alias instantiation still owns its use-site TS2589; got {codes:?}"
    );
}

#[test]
fn ordinary_conditional_tuple_builder_still_reports_depth_at_deep_use() {
    let codes = semantic_codes(
        r#"
type BuildTuple<T, N extends number> = N extends N
  ? number extends N
    ? T[]
    : _BuildTuple<T, N, []>
  : never;
type _BuildTuple<T, N extends number, R extends unknown[]> =
  R["length"] extends N ? R : _BuildTuple<T, N, [T, ...R]>;
type Thousand = BuildTuple<number, 1000>;
"#,
    );

    assert_eq!(
        codes,
        vec![2589],
        "ordinary nonrecursive conditional wrappers should still report deep concrete instantiations; got {codes:?}"
    );
}

#[test]
fn awaited_style_recursive_thenable_reports_depth_at_use() {
    let codes = semantic_codes(
        r#"
interface Chain {
  then(cb: (value: Chain) => void): void;
}
type Settled<T> =
  T extends null | undefined ? T :
  T extends object & { then(onfulfilled: infer F, ...args: infer _): any; } ?
    F extends ((value: infer V, ...args: infer _) => any) ? Settled<V> : never :
  T;
type Result = Settled<Chain>;
"#,
    );

    assert_eq!(
        codes,
        vec![2589],
        "recursive thenable unwrapping owns a use-site TS2589; got {codes:?}"
    );
}

#[test]
fn bounded_recursive_merge_wrapped_in_list_alias_no_ts2589() {
    let codes = semantic_codes(
        r#"
interface ReadonlyArray<T> {
    readonly length: number;
    readonly [n: number]: T;
}
type List<A = any> = ReadonlyArray<A>;
type Cast<A1 extends any, A2 extends any> = A1 extends A2 ? A1 : A2;
type Extends<A1 extends any, A2 extends any> =
    [A1] extends [never] ? 0 : A1 extends A2 ? 1 : 0;

type Iteration = [
    value: number,
    next: keyof IterationMap,
];
type IterationMap = {
    "__": [number, "__"],
    "0": [0, "1"],
    "1": [1, "__"],
};
type IterationOf<N extends keyof IterationMap> = IterationMap[N];
type Next<I extends Iteration> = IterationMap[I[1]];
type Pos<I extends Iteration> = I[0];
type Length<L extends List> = L["length"];

type Merge<O extends object, O1 extends object> = O & O1;
type __MergeAll<
    O extends object,
    Os extends List<object>,
    I extends Iteration = IterationOf<"0">,
> = {
    0: __MergeAll<Merge<O, Os[Pos<I>] & object>, Os, Next<I>>;
    1: O;
}[Extends<Pos<I>, Length<Os>>];
type _MergeAll<O extends object, Os extends List<object>> = __MergeAll<O, Os>;
type MergeAll<L extends List, Ls extends List<List>> =
    Cast<_MergeAll<L, Ls>, List>;

type Result = MergeAll<[{}], [[{ value: 1 }]]>;
"#,
    );

    assert!(
        !codes.contains(&2589),
        "bounded recursive merge through a non-recursive List wrapper must not emit TS2589; got {codes:?}"
    );
}

#[test]
fn generic_recursive_merge_cast_to_list_alias_no_declaration_ts2589() {
    let codes = semantic_codes(
        r##"
interface ReadonlyArray<T> {
    readonly length: number;
    readonly [n: number]: T;
}

type List<A = any> = ReadonlyArray<A>;
type Cast<A1 extends any, A2 extends any> = A1 extends A2 ? A1 : A2;
type Extends<A1 extends any, A2 extends any> =
    [A1] extends [never] ? 0 : A1 extends A2 ? 1 : 0;

type Iteration = [
    value: number,
    next: keyof IterationMap,
];
type IterationMap = {
    "__": [number, "__"],
    "0": [0, "1"],
    "1": [1, "__"],
};
type IterationOf<N extends keyof IterationMap> = IterationMap[N];
type Next<I extends Iteration> = IterationMap[I[1]];
type Pos<I extends Iteration> = I[0];
type Length<L extends List> = L["length"];
type Depth = "flat" | "deep";
type BuiltIn = Function | Date | Error | RegExp;

type Merge<O extends object, O1 extends object> = O & O1;
type __MergeAll<
    O extends object,
    Os extends List<object>,
    depth extends Depth,
    ignore extends object,
    fill extends any,
    I extends Iteration = IterationOf<"0">,
> = {
    0: __MergeAll<Merge<O, Os[Pos<I>] & object>, Os, depth, ignore, fill, Next<I>>;
    1: O;
}[Extends<Pos<I>, Length<Os>>];
type _MergeAll<
    O extends object,
    Os extends List<object>,
    depth extends Depth,
    ignore extends object,
    fill extends any,
> = __MergeAll<O, Os, depth, ignore, fill> extends infer X
    ? Cast<X, object>
    : never;
type ObjectMergeAll<
    O extends object,
    Os extends List<object>,
    depth extends Depth = "flat",
    ignore extends object = BuiltIn,
    fill extends any = undefined,
> = O extends unknown
    ? Os extends unknown
        ? _MergeAll<O, Os, depth, ignore, fill>
        : never
    : never;
type MergeAll<
    L extends List,
    Ls extends List<List>,
    depth extends Depth = "flat",
    ignore extends object = BuiltIn,
    fill extends any = undefined,
> = Cast<ObjectMergeAll<L, Ls, depth, ignore, fill>, List>;
"##,
    );

    assert!(
        !codes.contains(&2589),
        "generic list wrapper declaration around bounded recursive merge must not emit TS2589; got {codes:?}"
    );
}

#[test]
fn imported_generic_recursive_merge_cast_to_list_alias_no_declaration_ts2589() {
    let files = [
        (
            "list.ts",
            r#"
export interface ReadonlyArray<T> {
    readonly length: number;
    readonly [n: number]: T;
}
export type List<A = any> = ReadonlyArray<A>;
"#,
        ),
        (
            "cast.ts",
            r#"
export type Cast<A1 extends any, A2 extends any> = A1 extends A2 ? A1 : A2;
export type Extends<A1 extends any, A2 extends any> =
    [A1] extends [never] ? 0 : A1 extends A2 ? 1 : 0;
"#,
        ),
        (
            "objectMergeAll.ts",
            r##"
import {Cast, Extends} from "./cast";
import {List} from "./list";

type Iteration = [
    value: number,
    next: keyof IterationMap,
];
type IterationMap = {
    "__": [number, "__"],
    "0": [0, "1"],
    "1": [1, "__"],
};
type IterationOf<N extends keyof IterationMap> = IterationMap[N];
type Next<I extends Iteration> = IterationMap[I[1]];
type Pos<I extends Iteration> = I[0];
type Length<L extends List> = L["length"];
export type Depth = "flat" | "deep";
export type BuiltIn = { readonly __brand?: never };

type Merge<O extends object, O1 extends object> = O & O1;
type __MergeAll<
    O extends object,
    Os extends List<object>,
    depth extends Depth,
    ignore extends object,
    fill extends any,
    I extends Iteration = IterationOf<"0">,
> = {
    0: __MergeAll<Merge<O, Os[Pos<I>] & object>, Os, depth, ignore, fill, Next<I>>;
    1: O;
}[Extends<Pos<I>, Length<Os>>];
type _MergeAll<
    O extends object,
    Os extends List<object>,
    depth extends Depth,
    ignore extends object,
    fill extends any,
> = __MergeAll<O, Os, depth, ignore, fill> extends infer X
    ? Cast<X, object>
    : never;
export type ObjectMergeAll<
    O extends object,
    Os extends List<object>,
    depth extends Depth = "flat",
    ignore extends object = BuiltIn,
    fill extends any = undefined,
> = O extends unknown
    ? Os extends unknown
        ? _MergeAll<O, Os, depth, ignore, fill>
        : never
    : never;
"##,
        ),
        (
            "mergeAll.ts",
            r#"
import {Cast} from "./cast";
import {List} from "./list";
import {BuiltIn, Depth, ObjectMergeAll} from "./objectMergeAll";

export type MergeAll<
    L extends List,
    Ls extends List<List>,
    depth extends Depth = "flat",
    ignore extends object = BuiltIn,
    fill extends any = undefined,
> = Cast<ObjectMergeAll<L, Ls, depth, ignore, fill>, List>;
"#,
        ),
    ];
    let entry_diagnostics =
        check_multi_file_with_global_index(&files, "mergeAll.ts", CheckerOptions::default());
    let diagnostics = check_all_multi_file_with_global_index(&files, CheckerOptions::default());
    let entry_codes = diagnostic_codes(&diagnostics_without_codes(&entry_diagnostics, &[2318]));
    let codes = diagnostic_codes(&diagnostics_without_codes(&diagnostics, &[2318]));

    assert!(
        !entry_codes.contains(&2589),
        "entry-only imported generic list wrapper declaration must not emit TS2589; got {entry_codes:?}"
    );
    assert!(
        !codes.contains(&2589),
        "project-style imported generic list wrapper declaration around bounded recursive merge must not emit TS2589; got {codes:?}"
    );
}
