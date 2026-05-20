//! Tests for TS2352 type-assertion overlap with the `object` primitive
//! and the `{}` empty object type.
//!
//! Conformance test:
//! `TypeScript/tests/cases/compiler/genericWithNoConstraintComparableWithCurlyCurly.ts`
//!
//! Before this fix, `{} as T` where `T extends object | null | undefined`
//! emitted a false-positive TS2352 because the assertion-overlap check
//! returned false when both sides had no extractable properties:
//! `types_have_common_properties_relaxed` short-circuits on the
//! "both empty" branch. tsc treats the `object` primitive as overlapping
//! with any object-like type, and walks a `TypeParameter`'s constraint
//! when the source is the empty object type `{}`.
//!
//! Two narrow rules added in `types_are_comparable_for_assertion_inner`:
//!   1. `object` primitive ↔ Object/Array/Tuple/Callable/Function/Intersection
//!      → comparable.
//!   2. `{}` (empty object: no required props, no index sigs) ↔
//!      `TypeParameter` with constraint → recurse against the constraint.
//!
//! The "narrow to {}" gating is important: fully unwrapping any source's
//! type-parameter constraint would over-permit assertions like
//! `B as T extends A` (genericTypeAssertions4.ts).

use crate::test_utils::check_source_strict_codes as check_strict;

/// `{} as T` with `T extends object | null | undefined` must NOT emit TS2352.
/// The constraint contains `object`, which the empty-object source overlaps with.
#[test]
fn empty_object_assert_to_typeparam_with_object_in_constraint_no_ts2352() {
    let source = r#"
function yes<T extends object | null | undefined>() {
    let x = {};
    x as T;
}
"#;
    let codes = check_strict(source);
    let ts2352: Vec<&u32> = codes.iter().filter(|c| **c == 2352).collect();
    assert!(
        ts2352.is_empty(),
        "no TS2352 expected — `{{}}` overlaps with `object` in T's constraint. Got: {codes:?}"
    );
}

/// `{} as T` with `T extends null | undefined` SHOULD emit TS2352.
/// The constraint has no object-like member; the empty-object source has no
/// overlap with null/undefined alone.
#[test]
fn empty_object_assert_to_typeparam_without_object_in_constraint_emits_ts2352() {
    let source = r#"
function no<T extends null | undefined>() {
    let x = {};
    x as T;
}
"#;
    let codes = check_strict(source);
    let ts2352: Vec<&u32> = codes.iter().filter(|c| **c == 2352).collect();
    assert!(
        !ts2352.is_empty(),
        "TS2352 expected — `{{}}` does not overlap with `null | undefined`. Got: {codes:?}"
    );
}

/// `{} as T` with `T` (no constraint), `T extends unknown`, `T extends {}`,
/// `T extends object` — none of these emit TS2352. Each constraint either
/// has no upper bound (T = unknown) or an object-like upper bound.
#[test]
fn empty_object_assert_to_typeparam_no_or_object_constraint_no_ts2352() {
    let source = r#"
function foo<T>() {
    let x = {};
    x as T;
}
function bar<T extends unknown>() {
    let x = {};
    x as T;
}
function baz<T extends {}>() {
    let x = {};
    x as T;
}
function bat<T extends object>() {
    let x = {};
    x as T;
}
"#;
    let codes = check_strict(source);
    let ts2352: Vec<&u32> = codes.iter().filter(|c| **c == 2352).collect();
    assert!(
        ts2352.is_empty(),
        "no TS2352 expected for any of foo/bar/baz/bat. Got: {codes:?}"
    );
}

/// Sanity: the empty-object special case must NOT cause `B as T extends A`
/// (where B is a specific subclass, not the empty object) to lose TS2352.
/// tsc emits TS2352 here because B is just one of many possible subtypes of A,
/// and T is opaque — B is not necessarily T.
#[test]
fn specific_subclass_assert_to_typeparam_emits_ts2352() {
    let source = r#"
class A { foo() { return ""; } }
class B extends A { bar() { return 1; } }
declare let b: B;
function foo2<T extends A>() {
    let y = b as T;
}
"#;
    let codes = check_strict(source);
    let ts2352: Vec<&u32> = codes.iter().filter(|c| **c == 2352).collect();
    assert!(
        !ts2352.is_empty(),
        "TS2352 expected — B is one specific subtype of A, not opaque T. Got: {codes:?}"
    );
}

/// Sanity: `object` primitive overlaps with arbitrary object/array/tuple
/// shapes for assertion purposes.
#[test]
fn object_primitive_overlaps_array_assertion() {
    let source = r#"
declare let o: object;
let arr = o as number[];
"#;
    let codes = check_strict(source);
    let ts2352: Vec<&u32> = codes.iter().filter(|c| **c == 2352).collect();
    assert!(
        ts2352.is_empty(),
        "no TS2352 expected — `object` overlaps with array shapes. Got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Primitive ↔ constrained type parameter assertions (issue #5957)
// ---------------------------------------------------------------------------
// tsc's `comparableRelation` reduces a type parameter to its constraint before
// checking primitive overlap. `number as T extends number` is valid because
// `number` is comparable to `number` (the constraint).

/// `x as T` where `T extends number` and `x: number` must NOT emit TS2352.
/// The primitive type `number` overlaps with the constraint `number`.
#[test]
fn number_assert_to_number_constrained_typeparam_no_ts2352() {
    for var_name in ["T", "K", "U"] {
        let source = format!(
            r#"function f<{var_name} extends number>(x: number): {var_name} {{
    return x as {var_name};
}}"#
        );
        let codes = check_strict(&source);
        let ts2352: Vec<&u32> = codes.iter().filter(|c| **c == 2352).collect();
        assert!(
            ts2352.is_empty(),
            "no TS2352 expected for `number as {var_name} extends number`. Got: {codes:?}"
        );
    }
}

/// `x as T` where `T extends string` and `x: string` must NOT emit TS2352.
#[test]
fn string_assert_to_string_constrained_typeparam_no_ts2352() {
    let source = r#"function f<T extends string>(x: string): T {
    return x as T;
}"#;
    let codes = check_strict(source);
    let ts2352: Vec<&u32> = codes.iter().filter(|c| **c == 2352).collect();
    assert!(
        ts2352.is_empty(),
        "no TS2352 expected for `string as T extends string`. Got: {codes:?}"
    );
}

/// `x as T` where `T extends boolean` and `x: boolean` must NOT emit TS2352.
#[test]
fn boolean_assert_to_boolean_constrained_typeparam_no_ts2352() {
    let source = r#"function f<T extends boolean>(x: boolean): T {
    return x as T;
}"#;
    let codes = check_strict(source);
    let ts2352: Vec<&u32> = codes.iter().filter(|c| **c == 2352).collect();
    assert!(
        ts2352.is_empty(),
        "no TS2352 expected for `boolean as T extends boolean`. Got: {codes:?}"
    );
}

/// The zero literal `0 as T extends number` (canonical issue repro) must NOT
/// emit TS2352. tsc widens `0` to `number` for the expression type, which
/// overlaps with the constraint `number`.
#[test]
fn zero_literal_assert_to_number_constrained_typeparam_no_ts2352() {
    let source = r#"declare function reduce<T extends number>(f: (a: T, b: T) => T, init: T): T;
function test<T extends number>(): T {
    return 0 as T;
}"#;
    let codes = check_strict(source);
    let ts2352: Vec<&u32> = codes.iter().filter(|c| **c == 2352).collect();
    assert!(
        ts2352.is_empty(),
        "no TS2352 expected for `0 as T extends number`. Got: {codes:?}"
    );
}

/// Cross-primitive assertion (`string as T extends number`) MUST still emit
/// TS2352. `string` does not overlap with the constraint `number`.
#[test]
fn cross_primitive_assert_to_number_constrained_typeparam_emits_ts2352() {
    let source = r#"function bad<T extends number>(x: string): T {
    return x as T;
}"#;
    let codes = check_strict(source);
    let ts2352: Vec<&u32> = codes.iter().filter(|c| **c == 2352).collect();
    assert!(
        !ts2352.is_empty(),
        "TS2352 expected — `string` does not overlap with `number` constraint. Got: {codes:?}"
    );
}
