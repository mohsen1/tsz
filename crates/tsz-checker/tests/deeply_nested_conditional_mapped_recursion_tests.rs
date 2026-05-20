//! Regression tests for #8684: deeply nested conditional mapped types and
//! display alias correctness.
//!
//! Two bug classes are covered:
//!
//! **Bug A — display alias corruption**: When two type aliases evaluate to the
//! same structural type, the first alias's display name must be preserved.
//! A later evaluation (`NestedRecord<"x.y.z", string>`) must not overwrite the
//! display alias of an earlier application (`Id<{x: number}>`).
//!
//! **Bug B — recursion identity for conditional aliases**: When comparing
//! same-base Application types whose base is a conditional alias, tsz must
//! engage the `def_guard` cycle detector (matching tsc's `getRecursionIdentity`
//! mechanism). Without this, deeply recursive conditional types like
//! `RequiredDeep<T>`, `DeepReadonly<T>`, or `NestedRecord<K,V>` hit the depth
//! guard and either produce spurious TS2589 or silently bail with wrong results.
//!
//! Structural rule: "When same-base Application types whose base is a
//! conditional alias are compared, the `def_guard` cycle detector must be
//! engaged (not bypassed); once the guard sees the same `DefId` pair a second
//! time, it returns compatible, matching tsc's `getRecursionIdentity` behavior."
//!
//! Non-conditional mapped aliases (`Id<T>`, `Readonly<T>`) are NOT affected and
//! continue using the variance fast path.

use tsz_checker::test_utils::check_source_codes;

/// tsc rule: a deeply nested conditional type compared against itself
/// must not produce TS2589 (type instantiation excessively deep).
/// `RequiredDeep` is the canonical repro shape from tsc's suite.
#[test]
fn deeply_nested_conditional_type_no_ts2589() {
    let source = r#"
type RequiredDeep<T> = T extends object ? { [K in keyof T]-?: RequiredDeep<T[K]> } : T;

declare function check<T>(a: T, b: T): void;

// Deep nesting should not trigger TS2589
declare const x: RequiredDeep<{ a: { b: { c: { d: { e: number } } } } }>;
declare const y: RequiredDeep<{ a: { b: { c: { d: { e: number } } } } }>;
check(x, y);
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2589),
        "RequiredDeep deep nesting must not produce TS2589 (excessively deep). Got: {codes:?}"
    );
    assert!(
        !codes.contains(&2345),
        "RequiredDeep deep nesting must not produce TS2345 (argument not assignable). Got: {codes:?}"
    );
}

/// Same test as above but with a different alias name to confirm the fix is
/// structural (not hardcoded to `RequiredDeep`).
#[test]
fn deeply_nested_conditional_type_no_ts2589_alt_name() {
    let source = r#"
type DeepRequired<T> = T extends object ? { [P in keyof T]-?: DeepRequired<T[P]> } : T;

declare function verify<T>(a: T, b: T): void;

declare const p: DeepRequired<{ x: { y: { z: { w: { v: number } } } } }>;
declare const q: DeepRequired<{ x: { y: { z: { w: { v: number } } } } }>;
verify(p, q);
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2589),
        "DeepRequired deep nesting must not produce TS2589. Got: {codes:?}"
    );
    assert!(
        !codes.contains(&2345),
        "DeepRequired deep nesting must not produce TS2345. Got: {codes:?}"
    );
}

/// tsc rule: a deeply recursive conditional type like `NestedRecord<K,V>` that
/// recurses with dot-separated keys must terminate without TS2589.
#[test]
fn nested_record_conditional_type_no_ts2589() {
    let source = r#"
type NestedRecord<K extends string, V> = K extends `${infer A}.${infer B}`
    ? { [X in A]: NestedRecord<B, V> }
    : { [X in K]: V };

declare const r1: NestedRecord<"a.b.c", number>;
declare const r2: NestedRecord<"a.b.c", number>;
declare function match<T>(a: T, b: T): void;
match(r1, r2);
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2589),
        "NestedRecord with dot-separated keys must not produce TS2589. Got: {codes:?}"
    );
    assert!(
        !codes.contains(&2345),
        "NestedRecord must accept same-typed values as argument. Got: {codes:?}"
    );
}

/// Variant with different type-param names to prove the fix is not hardcoded
/// to the specific name `NestedRecord` or `K`/`V`.
#[test]
fn nested_record_conditional_type_no_ts2589_renamed_params() {
    let source = r#"
type PathRecord<Path extends string, Value> = Path extends `${infer Head}.${infer Tail}`
    ? { [Key in Head]: PathRecord<Tail, Value> }
    : { [Key in Path]: Value };

declare const a: PathRecord<"one.two.three", string>;
declare const b: PathRecord<"one.two.three", string>;
declare function eq<T>(x: T, y: T): void;
eq(a, b);
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2589),
        "PathRecord with renamed params must not produce TS2589. Got: {codes:?}"
    );
    assert!(
        !codes.contains(&2345),
        "PathRecord renamed params must not produce TS2345. Got: {codes:?}"
    );
}

/// tsc rule: a non-conditional mapped alias compared against itself must still
/// work correctly and is NOT affected by the conditional alias fix. This test
/// guards against regression in the variance fast path for non-conditional
/// same-base applications.
#[test]
fn non_conditional_mapped_alias_identity_still_works() {
    let source = r#"
type Id<T> = { [K in keyof T]: T[K] };
declare const a: Id<{ x: number; y: string }>;
declare const b: Id<{ x: number; y: string }>;
declare function accept<T>(x: T, y: T): void;
accept(a, b);
"#;
    let codes = check_source_codes(source);
    assert!(
        codes.is_empty(),
        "Non-conditional mapped alias Id<T> same-type comparison must produce no errors. Got: {codes:?}"
    );
}

/// tsc rule: a genuinely incompatible assignment to a deeply nested conditional
/// type must still produce an error. Recursion identity must not suppress
/// different application arguments.
#[test]
fn deeply_nested_conditional_type_still_errors_on_mismatch() {
    let source = r#"
type DeepReadonly<T> = T extends object ? { readonly [K in keyof T]: DeepReadonly<T[K]> } : T;

declare const good: DeepReadonly<{ a: number }>;
const bad: DeepReadonly<{ a: string }> = good;
"#;
    let codes = check_source_codes(source);
    assert!(
        codes.contains(&2322),
        "DeepReadonly leaf mismatch must still produce TS2322. Got: {codes:?}"
    );
    assert!(
        !codes.contains(&2589),
        "DeepReadonly mismatch test must not produce TS2589. Got: {codes:?}"
    );
}

#[test]
fn nested_record_conditional_type_still_errors_on_leaf_mismatch() {
    let source = r#"
type NestedRecord<K extends string, V> = K extends `${infer A}.${infer B}`
    ? { [X in A]: NestedRecord<B, V> }
    : { [X in K]: V };

declare const numberRecord: NestedRecord<"a.b", number>;
const stringRecord: NestedRecord<"a.b", string> = numberRecord;
"#;
    let codes = check_source_codes(source);
    assert!(
        codes.contains(&2322),
        "NestedRecord leaf mismatch must still produce TS2322. Got: {codes:?}"
    );
    assert!(
        !codes.contains(&2589),
        "NestedRecord mismatch test must not produce TS2589. Got: {codes:?}"
    );
}

/// tsc rule: assigning a value to a `RequiredDeep` annotated variable that has
/// the exact right structure must not emit any error.
#[test]
fn required_deep_assignment_of_exact_type_no_error() {
    let source = r#"
type RequiredDeep<T> = T extends object ? { [K in keyof T]-?: RequiredDeep<T[K]> } : T;

interface Deep {
    a: {
        b: {
            c: number;
        };
    };
}

declare const src: RequiredDeep<Deep>;
const dst: RequiredDeep<Deep> = src;
"#;
    let codes = check_source_codes(source);
    assert!(
        codes.is_empty(),
        "RequiredDeep assignment of exact type must produce no errors. Got: {codes:?}"
    );
}

/// Adjacent case: `DeepPartial` (conditional alias that adds optional modifier).
/// Ensures the fix generalizes beyond the specific `RequiredDeep` shape.
#[test]
fn deep_partial_conditional_alias_no_ts2589() {
    let source = r#"
type DeepPartial<T> = T extends object ? { [K in keyof T]?: DeepPartial<T[K]> } : T;

declare const a: DeepPartial<{ x: { y: { z: number } } }>;
declare const b: DeepPartial<{ x: { y: { z: number } } }>;
declare function same<T>(x: T, y: T): void;
same(a, b);
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2589),
        "DeepPartial must not produce TS2589. Got: {codes:?}"
    );
    assert!(
        !codes.contains(&2345),
        "DeepPartial same-type comparison must not produce TS2345. Got: {codes:?}"
    );
}
