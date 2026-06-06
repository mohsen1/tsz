//! TS4105 ("Private or protected member '{0}' cannot be accessed on a type
//! parameter.") for indexed-access types in *signature* positions.
//!
//! tsc reports TS4105 for `this["<nonpublic>"]` / `T["<nonpublic>"]` wherever
//! the indexed-access type appears, including class method return and parameter
//! type annotations, standalone function declarations, function-type literals,
//! constructor parameters, accessors, overload signatures, and abstract method
//! signatures. tsz previously only ran indexed-access type validation for
//! type-alias bodies and interface members, so the same annotation on a class
//! method or function declaration silently passed. These tests pin the
//! signature-position behavior across declaration kinds, with renamed binders
//! (no name-specific shortcut) and negative cases (public members must stay
//! clean).

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_all_multi_file_with_global_index;
use tsz_common::common::{ModuleKind, ScriptTarget};

const TS4105: u32 = 4105;

fn diagnostics(source: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_with_options(source, CheckerOptions::default())
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

fn count(diags: &[(u32, String)], code: u32) -> usize {
    diags.iter().filter(|(c, _)| *c == code).count()
}

/// A class method return-type annotation of `this["<private>"]` reports TS4105.
#[test]
fn class_method_return_this_indexed_private_reports_ts4105() {
    let diags = diagnostics(
        r#"
class Vault {
    private secret!: number;
    reveal(): this["secret"] { return this.secret; }
}
"#,
    );
    assert_eq!(
        count(&diags, TS4105),
        1,
        "expected TS4105 for a class method return `this[\"secret\"]`; got {diags:?}"
    );
}

/// A class method parameter annotation of `this["<protected>"]` reports TS4105.
#[test]
fn class_method_param_this_indexed_protected_reports_ts4105() {
    let diags = diagnostics(
        r#"
class Channel {
    protected token!: string;
    accept(value: this["token"]): void {}
}
"#,
    );
    assert_eq!(
        count(&diags, TS4105),
        1,
        "expected TS4105 for a class method parameter `this[\"token\"]`; got {diags:?}"
    );
}

/// A standalone function declaration return type `T["<private>"]` reports TS4105.
#[test]
fn function_declaration_return_type_param_indexed_private_reports_ts4105() {
    let diags = diagnostics(
        r#"
class Store { private data!: number; }
function read<T extends Store>(): T["data"] { return 0 as any; }
"#,
    );
    assert_eq!(
        count(&diags, TS4105),
        1,
        "expected TS4105 for a function declaration return `T[\"data\"]`; got {diags:?}"
    );
}

/// A function-type literal with its own type parameters reports TS4105 in its
/// return annotation, and resolving the type parameter must NOT leak a spurious
/// TS2304 (Cannot find name).
#[test]
fn function_type_literal_return_indexed_private_reports_ts4105_only() {
    let diags = diagnostics(
        r#"
class Bag { private payload!: string; }
const get: <T extends Bag>() => T["payload"] = null as any;
"#,
    );
    assert_eq!(
        count(&diags, TS4105),
        1,
        "expected TS4105 for a function-type return `T[\"payload\"]`; got {diags:?}"
    );
    assert_eq!(
        count(&diags, 2304),
        0,
        "the function type's own type parameter must resolve (no TS2304); got {diags:?}"
    );
}

/// Renamed binders behave identically (no name-specific shortcut).
#[test]
fn class_method_signature_renamed_binders_reports_ts4105() {
    let diags = diagnostics(
        r#"
class Crate {
    protected hidden!: boolean;
    inspect(probe: this["hidden"]): void {}
}
"#,
    );
    assert_eq!(
        count(&diags, TS4105),
        1,
        "expected TS4105 with renamed binders; got {diags:?}"
    );
}

/// An overload signature return type of `this["<private>"]` reports TS4105.
#[test]
fn overload_signature_return_this_indexed_private_reports_ts4105() {
    let diags = diagnostics(
        r#"
class Dispatcher {
    private handle!: number;
    on(name: string): this["handle"];
    on(name: number): number;
    on(name: any): any { return name; }
}
"#,
    );
    assert_eq!(
        count(&diags, TS4105),
        1,
        "expected TS4105 for an overload return `this[\"handle\"]`; got {diags:?}"
    );
}

/// An abstract method signature return type of `this["<protected>"]` reports
/// TS4105 even though it has no body.
#[test]
fn abstract_method_signature_this_indexed_protected_reports_ts4105() {
    let diags = diagnostics(
        r#"
abstract class Shape {
    protected area!: number;
    abstract measure(): this["area"];
}
"#,
    );
    assert_eq!(
        count(&diags, TS4105),
        1,
        "expected TS4105 for an abstract method return `this[\"area\"]`; got {diags:?}"
    );
}

/// Public members indexed through `this`/`Base` in signature positions must NOT
/// report TS4105 (return, parameter, and property positions all stay clean).
#[test]
fn public_member_signature_positions_no_ts4105() {
    let diags = diagnostics(
        r#"
class Base { open!: string; }
class Clean {
    open!: string;
    ret(): this["open"] { return this.open; }
    take(value: this["open"]): void {}
    field: Base["open"] = "x";
}
"#,
    );
    assert_eq!(
        count(&diags, TS4105),
        0,
        "public members in signature positions must not trigger TS4105; got {diags:?}"
    );
}

/// Generic keyed access over a type parameter in method/function signatures must
/// stay clean — this is the common `obj[key]` shape and must not regress into a
/// false TS4105/TS2536.
#[test]
fn generic_keyed_access_signatures_stay_clean() {
    let diags = diagnostics(
        r#"
class Emitter<M> {
    on<E extends keyof M>(e: E, cb: (p: M[E]) => void): void {}
    get<E extends keyof M>(e: E): M[E] { return null as any; }
}
function pick<T, K extends keyof T>(obj: T, key: K): T[K] { return obj[key]; }
"#,
    );
    assert_eq!(
        count(&diags, TS4105),
        0,
        "generic keyed access must not trigger TS4105; got {diags:?}"
    );
    assert_eq!(
        count(&diags, 2536),
        0,
        "generic keyed access must not trigger TS2536; got {diags:?}"
    );
}

/// Utility/generic type applications indexed by a public literal property must
/// stay clean. The signature-position TS4105 probe is only for bare
/// type-parameter/`this` candidates such as `T["private"]`, not for resolving
/// generic helper applications like `Parameters<F>["length"]`.
#[test]
fn generic_type_application_literal_index_signatures_stay_clean() {
    let diags = diagnostics(
        r#"
type DataFirst = (value: string, count: number) => void;
type Box<T> = { value: T };

function arity(value: Parameters<DataFirst>["length"]): Parameters<DataFirst>["length"] {
    return value;
}

function unwrap<T>(value: Box<T>["value"]): T {
    return value;
}
"#,
    );
    assert_eq!(
        count(&diags, TS4105),
        0,
        "generic type applications with public literal indexes must not trigger TS4105; got {diags:?}"
    );
}

/// Conditional-type `infer` binders are in scope for the true branch. The
/// signature-position TS4105 probe must not resolve `R["length"]` as a missing
/// top-level name while walking declaration-signature types.
#[test]
fn conditional_infer_indexed_access_stays_in_scope() {
    let diags = diagnostics(
        r#"
export type LengthOfTail<T extends unknown[]> =
    T extends [unknown, ...infer R] ? R["length"] : never;

export declare function tailLength<T extends unknown[]>(
    value: LengthOfTail<T>
): LengthOfTail<T>;
"#,
    );
    assert_eq!(
        count(&diags, 2304),
        0,
        "conditional true-branch `infer` binders must stay in scope; got {diags:?}"
    );
    assert_eq!(
        count(&diags, TS4105),
        0,
        "public indexed access through an infer binder must not trigger TS4105; got {diags:?}"
    );
}

/// Recursive declaration-file aliases may use a rest-position `infer` binding
/// and immediately index that inferred tuple in the conditional true branch.
/// Signature-position TS4105 probing must not surface a duplicate TS2304 for
/// that inferred binder while checking a consumer file.
#[test]
fn recursive_declaration_alias_infer_indexed_access_stays_in_scope() {
    let files = [
        (
            "input.d.ts",
            r#"
type _BuildPowersOf2LengthArrays<
    Length extends number,
    AccumulatedArray extends never[][],
> = AccumulatedArray[0][Length] extends never
    ? AccumulatedArray
    : _BuildPowersOf2LengthArrays<
        Length,
        [[...AccumulatedArray[0], ...AccumulatedArray[0]], ...AccumulatedArray]
    >;

type _ConcatLargestUntilDone<
    Length extends number,
    AccumulatedArray extends never[][],
    NextArray extends never[],
> = NextArray["length"] extends Length
    ? NextArray
    : [...AccumulatedArray[0], ...NextArray][Length] extends never
    ? _ConcatLargestUntilDone<
        Length,
        AccumulatedArray extends [AccumulatedArray[0], ...infer U]
        ? U extends never[][]
        ? U
        : never
        : never,
        NextArray
    >
    : _ConcatLargestUntilDone<
        Length,
        AccumulatedArray extends [AccumulatedArray[0], ...infer U]
        ? U extends never[][]
        ? U
        : never
        : never,
        [...AccumulatedArray[0], ...NextArray]
    >

type _Replace<R extends unknown[], T> = { [K in keyof R]: T };

export type TupleOf<Type, Length extends number> = number extends Length
    ? Type[]
    : {
        [LengthKey in Length]: _BuildPowersOf2LengthArrays<
            LengthKey,
            [[never]]
        > extends infer TwoDimensionalArray
        ? TwoDimensionalArray extends never[][]
        ? _Replace<_ConcatLargestUntilDone<LengthKey, TwoDimensionalArray, []>, Type>
        : never
        : never
    }[Length];

export type Subtract<N1 extends number, N2 extends number> = TupleOf<never, N1> extends [
    ...TupleOf<never, N2>,
    ...infer R,
]
    ? R["length"]
    : never;

export type Decrement<T extends number> = Subtract<T, 1>;
export type Add<N1 extends number, N2 extends number> = [
    ...TupleOf<never, N1>,
    ...TupleOf<never, N2>,
]["length"] & number;
type _MultiAdd<
    Num extends number,
    Accumulator extends number,
    IterationsLeft extends number,
> = IterationsLeft extends 0
    ? Accumulator
    : _MultiAdd<Num, Add<Num, Accumulator>, Decrement<IterationsLeft>>
export type Multiply<N1 extends number, N2 extends number> = number extends N1 | N2
    ? number
    : { [K2 in N2]: { [K1 in N1]: _MultiAdd<K1, 0, N2> }[N1] }[N2]
type PowerTailRec<
    Num extends number,
    PowerOf extends number,
    Result extends number,
> = number extends PowerOf
    ? number
    : PowerOf extends 0
    ? Result
    : PowerTailRec<Num, Decrement<PowerOf>, Multiply<Result, Num>>;
export type Power<Num extends number, PowerOf extends number> =
    PowerTailRec<Num, PowerOf, 1>;
"#,
        ),
        (
            "a.tsx",
            r#"
import { Power } from "./input";

export const power = <Num extends number, PowerOf extends number>(
    num: Num,
    powerOf: PowerOf
): Power<Num, PowerOf> => (num ** powerOf) as never;
"#,
        ),
    ];
    let diags: Vec<(u32, String)> = check_all_multi_file_with_global_index(
        &files,
        CheckerOptions {
            emit_declarations: true,
            module: ModuleKind::CommonJS,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|d| (d.code, d.message_text))
    .collect();
    assert_eq!(
        count(&diags, 2304),
        0,
        "rest-position conditional `infer` binders must stay in scope; got {diags:?}"
    );
}
