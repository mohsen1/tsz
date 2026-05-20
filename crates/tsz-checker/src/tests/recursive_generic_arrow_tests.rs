//! Tests for recursive generic const arrow functions (BLOCK_SCOPED_VARIABLE symbols).
//!
//! When a const arrow function recursively calls itself with type arguments, the
//! checker must preserve the recursive call as `App(Lazy(def_id), type_args)` rather
//! than collapsing to ERROR. This allows:
//!   1. Property access on the result to resolve correctly.
//!   2. DTS emit to depth-expand the type up to 10 levels then emit `/*elided*/ any`.
//!
//! Structural rule: when a call expression's callee resolves to a symbol currently
//! in `symbol_resolution_set` (circular placeholder), and the callee is an identifier,
//! the call result is `App(Lazy(def_id), type_args)` — not ERROR.

use crate::test_utils::check_source_codes;

fn assert_no_diagnostics(src: &str) {
    let codes = check_source_codes(src);
    assert!(
        codes.is_empty(),
        "expected no diagnostics, got: {codes:?}\nsrc:\n{src}"
    );
}

fn assert_no_ts7023(src: &str) {
    let codes = check_source_codes(src);
    assert!(
        !codes.contains(&7023),
        "unexpected TS7023 (implicit-any return type). Got: {codes:?}\nsrc:\n{src}"
    );
}

fn assert_diagnostic(src: &str, expected_code: u32) {
    let codes = check_source_codes(src);
    assert!(
        codes.contains(&expected_code),
        "expected TS{expected_code}, got: {codes:?}\nsrc:\n{src}"
    );
}

// ---------------------------------------------------------------------------
// Basic: recursive const arrow function with explicit type arg
// ---------------------------------------------------------------------------

#[test]
fn recursive_generic_const_arrow_no_implicit_any_return() {
    // `testRecFun` calls itself with type argument `T & U`.
    // Before the fix, the circular placeholder returned ERROR → any, triggering TS7023.
    assert_no_ts7023(
        r#"
export const testRecFun = <T extends object>(parent: T) => {
    return {
        result: parent,
        deeper: <U extends object>(child: U) =>
            testRecFun<T & U>({ ...parent, ...child })
    };
};
"#,
    );
}

#[test]
fn recursive_generic_const_arrow_property_access_no_error() {
    // Property access on the result of a recursive generic arrow function should
    // resolve correctly — `p2.result.one` must NOT emit TS2339 (property not found).
    assert_no_diagnostics(
        r#"
const testRecFun = <T extends object>(parent: T) => {
    return {
        result: parent,
        deeper: <U extends object>(child: U) =>
            testRecFun<T & U>({ ...parent, ...child })
    };
};
let p1 = testRecFun({ one: '1' });
void p1.result.one;
let p2 = p1.deeper({ two: '2' });
void p2.result.one;
void p2.result.two;
let p3 = p2.deeper({ three: '3' });
void p3.result.one;
void p3.result.two;
void p3.result.three;
"#,
    );
}

// ---------------------------------------------------------------------------
// Variant: different type parameter names (guards against hardcoding)
// ---------------------------------------------------------------------------

#[test]
fn recursive_generic_const_arrow_arbitrary_type_param_names_no_error() {
    // Same shape as testRecFun but with type param names X, Y instead of T, U.
    assert_no_ts7023(
        r#"
export const rec = <X extends object>(a: X) => {
    return {
        val: a,
        next: <Y extends object>(b: Y) =>
            rec<X & Y>({ ...a, ...b })
    };
};
"#,
    );
}

// ---------------------------------------------------------------------------
// Variant: recursive arrow function without explicit type args (no-arg recursion)
// ---------------------------------------------------------------------------

#[test]
fn recursive_const_arrow_no_type_args_no_ts7023() {
    // A const arrow that calls itself without type arguments should also not
    // emit TS7023. This is the simpler non-generic self-referential case.
    assert_no_ts7023(
        r#"
export const loop = () => {
    return { go: () => loop() };
};
"#,
    );
}

// ---------------------------------------------------------------------------
// Negative: unrelated error should still be caught
// ---------------------------------------------------------------------------

#[test]
fn recursive_generic_const_arrow_wrong_return_type_emits_error() {
    // If the recursive arrow has an actual type error, it should still be caught.
    // This verifies we haven't silenced all errors inside recursive arrows.
    let codes = check_source_codes(
        r#"
const bad = <T extends object>(x: T): number => {
    return "not a number";  // TS2322
};
"#,
    );
    assert!(
        codes.contains(&2322),
        "expected TS2322 for explicit return type mismatch. Got: {codes:?}"
    );
}

#[test]
fn recursive_generic_const_arrow_bad_recursive_argument_still_errors() {
    assert_diagnostic(
        r#"
const rec = <T extends object>(parent: T) => {
    return {
        result: parent,
        deeper: () => rec<T>(123)
    };
};
"#,
        2345,
    );
}

#[test]
fn recursive_generic_const_arrow_bad_type_argument_constraint_still_errors() {
    assert_diagnostic(
        r#"
const rec = <T extends object>(parent: T) => {
    return {
        result: parent,
        deeper: () => rec<string>("nope")
    };
};
"#,
        2344,
    );
}

#[test]
fn recursive_generic_const_arrow_extra_type_argument_still_errors() {
    assert_diagnostic(
        r#"
const rec = <Item extends object>(parent: Item) => {
    return {
        result: parent,
        deeper: () => rec<Item, Item>(parent)
    };
};
"#,
        2558,
    );
}
