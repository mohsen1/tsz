//! Regression tests for issue #10804 — generic method override / callable
//! assignment variance with *outer* type parameters.
//!
//! When two callable types both reference a type parameter from an enclosing
//! scope (a class type parameter for a method override, or an outer function's
//! `<T>` for a function-typed value), tsc still compares them structurally and
//! reports a genuine incompatibility. tsz previously *suppressed* TS2322/TS2416
//! for any callable-to-callable comparison where the source mentioned an outer
//! type parameter and the two signatures were not fully *disjoint* in their
//! bare type-parameter positions. That heuristic hid real mismatches:
//!
//! ```ts
//! class Base<T> { m(x: T[]): T[] { return []; } }
//! class Child<T> extends Base<T> { m(x: T[]): number { return 0; } } // TS2416
//! ```
//!
//! The disjoint-only discriminator could not see the type parameter nested
//! inside `T[]`, so it suppressed the diagnostic even though `number` is not a
//! valid override return for `T[]`. The fix defers to the solver's opaque
//! (`no_erase_generics`) relation — the same one the interface-heritage
//! (TS2430) path already trusts — so a structurally-confirmed mismatch is never
//! suppressed while the unresolved-inference shapes the heuristic guarded stay
//! suppressed.
//!
//! Issue: <https://github.com/mohsen1/tsz/issues/10804>

use crate::test_utils::check_source_codes;

fn assert_has(code: u32, src: &str) {
    let codes = check_source_codes(src);
    assert!(
        codes.contains(&code),
        "expected TS{code}, got none. Got: {codes:?}\nSource:\n{src}"
    );
}

fn assert_no(code: u32, src: &str) {
    let codes = check_source_codes(src);
    assert!(
        !codes.contains(&code),
        "unexpected TS{code} (false positive). Got: {codes:?}\nSource:\n{src}"
    );
}

// ---------------------------------------------------------------------------
// Class override (TS2416): generic `Child<T> extends Base<T>` whose method
// rewrites the return type to an incompatible concrete type. The base method's
// parameter references `T` in a *nested* position (`T[]`, `T | null`, callback,
// rest), which the old disjoint-only heuristic could not see.
// ---------------------------------------------------------------------------

#[test]
fn keeps_2416_generic_child_array_param_return_to_number() {
    assert_has(
        2416,
        "class Base<T> { m(x: T[]): T[] { return []; } }
         class Child<T> extends Base<T> { m(x: T[]): number { return 0; } }",
    );
}

// Renamed type parameters — the rule is structural, not identifier-keyed.
#[test]
fn keeps_2416_generic_child_array_param_return_to_number_renamed() {
    assert_has(
        2416,
        "class Base<A> { m(x: A[]): A[] { return []; } }
         class Child<B> extends Base<B> { m(x: B[]): number { return 0; } }",
    );
}

#[test]
fn keeps_2416_generic_child_union_param_return_to_number() {
    assert_has(
        2416,
        "class Base<T> { m(x: T | null): T { return null as any; } }
         class Child<T> extends Base<T> { m(x: T | null): number { return 0; } }",
    );
}

#[test]
fn keeps_2416_generic_child_callback_param_return_to_number() {
    assert_has(
        2416,
        "class Base<T> { m(cb: (v: T) => void): T { return null as any; } }
         class Child<T> extends Base<T> { m(cb: (v: T) => void): number { return 0; } }",
    );
}

#[test]
fn keeps_2416_generic_child_rest_param_return_to_number() {
    assert_has(
        2416,
        "class Base<T> { m(...xs: T[]): T { return null as any; } }
         class Child<T> extends Base<T> { m(...xs: T[]): number { return 0; } }",
    );
}

// Multi-level generic chain: the offending override sits at the bottom and the
// base method is inherited through an empty intermediate level.
#[test]
fn keeps_2416_multi_level_generic_chain_return_mismatch() {
    assert_has(
        2416,
        "class A<T> { m(x: T[]): T[] { return []; } }
         class B<T> extends A<T> {}
         class C<T> extends B<T> { m(x: T[]): number { return 0; } }",
    );
}

// ---------------------------------------------------------------------------
// Negative controls (class): a structurally-valid override must NOT error.
// ---------------------------------------------------------------------------

#[test]
fn no_false_2416_generic_child_matching_array_signature() {
    assert_no(
        2416,
        "class Base<T> { m(x: T[]): T[] { return []; } }
         class Child<T> extends Base<T> { m(x: T[]): T[] { return x; } }",
    );
}

#[test]
fn no_false_2416_generic_child_covariant_array_return() {
    // Returning `readonly T[]` for a base `T[]`? No — keep it simple: identical
    // signature with a different but compatible parameter name.
    assert_no(
        2416,
        "class Base<T> { m(first: T[]): T[] { return []; } }
         class Child<T> extends Base<T> { m(second: T[]): T[] { return second; } }",
    );
}

// ---------------------------------------------------------------------------
// Direct function-type assignment (TS2322): both sides reference the enclosing
// function's `<T>`. A concrete return must not be silently accepted for a
// type-parameter return.
// ---------------------------------------------------------------------------

#[test]
fn keeps_2322_function_type_param_param_return_mismatch() {
    assert_has(
        2322,
        "function f<T>(a: (x: T) => T, b: (x: T) => number) { a = b; }",
    );
}

#[test]
fn keeps_2322_function_array_param_return_mismatch() {
    assert_has(
        2322,
        "function f<T>(a: (x: T[]) => T[], b: (x: T[]) => number) { a = b; }",
    );
}

// Two outer type parameters swapped between parameter and return positions:
// `(x: T) => U` is not assignable to `(x: U) => T`. The old heuristic treated
// the overlapping `{T, U}` sets as "not disjoint" and suppressed the error.
#[test]
fn keeps_2322_swapped_outer_type_parameters() {
    assert_has(
        2322,
        "function f<T, U>(a: (x: U) => T, b: (x: T) => U) { a = b; }",
    );
}

// ---------------------------------------------------------------------------
// Negative controls (assignment): genuinely compatible callables that both
// reference outer type parameters must NOT be flagged — this is the
// false-positive shape the original suppression guarded.
// ---------------------------------------------------------------------------

#[test]
fn no_false_2322_function_identical_type_param_signatures() {
    assert_no(
        2322,
        "function f<T>(a: (x: T) => T, b: (x: T) => T) { a = b; }",
    );
}

#[test]
fn no_false_2322_function_array_identical_signatures() {
    assert_no(
        2322,
        "function f<T>(a: (x: T[]) => T[], b: (x: T[]) => T[]) { a = b; }",
    );
}
