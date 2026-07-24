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
use tsz_checker::test_utils::{check_source, check_source_strict};

fn ts2416_count(source: &str) -> usize {
    check_source(source, "test.ts", CheckerOptions::default())
        .iter()
        .filter(|d| d.code == 2416)
        .count()
}

fn ts2416_count_strict(source: &str) -> usize {
    check_source_strict(source)
        .iter()
        .filter(|d| d.code == 2416)
        .count()
}

fn ts2416_count_nonstrict(source: &str) -> usize {
    let options = CheckerOptions {
        strict: false,
        strict_function_types: false,
        ..CheckerOptions::default()
    };
    check_source(source, "test.ts", options)
        .iter()
        .filter(|d| d.code == 2416)
        .count()
}

fn ts2322_count_nonstrict(source: &str) -> usize {
    let options = CheckerOptions {
        strict: false,
        strict_function_types: false,
        ..CheckerOptions::default()
    };
    check_source(source, "test.ts", options)
        .iter()
        .filter(|d| d.code == 2322)
        .count()
}

fn heritage_mismatch_count_strict(source: &str) -> usize {
    check_source_strict(source)
        .iter()
        .filter(|d| matches!(d.code, 2416 | 2420))
        .count()
}

const CONTRAVARIANT_NEVER_ANY_OVERLOAD: &str = r#"
interface Failure { readonly failure: true; }
interface Sink<T> { accept: (value: T) => void; }
interface Base<O> {
  choose<K extends keyof O>(key: K): keyof O extends K ? Sink<any> : Failure;
  choose<K extends keyof O>(keys: readonly K[]): keyof O extends K ? Sink<any> : Failure;
}
class Impl<O> implements Base<O> {
  choose(_value: unknown): Sink<never> {
    return { accept: (_value: never) => {} };
  }
}
"#;

const EXPLICIT_CONTRAVARIANT_NEVER_ANY_OVERLOAD: &str = r#"
interface Failure { readonly failure: true; }
interface Sink<in T> { accept: (value: T) => void; }
interface Base<O> {
  choose<K extends keyof O>(key: K): keyof O extends K ? Sink<any> : Failure;
  choose<K extends keyof O>(keys: readonly K[]): keyof O extends K ? Sink<any> : Failure;
}
class Impl<O> implements Base<O> {
  choose(_value: unknown): Sink<never> {
    return { accept: (_value: never) => {} };
  }
}
"#;

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

/// Negative: the erased-return fallback is only valid when the top-level
/// generic application family matches. Sharing a nested generic application is
/// not enough to ignore incompatible outer return families.
#[test]
fn overloaded_interface_method_nested_shared_return_application_still_ts2416() {
    let source = r#"
interface Box<T> { value: T; }
interface SourceWrap<T> { source: T; }
interface TargetWrap<T> { target: T; }
interface Base {
  make(x: "target"): TargetWrap<Box<string>>;
}
class Impl implements Base {
  make(x: string): SourceWrap<Box<number>> {
    return undefined as any;
  }
}
"#;
    assert!(
        ts2416_count(source) >= 1,
        "A nested shared return application base must not erase incompatible outer returns"
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

/// Kysely's `$asTuple` shape: erasing each overload's method-local key
/// parameters makes the return conditional unconditionally select the
/// `ExpressionWrapper` branch. The non-generic implementation's `any` type
/// argument then satisfies every selected wrapper instantiation.
#[test]
fn overloaded_tuple_conditional_return_erased_to_true_branch_no_ts2416() {
    let source = r#"
interface Failure<Message extends string> { readonly failure: Message; }
declare class Chained<T> { readonly value?: T; }
declare class Wrapper<T> {
  readonly value?: T;
  chain(): T extends boolean ? Chained<boolean> : Failure<'not boolean'>;
  notNull(): Wrapper<Exclude<T, null>>;
}
interface Base<O> {
  tuple<K1 extends keyof O, K2 extends Exclude<keyof O, K1>>(
    key1: K1, key2: K2
  ): keyof O extends K1 | K2
    ? Wrapper<[O[K1], O[K2]]>
    : Failure<'missing key'>;
  tuple<
    K1 extends keyof O,
    K2 extends Exclude<keyof O, K1>,
    K3 extends Exclude<keyof O, K1 | K2>
  >(key1: K1, key2: K2, key3: K3): keyof O extends K1 | K2 | K3
    ? Wrapper<[O[K1], O[K2], O[K3]]>
    : Failure<'missing key'>;
  tuple<
    K1 extends keyof O,
    K2 extends Exclude<keyof O, K1>,
    K3 extends Exclude<keyof O, K1 | K2>,
    K4 extends Exclude<keyof O, K1 | K2 | K3>
  >(key1: K1, key2: K2, key3: K3, key4: K4): keyof O extends K1 | K2 | K3 | K4
    ? Wrapper<[O[K1], O[K2], O[K3], O[K4]]>
    : Failure<'missing key'>;
  tuple<
    K1 extends keyof O,
    K2 extends Exclude<keyof O, K1>,
    K3 extends Exclude<keyof O, K1 | K2>,
    K4 extends Exclude<keyof O, K1 | K2 | K3>,
    K5 extends Exclude<keyof O, K1 | K2 | K3 | K4>
  >(
    key1: K1, key2: K2, key3: K3, key4: K4, key5: K5
  ): keyof O extends K1 | K2 | K3 | K4 | K5
    ? Wrapper<[O[K1], O[K2], O[K3], O[K4], O[K5]]>
    : Failure<'missing key'>;
}
class Impl<O> implements Base<O> {
  tuple(): Wrapper<any> {
    return undefined as any;
  }
}
"#;
    assert_eq!(
        ts2416_count(source),
        0,
        "method-local erasure makes every tuple return conditional select the wrapper branch"
    );
}

/// The check operand need not be spelled as `keyof`: any non-distributive
/// outer-generic expression takes the true branch once the method-local
/// extends operand erases to `any`.
#[test]
fn overloaded_indexed_conditional_return_erased_to_true_branch_no_ts2416() {
    let source = r#"
interface Failure { readonly failure: true; }
interface Wrapped<T> { readonly wrapped: T; }
interface Base<O extends { value: unknown }> {
  choose<K>(value: K): O['value'] extends K ? Wrapped<O> : Failure;
  choose<K>(value: readonly K[]): O['value'] extends K ? Wrapped<O> : Failure;
}
class Impl<O extends { value: unknown }> implements Base<O> {
  choose(): Wrapped<any> {
    return undefined as any;
  }
}
"#;
    assert_eq!(
        ts2416_count(source),
        0,
        "a non-distributive indexed check must select the true branch after local erasure"
    );
}

/// Binder names and an alias around the conditional result must not affect the
/// erased-overload rule.
#[test]
fn overloaded_tuple_conditional_return_renamed_alias_no_ts2416() {
    let source = r#"
interface Problem<Message extends string> { readonly problem: Message; }
declare class Parcel<Value> { readonly value?: Value; }
type TupleResult<Row, Left extends keyof Row, Right extends keyof Row> =
  keyof Row extends Left | Right
    ? Parcel<[Row[Left], Row[Right]]>
    : Problem<'incomplete'>;
interface Contract<Row> {
  pack<Left extends keyof Row, Right extends Exclude<keyof Row, Left>>(
    left: Left, right: Right
  ): TupleResult<Row, Left, Right>;
  pack<
    First extends keyof Row,
    Second extends Exclude<keyof Row, First>,
    Third extends Exclude<keyof Row, First | Second>
  >(first: First, second: Second, third: Third):
    keyof Row extends First | Second | Third
      ? Parcel<[Row[First], Row[Second], Row[Third]]>
      : Problem<'incomplete'>;
}
class Provider<Row> implements Contract<Row> {
  pack(): Parcel<any> {
    return undefined as any;
  }
}
"#;
    assert_eq!(
        ts2416_count(source),
        0,
        "renamed binders and an alias wrapper must preserve erased conditional-return parity"
    );
}

/// Erasing a method-local check parameter to `any` keeps tsc's wildcard rule:
/// both conditional branches remain reachable. An implementation returning the
/// false branch is therefore valid and must not be forced through the true
/// branch's application family.
#[test]
fn overloaded_wildcard_any_conditional_keeps_both_return_branches() {
    let source = r#"
interface Failure { readonly failure: true; }
interface Wrapped<T> { readonly wrapped: T; }
interface Base {
  choose<T>(value: T): T extends string ? Wrapped<T> : Failure;
  choose<T>(value: readonly T[]): T extends number ? Wrapped<T> : Failure;
}
class Impl implements Base {
  choose(_value: unknown): Failure {
    return { failure: true };
  }
}
"#;
    assert_eq!(
        ts2416_count(source),
        0,
        "an `any` check must retain both return branches after erasure"
    );
}

/// A conditional that still depends on an outer type parameter after erasing
/// method-local binders is not decidable. The implementation cannot assume its
/// wrapper branch, so the genuine mismatch remains.
#[test]
fn overloaded_outer_deferred_conditional_still_ts2416() {
    let source = r#"
interface Failure { readonly failure: true; }
interface Wrapped<T> { readonly wrapped: T; }
interface Base<O> {
  choose<K extends keyof O>(key: K):
    O extends { ready: true } ? Wrapped<O[K]> : Failure;
  choose<K extends keyof O>(keys: readonly K[]):
    O extends { ready: true } ? Wrapped<O[K]> : Failure;
}
class Impl<O> implements Base<O> {
  choose(): Wrapped<any> {
    return undefined as any;
  }
}
"#;
    assert!(
        ts2416_count(source) >= 1,
        "an outer-generic conditional must remain deferred and reject the wrapper-only implementation"
    );
}

/// A deferred outer-generic conditional remains semantically observable even
/// when it is nested below the same application base as the implementation.
/// The historical same-base fallback must not discard that argument.
#[test]
fn overloaded_nested_deferred_conditional_does_not_reach_same_base_fallback() {
    let source = r#"
interface Box<T> { readonly value: T; }
interface Outer<T> { readonly outer: T; }
interface Failure { readonly failure: true; }
interface Base<O> {
  choose<K extends keyof O>(key: K):
    Outer<O extends { ready: true } ? Box<number> : Failure>;
  choose<K extends keyof O>(keys: readonly K[]):
    Outer<O extends { ready: true } ? Box<number> : Failure>;
}
class Impl<O> implements Base<O> {
  choose(_value: unknown): Outer<Box<any>> { return undefined as any; }
}
"#;
    assert!(ts2416_count_strict(source) >= 1);
}

/// A deferred child does not hide an independent determinate rejection in a
/// sibling property. Normalization leaves the deferred child intact while
/// selecting the method-local conditional in place.
#[test]
fn overloaded_mixed_determinate_and_deferred_return_keeps_known_mismatch() {
    let source = r#"
interface Box<T> { readonly value: T; }
interface Failure { readonly failure: true; }
interface Base<O> {
  choose<K extends keyof O>(key: K): {
    known: keyof O extends K ? Box<never> : Failure;
    pending: O extends { ready: true } ? { yes: O } : { no: O };
  };
  choose<K extends keyof O>(keys: readonly K[]): {
    known: keyof O extends K ? Box<never> : Failure;
    pending: O extends { ready: true } ? { yes: O } : { no: O };
  };
}
class Impl<O> implements Base<O> {
  choose(_value: unknown): {
    known: Box<any>;
    pending: O extends { ready: true } ? { yes: O } : { no: O };
  } { return undefined as any; }
}
"#;
    assert!(ts2416_count_strict(source) >= 1);
}

/// An `infer` binder confined to the unreachable false branch does not make an
/// erased conditional indeterminate. Branch selection inspects only the check,
/// extends, and selected true-branch types.
#[test]
fn overloaded_erased_conditional_ignores_infer_in_unreachable_false_branch() {
    let source = r#"
interface Box<T> { readonly value: T; }
interface Base<O> {
  choose<K extends keyof O>(key: K):
    keyof O extends K ? Box<any> : (K extends infer U ? { bad: U } : never);
  choose<K extends keyof O>(keys: readonly K[]):
    keyof O extends K ? Box<any> : (K extends infer U ? { bad: U } : never);
}
class Impl<O> implements Base<O> {
  choose(_value: unknown): Box<any> { return undefined as any; }
}
"#;
    assert_eq!(ts2416_count_strict(source), 0);
}

/// Resolving the conditional branch must not erase the identity of the outer
/// return family.
#[test]
fn overloaded_conditional_unrelated_return_wrapper_still_ts2416() {
    let source = r#"
interface Failure { readonly failure: true; }
interface TargetWrap<T> { readonly target: T; }
interface SourceWrap<T> { readonly source: T; }
interface Base<O> {
  tuple<K1 extends keyof O, K2 extends Exclude<keyof O, K1>>(
    key1: K1, key2: K2
  ): keyof O extends K1 | K2 ? TargetWrap<[O[K1], O[K2]]> : Failure;
  tuple<
    K1 extends keyof O,
    K2 extends Exclude<keyof O, K1>,
    K3 extends Exclude<keyof O, K1 | K2>
  >(key1: K1, key2: K2, key3: K3):
    keyof O extends K1 | K2 | K3
      ? TargetWrap<[O[K1], O[K2], O[K3]]>
      : Failure;
}
class Impl<O> implements Base<O> {
  tuple(): SourceWrap<any> {
    return undefined as any;
  }
}
"#;
    assert!(
        ts2416_count(source) >= 1,
        "an unrelated top-level wrapper family must remain incompatible"
    );
}

/// Selecting a determinate conditional branch must retain its application
/// arguments. Matching outer wrapper identities do not make incompatible
/// payloads interchangeable.
#[test]
fn overloaded_conditional_same_wrapper_incompatible_payload_still_ts2416() {
    let source = r#"
interface Failure { readonly failure: true; }
interface Box<T> { readonly value: T; }
interface Base<O> {
  choose<K extends keyof O>(key: K): keyof O extends K ? Box<number> : Failure;
  choose<K extends keyof O>(keys: readonly K[]): keyof O extends K ? Box<number> : Failure;
}
class Impl<O> implements Base<O> {
  choose(_value: unknown): Box<string> {
    return { value: 'wrong' };
  }
}
"#;
    assert!(
        ts2416_count(source) >= 1,
        "the selected return branch must compare Box<string> against Box<number>"
    );
}

/// An erased `any` slot must not hide an independent concrete mismatch in a
/// different application argument.
#[test]
fn overloaded_conditional_any_slot_keeps_other_payload_mismatch_ts2416() {
    let source = r#"
interface Failure { readonly failure: true; }
interface Pair<Left, Right> { readonly left: Left; readonly right: Right; }
interface Base<O> {
  choose<K extends keyof O>(key: K):
    keyof O extends K ? Pair<number, boolean> : Failure;
  choose<K extends keyof O>(keys: readonly K[]):
    keyof O extends K ? Pair<number, boolean> : Failure;
}
class Impl<O> implements Base<O> {
  choose(_value: unknown): Pair<any, string> {
    return { left: undefined as any, right: 'wrong' };
  }
}
"#;
    assert!(
        ts2416_count(source) >= 1,
        "the wildcard first slot must not silence the string/boolean mismatch"
    );
}

/// `any` is not assignable to `never`; overload erasure must preserve that
/// bottom-type exception even when the enclosing generic identity matches.
#[test]
fn overloaded_conditional_any_payload_does_not_satisfy_never_ts2416() {
    let source = r#"
interface Failure { readonly failure: true; }
interface Box<T> { readonly value: T; }
interface Base<O> {
  choose<K extends keyof O>(key: K): keyof O extends K ? Box<never> : Failure;
  choose<K extends keyof O>(keys: readonly K[]): keyof O extends K ? Box<never> : Failure;
}
class Impl<O> implements Base<O> {
  choose(_value: unknown): Box<any> {
    return { value: undefined as any };
  }
}
"#;
    assert!(
        ts2416_count_strict(source) >= 1,
        "the erased wildcard must not make any assignable to never"
    );
}

/// Inherited public members use the general heritage relation boundary rather
/// than the direct-member boundary. Preserve the raw overload return identity
/// there as well, before whole-member preparation expands `Box<any>`.
#[test]
fn inherited_overloaded_conditional_any_never_reports_heritage_mismatch() {
    let source = r#"
interface Failure { readonly failure: true; }
interface Box<T> { readonly value: T; }
interface Base<O> {
  choose<K extends keyof O>(key: K): keyof O extends K ? Box<never> : Failure;
  choose<K extends keyof O>(keys: readonly K[]): keyof O extends K ? Box<never> : Failure;
}
class Parent<O> {
  choose(_value: unknown): Box<any> {
    return { value: undefined as any };
  }
}
class Impl<O> extends Parent<O> implements Base<O> {}
"#;
    assert!(
        heritage_mismatch_count_strict(source) >= 1,
        "an inherited implementation must retain the overload return mismatch"
    );
}

/// The `any`/`never` exception is directional. In a contravariant application
/// slot, relating `Sink<any>` to `Sink<never>` checks `never` against `any` and
/// remains valid under strict function types.
#[test]
fn overloaded_conditional_contravariant_any_never_no_ts2416() {
    let source = r#"
interface Failure { readonly failure: true; }
interface Sink<T> { accept: (value: T) => void; }
interface Base<O> {
  choose<K extends keyof O>(key: K): keyof O extends K ? Sink<never> : Failure;
  choose<K extends keyof O>(keys: readonly K[]): keyof O extends K ? Sink<never> : Failure;
}
class Impl<O> implements Base<O> {
  choose(_value: unknown): Sink<any> {
    return { accept: (_value: any) => {} };
  }
}
"#;
    assert_eq!(
        ts2416_count_strict(source),
        0,
        "the contravariant slot must compare never to any in reverse"
    );
}

/// Reversing the intrinsic arguments also reverses the contravariant relation:
/// this would require `any` to be assignable to `never`, which tsc rejects.
#[test]
fn overloaded_conditional_contravariant_never_any_still_ts2416() {
    assert!(
        ts2416_count_strict(CONTRAVARIANT_NEVER_ANY_OVERLOAD) >= 1,
        "contravariance reverses the pair and must reject any to never"
    );
}

/// Without strict function types, function-property parameters are bivariant.
/// The same `Sink<never>`/`Sink<any>` pair that fails above is therefore valid.
#[test]
fn overloaded_conditional_contravariant_never_any_nonstrict_no_ts2416() {
    assert_eq!(
        ts2416_count_nonstrict(CONTRAVARIANT_NEVER_ANY_OVERLOAD),
        0,
        "non-strict callback bivariance must remain authoritative"
    );
}

/// Explicit variance annotations remain authoritative even when callback
/// parameters would otherwise be bivariant.
#[test]
fn explicit_contravariant_never_any_nonstrict_still_ts2416() {
    assert!(
        ts2416_count_nonstrict(EXPLICIT_CONTRAVARIANT_NEVER_ANY_OVERLOAD) >= 1,
        "an explicit `in` annotation must reject the reverse pair"
    );
}

/// A declared variance mask can be partial. Unannotated slots still use their
/// structurally inferred variance instead of becoming independent.
#[test]
fn partial_declared_variance_keeps_unannotated_any_never_rejection() {
    let source = r#"
interface Failure { readonly failure: true; }
interface Pair<out Left, Right> {
  readonly left: Left;
  readonly right: Right;
}
interface Base<O> {
  choose<K extends keyof O>(key: K): keyof O extends K ? Pair<string, never> : Failure;
  choose<K extends keyof O>(keys: readonly K[]): keyof O extends K ? Pair<string, never> : Failure;
}
class Impl<O> implements Base<O> {
  choose(_value: unknown): Pair<string, any> {
    return { left: 'ok', right: undefined as any };
  }
}
"#;
    assert!(
        ts2416_count_strict(source) >= 1,
        "the unannotated covariant slot must still reject any to never"
    );
}

/// The same partially annotated mask applies outside overload checking. A
/// mixed argument list must inspect the exceptional slot rather than treating
/// an unrelated identical slot as permission for `any` to satisfy `never`.
#[test]
fn partial_declared_variance_direct_mixed_any_never_rejection() {
    let source = r#"
interface Pair<out Left, Right> {
  readonly left: Left;
  readonly right: Right;
}
declare const pair: Pair<string, any>;
const target: Pair<string, never> = pair;
"#;
    let errors = check_source_strict(source)
        .iter()
        .filter(|diagnostic| diagnostic.code == 2322)
        .count();
    assert_eq!(errors, 1);
}

/// Computing the structural variance of an unannotated root slot must still
/// honor explicit annotations on nested generics. Here the second `Pair` slot
/// inherits `Cell`'s explicit invariance even with callback bivariance enabled.
#[test]
fn partial_declared_variance_honors_nested_explicit_variance() {
    let source = r#"
interface Cell<in out T> { readonly value: T; }
interface Pair<out Left, Right> {
  readonly left: Left;
  readonly inner: Cell<Right>;
}
declare const pair: Pair<string, never>;
const target: Pair<string, any> = pair;
"#;
    assert_eq!(ts2322_count_nonstrict(source), 1);
}

/// A failed exceptional classifier is undecided rather than incompatible.
/// Nonstrict function-property bivariance therefore remains owned by the
/// structural relation.
#[test]
fn direct_generic_any_never_keeps_nonstrict_callback_bivariance() {
    let source = r#"
interface Sink<T> { accept: (value: T) => void; }
declare const sink: Sink<never>;
const target: Sink<any> = sink;
"#;
    assert_eq!(ts2322_count_nonstrict(source), 0);
}

/// Conversely, a non-callback structural operator can reject the same raw
/// argument pair in nonstrict mode. The all-`any` shortcut must not accept an
/// undecided variance classification before `keyof` is expanded.
#[test]
fn direct_generic_any_never_nonstrict_keyof_uses_structural_relation() {
    let source = r#"
interface Keys<T> { readonly key: keyof T; }
declare const keys: Keys<never>;
const target: Keys<any> = keys;
"#;
    assert_eq!(ts2322_count_nonstrict(source), 1);
}

/// A transparent source alias must inherit the body generic's variance instead
/// of letting its `any` argument bypass the `never` exception.
#[test]
fn pass_through_source_alias_any_does_not_satisfy_body_never() {
    let source = r#"
interface Failure { readonly failure: true; }
interface Box<T> { readonly value: T; }
type Alias<T> = Box<T>;
interface Base<O> {
  choose<K extends keyof O>(key: K): keyof O extends K ? Box<never> : Failure;
  choose<K extends keyof O>(keys: readonly K[]): keyof O extends K ? Box<never> : Failure;
}
class Impl<O> implements Base<O> {
  choose(_value: unknown): Alias<any> { return undefined as any; }
}
"#;
    assert!(
        ts2416_count_strict(source) >= 1,
        "the alias/body shortcut must preserve covariant any-to-never rejection"
    );
}

/// The opposite alias orientation retains contravariant argument direction.
#[test]
fn pass_through_target_alias_any_keeps_contravariant_never_rejection() {
    let source = r#"
interface Failure { readonly failure: true; }
interface Sink<T> { accept: (value: T) => void; }
type Alias<T> = Sink<T>;
interface Base<O> {
  choose<K extends keyof O>(key: K): keyof O extends K ? Alias<any> : Failure;
  choose<K extends keyof O>(keys: readonly K[]): keyof O extends K ? Alias<any> : Failure;
}
class Impl<O> implements Base<O> {
  choose(_value: unknown): Sink<never> { return undefined as any; }
}
"#;
    assert!(
        ts2416_count_strict(source) >= 1,
        "the body/alias shortcut must preserve contravariant any-to-never rejection"
    );
}

/// Exact pass-through alias chains inherit the ultimate interface variance;
/// each wrapper level must preserve the covariant `any`/`never` rejection.
#[test]
fn pass_through_alias_chain_any_never_keeps_body_rejection() {
    let source = r#"
interface Box<T> { readonly value: T; }
interface Outer<T> { readonly value: T; }
type Middle<T> = Box<T>;
type Alias<T> = Middle<T>;
declare const direct: Alias<any>;
declare const nested: Outer<Alias<any>>;
const badDirect: Middle<never> = direct;
const badNested: Outer<Middle<never>> = nested;
"#;
    let errors = check_source_strict(source)
        .iter()
        .filter(|diagnostic| diagnostic.code == 2322)
        .count();
    assert_eq!(errors, 2);
}

/// Turning off strict function types makes callback-derived contravariance
/// bivariant even when one side retains a transparent pass-through alias.
#[test]
fn target_alias_nonstrict_keeps_callback_bivariance() {
    let callback = r#"
interface Failure { readonly failure: true; }
interface Sink<T> { accept: (value: T) => void; }
type Alias<T> = Sink<T>;
interface Base<O> {
  choose<K extends keyof O>(key: K): keyof O extends K ? Alias<any> : Failure;
  choose<K extends keyof O>(keys: readonly K[]): keyof O extends K ? Alias<any> : Failure;
}
class Impl<O> implements Base<O> {
  choose(_value: unknown): Sink<never> { return undefined as any; }
}
"#;
    assert_eq!(ts2416_count_nonstrict(callback), 0);
}

/// Matching base/arity is not enough to prove pass-through: this alias ignores
/// its parameter, so `Alias<any>` is exactly `Box<never>` and remains valid.
#[test]
fn constant_body_alias_any_still_satisfies_body_never() {
    let source = r#"
interface Failure { readonly failure: true; }
interface Box<T> { readonly value: T; }
type Alias<T> = Box<never>;
interface Base<O> {
  choose<K extends keyof O>(key: K): keyof O extends K ? Box<never> : Failure;
  choose<K extends keyof O>(keys: readonly K[]): keyof O extends K ? Box<never> : Failure;
}
class Impl<O> implements Base<O> {
  choose(_value: unknown): Alias<any> { return undefined as any; }
}
"#;
    assert_eq!(
        ts2416_count_strict(source),
        0,
        "a constant-body alias must not be classified as pass-through"
    );
}

/// An indexed-access alias is a transparent transform, so its expanded result
/// remains authoritative even when the raw application arguments are
/// `any`/`never`. Here both target and implementation normalize compatibly.
#[test]
fn indexed_access_alias_any_never_uses_structural_result() {
    let source = r#"
interface Failure { readonly failure: true; }
type Field<T> = ({ value: T } | { value: string })['value'];
interface Base<O> {
  choose<K extends keyof O>(key: K): keyof O extends K ? Field<never> : Failure;
  choose<K extends keyof O>(keys: readonly K[]): keyof O extends K ? Field<never> : Failure;
}
class Impl<O> implements Base<O> {
  choose(_value: unknown): Field<any> { return undefined as any; }
}
"#;
    assert_eq!(
        ts2416_count_strict(source),
        0,
        "indexed-access aliases must be decided by their expanded result"
    );
}

/// Ordinary type aliases are transparent too. Under a nominal outer wrapper,
/// expanding the union makes the nested source `any` and target `string`, so
/// raw alias arguments cannot force a rejection before normalization.
#[test]
fn nested_transparent_union_alias_any_never_uses_normalized_result() {
    let source = r#"
type Alias<T> = T | string;
interface Outer<T> { readonly value: T; }
declare const nested: Outer<Alias<any>>;
const nestedTarget: Outer<Alias<never>> = nested;
"#;
    let errors = check_source_strict(source)
        .iter()
        .filter(|diagnostic| diagnostic.code == 2322)
        .count();
    assert_eq!(errors, 0);
}

/// Alias applications use variance only for the body kinds on which
/// TypeScript supports variance measurement. Object and mapped bodies retain
/// the raw argument mismatch; union, tuple, and conditional transforms expand
/// to their normalized results.
#[test]
fn alias_any_never_respects_variance_supported_body_kinds() {
    let source = r#"
type ObjectAlias<T> = { readonly value: T | string };
type MappedAlias<T> = { [K in 'value']: T | string };
type UnionAlias<T> = T | string;
type TupleAlias<T> = [T | string];
type ConditionalAlias<T> = [T] extends [unknown] ? string : string;
interface Holder<T> { readonly value: T; }

declare const objectAny: Holder<ObjectAlias<any>>;
declare const mappedAny: Holder<MappedAlias<any>>;
declare const unionAny: Holder<UnionAlias<any>>;
declare const tupleAny: Holder<TupleAlias<any>>;
declare const conditionalAny: Holder<ConditionalAlias<any>>;

const badObject: Holder<ObjectAlias<never>> = objectAny;
const badMapped: Holder<MappedAlias<never>> = mappedAny;
const goodUnion: Holder<UnionAlias<never>> = unionAny;
const goodTuple: Holder<TupleAlias<never>> = tupleAny;
const goodConditional: Holder<ConditionalAlias<never>> = conditionalAny;
"#;
    let errors = check_source_strict(source)
        .iter()
        .filter(|diagnostic| diagnostic.code == 2322)
        .count();
    assert_eq!(errors, 2);
}

/// A supported alias body's explicit variance annotation is authoritative in
/// the reverse direction too, even though the expanded object alone would
/// accept `never` as a source for an `any` property.
#[test]
fn explicit_invariant_object_alias_rejects_never_to_any() {
    let source = r#"
type Alias<in out T> = { readonly value: T };
declare const alias: Alias<never>;
const target: Alias<any> = alias;
"#;
    let errors = check_source_strict(source)
        .iter()
        .filter(|diagnostic| diagnostic.code == 2322)
        .count();
    assert_eq!(errors, 1);
}

/// Instantiation comparison honors a supported alias body's declared
/// contravariance even when validation separately reports that the annotation
/// disagrees with the body. The declaration error must not be compounded by a
/// spurious assignment error.
#[test]
fn explicit_contravariant_object_alias_accepts_declared_direction() {
    let source = r#"
type Alias<in T> = { readonly value: T };
declare const alias: Alias<any>;
const target: Alias<never> = alias;
"#;
    let diagnostics = check_source_strict(source);
    assert!(diagnostics.iter().any(|diagnostic| diagnostic.code == 2636));
    assert_eq!(
        diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == 2322)
            .count(),
        0
    );
}

/// The erased-overload classifier also defers same-base transparent aliases to
/// their expanded result. A normalizing union stays valid, while an exact
/// pass-through to a covariant interface retains the genuine mismatch.
#[test]
fn overloaded_same_alias_any_never_follows_expanded_body() {
    let union_alias = r#"
interface Failure { readonly failure: true; }
type Alias<T> = T | string;
interface Base<O> {
  choose<K extends keyof O>(key: K): keyof O extends K ? Alias<never> : Failure;
  choose<K extends keyof O>(keys: readonly K[]): keyof O extends K ? Alias<never> : Failure;
}
class Impl<O> implements Base<O> {
  choose(_value: unknown): Alias<any> { return undefined as any; }
}
"#;
    assert_eq!(ts2416_count_strict(union_alias), 0);

    let pass_through = r#"
interface Failure { readonly failure: true; }
interface Box<T> { readonly value: T; }
type Alias<T> = Box<T>;
interface Base<O> {
  choose<K extends keyof O>(key: K): keyof O extends K ? Alias<never> : Failure;
  choose<K extends keyof O>(keys: readonly K[]): keyof O extends K ? Alias<never> : Failure;
}
class Impl<O> implements Base<O> {
  choose(_value: unknown): Alias<any> { return undefined as any; }
}
"#;
    assert!(
        ts2416_count_strict(pass_through) > 0,
        "a pass-through alias must retain the expanded interface mismatch"
    );
}

fn nested_overload_source(
    declarations: &str,
    target_return: &str,
    implementation_return: &str,
) -> String {
    format!(
        r#"
{declarations}
interface Failure {{ readonly failure: true; }}
interface Outer<T> {{ readonly outer: T; }}
interface Base<O> {{
  choose<K extends keyof O>(key: K): keyof O extends K ? {target_return} : Failure;
  choose<K extends keyof O>(keys: readonly K[]): keyof O extends K ? {target_return} : Failure;
}}
class Impl<O> implements Base<O> {{
  choose(_value: unknown): {implementation_return} {{ return undefined as any; }}
}}
"#
    )
}

/// Reliable variance composes through nested applications. Covariance keeps
/// argument direction while contravariance reverses it at the inner wrapper.
#[test]
fn nested_any_never_variance_composes_in_strict_mode() {
    let declarations = r#"
interface Box<T> { readonly value: T; }
interface Sink<T> { accept: (value: T) => void; }
"#;
    let cases = [
        ("Outer<Box<never>>", "Outer<Box<any>>", true),
        ("Outer<Box<any>>", "Outer<Box<never>>", false),
        ("Outer<Sink<any>>", "Outer<Sink<never>>", true),
        ("Outer<Sink<never>>", "Outer<Sink<any>>", false),
    ];
    for (target, implementation, rejects) in cases {
        let source = nested_overload_source(declarations, target, implementation);
        assert_eq!(
            ts2416_count_strict(&source) > 0,
            rejects,
            "unexpected nested variance result for {implementation} -> {target}"
        );
    }
}

/// Explicit invariance remains bidirectional through a covariant outer
/// wrapper, while an unused type parameter is independent in both directions.
#[test]
fn nested_explicit_invariant_and_independent_positions() {
    let invariant = "interface Cell<in out T> { get(): T; set: (value: T) => void; }";
    for (target, implementation) in [
        ("Outer<Cell<never>>", "Outer<Cell<any>>"),
        ("Outer<Cell<any>>", "Outer<Cell<never>>"),
    ] {
        let source = nested_overload_source(invariant, target, implementation);
        assert!(
            ts2416_count_nonstrict(&source) > 0,
            "explicit invariance must reject {implementation} -> {target}"
        );
    }

    let independent = "interface Phantom<T> { readonly tag: string; }";
    for (target, implementation) in [
        ("Outer<Phantom<never>>", "Outer<Phantom<any>>"),
        ("Outer<Phantom<any>>", "Outer<Phantom<never>>"),
    ] {
        let source = nested_overload_source(independent, target, implementation);
        assert_eq!(
            ts2416_count_strict(&source),
            0,
            "independent arguments must accept {implementation} -> {target}"
        );
    }
}

/// In non-strict mode inferred parameter variance retains callback bivariance;
/// an explicit `in` annotation remains authoritative. Inferred invariance keeps
/// only its covariant rejection direction.
#[test]
fn nested_any_never_variance_respects_nonstrict_bivariance() {
    let inferred_sink = "interface Sink<T> { accept: (value: T) => void; }";
    let source = nested_overload_source(inferred_sink, "Outer<Sink<any>>", "Outer<Sink<never>>");
    assert_eq!(ts2416_count_nonstrict(&source), 0);

    let explicit_sink = "interface Sink<in T> { accept: (value: T) => void; }";
    let source = nested_overload_source(explicit_sink, "Outer<Sink<any>>", "Outer<Sink<never>>");
    assert!(ts2416_count_nonstrict(&source) > 0);

    let inferred_cell = "interface Cell<T> { get(): T; set: (value: T) => void; }";
    let reverse = nested_overload_source(inferred_cell, "Outer<Cell<any>>", "Outer<Cell<never>>");
    assert_eq!(ts2416_count_nonstrict(&reverse), 0);
    let forward = nested_overload_source(inferred_cell, "Outer<Cell<never>>", "Outer<Cell<any>>");
    assert!(ts2416_count_nonstrict(&forward) > 0);
}

/// The general application fast path follows the same directional rule outside
/// heritage and overload checking.
#[test]
fn direct_generic_any_never_assignments_follow_strict_variance() {
    let source = r#"
interface Box<T> { readonly value: T; }
interface Sink<T> { accept: (value: T) => void; }
declare const boxAny: Box<any>;
declare const sinkAny: Sink<any>;
declare const sinkNever: Sink<never>;
const badBox: Box<never> = boxAny;
const badSink: Sink<any> = sinkNever;
const goodSink: Sink<never> = sinkAny;
"#;
    let errors = check_source_strict(source)
        .iter()
        .filter(|d| d.code == 2322)
        .count();
    assert_eq!(errors, 2, "covariance and contravariance are directional");
}

/// Four overloads exercise accumulation beyond the first combined pair. The
/// implementation is too narrow only for the first overload; dropping that
/// signature would hide tsc's genuine TS2416.
#[test]
fn four_overloads_preserve_first_incompatible_signature() {
    let source = r#"
interface Animal { name: string; }
interface Dog extends Animal { bark(): void; }
interface Base {
  handle(value: Animal): void;
  handle(value: string): void;
  handle(value: number): void;
  handle(value: boolean): void;
}
class Impl implements Base {
  handle(_value: Dog | string | number | boolean): void {}
}
"#;
    assert!(
        ts2416_count(source) >= 1,
        "the first overload must survive accumulation and retain its contravariant mismatch"
    );
}

/// Invalid mixed method/property declarations still retain the complete method
/// overload family for recovery. Duplicate-declaration diagnostics do not make
/// it safe to discard the first method and hide its implementation mismatch.
#[test]
fn intervening_property_does_not_discard_first_method_overload() {
    let source = r#"
interface Animal { name: string; }
interface Base {
  handle(value: Animal): void;
  handle: (value: string) => void;
  handle(value: number): void;
  handle(value: boolean): void;
}
class Impl implements Base {
  handle(_value: number | boolean): void {}
}
"#;
    assert!(
        ts2416_count(source) >= 1,
        "invalid mixed-declaration recovery must retain the first method overload"
    );
}

/// Once a property terminates an invalid mixed declaration family, later
/// methods do not become a replacement overload set. An implementation that
/// satisfies the initial method therefore remains compatible.
#[test]
fn intervening_property_freezes_initial_method_family() {
    let source = r#"
interface Animal { name: string; }
interface Base {
  handle(value: Animal): void;
  handle: (value: string) => void;
  handle(value: number): void;
  handle(value: boolean): void;
}
class Impl implements Base {
  handle(_value: Animal): void {}
}
"#;
    assert_eq!(ts2416_count(source), 0);
}

/// When the first declaration is a property, that property remains the
/// implementation target and later same-name methods are ignored for recovery.
#[test]
fn property_first_mixed_declaration_remains_authoritative() {
    let source = r#"
interface Animal { name: string; }
interface Base {
  handle: (value: Animal) => void;
  handle(value: number): void;
  handle(value: boolean): void;
}
class Impl implements Base {
  handle(_value: number | boolean): void {}
}
"#;
    assert!(ts2416_count(source) >= 1);
}

/// A determinate erased conditional remains a semantic part of the return when
/// nested below a generic application. Matching only the outer `Outer` base
/// must not erase the selected branch's incompatible payload.
#[test]
fn overloaded_erased_conditional_nested_in_application_keeps_payload_mismatch() {
    let source = r#"
interface Outer<T> { readonly outer: T; }
interface Box<T> { readonly value: T; }
interface Failure { readonly failure: true; }
interface Base<O> {
  choose<K extends keyof O>(key: K):
    Outer<keyof O extends K ? Box<number> : Failure>;
  choose<K extends keyof O>(keys: readonly K[]):
    Outer<keyof O extends K ? Box<number> : Failure>;
}
class Impl<O> implements Base<O> {
  choose(_value: unknown): Outer<Box<string>> { return undefined as any; }
}
"#;
    assert!(
        ts2416_count_strict(source) >= 1,
        "normalizing the nested conditional must retain Box<string>/Box<number> incompatibility"
    );
}

/// Intersection is another value-projection wrapper. The unreachable false
/// branch must not make a compatible selected branch fail merely because the
/// conditional is not the top-level return node.
#[test]
fn overloaded_erased_conditional_nested_in_intersection_selects_true_branch() {
    let source = r#"
interface Box<T> { readonly value: T; }
interface Marker { readonly marker: true; }
interface Failure { readonly failure: true; }
interface Base<O> {
  choose<K extends keyof O>(key: K):
    (keyof O extends K ? Box<any> : Failure) & Marker;
  choose<K extends keyof O>(keys: readonly K[]):
    (keyof O extends K ? Box<any> : Failure) & Marker;
}
class Impl<O> implements Base<O> {
  choose(_value: unknown): Box<any> & Marker { return undefined as any; }
}
"#;
    assert_eq!(ts2416_count_strict(source), 0);
}

/// The tuple-union parameter shortcut proves only overload parameter coverage.
/// It must not make a derived overload family compatible when every matching
/// signature has an erased conditional return rejected by `any`/`never`
/// variance.
#[test]
fn tuple_union_overload_coverage_does_not_bypass_conditional_return_mismatch() {
    let source = r#"
interface Box<T> { readonly value: T; }
interface Failure { readonly failure: true; }
interface Base<O> {
  choose<K extends keyof O>(...args: [K] | [readonly K[]]):
    keyof O extends K ? Box<never> : Failure;
  choose<K extends keyof O>(...args: [K, K] | [readonly K[], readonly K[]]):
    keyof O extends K ? Box<never> : Failure;
}

interface Derived<O> extends Base<O> {
  choose<K extends keyof O>(value: K): Box<any>;
  choose<K extends keyof O>(value: readonly K[]): Box<any>;
  choose<K extends keyof O>(left: K, right: K): Box<any>;
  choose<K extends keyof O>(left: readonly K[], right: readonly K[]): Box<any>;
}
"#;
    let mismatches = check_source_strict(source)
        .iter()
        .filter(|diagnostic| diagnostic.code == 2430)
        .count();
    assert!(
        mismatches >= 1,
        "parameter-only tuple coverage must not suppress the return mismatch"
    );
}

/// Tuple-union coverage is never a return-type shortcut, even when the target
/// return is a plain generic application with no conditional to normalize.
#[test]
fn tuple_union_overload_coverage_does_not_bypass_plain_return_mismatch() {
    let source = r#"
interface Box<T> { readonly value: T; }
interface Base {
  choose(...args: [string] | [number]): Box<number>;
}
interface Derived extends Base {
  choose(value: string): Box<string>;
  choose(value: number): Box<string>;
}
"#;
    let diagnostics = check_source_strict(source);
    let mismatches = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == 2430)
        .count();
    assert_eq!(mismatches, 1, "{diagnostics:?}");
}

/// A tuple is part of the returned value shape, not an opaque boundary. Once
/// erasure selects the true branch, the tuple element is the same `Box<any>`
/// supplied by the implementation.
#[test]
fn overloaded_erased_conditional_nested_in_tuple_selects_true_branch() {
    let source = r#"
interface Box<T> { readonly value: T; }
interface Failure { readonly failure: true; }
interface Base<O> {
  choose<K extends keyof O>(key: K):
    [keyof O extends K ? Box<any> : Failure];
  choose<K extends keyof O>(keys: readonly K[]):
    [keyof O extends K ? Box<any> : Failure];
}
class Impl<O> implements Base<O> {
  choose(_value: unknown): [Box<any>] { return undefined as any; }
}
"#;
    assert_eq!(ts2416_count_strict(source), 0);
}

/// Object properties are covariant return projections. An erased conditional
/// nested in a property is selected before the ordinary object relation runs.
#[test]
fn overloaded_erased_conditional_nested_in_object_property_selects_true_branch() {
    let source = r#"
interface Box<T> { readonly value: T; }
interface Failure { readonly failure: true; }
interface Base<O> {
  choose<K extends keyof O>(key: K):
    { value: keyof O extends K ? Box<any> : Failure };
  choose<K extends keyof O>(keys: readonly K[]):
    { value: keyof O extends K ? Box<any> : Failure };
}
class Impl<O> implements Base<O> {
  choose(_value: unknown): { value: Box<any> } { return undefined as any; }
}
"#;
    assert_eq!(ts2416_count_strict(source), 0);
}

/// The returned function's result is still part of the outer method's return
/// value. Normalization descends through that function shape before relating it.
#[test]
fn overloaded_erased_conditional_nested_in_function_return_selects_true_branch() {
    let source = r#"
interface Box<T> { readonly value: T; }
interface Failure { readonly failure: true; }
interface Base<O> {
  choose<K extends keyof O>(key: K):
    () => (keyof O extends K ? Box<any> : Failure);
  choose<K extends keyof O>(keys: readonly K[]):
    () => (keyof O extends K ? Box<any> : Failure);
}
class Impl<O> implements Base<O> {
  choose(_value: unknown): () => Box<any> { return undefined as any; }
}
"#;
    assert_eq!(ts2416_count_strict(source), 0);
}

/// Tuple-union rest coverage is a parameter-only proof. It cannot suppress a
/// conditional return mismatch merely because that mismatch is nested in an
/// object property rather than exposed as the top-level application.
#[test]
fn tuple_union_overload_coverage_checks_nested_conditional_return() {
    let source = r#"
interface Box<T> { readonly value: T; }
interface Failure { readonly failure: true; }
interface Base<O> {
  choose<K extends keyof O>(...args: [K] | [readonly K[]]):
    { value: keyof O extends K ? Box<never> : Failure };
  choose<K extends keyof O>(...args: [K, K] | [readonly K[], readonly K[]]):
    { value: keyof O extends K ? Box<never> : Failure };
}
interface Derived<O> extends Base<O> {
  choose<K extends keyof O>(value: K): { value: Box<any> };
  choose<K extends keyof O>(value: readonly K[]): { value: Box<any> };
  choose<K extends keyof O>(left: K, right: K): { value: Box<any> };
  choose<K extends keyof O>(left: readonly K[], right: readonly K[]): { value: Box<any> };
}
"#;
    let mismatches = check_source_strict(source)
        .iter()
        .filter(|diagnostic| diagnostic.code == 2430)
        .count();
    assert!(
        mismatches >= 1,
        "tuple coverage must preserve nested conditional return incompatibility"
    );
}

/// Per-slot masking must not turn an accepted explicitly contravariant slot or
/// an unused slot into a whole-application rejection. The deliberately invalid
/// declaration still reports TS2636, but tsc does not add TS2322: its declared
/// `in` direction remains authoritative for assignment, and `B` is independent.
#[test]
fn partial_declared_variance_masks_accepted_and_independent_slots() {
    let source = r#"
interface Mix<in A, B> { readonly value: A; }
declare const mixed: Mix<any, any>;
const target: Mix<never, never> = mixed;
"#;
    let diagnostics = check_source_strict(source);
    assert!(diagnostics.iter().any(|diagnostic| diagnostic.code == 2636));
    assert_eq!(
        diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == 2322)
            .count(),
        0
    );
}

/// Partial masks compose through more than one generic declaration. Both
/// unannotated second slots are structurally covariant and therefore preserve
/// the nested `any`-to-`never` rejection.
#[test]
fn nested_partial_declared_variance_fills_structural_holes_directly() {
    let source = r#"
interface Inner<out A, B> { readonly left: A; readonly right: B; }
interface Outer<out X, Y> { readonly inner: Inner<X, Y>; }
declare const outer: Outer<string, any>;
const target: Outer<string, never> = outer;
"#;
    let errors = check_source_strict(source)
        .iter()
        .filter(|diagnostic| diagnostic.code == 2322)
        .count();
    assert_eq!(errors, 1);
}

/// The same nested partial-mask rule is visible after method-local erasure in
/// an overload return; it is not only an ordinary assignment fast-path rule.
#[test]
fn nested_partial_declared_variance_fills_structural_holes_in_overload() {
    let source = r#"
interface Failure { readonly failure: true; }
interface Inner<out A, B> { readonly left: A; readonly right: B; }
interface Shell<out X, Y> { readonly inner: Inner<X, Y>; }
interface Base<O> {
  choose<K extends keyof O>(key: K):
    keyof O extends K ? Shell<string, never> : Failure;
  choose<K extends keyof O>(keys: readonly K[]):
    keyof O extends K ? Shell<string, never> : Failure;
}
class Impl<O> implements Base<O> {
  choose(_value: unknown): Shell<string, any> { return undefined as any; }
}
"#;
    assert!(ts2416_count_strict(source) >= 1);
}

/// Mapped modifiers make a variance-only result insufficient. Structural
/// fallback must still expose the required `x` property and preserve direction:
/// `any` cannot flow to `never`, while the reverse assignment remains valid.
#[test]
fn mapped_modifier_any_never_uses_structural_fallback() {
    let source = r#"
type RequiredValue<T> = { [K in 'x']-?: T };
declare const anyValue: RequiredValue<any>;
const rejected: RequiredValue<never> = anyValue;
declare const neverValue: RequiredValue<never>;
const accepted: RequiredValue<any> = neverValue;
"#;
    let errors = check_source_strict(source)
        .iter()
        .filter(|diagnostic| diagnostic.code == 2322)
        .count();
    assert_eq!(errors, 1);
}

/// In non-strict mode each callback-property parameter is bivariant. Two
/// nested callback layers therefore accept both directions, for direct
/// assignments and for erased overloaded-method returns.
#[test]
fn double_sink_is_bivariant_nonstrict_direct_and_overload() {
    let direct = r#"
interface Sink<T> { accept: (value: T) => void; }
interface Envelope<T> { readonly inner: Sink<Sink<T>>; }
declare const anyEnvelope: Envelope<any>;
const neverEnvelope: Envelope<never> = anyEnvelope;
declare const sourceNever: Envelope<never>;
const targetAny: Envelope<any> = sourceNever;
"#;
    assert_eq!(ts2322_count_nonstrict(direct), 0);

    let declarations = "interface Sink<T> { accept: (value: T) => void; }";
    for (target, implementation) in [
        ("Outer<Sink<Sink<any>>>", "Outer<Sink<Sink<never>>>"),
        ("Outer<Sink<Sink<never>>>", "Outer<Sink<Sink<any>>>"),
    ] {
        let source = nested_overload_source(declarations, target, implementation);
        assert_eq!(
            ts2416_count_nonstrict(&source),
            0,
            "nested callbacks must remain bivariant for {implementation} -> {target}"
        );
    }
}

/// An explicit variance annotation nested inside a callback does not override
/// the enclosing callback's non-strict bivariance. The direct covariant path is
/// retained as a negative control, and the overload path follows the callback.
#[test]
fn nested_explicit_variance_respects_enclosing_nonstrict_callback() {
    let direct = r#"
interface Covariant<out T> { readonly value: T; }
interface CallbackOuter<T> { callback: (value: Covariant<T>) => void; }
interface DirectOuter<T> { readonly nested: Covariant<T>; }
declare const callbackNever: CallbackOuter<never>;
const callbackAny: CallbackOuter<any> = callbackNever;
declare const directAny: DirectOuter<any>;
const directNever: DirectOuter<never> = directAny;
"#;
    assert_eq!(
        ts2322_count_nonstrict(direct),
        1,
        "only the direct covariant path should reject"
    );

    let declarations = r#"
interface Covariant<out T> { readonly value: T; }
interface CallbackOuter<T> { callback: (value: Covariant<T>) => void; }
"#;
    let source = nested_overload_source(
        declarations,
        "Outer<CallbackOuter<any>>",
        "Outer<CallbackOuter<never>>",
    );
    assert_eq!(ts2416_count_nonstrict(&source), 0);
}

/// Alias traversal is cycle-safe rather than depth-limited. A transparent
/// chain beyond the former 32-level guard must still reach the covariant body.
#[test]
fn pass_through_alias_chain_beyond_32_levels_keeps_rejection() {
    let mut source = String::from(
        "interface Box<T> { readonly value: T; }\n\
         type Alias0<T> = Box<T>;\n",
    );
    for depth in 1..=40 {
        source.push_str(&format!("type Alias{depth}<T> = Alias{}<T>;\n", depth - 1));
    }
    source.push_str(
        "declare const value: Alias40<any>;\n\
         const target: Alias40<never> = value;\n",
    );
    let errors = check_source_strict(&source)
        .iter()
        .filter(|diagnostic| diagnostic.code == 2322)
        .count();
    assert_eq!(errors, 1);
}

/// The recursive any/never prefilter likewise has no fixed nesting limit.
/// Forty covariant applications around the rejected payload remain observable
/// through the erased-overload return path.
#[test]
fn deep_application_chain_beyond_32_levels_keeps_overload_rejection() {
    let mut target = String::from("Box<never>");
    let mut implementation = String::from("Box<any>");
    for _ in 0..40 {
        target = format!("Outer<{target}>");
        implementation = format!("Outer<{implementation}>");
    }
    let source = nested_overload_source(
        "interface Box<T> { readonly value: T; }",
        &target,
        &implementation,
    );
    assert!(ts2416_count_strict(&source) >= 1);
}

/// Bivariance permits either subtype direction; it does not make a parameter
/// independent for unrelated concrete types. Effective any/never masks must
/// therefore stay scoped to that exceptional relation path.
#[test]
fn effective_any_never_bivariance_does_not_accept_unrelated_arguments() {
    let method = r#"
interface Method<T> { consume(value: T): void; }
declare const strings: Method<string>;
const numbers: Method<number> = strings;
"#;
    let strict_errors = check_source_strict(method)
        .iter()
        .filter(|diagnostic| diagnostic.code == 2322)
        .count();
    assert_eq!(strict_errors, 1);

    let callback = r#"
interface Callback<T> { consume: (value: T) => void; }
declare const strings: Callback<string>;
const numbers: Callback<number> = strings;
"#;
    assert_eq!(ts2322_count_nonstrict(callback), 1);
}

/// Callback bivariance can choose either parameter direction, but neither
/// direction is valid when the nested generic explicitly requires invariance.
#[test]
fn nonstrict_callback_preserves_nested_explicit_invariance() {
    let source = r#"
interface Cell<in out T> { get(): T; set(value: T): void; }
interface Outer<T> { callback: (value: Cell<T>) => void; }
declare const anyOuter: Outer<any>;
const neverTarget: Outer<never> = anyOuter;
declare const neverOuter: Outer<never>;
const anyTarget: Outer<any> = neverOuter;
"#;
    assert_eq!(ts2322_count_nonstrict(source), 2);

    let aliased_method = r#"
interface Cell<in out T> { get(): T; set(value: T): void; }
type Vessel<Item> = Cell<Item>;
interface MethodOuter<Row> { consume(value: Vessel<Row>): void; }
declare const anyOuter: MethodOuter<any>;
const neverTarget: MethodOuter<never> = anyOuter;
declare const neverOuter: MethodOuter<never>;
const anyTarget: MethodOuter<any> = neverOuter;
"#;
    let strict_errors = check_source_strict(aliased_method)
        .iter()
        .filter(|diagnostic| diagnostic.code == 2322)
        .count();
    assert_eq!(strict_errors, 2);
}

/// An accepted any/never method slot must not short-circuit the remaining
/// ordinary compatibility rules. Here the second slot is valid specifically
/// because a value-returning function is assignable to a void-return target.
#[test]
fn accepted_any_never_slot_preserves_void_return_compatibility() {
    let source = r#"
interface Generic<A, B> { consume(value: A): void; readonly callback: B; }
declare const sourceValue: Generic<any, () => number>;
const targetValue: Generic<never, () => void> = sourceValue;
"#;
    let diagnostics = check_source_strict(source);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

/// An explicit `this` parameter originates in a method parameter position. It
/// keeps method bivariance for the exceptional any/never pair, while unrelated
/// concrete receivers still require a structural relation in either direction.
#[test]
fn explicit_method_this_parameter_keeps_bivariance() {
    let source = r#"
interface MethodReceiver<T> { invoke(this: T): void; }
declare const anyReceiver: MethodReceiver<any>;
const neverReceiver: MethodReceiver<never> = anyReceiver;
declare const sourceNever: MethodReceiver<never>;
const targetAny: MethodReceiver<any> = sourceNever;
declare const stringReceiver: MethodReceiver<string>;
const numberReceiver: MethodReceiver<number> = stringReceiver;
"#;
    let errors = check_source_strict(source)
        .iter()
        .filter(|diagnostic| diagnostic.code == 2322)
        .count();
    assert_eq!(errors, 1);
}

/// Type-predicate payloads contribute covariantly like return types. Both a
/// method predicate and a function-valued property reject any-to-never while
/// retaining the valid never-to-any direction.
#[test]
fn type_predicate_payload_contributes_covariant_variance() {
    let source = r#"
interface MethodPredicate<T> { matches(value: unknown): value is T; }
declare const anyMethod: MethodPredicate<any>;
const neverMethod: MethodPredicate<never> = anyMethod;
declare const sourceNeverMethod: MethodPredicate<never>;
const targetAnyMethod: MethodPredicate<any> = sourceNeverMethod;

interface PropertyPredicate<Item> { matches: (value: unknown) => value is Item; }
declare const anyProperty: PropertyPredicate<any>;
const neverProperty: PropertyPredicate<never> = anyProperty;
declare const sourceNeverProperty: PropertyPredicate<never>;
const targetAnyProperty: PropertyPredicate<any> = sourceNeverProperty;
"#;
    let errors = check_source_strict(source)
        .iter()
        .filter(|diagnostic| diagnostic.code == 2322)
        .count();
    assert_eq!(errors, 2);
}

/// `NoInfer` blocks inference candidates, not structural variance. Its inner
/// type remains a covariant occurrence for application relations.
#[test]
fn no_infer_wrapper_preserves_covariant_variance() {
    let source = r#"
interface Boxed<T> { readonly value: NoInfer<T>; }
declare const anyBox: Boxed<any>;
const neverBox: Boxed<never> = anyBox;
declare const sourceNever: Boxed<never>;
const targetAny: Boxed<any> = sourceNever;
"#;
    let errors = check_source_strict(source)
        .iter()
        .filter(|diagnostic| diagnostic.code == 2322)
        .count();
    assert_eq!(errors, 1);
}

/// Signature-local generic binders and their constraints do not witness the
/// enclosing application's variance. Shadowed binder names remain local too.
#[test]
fn signature_local_generic_constraints_do_not_pin_outer_variance() {
    let source = r#"
interface GenericConstraint<T> { callback: <U extends T>(value: U) => void; }
declare const anyValue: GenericConstraint<any>;
const neverValue: GenericConstraint<never> = anyValue;
declare const sourceNever: GenericConstraint<never>;
const targetAny: GenericConstraint<any> = sourceNever;
declare const strings: GenericConstraint<string>;
const numbers: GenericConstraint<number> = strings;

interface Shadowed<Outer> { callback: <Outer>(value: Outer) => void; }
declare const shadowStrings: Shadowed<string>;
const shadowNumbers: Shadowed<number> = shadowStrings;
"#;
    let diagnostics = check_source_strict(source);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

/// Symbol index signatures contribute the same covariant value surface as
/// string and number index signatures.
#[test]
fn symbol_index_signature_contributes_covariant_variance() {
    let source = r#"
interface SymbolTable<T> { [key: symbol]: T; }
declare const anyTable: SymbolTable<any>;
const neverTable: SymbolTable<never> = anyTable;
declare const sourceNever: SymbolTable<never>;
const targetAny: SymbolTable<any> = sourceNever;
"#;
    let errors = check_source_strict(source)
        .iter()
        .filter(|diagnostic| diagnostic.code == 2322)
        .count();
    assert_eq!(errors, 1);
}

/// Adding a call signature does not change ordinary mutable-property variance;
/// a split accessor remains invariant because it has a distinct write surface.
#[test]
fn callable_properties_match_object_variance_rules() {
    let source = r#"
interface CallableMutable<T> { (): void; value: T; }
declare const anyMutable: CallableMutable<any>;
const neverMutable: CallableMutable<never> = anyMutable;
declare const sourceNeverMutable: CallableMutable<never>;
const targetAnyMutable: CallableMutable<any> = sourceNeverMutable;

interface CallableAccessor<T> { (): void; get value(): T; set value(input: T); }
declare const anyAccessor: CallableAccessor<any>;
const neverAccessor: CallableAccessor<never> = anyAccessor;
declare const sourceNeverAccessor: CallableAccessor<never>;
const targetAnyAccessor: CallableAccessor<any> = sourceNeverAccessor;
"#;
    let errors = check_source_strict(source)
        .iter()
        .filter(|diagnostic| diagnostic.code == 2322)
        .count();
    assert_eq!(errors, 2);
}

/// Assignment-expression shortcuts must leave directional any/never pairs to
/// the same variance classifier as variable initializers.
#[test]
fn assignment_expressions_preserve_directional_any_never_variance() {
    let source = r#"
interface Boxed<T> { readonly value: T; }
let neverBox!: Boxed<never>;
declare const anyBox: Boxed<any>;
neverBox = anyBox;
let anyBoxTarget!: Boxed<any>;
declare const sourceNeverBox: Boxed<never>;
anyBoxTarget = sourceNeverBox;

interface Sink<T> { accept: (value: T) => void; }
let anySink!: Sink<any>;
declare const neverSink: Sink<never>;
anySink = neverSink;
let neverSinkTarget!: Sink<never>;
declare const sourceAnySink: Sink<any>;
neverSinkTarget = sourceAnySink;
"#;
    let errors = check_source_strict(source)
        .iter()
        .filter(|diagnostic| diagnostic.code == 2322)
        .count();
    assert_eq!(errors, 2);
}

/// Exact positional aliases forward every generic slot to the body owner; the
/// exceptional mismatch is not limited to one-parameter aliases.
#[test]
fn multi_parameter_pass_through_alias_preserves_any_never_rejection() {
    let source = r#"
interface Pair<A, B> { readonly first: A; readonly second: B; }
type Alias<X, Y> = Pair<X, Y>;
interface Failure { readonly failure: true; }
interface Base<O> {
  choose<K extends keyof O>(key: K): keyof O extends K ? Pair<never, string> : Failure;
  choose<K extends keyof O>(keys: readonly K[]): keyof O extends K ? Pair<never, string> : Failure;
}
class Impl<O> implements Base<O> {
  choose(_value: unknown): Alias<any, string> { return undefined as any; }
}
"#;
    assert!(ts2416_count_strict(source) >= 1);
}

/// Determinate erased conditionals are normalized through inline callable
/// return wrappers; compatible payloads remain accepted.
#[test]
fn erased_conditional_nested_in_callable_return_selects_true_branch() {
    let source = r#"
interface Box<T> { readonly value: T; }
interface Failure { readonly failure: true; }
interface Base<O> {
  choose<K extends keyof O>(key: K): { (): keyof O extends K ? Box<any> : Failure };
  choose<K extends keyof O>(keys: readonly K[]): { (): keyof O extends K ? Box<any> : Failure };
}
class Impl<O> implements Base<O> {
  choose(_value: unknown): { (): Box<any> } { return undefined as any; }
}
"#;
    assert_eq!(ts2416_count_strict(source), 0);
}

/// The same callable projection must preserve a proven any-to-never payload
/// rejection through both the direct N×M pass and its erased retry.
#[test]
fn erased_conditional_nested_in_callable_return_keeps_rejection() {
    let source = r#"
interface Box<T> { readonly value: T; }
interface Failure { readonly failure: true; }
interface Base<O> {
  choose<K extends keyof O>(key: K): { (): keyof O extends K ? Box<never> : Failure };
  choose<K extends keyof O>(keys: readonly K[]): { (): keyof O extends K ? Box<never> : Failure };
}
class Impl<O> implements Base<O> {
  choose(_value: unknown): { (): Box<any> } { return undefined as any; }
}
"#;
    assert!(ts2416_count_strict(source) >= 1);
}
