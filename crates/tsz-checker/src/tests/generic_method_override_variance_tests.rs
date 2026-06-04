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
//! Issue: <https://github.com/tsz-org/tsz/issues/10681>

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
// Single-signature method-local generic on base, non-generic implementation:
// tsc preserves the target's universal quantification and rejects when the
// constrained parameter appears in a covariant (or invariant) position of
// the return type. Earlier revisions of this file asserted the opposite,
// matching an over-permissive `getBaseSignature`-style erasure path that
// has since been removed; the structural rule now matches tsc.
// ---------------------------------------------------------------------------

#[test]
fn keeps_2416_single_sig_output_param_covariant_position() {
    // `Box<P>` has property `tag: P | number`, so `P` appears in a covariant
    // position of the return type. A concrete `Box<string>` is not a valid
    // override because the caller could instantiate `P` with a narrower
    // subtype of `string` and expect `Box<that subtype>` back.
    assert_has_2416(
        "interface Box<P extends string> { tag: P | number }
         interface I { m<P extends string>(x: number): Box<P> }
         class C implements I { m(x: number): Box<string> { return {} as any } }",
    );
}

// Same structural rule with a different type-parameter name — proves the rule
// is not keyed on a particular identifier.
#[test]
fn keeps_2416_single_sig_output_param_covariant_position_renamed() {
    assert_has_2416(
        "interface Box<Z extends string> { tag: Z | number }
         interface I { m<Z extends string>(x: number): Box<Z> }
         class C implements I { m(x: number): Box<string> { return {} as any } }",
    );
}

// ---------------------------------------------------------------------------
// Contravariant return position: a constrained method-local type parameter
// inside a function/callback return is contravariant in the outer return
// covariance, so `FBox<string>` IS a valid implementation of `FBox<P>` for
// any `P extends string`. tsc accepts this; tsz must too.
// ---------------------------------------------------------------------------

#[test]
fn no_false_2416_single_sig_constrained_param_contravariant_position() {
    assert_no_2416(
        "interface FBox<P extends string> { apply: (value: P) => void }
         interface I { m<T extends string>(x: number): FBox<T> }
         class C implements I { m(x: number): FBox<string> { return {} as any } }",
    );
}

// Phantom type parameter (does not appear in any property): erasable, valid
// override. tsc accepts.
#[test]
fn no_false_2416_single_sig_phantom_constrained_param() {
    assert_no_2416(
        "interface PBox<P extends string> {}
         interface I { m<T extends string>(x: number): PBox<T> }
         class C implements I { m(x: number): PBox<string> { return {} as any } }",
    );
}

// Constrained method-local type parameter appears only in *input* position:
// the implementation's wider parameter accepts any narrower instantiation,
// so the override is valid. tsc accepts.
#[test]
fn no_false_2416_single_sig_constrained_param_input_only() {
    assert_no_2416(
        "interface I { m<T extends string>(x: T): void }
         class C implements I { m(x: string): void {} }",
    );
}

// Bare type parameter in return position: the implementation can never
// satisfy the universal quantification. tsc rejects.
#[test]
fn keeps_2416_single_sig_bare_param_in_return() {
    assert_has_2416(
        "interface I { m<T extends string>(): T }
         class C implements I { m(): string { return \"\" } }",
    );
}

// Invariant container (read+write of same parameter): variance is invariant,
// so the override must be rejected. tsc rejects.
#[test]
fn keeps_2416_single_sig_invariant_position() {
    assert_has_2416(
        "interface Cell<P extends string> { read: P; write: (v: P) => void }
         interface I { m<T extends string>(x: number): Cell<T> }
         class C implements I { m(x: number): Cell<string> { return {} as any } }",
    );
}

// Mixed inheritance chain: `class C extends Base<concrete> implements I` where
// `I` has a method-local generic with a covariant return. The variance
// rejection must apply through the implements check even when the offending
// method is inherited from the extends base (TS2420 family is also acceptable
// — the regression guard is that *some* diagnostic surfaces, not silent
// acceptance).
#[test]
fn keeps_diagnostic_for_inherited_variance_violation_in_mixed_chain() {
    let codes = crate::test_utils::check_source_codes(
        "interface I { m<T extends string>(x: number): { read: T } }
         class Base { m(x: number): { read: string } { return { read: \"\" } } }
         class C extends Base implements I {}",
    );
    assert!(
        codes.contains(&2416) || codes.contains(&2420) || codes.contains(&2430),
        "expected TS2416/TS2420/TS2430 for inherited variance violation; got {codes:?}"
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

// ---------------------------------------------------------------------------
// Mixed inheritance chain: class `extends GenericBase<Concrete>` and
// `implements I` — the inherited method's open base type parameter must be
// substituted with the extends-clause type argument before being compared to
// the interface member. (Issue #10861.)
// ---------------------------------------------------------------------------

#[test]
fn no_false_2416_inherited_method_substitutes_extends_type_argument() {
    assert_no_2416(
        "interface IRepo<T> { save(item: T): T; }
         interface IUserRepo extends IRepo<{ id: number }> {}
         abstract class BaseRepo<T> { save(item: T): T { return item; } }
         class UserRepo extends BaseRepo<{ id: number }> implements IUserRepo {}",
    );
}

// Same structural rule with a renamed base type parameter — proves the fix
// is not keyed on the identifier name (§25).
#[test]
fn no_false_2416_inherited_method_substitutes_extends_type_argument_renamed() {
    assert_no_2416(
        "interface IRepo<K> { save(item: K): K; }
         interface IUserRepo extends IRepo<{ id: number }> {}
         abstract class BaseRepo<K> { save(item: K): K { return item; } }
         class UserRepo extends BaseRepo<{ id: number }> implements IUserRepo {}",
    );
}

// Multi-level extends chain: each level contributes a substitution; an open
// base type parameter must be threaded through every intermediate
// substitution. `T → U[]` from Level1→Level2 then `U → string` from
// Level2→Level3 means an inherited `method(x: T): T` from Level1 must read as
// `(x: string[]) => string[]` when checked against `IFoo<string[]>`.
#[test]
fn no_false_2416_multi_level_extends_chain_substitutes_through() {
    assert_no_2416(
        "interface IFoo<T> { method(x: T): T; }
         class Level1<T> { method(x: T): T { return x; } }
         class Level2<U> extends Level1<U[]> {}
         class Level3 extends Level2<string> implements IFoo<string[]> {}",
    );
}

// Same multi-level shape with renamed type parameters at every level.
#[test]
fn no_false_2416_multi_level_extends_chain_substitutes_through_renamed() {
    assert_no_2416(
        "interface IFoo<A> { method(x: A): A; }
         class Level1<X> { method(x: X): X { return x; } }
         class Level2<Y> extends Level1<Y[]> {}
         class Level3 extends Level2<string> implements IFoo<string[]> {}",
    );
}

// Constructor parameter property inherited from a generic base must also
// have its declared type substituted via the extends clause type argument.
#[test]
fn no_false_2416_inherited_ctor_param_property_substitutes_extends_type_argument() {
    assert_no_2416(
        "interface IHolder<T> { value: T; }
         class BaseHolder<T> { constructor(public value: T) {} }
         class StringHolder extends BaseHolder<string> implements IHolder<string> {}",
    );
}

// Negative case: keep the diagnostic when the inherited method's signature is
// genuinely incompatible with the interface after substitution. (Here the
// inherited `do` returns `number`, not `T`, so even with `T = { id: number }`
// the signatures do not match.) The fix must not silently accept this.
#[test]
fn keeps_diagnostic_for_genuinely_incompatible_inherited_method() {
    let codes = crate::test_utils::check_source_codes(
        "interface IBad<T> { do(x: T): T; }
         interface IBadUser extends IBad<{ id: number }> {}
         abstract class BaseBad<T> { do(x: number): number { return 0; } }
         class BadUser extends BaseBad<{ id: number }> implements IBadUser {}",
    );
    // tsc emits TS2420 for inherited-member shape mismatches here, but the
    // structural rule is that the diagnostic family must fire — either TS2416
    // (property type mismatch) or TS2420 (class incorrectly implements). The
    // critical regression guard is that *some* diagnostic survives, not
    // silent acceptance from substitution-aware collection.
    assert!(
        codes.contains(&2416) || codes.contains(&2420),
        "expected TS2416 or TS2420 for genuinely incompatible inherited method; got {codes:?}"
    );
}
