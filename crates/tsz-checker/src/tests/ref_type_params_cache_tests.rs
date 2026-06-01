use crate::test_utils::check_source_diagnostics;

/// When multiple type references in the same alias body refer to the same
/// generic utility type, tsz must resolve that type's parameter list once per
/// symbol (not once per reference site).  These tests verify correctness and
/// that arity validation still fires when argument counts are wrong.

#[test]
fn multiple_refs_to_same_two_param_utility_no_spurious_errors() {
    let diags = check_source_diagnostics(
        r#"
type Without<T, U> = { [P in Exclude<keyof T, keyof U>]?: never };
type Exclusive<T, U> = (T & Without<U, T>) | (U & Without<T, U>);
type Exclude<T, U> = T extends U ? never : T;

type A = { a: number; shared: string };
type B = { b: number; shared: string };

declare const x: Exclusive<A, B>;
"#,
    );

    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 2314 | 2315 | 2344 | 2345))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Multiple references to Without should not produce spurious errors; got: {relevant:#?}"
    );
}

#[test]
fn multiple_refs_renamed_params_k_v_produce_same_arity_errors() {
    let diags = check_source_diagnostics(
        r#"
type Pair<K, V> = { key: K; value: V };

type Combined = Pair<string, number> | Pair<number, string>;
type BadArity = Pair<string>;
"#,
    );

    let arity_errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2314 || d.code == 2558)
        .collect();
    assert_eq!(
        arity_errors.len(),
        1,
        "Expected exactly one arity error for Pair<string> (missing second arg); got: {arity_errors:#?}"
    );
}

#[test]
fn repeated_refs_to_three_param_utility_all_valid() {
    let diags = check_source_diagnostics(
        r#"
type Triple<A, B, C> = { first: A; second: B; third: C };

type T1 = Triple<string, number, boolean>;
type T2 = Triple<number, boolean, string>;
type T3 = Triple<boolean, string, number>;
"#,
    );

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 2314 | 2315 | 2344 | 2345 | 2558))
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Three valid instantiations of Triple should produce no errors; got: {errors:#?}"
    );
}

#[test]
fn alias_wrapping_same_generic_shares_params() {
    let diags = check_source_diagnostics(
        r#"
type Box<T> = { value: T };

type StringBox = Box<string>;
type NumberBox = Box<number>;
type BadBox = Box<string, number>;
"#,
    );

    let arity_errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2314 || d.code == 2558)
        .collect();
    assert_eq!(
        arity_errors.len(),
        1,
        "Expected one arity error for Box<string, number>; got: {arity_errors:#?}"
    );
}

#[test]
fn nested_generic_refs_same_utility_no_spurious_errors() {
    let diags = check_source_diagnostics(
        r#"
type Nullable<T> = T | null;
type Optional<T> = T | undefined;
type Maybe<T> = Nullable<Optional<T>>;
type MaybeString = Maybe<string>;
type MaybeNumber = Maybe<number>;
"#,
    );

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 2314 | 2315 | 2344 | 2345))
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Nested alias chains reusing the same generic should produce no errors; got: {errors:#?}"
    );
}

#[test]
fn default_type_param_instantiations_produce_no_errors() {
    let diags = check_source_diagnostics(
        r#"
type Container<T, U = string> = { item: T; meta: U };

type C1 = Container<number>;
type C2 = Container<number, boolean>;
type C3 = Container<string>;
type C4 = Container<boolean, null>;
"#,
    );

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 2314 | 2344 | 2558))
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Four valid Container instantiations should produce no errors; got: {errors:#?}"
    );
}

#[test]
fn constrained_type_param_validated_consistently_across_refs() {
    let diags = check_source_diagnostics(
        r#"
type StringMap<K extends string, V> = { [key in K]: V };

type M1 = StringMap<"a" | "b", number>;
type M2 = StringMap<string, boolean>;
"#,
    );

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 2344 | 2314 | 2558))
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Valid instantiations of constrained StringMap should produce no errors; got: {errors:#?}"
    );
}

#[test]
fn many_refs_to_same_two_param_type_all_valid() {
    let diags = check_source_diagnostics(
        r#"
type Either<L, R> = { tag: "left"; value: L } | { tag: "right"; value: R };

type E01 = Either<string, number>;
type E02 = Either<number, string>;
type E03 = Either<boolean, string>;
type E04 = Either<string, boolean>;
type E05 = Either<null, string>;
type E06 = Either<string, null>;
type E07 = Either<undefined, number>;
type E08 = Either<number, undefined>;
type E09 = Either<E01, E02>;
type E10 = Either<E03, E04>;
"#,
    );

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 2314 | 2344 | 2558))
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Ten valid instantiations of Either should produce no errors; got: {errors:#?}"
    );
}
