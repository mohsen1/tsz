//! Regression tests for issue #10681: a class that `implements` an interface
//! whose method is **overloaded** (multiple call signatures) must be checked
//! against the *combined* overload set, not a single (last) overload.
//!
//! tsc's `signaturesRelatedTo` relates a class member against an overloaded
//! interface member using the multi-signature (N×M) path: type parameters are
//! erased to their constraints and parameters are compared contravariantly.
//! Previously tsz rebuilt each interface method-signature declaration
//! individually and let the last one overwrite the property, so a non-generic
//! implementation was compared against a single generic overload whose return
//! type depends on the method type parameter — producing a false TS2416.
//!
//! The reported witness was kysely's `RawBuilderImpl.as` vs `RawBuilder.as`.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

fn ts2416_count(source: &str) -> usize {
    check_source(source, "test.ts", CheckerOptions::default())
        .iter()
        .filter(|d| d.code == 2416)
        .count()
}

/// The reported repro shape: an overloaded generic interface method whose
/// return type depends on the type parameter, implemented by a non-generic
/// method with a broad parameter that accepts every overload's parameter.
/// tsc accepts this; tsz must not emit TS2416.
#[test]
fn overloaded_interface_method_broad_impl_no_ts2416() {
    let source = r#"
interface Expr<T> { readonly t?: T; toNode(): number; }
interface Box<A extends string> { readonly a?: A; }
interface Base {
  as<A extends string>(alias: A): Box<A>;
  as<A extends string>(alias: Expr<any>): Box<A>;
}
class Impl implements Base {
  as(alias: string | Expr<unknown>): Box<string> {
    return {};
  }
}
"#;
    assert_eq!(
        ts2416_count(source),
        0,
        "Non-generic impl that satisfies every overload must not emit TS2416"
    );
}

/// Same rule, different bound-variable spellings (`K`/`P` instead of `A`).
/// If the fix were name-based this would behave differently.
#[test]
fn overloaded_interface_method_renamed_type_params_no_ts2416() {
    let source = r#"
interface Expr<T> { readonly t?: T; toNode(): number; }
interface Box<K extends string> { readonly a?: K; }
interface Base {
  as<K extends string>(alias: K): Box<K>;
  as<P extends string>(alias: Expr<any>): Box<P>;
}
class Impl implements Base {
  as(alias: string | Expr<unknown>): Box<string> {
    return {};
  }
}
"#;
    assert_eq!(ts2416_count(source), 0);
}

/// Three overloads, including a concrete (non-generic) one. The broad impl
/// parameter must accept all three. tsc clean.
#[test]
fn overloaded_interface_method_three_overloads_no_ts2416() {
    let source = r#"
interface Expr<T> { readonly t?: T; toNode(): number; }
interface Box<A extends string> { readonly a?: A; }
interface Base {
  as<A extends string>(alias: A): Box<A>;
  as<A extends string>(alias: Expr<any>): Box<A>;
  as(alias: number): Box<string>;
}
class Impl implements Base {
  as(alias: string | Expr<unknown> | number): Box<string> {
    return {};
  }
}
"#;
    assert_eq!(ts2416_count(source), 0);
}

/// Negative: the impl parameter is too narrow to accept the second overload's
/// parameter (`Expr<any>`). tsc rejects with TS2416 because, after erasing the
/// type parameter, `Expr<any>` is not contravariantly assignable to `string`.
/// The fix must NOT over-accept this.
#[test]
fn overloaded_interface_method_narrow_impl_still_ts2416() {
    let source = r#"
interface Expr<T> { readonly t?: T; toNode(): number; }
interface Box<A extends string> { readonly a?: A; }
interface Base {
  as<A extends string>(alias: A): Box<A>;
  as<A extends string>(alias: Expr<any>): Box<A>;
}
class Impl implements Base {
  as(alias: string): Box<string> {
    return {};
  }
}
"#;
    assert!(
        ts2416_count(source) >= 1,
        "A too-narrow impl parameter must still emit TS2416"
    );
}

/// Negative: the impl return type is genuinely incompatible with the overload
/// return types. The combined-overload comparison must still reject it.
#[test]
fn overloaded_interface_method_incompatible_return_still_ts2416() {
    let source = r#"
interface Base {
  as<A extends string>(alias: A): A;
  as(alias: number): number;
}
class Impl implements Base {
  as(alias: string | number): boolean {
    return true;
  }
}
"#;
    assert!(
        ts2416_count(source) >= 1,
        "An incompatible impl return type must still emit TS2416"
    );
}

/// Negative for overloads specifically: with multiple overloads, tsc compares
/// parameters *contravariantly* (not bivariantly), so an impl whose parameter
/// is narrower than an overload's parameter is rejected.
#[test]
fn overloaded_interface_method_narrower_param_is_contravariant_ts2416() {
    let source = r#"
interface Animal { n: string; }
interface Dog extends Animal { bark(): void; }
interface Base {
  m(x: Animal): void;
  m(x: number): void;
}
class Impl implements Base {
  m(x: Dog | number): void {}
}
"#;
    assert!(
        ts2416_count(source) >= 1,
        "Overloaded-method parameter checks are contravariant; a narrower impl param must emit TS2416"
    );
}

/// A *single* (non-overloaded) interface method keeps tsc's bivariant method
/// parameter rule: a narrower parameter is accepted. The overload fix must not
/// regress this.
#[test]
fn single_method_bivariant_narrower_param_no_ts2416() {
    let source = r#"
interface Animal { n: string; }
interface Dog extends Animal { bark(): void; }
interface Base {
  m(x: Animal): void;
}
class Impl implements Base {
  m(x: Dog): void {}
}
"#;
    assert_eq!(
        ts2416_count(source),
        0,
        "Single-method override keeps bivariant parameters (narrower param accepted)"
    );
}

/// Control: an overloaded interface method where every overload is satisfied
/// by a simple union-parameter impl. tsc clean.
#[test]
fn overloaded_interface_method_all_compatible_no_ts2416() {
    let source = r#"
interface Base {
  m(x: string): void;
  m(x: number): void;
}
class Impl implements Base {
  m(x: string | number): void {}
}
"#;
    assert_eq!(ts2416_count(source), 0);
}
