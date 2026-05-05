//! Locks in that `Array<T>` and `T[]` (and `ReadonlyArray<T>` / `readonly T[]`)
//! produce identical type identities in the checker, so they don't trigger
//! spurious TS2403 (Subsequent variable declarations must have the same type)
//! when used together in a redeclaration pair.
//!
//! Regression: `booleanFilterAnyArray.ts` — declared
//!     var foor: Array<{name: string}>
//!     var foor = foo.filter(x => x.name)   // inferred as `{name: string}[]`
//! emitted a false TS2403 because the generic-form `Array<T>` was lowered to a
//! generic application form while the shorthand `T[]` lowered directly to the
//! solver's array form — bidirectional identity comparison saw two different
//! shapes for the same type.

use tsz_checker::test_utils::check_source_codes;

#[test]
fn array_generic_and_shorthand_are_identical_for_redeclaration() {
    let source = r#"
var a: Array<{x: number}>;
var a: {x: number}[];
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2403),
        "Array<X> vs X[] should be identical for redeclaration; got {codes:?}",
    );
}

#[test]
fn readonly_array_generic_and_shorthand_are_identical_for_redeclaration() {
    let source = r#"
var a: ReadonlyArray<{x: number}>;
var a: readonly {x: number}[];
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2403),
        "ReadonlyArray<X> vs readonly X[] should be identical for redeclaration; got {codes:?}",
    );
}

#[test]
fn boolean_filter_any_array_pattern_no_ts2403() {
    // Mirror of TypeScript/tests/cases/compiler/booleanFilterAnyArray.ts
    // (the relevant subset). The original test annotates a var with
    // `Array<{name: string}>` then assigns the result of `.filter(...)`,
    // whose inferred type is `{name: string}[]`. tsc emits no TS2403 here.
    let source = r#"
var foo = [{ name: 'x' }];
var foor: Array<{name: string}>;
var foor = foo.filter(x => x.name);
var foos: Array<boolean>;
var foos = [true, true, false].filter((thing): thing is boolean => thing !== null);
"#;
    let codes = check_source_codes(source);
    let count_2403 = codes.iter().filter(|&&c| c == 2403).count();
    assert_eq!(
        count_2403, 0,
        "expected no TS2403 false positives; got {codes:?}",
    );
}

#[test]
fn array_generic_inside_other_annotations_still_canonical() {
    // Generic Array<T> appearing inside a more complex annotation should
    // still canonicalize. Without this, e.g. function return types or
    // tuple element types would emit spurious TS2403 when the redeclared
    // form uses the shorthand.
    let source = r#"
var f: () => Array<number>;
var f: () => number[];
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2403),
        "Array<X> nested in function return type should match X[]; got {codes:?}",
    );
}
