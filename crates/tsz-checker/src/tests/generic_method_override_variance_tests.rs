//! Regression tests for false-positive TS2416 on generic-method override
//! variance.
//!
//! When a non-generic implementation/override method is checked against a
//! *generic* base/interface method, tsc instantiates the base method's type
//! parameters to their constraints (`getBaseSignature`) before comparing. A
//! constrained type parameter that appears in a covariant output position then
//! reduces to its constraint, so a concrete implementation whose result
//! satisfies the constraint is a valid override. tsz previously kept the base
//! type parameter opaque and emitted a false TS2416.
//!
//! Type parameters with no meaningful constraint must stay opaque so that the
//! universal quantification a generic target demands is preserved: a
//! non-generic `(x: string) => string` is still not a valid implementation of
//! `<T>(x: T) => T`.
//!
//! Issue: <https://github.com/mohsen1/tsz/issues/10681>

use crate::test_utils::check_source_codes;

fn assert_no_2416(src: &str) {
    let codes = check_source_codes(src);
    assert!(!codes.contains(&2416), "unexpected TS2416. Got: {codes:?}");
}

fn assert_has_2416(src: &str) {
    let codes = check_source_codes(src);
    assert!(
        codes.contains(&2416),
        "expected TS2416, got none. Got: {codes:?}"
    );
}

fn assert_has_2430(src: &str) {
    let codes = check_source_codes(src);
    assert!(
        codes.contains(&2430),
        "expected TS2430, got none. Got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Output-only constrained type parameter — valid override.
// ---------------------------------------------------------------------------

#[test]
fn no_false_2416_output_only_constrained_param() {
    assert_no_2416(
        "interface Box<P extends string> { tag: P | number }
         interface I { m<P extends string>(x: number): Box<P> }
         class C implements I { m(x: number): Box<string> { return {} as any } }",
    );
}

// Same structural rule with a different type-parameter name — proves the fix
// is not keyed on a particular identifier (§25).
#[test]
fn no_false_2416_output_only_constrained_param_renamed() {
    assert_no_2416(
        "interface Box<Z extends string> { tag: Z | number }
         interface I { m<Z extends string>(x: number): Box<Z> }
         class C implements I { m(x: number): Box<string> { return {} as any } }",
    );
}

// ---------------------------------------------------------------------------
// Overloaded generic builder method — the reported kysely `as` shape.
// The base method has a generic overload set; the implementation is a single
// broader non-generic signature whose return satisfies the alias constraint.
// ---------------------------------------------------------------------------

#[test]
fn no_false_2416_overloaded_generic_builder_method() {
    assert_no_2416(
        "interface Expr<T> { readonly _t?: T }
         interface Aliased<T, A extends string> { readonly _a?: A; readonly _v?: T }
         interface Builder<O> {
           as<A extends string>(alias: A): Aliased<O, A>
           as<A extends string>(alias: Expr<unknown>): Aliased<O, A>
         }
         class BuilderImpl<O> implements Builder<O> {
           as(alias: string | Expr<unknown>): Aliased<O, string> { return {} as any }
         }",
    );
}

// The same builder shape with a renamed alias type parameter.
#[test]
fn no_false_2416_overloaded_generic_builder_method_renamed() {
    assert_no_2416(
        "interface Expr<T> { readonly _t?: T }
         interface Aliased<T, K extends string> { readonly _a?: K; readonly _v?: T }
         interface Builder<O> {
           as<K extends string>(alias: K): Aliased<O, K>
           as<K extends string>(alias: Expr<unknown>): Aliased<O, K>
         }
         class BuilderImpl<O> implements Builder<O> {
           as(alias: string | Expr<unknown>): Aliased<O, string> { return {} as any }
         }",
    );
}

// ---------------------------------------------------------------------------
// Negative controls — must still reject.
// ---------------------------------------------------------------------------

// Unconstrained type parameter in both input and output positions: a concrete
// `(x: string) => string` cannot satisfy `<T>(x: T) => T` for every `T`.
#[test]
fn keeps_2416_for_unconstrained_generic_identity_method() {
    assert_has_2416(
        "interface I { m<T>(x: T): T }
         class C implements I { m(x: string): string { return x } }",
    );
}

// Same negative control with a renamed parameter.
#[test]
fn keeps_2416_for_unconstrained_generic_identity_method_renamed() {
    assert_has_2416(
        "interface I { m<K>(x: K): K }
         class C implements I { m(x: string): string { return x } }",
    );
}

// A constrained type parameter in a contravariant *input* position whose
// constraint the implementation does not accept must still be rejected: the
// base permits any `N extends number`, but the implementation only accepts
// `string`.
#[test]
fn keeps_2416_when_impl_param_rejects_constraint() {
    assert_has_2416(
        "interface I { m<N extends number>(x: N): void }
         class C implements I { m(x: string): void {} }",
    );
}

// Interface-heritage analogue (TS2430): a derived member pinned to the
// interface's own outer type parameter cannot satisfy a universally quantified
// generic base member where the parameter is bare in a value position. The
// erasure exemption is only for application-only-constrained parameters, so this
// must still be reported. (Regression guard for the
// `callSignatureAssignabilityInInheritance6` conformance family.)
#[test]
fn keeps_2430_outer_param_member_overrides_bare_generic_member() {
    assert_has_2430(
        "interface A { a: <T>(x: T) => T[]; }
         interface I<T> extends A { a: (x: T) => T[]; }",
    );
}

// Same heritage rule with a renamed interface parameter.
#[test]
fn keeps_2430_outer_param_member_overrides_bare_generic_member_renamed() {
    assert_has_2430(
        "interface A { a: <K>(x: K) => K[]; }
         interface I<U> extends A { a: (x: U) => U[]; }",
    );
}
