//! Regression tests for issue #10836 — generic method override checks in
//! mixed inheritance chains.
//!
//! "Mixed inheritance chain" covers:
//! 1. Interface `Derived<U>` extends `Base<U>` — both generic, same substituted
//!    method signature should produce no diagnostic.
//! 2. Multi-level interface chain (`C<V> extends B<V> extends A<V>`).
//! 3. Class that `extends GenericBase<T> implements GenericInterface<T>` where
//!    method has method-local generics alongside outer type params.
//! 4. Covariant builder return: derived re-declares method returning derived
//!    interface (covariant override of base returning base interface).
//! 5. Interface extending both generic and non-generic bases.
//!
//! The invariant: tsz must match tsc's variance decisions exactly — no false
//! positives and no missed diagnostics.
//!
//! Issue: <https://github.com/mohsen1/tsz/issues/10836>

use crate::test_utils::check_source_codes;

fn assert_no_2430(src: &str) {
    let codes = check_source_codes(src);
    assert!(
        !codes.contains(&2430),
        "unexpected TS2430 (false positive). Got: {codes:?}\nSource:\n{src}"
    );
}

fn assert_has_2430(src: &str) {
    let codes = check_source_codes(src);
    assert!(
        codes.contains(&2430),
        "expected TS2430, got none. Got: {codes:?}\nSource:\n{src}"
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
// Case 1: Interface Derived<U> extends Base<U> — same substituted method
// signature on both sides. tsc accepts; tsz must not produce TS2430.
// ---------------------------------------------------------------------------

#[test]
fn no_false_2430_derived_redeclares_method_with_matching_substituted_sig() {
    assert_no_2430(
        "interface Base<T> { parse(value: T): T }
         interface Derived<U> extends Base<U> { parse(value: U): U }",
    );
}

// Same rule with renamed type parameters — proves no name-based keying.
#[test]
fn no_false_2430_derived_redeclares_method_matching_sig_renamed_params() {
    assert_no_2430(
        "interface Base<X> { parse(value: X): X }
         interface Derived<Y> extends Base<Y> { parse(value: Y): Y }",
    );
}

// With a generic return type (e.g. Promise) — covariant outer param.
#[test]
fn no_false_2430_derived_redeclares_method_with_promise_return() {
    assert_no_2430(
        "interface Base<T> { parse(value: T): Promise<T> }
         interface Derived<U> extends Base<U> { parse(value: U): Promise<U> }",
    );
}

// Renamed — structural equivalence is the rule, not identifier matching.
#[test]
fn no_false_2430_derived_redeclares_promise_method_renamed() {
    assert_no_2430(
        "interface Base_55<T> { parse(value: T): Promise<T> }
         interface Derived_55<U> extends Base_55<U> { parse(value: U): Promise<U> }",
    );
}

// ---------------------------------------------------------------------------
// Case 2: Multi-level interface chain — three levels of substitution.
// Each level re-declares the method with its outer type parameter; after
// threading T→U→V the signatures must all be structurally equivalent.
// ---------------------------------------------------------------------------

#[test]
fn no_false_2430_three_level_interface_chain_redefine_same_sig() {
    assert_no_2430(
        "interface A<T> { m(x: T): T }
         interface B<U> extends A<U> { m(x: U): U }
         interface C<V> extends B<V> { m(x: V): V }",
    );
}

// Renamed at every level.
#[test]
fn no_false_2430_three_level_interface_chain_redefine_renamed() {
    assert_no_2430(
        "interface A<P> { m(x: P): P }
         interface B<Q> extends A<Q> { m(x: Q): Q }
         interface C<R> extends B<R> { m(x: R): R }",
    );
}

// ---------------------------------------------------------------------------
// Case 3: Covariant builder return — derived re-declares method whose return
// type is the derived interface itself (covariant specialisation of the base
// return). tsc accepts; tsz must not emit TS2430.
// ---------------------------------------------------------------------------

#[test]
fn no_false_2430_covariant_derived_return_builder_pattern() {
    assert_no_2430(
        "interface Base<T> { select(x: T): Base<T> }
         interface Derived<T> extends Base<T> { select(x: T): Derived<T> }",
    );
}

// Renamed.
#[test]
fn no_false_2430_covariant_derived_return_builder_pattern_renamed() {
    assert_no_2430(
        "interface QueryBuilder<O> { execute(): QueryBuilder<O> }
         interface SelectQueryBuilder<O> extends QueryBuilder<O> { execute(): SelectQueryBuilder<O> }",
    );
}

// ---------------------------------------------------------------------------
// Case 4: Method with method-local generic alongside outer type params in a
// mixed chain. Both base and derived have the same method-local generic.
// tsc accepts the override; tsz must not emit TS2430.
// ---------------------------------------------------------------------------

#[test]
fn no_false_2430_method_local_generic_alongside_outer_params_same_sig() {
    assert_no_2430(
        "interface Base<T> { map<R>(fn: (x: T) => R): Base<R> }
         interface Derived<T> extends Base<T> { map<R>(fn: (x: T) => R): Derived<R> }",
    );
}

// Same but with covariant return of derived type — the most common builder
// pattern in kysely and similar fluent APIs.
#[test]
fn no_false_2430_method_local_generic_outer_params_covariant_derived_return() {
    assert_no_2430(
        "interface Builder<DB, O> {
           select<SE extends keyof DB>(col: SE): Builder<DB, Pick<DB, SE>>
         }
         interface SelectBuilder<DB, O> extends Builder<DB, O> {
           select<SE extends keyof DB>(col: SE): SelectBuilder<DB, Pick<DB, SE>>
         }",
    );
}

// Renamed outer params.
#[test]
fn no_false_2430_method_local_generic_covariant_builder_renamed() {
    assert_no_2430(
        "interface Builder<Schema, Result> {
           project<Col extends keyof Schema>(col: Col): Builder<Schema, Pick<Schema, Col>>
         }
         interface SelectBuilder<Schema, Result> extends Builder<Schema, Result> {
           project<Col extends keyof Schema>(col: Col): SelectBuilder<Schema, Pick<Schema, Col>>
         }",
    );
}

// ---------------------------------------------------------------------------
// Case 5: Mixed bases — interface extends both generic and non-generic.
// The non-generic method from the non-generic base must still be checked, and
// the generic-substituted method must also be accepted.
// ---------------------------------------------------------------------------

#[test]
fn no_false_2430_mixed_generic_nongeneric_bases_accept_compatible() {
    assert_no_2430(
        "interface Named { name(): string }
         interface Identifiable<T> { id(): T }
         interface Entity<T> extends Named, Identifiable<T> {
           name(): string
           id(): T
         }",
    );
}

// ---------------------------------------------------------------------------
// Case 6: Class with mixed extends + implements — class inherits from a
// generic base and implements a generic interface, both with the same method.
// tsc accepts; tsz must not produce TS2416 or TS2420.
// ---------------------------------------------------------------------------

#[test]
fn no_false_2416_class_generic_extends_and_implements_matching_method() {
    let codes = check_source_codes(
        "interface IParser<T> { parse(value: T): T }
         class BaseParser<T> { parse(value: T): T { return value } }
         class ConcreteParser<T> extends BaseParser<T> implements IParser<T> {}",
    );
    assert!(
        !codes.contains(&2416) && !codes.contains(&2420),
        "false TS2416/TS2420 for matching generic extends+implements. Got: {codes:?}"
    );
}

// With a Promise return — async variant.
#[test]
fn no_false_2416_class_generic_extends_implements_async_method() {
    let codes = check_source_codes(
        "interface IParser<T> { parse(value: T): Promise<T> }
         class BaseParser<T> { parse(value: T): Promise<T> { return Promise.resolve(value) } }
         class ConcreteParser<T> extends BaseParser<T> implements IParser<T> {}",
    );
    assert!(
        !codes.contains(&2416) && !codes.contains(&2420),
        "false TS2416/TS2420 for async generic extends+implements. Got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Negative controls — must still reject genuine variance violations.
// ---------------------------------------------------------------------------

// Derived changes return type to an unrelated type → TS2430 must fire.
#[test]
fn keeps_2430_derived_changes_return_to_incompatible_type() {
    assert_has_2430(
        "interface Base<T> { parse(value: T): T }
         interface Derived<U> extends Base<U> { parse(value: U): string }",
    );
}

// TypeScript method parameters are bivariant: narrowing is accepted.
// `parse(value: string)` overriding `parse(value: unknown)` passes because
// `string` assignable to `unknown` (covariant direction satisfies bivariance).
#[test]
fn no_false_2430_derived_narrows_param_bivariant() {
    assert_no_2430(
        "interface Base { parse(value: unknown): string }
         interface Derived extends Base { parse(value: string): string }",
    );
}

// Three-level chain where an intermediate level changes the return type.
#[test]
fn keeps_2430_three_level_chain_intermediate_breaks_variance() {
    let codes = check_source_codes(
        "interface A<T> { m(x: T): T }
         interface B<U> extends A<U> { m(x: U): string }
         interface C<V> extends B<V> { m(x: V): V }",
    );
    assert!(
        codes.contains(&2430),
        "expected TS2430 when intermediate level changes return type; got: {codes:?}"
    );
}
