//! Regression tests for issue #10869 — generic method override variance in
//! class `extends` chains.
//!
//! When a class `extends` a base whose method is *generic* and the override
//! drops or rewrites the method-local type parameter to a concrete type, the
//! override must still satisfy the base method's universal quantification.
//! A concrete `m(x: string): string` is not a valid override of a generic
//! `m<T extends string>(x: T): T`, because a caller could instantiate `T` with
//! a proper subtype of `string` and expect that subtype back. tsc reports
//! TS2416 for these.
//!
//! The class `extends` override path checks methods bivariantly (method
//! parameters are bivariant), but the previous bivariant relation *erased* the
//! base method's method-local generics to their constraints, hiding the
//! mismatch. The `implements` member-override path already used the strict
//! `no_erase_generics` relation and did not have this hole. The fix routes the
//! bivariant override decision through a no-erase-first relation with the same
//! safe-erasure fallback used by the `implements` path, so both heritage forms
//! make identical variance decisions while preserving method-parameter
//! bivariance.

use crate::test_utils::check_source_codes;

fn assert_has_2416(src: &str) {
    let codes = check_source_codes(src);
    assert!(
        codes.contains(&2416),
        "expected TS2416, got none. Got: {codes:?}\nSource:\n{src}"
    );
}

fn assert_no_2416(src: &str) {
    let codes = check_source_codes(src);
    assert!(
        !codes.contains(&2416),
        "unexpected TS2416 (false positive). Got: {codes:?}\nSource:\n{src}"
    );
}

// ---------------------------------------------------------------------------
// Constrained method-local generic in the base, concrete override: tsc rejects
// because the override pins the type parameter to its constraint and can no
// longer return a narrower instantiation. (Was a false negative — bivariant
// path erased the base type parameter.)
// ---------------------------------------------------------------------------

#[test]
fn keeps_2416_constrained_generic_dropped_in_extends_override() {
    assert_has_2416(
        "class Base { m<T extends string>(x: T): T { return x; } }
         class Child extends Base { override m(x: string): string { return x; } }",
    );
}

// Renamed type parameter — proves the rule is structural, not identifier-keyed.
#[test]
fn keeps_2416_constrained_generic_dropped_in_extends_override_renamed() {
    assert_has_2416(
        "class Base { m<K extends string>(x: K): K { return x; } }
         class Child extends Base { override m(x: string): string { return x; } }",
    );
}

// Without the `override` modifier the check must still fire (the modifier only
// affects TS4114-style suggestions, not the compatibility relation).
#[test]
fn keeps_2416_constrained_generic_dropped_no_override_keyword() {
    assert_has_2416(
        "class Base { m<T extends string>(x: T): T { return x; } }
         class Child extends Base { m(x: string): string { return x; } }",
    );
}

// Unconstrained method-local generic (covered before the fix, kept as a guard).
#[test]
fn keeps_2416_unconstrained_generic_dropped_in_extends_override() {
    assert_has_2416(
        "class Base { m<T>(x: T): T { return x; } }
         class Child extends Base { override m(x: string): string { return x; } }",
    );
}

// Method-local generic in a covariant output position only (`T[]`): a concrete
// `string[]` is not a valid override of `T[]` for all `T extends string`.
#[test]
fn keeps_2416_constrained_generic_covariant_array_return() {
    assert_has_2416(
        "class Base { m<T extends string>(x: number): T[] { return []; } }
         class Child extends Base { override m(x: number): string[] { return []; } }",
    );
}

// Bare method-local generic in return position.
#[test]
fn keeps_2416_bare_generic_return_in_extends_override() {
    assert_has_2416(
        "class Base { m<T>(): T { return {} as any; } }
         class Child extends Base { override m(): string { return \"\"; } }",
    );
}

// Abstract generic method, concrete implementation in the subclass.
#[test]
fn keeps_2416_abstract_generic_method_concrete_override() {
    assert_has_2416(
        "abstract class Base { abstract m<T extends string>(x: T): T; }
         class Child extends Base { m(x: string): string { return x; } }",
    );
}

// Generic base *class* plus generic method: the outer type argument is
// substituted (`B = number`) and the method-local generic stays opaque.
#[test]
fn keeps_2416_generic_base_class_with_generic_method() {
    assert_has_2416(
        "class Base<B> { m<T extends string>(x: T, b: B): T { return x; } }
         class Child extends Base<number> { override m(x: string, b: number): string { return x; } }",
    );
}

// ---------------------------------------------------------------------------
// Must STILL be accepted (no false positives).
// ---------------------------------------------------------------------------

// Override re-declares the same method-local generic: the universal
// quantification is preserved, so the override is valid.
#[test]
fn no_false_2416_matching_generic_method_in_extends_override() {
    assert_no_2416(
        "class Base { m<T extends string>(x: T): T { return x; } }
         class Child extends Base { override m<U extends string>(x: U): U { return x; } }",
    );
}

// Renamed type parameters at both levels.
#[test]
fn no_false_2416_matching_generic_method_in_extends_override_renamed() {
    assert_no_2416(
        "class Base { m<P extends string>(x: P): P { return x; } }
         class Child extends Base { override m<Q extends string>(x: Q): Q { return x; } }",
    );
}

// Non-generic base, identical override — unchanged baseline.
#[test]
fn no_false_2416_identical_nongeneric_override() {
    assert_no_2416(
        "class Base<T> { method(v: T): T { return v; } }
         class Child extends Base<string> { override method(v: string): string { return v; } }",
    );
}

// Method-parameter bivariance must be preserved: narrowing a method parameter
// is accepted by tsc (unsound but intentional), and the fix must not regress
// it. (`NO_ERASE_GENERICS` only affects method-local generics, not parameter
// variance, so non-generic overrides are unaffected.)
#[test]
fn no_false_2416_method_param_narrowing_stays_bivariant() {
    assert_no_2416(
        "class Animal {}
         class Dog extends Animal {}
         class Base { handle(x: Animal): void {} }
         class Child extends Base { override handle(x: Dog): void {} }",
    );
}

// Widening a method parameter is also accepted (covariant-safe direction).
#[test]
fn no_false_2416_method_param_widening_accepted() {
    assert_no_2416(
        "class Animal {}
         class Dog extends Animal {}
         class Base { handle(x: Dog): void {} }
         class Child extends Base { override handle(x: Animal): void {} }",
    );
}

// ---------------------------------------------------------------------------
// False-positive guards for the danger regime introduced by the stricter
// `NO_ERASE_GENERICS` branch: the base member has a method-local generic the
// override does NOT carry. In these shapes tsc still ACCEPTS the override, so
// the no-erase relation must not over-report. (Verified against tsc 6.0.2.)
// ---------------------------------------------------------------------------

// Phantom/unused method-local generic: `T` appears nowhere in the signature,
// so dropping it is sound. tsc accepts.
#[test]
fn no_false_2416_phantom_unused_generic_dropped() {
    assert_no_2416(
        "class Base { read<T>(): number { return 0; } }
         class Child extends Base { override read(): number { return 0; } }",
    );
}

// Renamed phantom parameter — structural rule, not identifier-keyed.
#[test]
fn no_false_2416_phantom_unused_generic_dropped_renamed() {
    assert_no_2416(
        "class Base { read<Z>(): number { return 0; } }
         class Child extends Base { override read(): number { return 0; } }",
    );
}

// Phantom constrained generic with a covariant (subtype) return: the override
// returns `Child` where the base returns `Base`, and the dropped `T` is unused.
// tsc accepts the covariant narrowing.
#[test]
fn no_false_2416_phantom_constrained_generic_covariant_return() {
    assert_no_2416(
        "class Base { create<T extends string>(): Base { return this; } }
         class Child extends Base { override create(): Child { return this; } }",
    );
}

// Constrained method-local generic in input-only position with a covariant
// return: `with<K extends string>(k: K): Builder` overridden by
// `with(k: string): SubBuilder`. The wider concrete parameter accepts any
// `K extends string`, and the return narrows covariantly, so tsc accepts.
#[test]
fn no_false_2416_input_only_constrained_generic_covariant_return() {
    assert_no_2416(
        "class Builder { with<K extends string>(k: K): Builder { return this; } }
         class SubBuilder extends Builder { override with(k: string): SubBuilder { return this; } }",
    );
}

// Renamed input-only constrained generic.
#[test]
fn no_false_2416_input_only_constrained_generic_covariant_return_renamed() {
    assert_no_2416(
        "class Builder { with<C extends string>(k: C): Builder { return this; } }
         class SubBuilder extends Builder { override with(k: string): SubBuilder { return this; } }",
    );
}

// Override adds a `this` parameter while dropping a phantom method-local
// generic. The `this` parameter must not be mistaken for an arity/variance
// mismatch; tsc accepts.
#[test]
fn no_false_2416_added_this_param_with_dropped_phantom_generic() {
    assert_no_2416(
        "class Base { m<T extends string>(x: number): number { return x; } }
         class Child extends Base { override m(this: Child, x: number): number { return x; } }",
    );
}
