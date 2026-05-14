//! Tests for TS2345 when an `IndexAccess` type `S[I]` is passed to an
//! unconstrained `TypeParameter` `P` where `P` structurally appears in `S`.
//!
//! Structural rule: `S[I]` is not assignable to an unconstrained `P` when
//! `P` appears in `S`, because `P` can be instantiated with any type.
//! Inference correctly infers `P = S[I]`, but the instantiated-param check
//! would trivially pass (`S[I] <: S[I]`), so the original param TypeId must
//! be used for the assignability check instead.
//!
//! Issue: <https://github.com/mohsen1/tsz/issues/6630>

use tsz_checker::context::CheckerOptions;

fn get_codes(source: &str) -> Vec<u32> {
    tsz_checker::test_utils::check_source(source, "test.ts", CheckerOptions::default())
        .into_iter()
        .filter(|d| d.code != 2318)
        .map(|d| d.code)
        .collect()
}

fn has_error(source: &str, code: u32) -> bool {
    get_codes(source).contains(&code)
}

fn no_errors(source: &str) -> bool {
    get_codes(source).is_empty()
}

// ============================================================================
// Issue #6630 – direct repro
// ============================================================================

/// The original repro: recursive `deepMap` passes `value[key]` (type
/// `(T & object)[K]`) to the first parameter of type `T`. Because `T`
/// appears inside the `IndexAccess` object type, this must be TS2345.
#[test]
fn recursive_generic_index_access_emits_ts2345_t_name() {
    let source = r#"
function deepMap<T, U>(value: T, fn: (v: T) => U): U {
    if (typeof value === "object" && value !== null) {
        for (const key in value) {
            deepMap(value[key], fn);
        }
    }
    return fn(value);
}
"#;
    let codes = get_codes(source);
    assert!(
        codes.contains(&2345),
        "Expected TS2345 for value[key] passed to T parameter, got: {codes:?}"
    );
}

/// Same pattern with a different type-parameter name to confirm the fix is
/// not keyed on the identifier spelling.
#[test]
fn recursive_generic_index_access_emits_ts2345_x_name() {
    let source = r#"
function deepMap<X, Y>(value: X, fn: (v: X) => Y): Y {
    if (typeof value === "object" && value !== null) {
        for (const key in value) {
            deepMap(value[key], fn);
        }
    }
    return fn(value);
}
"#;
    assert!(
        has_error(source, 2345),
        "Expected TS2345 with type param named X"
    );
}

/// Same pattern with type-parameter named `Data` (multi-char, user-chosen).
#[test]
fn recursive_generic_index_access_emits_ts2345_data_name() {
    let source = r#"
function deepMap<Data, Out>(value: Data, fn: (v: Data) => Out): Out {
    if (typeof value === "object" && value !== null) {
        for (const key in value) {
            deepMap(value[key], fn);
        }
    }
    return fn(value);
}
"#;
    assert!(
        has_error(source, 2345),
        "Expected TS2345 with type param named Data"
    );
}

// ============================================================================
// Non-recursive calls should NOT emit TS2345
// ============================================================================

/// Calling a DIFFERENT generic function (identity) with `value[key]` should
/// NOT error, because the second function's type parameter is independent.
#[test]
fn non_recursive_indexed_access_no_error() {
    let source = r#"
function identity<U>(v: U): U { return v; }

function deepMap<T, Out>(value: T, fn: (v: T) => Out): Out {
    if (typeof value === "object" && value !== null) {
        for (const key in value) {
            identity(value[key]);
        }
    }
    return fn(value);
}
"#;
    assert!(
        !has_error(source, 2345),
        "identity(value[key]) should not emit TS2345"
    );
}

/// Passing a value of the same type parameter directly (non-IndexAccess)
/// should never emit TS2345.
#[test]
fn regular_value_no_false_positive() {
    let source = r#"
function process<T>(value: T): T { return value; }
function caller<T>(value: T): T { return process(value); }
"#;
    assert!(
        no_errors(source),
        "Passing T directly to T parameter should not emit any errors"
    );
}
