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
//! Issue: <https://github.com/tsz-org/tsz/issues/10836>

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

// ---------------------------------------------------------------------------
// Case 7: Overloaded method INHERITED from a base class, checked against an
// interface in a `class S extends Base implements I` mixed chain. (Issue
// #10828.) Collecting only the first overload signature of the inherited
// member drops the rest, producing a false TS2416/TS2420 when the interface's
// overload set relies on a later signature. The inherited member's *full*
// overload set must be compared against the interface member, mirroring the
// own-class overloaded path. tsc accepts these (no diagnostic).
// ---------------------------------------------------------------------------

fn assert_no_2416_or_2420(src: &str) {
    let codes = check_source_codes(src);
    assert!(
        !codes.contains(&2416) && !codes.contains(&2420),
        "unexpected TS2416/TS2420 (false positive). Got: {codes:?}\nSource:\n{src}"
    );
}

// Non-generic overloaded method inherited from the base satisfies the
// interface's identical overload set.
#[test]
fn no_false_2416_inherited_overloaded_method_matches_interface_overloads() {
    assert_no_2416_or_2420(
        "interface Schema {
           refine(check: string): number
           refine(check: boolean): number
         }
         class Base {
           refine(check: string): number
           refine(check: boolean): number
           refine(check: any): any { return 0 }
         }
         class S extends Base implements Schema {}",
    );
}

// Same shape with a renamed method so the rule is not keyed on an identifier.
#[test]
fn no_false_2416_inherited_overloaded_method_matches_interface_overloads_renamed() {
    assert_no_2416_or_2420(
        "interface Spec {
           validate(input: string): number
           validate(input: boolean): number
         }
         class Core {
           validate(input: string): number
           validate(input: boolean): number
           validate(input: any): any { return 0 }
         }
         class Impl extends Core implements Spec {}",
    );
}

// Generic builder shape (the reported zod-style repro): the inherited generic
// overload set (a type-guard overload plus a predicate overload, returning the
// builder) satisfies the interface's matching overload set through the
// `extends Base<O> implements Schema<O>` chain.
#[test]
fn no_false_2416_inherited_generic_overloaded_builder_through_mixed_chain() {
    assert_no_2416_or_2420(
        "interface Schema<O> {
           refine<R extends O>(check: (v: O) => v is R): Schema<R>
           refine(check: (v: O) => boolean): Schema<O>
         }
         class Base<O> {
           refine<R extends O>(check: (v: O) => v is R): Base<R>
           refine(check: (v: O) => boolean): Base<O>
           refine(check: any): any { return this }
         }
         class S<O> extends Base<O> implements Schema<O> {}",
    );
}

// Renamed generic builder shape — structural, not identifier-keyed.
#[test]
fn no_false_2416_inherited_generic_overloaded_builder_through_mixed_chain_renamed() {
    assert_no_2416_or_2420(
        "interface Validator<V> {
           narrow<N extends V>(guard: (value: V) => value is N): Validator<N>
           narrow(guard: (value: V) => boolean): Validator<V>
         }
         class BaseValidator<V> {
           narrow<N extends V>(guard: (value: V) => value is N): BaseValidator<N>
           narrow(guard: (value: V) => boolean): BaseValidator<V>
           narrow(guard: any): any { return this }
         }
         class Concrete<V> extends BaseValidator<V> implements Validator<V> {}",
    );
}

// The interface requires the SECOND overload specifically; collecting only the
// first inherited signature would (incorrectly) reject this. tsc accepts.
#[test]
fn no_false_2416_inherited_overloaded_method_interface_needs_second_overload() {
    assert_no_2416_or_2420(
        "interface I { f(x: number): number }
         class Base {
           f(x: string): string
           f(x: number): number
           f(x: any): any { return x }
         }
         class S extends Base implements I {}",
    );
}

// Negative control: the inherited overload set is genuinely incompatible with
// the interface (first overload returns `boolean` where the interface requires
// `string`), so a diagnostic must still fire. The combined-overload comparison
// must not silently accept a real mismatch.
#[test]
fn keeps_diagnostic_for_inherited_overloaded_method_genuine_mismatch() {
    let codes = check_source_codes(
        "interface I {
           f(x: string): boolean
           f(x: number): number
         }
         class Base {
           f(x: string): string
           f(x: number): number
           f(x: any): any { return x }
         }
         class S extends Base implements I {}",
    );
    assert!(
        codes.contains(&2416) || codes.contains(&2420),
        "expected TS2416/TS2420 for genuinely incompatible inherited overload set; got {codes:?}"
    );
}

// Negative control: the interface requires an overload (`boolean` parameter)
// that the inherited overload set cannot service at all. tsc reports.
#[test]
fn keeps_diagnostic_when_inherited_overloads_miss_required_interface_overload() {
    let codes = check_source_codes(
        "interface I {
           f(x: string): string
           f(x: number): number
           f(x: boolean): boolean
         }
         class Base {
           f(x: string): string
           f(x: number): number
           f(x: any): any { return x }
         }
         class S extends Base implements I {}",
    );
    assert!(
        codes.contains(&2416) || codes.contains(&2420),
        "expected TS2416/TS2420 when inherited overloads miss a required interface overload; got {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Case 8: A derived interface that re-declares an inherited method OVERRIDES
// (replaces) the base method — it does not accumulate an overload set across
// `extends`. Only declaration merging (two `interface Foo {}` blocks) and
// module augmentation accumulate same-named method overloads. Anonymous call
// signatures still accumulate because they have no name to override by.
//
// Before this was fixed, a generic derived redeclaration kept BOTH the
// instantiated base signature and the derived one as an overload set, so a
// failing call reported TS2769 ("No overload matches") instead of the single
// signature's TS2345, and a call that should fail against the narrowed derived
// signature was silently accepted against the surviving base signature.
// ---------------------------------------------------------------------------

// Generic derived redeclares the method with the same (substituted) signature.
// A call with the wrong argument must resolve against the single derived
// signature (TS2345), NOT report an overload-set failure (TS2769).
#[test]
fn generic_override_failing_call_reports_single_signature_not_overload() {
    let codes = check_source_codes(
        "interface Observer<T> { next(value: T): void }
         interface Subject<T> extends Observer<T> { next(value: T): void }
         declare const s: Subject<number>;
         s.next(\"x\");",
    );
    assert!(
        codes.contains(&2345) && !codes.contains(&2769),
        "expected single-signature TS2345 (not overload TS2769) for a redeclared \
         generic inherited method; got {codes:?}"
    );
}

// The derived signature NARROWS the inherited parameter. A call that matched
// the wider base parameter must now fail against the narrowed derived
// signature — the base signature must not survive in an overload set.
#[test]
fn generic_override_narrowed_param_rejects_widened_argument() {
    let codes = check_source_codes(
        "interface Base { m(x: string | number): void }
         interface Derived extends Base { m(x: number): void }
         declare const d: Derived;
         d.m(\"s\");",
    );
    assert!(
        codes.contains(&2345),
        "expected TS2345: the narrowed derived signature must replace the base one; got {codes:?}"
    );
}

// Renamed variant — the rule is structural, not keyed on identifiers.
#[test]
fn generic_override_failing_call_reports_single_signature_renamed() {
    let codes = check_source_codes(
        "interface Sink_77<E> { push(item: E): void }
         interface Queue_77<E> extends Sink_77<E> { push(item: E): void }
         declare const q: Queue_77<number>;
         q.push(\"x\");",
    );
    assert!(
        codes.contains(&2345) && !codes.contains(&2769),
        "expected single-signature TS2345 for renamed redeclared generic method; got {codes:?}"
    );
}

// Declaration merging (NOT heritage) of the SAME interface name must still
// accumulate same-named method signatures into an overload set: both calls are
// valid and a third unmatched argument reports the overload-set TS2769.
#[test]
fn declaration_merging_still_accumulates_method_overloads() {
    let codes = check_source_codes(
        "interface Foo<T> { m(x: T): void }
         interface Foo<T> { m(x: string): void }
         declare const f: Foo<number>;
         f.m(42);
         f.m(\"a\");
         f.m(true);",
    );
    assert!(
        codes.contains(&2769),
        "declaration merging must accumulate overloads (true matches neither); got {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Case 9: With the override fix above, the interface-extension compatibility
// check (TS2430) actually relates the derived method against the base method
// (previously masked because the merged overload set trivially contained the
// base signature). A self-returning method that drops an input-only
// method-local generic of a GENERIC base is a valid override and must NOT emit
// a false TS2430 — the dropped generic does not appear in the return, so the
// self-family return is an ordinary covariant position.
// ---------------------------------------------------------------------------

#[test]
fn no_false_2430_self_return_drops_input_only_generic_on_generic_base() {
    assert_no_2430(
        "interface Base<T> { with<K extends string>(k: K, t: T): Base<T> }
         interface Sub<T> extends Base<T> { with(k: string, t: T): Sub<T> }
         declare const d: Sub<number>;",
    );
}

// Renamed — structural, not identifier-keyed.
#[test]
fn no_false_2430_self_return_drops_input_only_generic_renamed() {
    assert_no_2430(
        "interface Cursor<Row> { seek<Key extends string>(k: Key, r: Row): Cursor<Row> }
         interface Scan<Row> extends Cursor<Row> { seek(k: string, r: Row): Scan<Row> }
         declare const c: Scan<number>;",
    );
}

// Negative control: a genuine parameter mismatch (the override takes `number`
// where the base's method-local generic is constrained to `string`) on a
// generic base with a self-family return must still emit TS2430. The
// self-family return must not suppress a real parameter incompatibility.
#[test]
fn keeps_2430_self_return_with_incompatible_param_on_generic_base() {
    assert_has_2430(
        "interface Base<T> { with<K extends string>(k: K, t: T): Base<T> }
         interface Sub<T> extends Base<T> { with(k: number, t: T): Sub<T> }
         declare const d: Sub<number>;",
    );
}

// Negative control: a method-local generic used COVARIANTLY in the return
// (`m(): T`) cannot be dropped to a concrete return (`m(): string`); TS2430
// must still fire.
#[test]
fn keeps_2430_covariant_method_local_generic_dropped_to_concrete_return() {
    assert_has_2430(
        "interface Base { m<T>(): T }
         interface Sub extends Base { m(): string }
         declare const d: Sub;",
    );
}
