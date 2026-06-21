//! Regression tests for conditional-flow substitution (`tsc`'s
//! `getConditionalFlowTypeOfType`) and the `TS2344` constraint checks that
//! depend on it.
//!
//! Structural rule: inside the true branch of `T extends C ? … T … : …`, every
//! reference to the check variable `T` carries the implied constraint `T & C`.
//! A use of `T` (or a generic application mentioning `T`) that requires `C` is
//! therefore well-formed. The narrowing must be decisive both ways — a check
//! against an *incompatible* constraint (`T extends number ? Need<T>`) must
//! still emit `TS2344`, and a bare unconstrained reference is not narrowed.
//!
//! All fixtures use a user-defined `extends string` constraint (`Need<S>`) so
//! the cases exercise the constraint machinery without depending on the lib's
//! `Capitalize<S extends string>` declaration, and binder names vary across
//! cases to keep the rule structural.

use crate::test_utils::check_source_codes;

macro_rules! assert_no_2344 {
    ($src:expr, $msg:literal) => {{
        let codes = check_source_codes($src);
        assert!(!codes.contains(&2344), concat!($msg, " Got: {:?}"), codes);
    }};
}

macro_rules! assert_2344 {
    ($src:expr, $msg:literal) => {{
        let codes = check_source_codes($src);
        assert!(codes.contains(&2344), concat!($msg, " Got: {:?}"), codes);
    }};
}

// ---------------------------------------------------------------------------
// Positive cases: the check variable is narrowed in the true branch.
// ---------------------------------------------------------------------------

#[test]
fn bare_check_var_satisfies_string_constraint_in_true_branch() {
    assert_no_2344!(
        "type Need<S extends string> = S;\n\
         type F<T> = T extends string ? Need<T> : never;\n\
         export {};",
        "`Need<T>` in a `T extends string` true branch must be clean."
    );
}

#[test]
fn nested_application_of_check_var_satisfies_string_constraint() {
    // The original issue shape: `Camel<U>` evaluates to a string-shaped template
    // once `U` is narrowed, so `Need<Camel<U>>` is clean.
    assert_no_2344!(
        "type Need<S extends string> = S;\n\
         type Camel<X> = X extends string ? `c${X}` : X;\n\
         type Use<U> = U extends string ? Need<Camel<U>> : never;\n\
         export {};",
        "`Need<Camel<U>>` in a `U extends string` true branch must be clean."
    );
}

#[test]
fn nested_conditional_true_branch_composes_constraints() {
    // `R` is narrowed by both enclosing true branches; the inner use still sees
    // a string-compatible narrowing.
    assert_no_2344!(
        "type Need<S extends string> = S;\n\
         type Deep<R> = R extends string ? (R extends `a${string}` ? Need<R> : Need<R>) : R;\n\
         export {};",
        "doubly-narrowed `Need<R>` in nested true branches must be clean."
    );
}

#[test]
fn tuple_rest_sees_array_constraint_from_true_branch_substitution() {
    let codes = check_source_codes(
        "type Spread<Items> = Items extends unknown[] ? [head: 0, ...Items] : never;\n\
         type Concrete = Spread<[1, 2]>;\n\
         export {};",
    );
    assert!(
        !codes.contains(&2574),
        "tuple rest must see the substitution constraint as array-like. Got: {codes:?}"
    );
}

#[test]
fn mapped_tuple_sees_array_constraint_from_true_branch_substitution() {
    let codes = check_source_codes(
        "type MustBeArray<T extends any[]> = T;\n\
         type MapArray<T extends any[]> = T extends number[] ? MustBeArray<{ [I in keyof T]: 1 }> : never;\n\
         type Concrete = MapArray<[3, 4, 5]>;\n\
         export {};",
    );
    assert!(
        !codes.contains(&2344),
        "mapped tuple must keep the substitution constraint array-like. Got: {codes:?}"
    );
}

#[test]
fn inferred_tail_satisfies_tuple_rest_helper_constraint() {
    let codes = check_source_codes(
        "type PascalCapitalizer<Type, Tuple extends readonly any[] = []> = Type extends [infer Head, ...infer Tail]\n\
         ? Head extends string\n\
           ? PascalCapitalizer<Tail, [...Tuple, Capitalize<Head>]>\n\
           : PascalCapitalizer<Tail, Tuple>\n\
         : Tuple;\n\
         type CamelCapitalizer<Type> = Type extends [infer First, ...infer Tail] ? PascalCapitalizer<Tail, [First]> : [];\n\
         type Concrete = CamelCapitalizer<[\"foo\", \"bar\"]>;\n\
         export {};",
    );
    assert!(
        !codes.contains(&2574),
        "inferred tuple tail must satisfy rest-element array-like grammar. Got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Negative cases: the narrowing must not over-accept.
// ---------------------------------------------------------------------------

#[test]
fn incompatible_constraint_still_emits_2344() {
    // `T` is narrowed to `T & number`, which is NOT assignable to `string`.
    assert_2344!(
        "type Need<S extends string> = S;\n\
         type Bad<T> = T extends number ? Need<T> : never;\n\
         export {};",
        "`Need<T>` narrowed to `T & number` must still emit TS2344."
    );
}

#[test]
fn unconstrained_reference_outside_true_branch_emits_2344() {
    // No enclosing conditional narrows `X`, so it does not satisfy `string`.
    assert_2344!(
        "type Need<S extends string> = S;\n\
         type Bad<X> = Need<X>;\n\
         export {};",
        "`Need<X>` with an unconstrained `X` must emit TS2344."
    );
}
