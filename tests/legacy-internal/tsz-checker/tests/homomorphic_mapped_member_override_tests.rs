//! Regression tests for assignability of a source type to a *generic
//! homomorphic mapped* target `{ [K in keyof T]: ... }` where `T` is still a
//! bare type parameter.
//!
//! Such a target has an unbounded required key-set: a concrete instantiation of
//! `T` may carry members its constraint does not advertise, so a concrete source
//! that merely matches the constraint shape is NOT assignable. `tsc` 6.0 rejects
//! these (TS2322 in value positions; TS2416 in `implements`/override member
//! checks). Only a source genuinely correlated with `T` (one that mentions `T`,
//! e.g. `T` itself or a `Readonly<T>` wrapper) — or a `+?` (Partial) target with
//! an empty/all-optional source — is accepted.
//!
//! tsz previously expanded the target through `T`'s constraint keys and accepted
//! the concrete source. The empty-object (`{}`) override witness additionally
//! leaked through `is_coinductive_return_type_cycle`, which mistook a genuine
//! `{}` return type for an incomplete class instance produced by circular
//! resolution.
//!
//! Issue: <https://github.com/tsz-org/tsz/issues/10812>

use crate::test_utils::check_source_codes;

fn assert_has(code: u32, src: &str) {
    let codes = check_source_codes(src);
    assert!(codes.contains(&code), "expected TS{code}, got {codes:?}");
}

fn assert_no(code: u32, src: &str) {
    let codes = check_source_codes(src);
    assert!(!codes.contains(&code), "unexpected TS{code}, got {codes:?}");
}

// ---------------------------------------------------------------------------
// implements / override member checks (TS2416)
// ---------------------------------------------------------------------------

// Empty-object override of a generic homomorphic mapped return. `{}` cannot
// satisfy `{ [K in keyof T]: T[K] }` for every `T extends object` (e.g.
// `T = { a: 1 }` demands `a`). tsc reports TS2416.
#[test]
fn rejects_empty_object_override_of_generic_homomorphic_mapped_return() {
    assert_has(
        2416,
        "interface I { m<T extends object>(x: number): { [K in keyof T]: T[K] } }
         class C implements I { m(x: number): {} { return {}; } }",
    );
}

// Same rule with a renamed type parameter and mapped variable — proves the rule
// is not keyed on a particular identifier.
#[test]
fn rejects_empty_object_override_of_generic_homomorphic_mapped_return_renamed() {
    assert_has(
        2416,
        "interface I { m<U extends object>(x: number): { [P in keyof U]: U[P] } }
         class C implements I { m(x: number): {} { return {}; } }",
    );
}

// A concrete (non-empty) object that matches the constraint shape is still not a
// valid override: a concrete `T` could carry additional members.
#[test]
fn rejects_concrete_object_override_of_generic_homomorphic_mapped_return() {
    assert_has(
        2416,
        "interface I { m<T extends object>(x: number): { [K in keyof T]: T[K] } }
         class C implements I { m(x: number): { a: number } { return { a: 1 }; } }",
    );
}

// The exact shape reported by the type-fest row: a zero-argument generic
// transform whose homomorphic mapped return is implemented as `{}`.
#[test]
fn rejects_empty_object_override_of_zero_arg_generic_transform() {
    assert_has(
        2416,
        "interface I { transform<T extends object>(): { [K in keyof T]: T[K] } }
         class C implements I { transform(): {} { return {}; } }",
    );
}

// ---------------------------------------------------------------------------
// value-position assignability (TS2322)
// ---------------------------------------------------------------------------

// Returning a freshly-built `{}` where a generic homomorphic mapped type is
// expected is rejected.
#[test]
fn rejects_empty_object_value_assignment_to_generic_homomorphic_mapped() {
    assert_has(
        2322,
        "function f<T extends object>(): { [K in keyof T]: T[K] } { return {} as {}; }",
    );
}

// A concrete object that matches the *constraint* shape exactly is still
// rejected, because a concrete `T` may be a proper subtype with more members.
#[test]
fn rejects_constraint_shaped_object_value_assignment_to_generic_homomorphic_mapped() {
    assert_has(
        2322,
        "function f<T extends { a: number }>(): { [K in keyof T]: T[K] } { return { a: 1 } as { a: number }; }",
    );
}

// ---------------------------------------------------------------------------
// Preserved accepts — must NOT regress to a false TS2322/TS2416.
// ---------------------------------------------------------------------------

// `T` itself is assignable to `{ [K in keyof T]: T[K] }` (the identity mapped
// type preserves `T`'s shape).
#[test]
fn accepts_type_param_source_for_generic_homomorphic_mapped() {
    assert_no(
        2322,
        "function f<T extends object>(t: T): { [K in keyof T]: T[K] } { return t; }",
    );
}

// `Readonly<T>` (homomorphic over the same `T`) remains assignable to the
// identity mapped type.
#[test]
fn accepts_readonly_wrapper_source_for_generic_homomorphic_mapped() {
    assert_no(
        2322,
        "function f<T extends object>(t: Readonly<T>): { [K in keyof T]: T[K] } { return t as any; }",
    );
}

// A `+?` (Partial) target accepts an empty object: every property is optional.
#[test]
fn accepts_empty_object_for_partial_generic_homomorphic_mapped() {
    assert_no(
        2416,
        "interface I { m<T extends object>(x: number): { [K in keyof T]?: T[K] } }
         class C implements I { m(x: number): {} { return {}; } }",
    );
}

// A generic homomorphic mapped *parameter* is contravariant, so a `{}` parameter
// in the implementation accepts the interface's mapped parameter.
#[test]
fn accepts_empty_object_param_against_generic_homomorphic_mapped_param() {
    assert_no(
        2416,
        "interface I { m<T extends object>(x: { [K in keyof T]: T[K] }): void }
         class C implements I { m(x: {}): void {} }",
    );
}

// ---------------------------------------------------------------------------
// `is_coinductive_return_type_cycle` discrimination: a genuine `{}` return must
// not be mistaken for an incomplete class instance from circular resolution.
// ---------------------------------------------------------------------------

// Empty-object override of a *concrete* (non-generic) object-returning member is
// still rejected (the suppression is only for real circular class instances).
#[test]
fn rejects_empty_object_override_of_concrete_object_return() {
    assert_has(
        2416,
        "interface I { m(): { a: number } }
         class C implements I { m(): {} { return {}; } }",
    );
}

// A method that genuinely returns `{}` against an interface member that also
// returns `{}` is accepted — the symbol-less empty object is a complete type.
#[test]
fn accepts_empty_object_override_of_empty_object_return() {
    assert_no(
        2416,
        "interface I { m(): {} }
         class C implements I { m(): {} { return {}; } }",
    );
}

// Self-referential class instance return (a real coinductive shape) must remain
// accepted: the class implements an interface whose member returns the interface
// and the class returns its own instance type.
#[test]
fn accepts_self_referential_class_instance_return() {
    assert_no(
        2416,
        "interface Container { self(): Container; }
         class Impl implements Container { self(): Impl { return this; } }",
    );
}
