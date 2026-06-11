//! Tests for relating two instantiations of the same nominal class through
//! class-member (`implements`) relations when the member types were eagerly
//! evaluated to structural shapes.
//!
//! Structural rule: when source and target are instantiations of the SAME
//! generic class/interface and every type-argument pair is either identical
//! or has `any` on one side, tsc relates them via `relateVariances` (an
//! invariant-strength per-argument check passes in both directions for
//! `any`) BEFORE any structural expansion. tsz's checker computes class
//! member types in evaluated form, so both sides can lose their
//! `Application` identity; the members of such classes may be deferred
//! conditionals (`T extends SqlBool ? ... : KyselyTypeError<...>`) that can
//! never relate structurally — the kysely `$asScalar` false TS2416.
//!
//! Owner layer: solver relation recovery
//! (`relations/subtype/cache.rs` accept-only variance recovery over the
//! semantic `application_eval_origin` provenance, and
//! `rules/generics.rs::try_same_base_args_identical_or_any`), fed by origin
//! recording in solver application evaluation and the checker
//! `evaluate_application_type` boundary.
//!
//! All witnesses tsc-5.9-verified (clean except the negative controls).

use tsz_checker::test_utils::check_source_diagnostics;

fn count(diags: &[tsz_checker::diagnostics::Diagnostic], code: u32) -> usize {
    diags.iter().filter(|d| d.code == code).count()
}

fn codes(diags: &[tsz_checker::diagnostics::Diagnostic]) -> Vec<(u32, String)> {
    diags
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

/// Minimal kysely `$asScalar` witness: non-generic impl member returning
/// `Wrapper<any>` vs interface member returning `Wrapper<O[keyof O]>`, where
/// `Wrapper` has a deferred-conditional member. tsc-clean.
#[test]
fn implements_member_any_arg_instantiation_with_conditional_member_no_ts2416() {
    let source = r#"
type SqlBool = boolean
interface KyselyTypeError<E extends string> {
  readonly error: E
}
declare class AndWrapper<T> {
  get expressionType(): T | undefined
}
declare class ExpressionWrapper<T> {
  get expressionType(): T | undefined
  and(
    rhs: string,
  ): T extends SqlBool ? AndWrapper<SqlBool> : KyselyTypeError<'nope'>
}
interface SQB<O> {
  $asScalar(): ExpressionWrapper<O[keyof O]>
}
class SQBImpl<O> implements SQB<O> {
  $asScalar(): ExpressionWrapper<any> {
    return undefined as any
  }
}
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 2416),
        0,
        "Wrapper<any> member must satisfy Wrapper<O[keyof O]> member; got: {:?}",
        codes(&diags)
    );
}

/// Renamed binders (anti-hardcoding): no name may drive the rule.
#[test]
fn implements_member_any_arg_instantiation_renamed_binders() {
    let source = r#"
type Flag = boolean
interface Oops<Msg extends string> {
  readonly oops: Msg
}
declare class Chained<Val> {
  get marker(): Val | undefined
}
declare class Wrap<Val> {
  get marker(): Val | undefined
  chain(rhs: string): Val extends Flag ? Chained<Flag> : Oops<'bad'>
}
interface Host<Row> {
  pluck(): Wrap<Row[keyof Row]>
}
class HostImpl<Row> implements Host<Row> {
  pluck(): Wrap<any> {
    return undefined as any
  }
}
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 2416),
        0,
        "renamed-binder form must behave identically; got: {:?}",
        codes(&diags)
    );
}

/// Multi-parameter form with identical leading args and a mapped-indexed
/// alias in the conditional member's sibling overload constraint (the kysely
/// `ReferenceExpression` shape, which flags the class variance as
/// structural-fallback). tsc-clean.
#[test]
fn implements_member_mixed_identical_and_any_args_no_ts2416() {
    let source = r#"
type SqlBool = boolean
interface KyselyTypeError<E extends string> {
  readonly error: E
}
interface Expression<T> {
  get expressionType(): T | undefined
}
type AnyColumn<DB, TB extends keyof DB> = { [T in TB]: keyof DB[T] }[TB] & string
type AnyColumnWithTable<DB, TB extends keyof DB> = {
  [T in TB]: `${T & string}.${keyof DB[T] & string}`
}[TB]
type ReferenceExpression<DB, TB extends keyof DB> =
  | AnyColumn<DB, TB>
  | AnyColumnWithTable<DB, TB>
  | Expression<any>
declare class AndWrapper<DB, TB extends keyof DB, T> {
  get expressionType(): T | undefined
}
class ExpressionWrapper<DB, TB extends keyof DB, T> implements Expression<T> {
  get expressionType(): T | undefined {
    return undefined
  }
  and<RE extends ReferenceExpression<DB, TB>>(
    lhs: RE,
    rhs: string,
  ): T extends SqlBool
    ? AndWrapper<DB, TB, SqlBool>
    : KyselyTypeError<'and() method can only be called on boolean expressions'>
  and<E extends Expression<SqlBool>>(
    expression: E,
  ): T extends SqlBool
    ? AndWrapper<DB, TB, SqlBool>
    : KyselyTypeError<'and() method can only be called on boolean expressions'>
  and(...args: any[]): any {
    return undefined
  }
}
interface SQB<DB, TB extends keyof DB, O> {
  $asScalar(): ExpressionWrapper<DB, TB, O[keyof O]>
}
class SQBImpl<DB, TB extends keyof DB, O> implements SQB<DB, TB, O> {
  $asScalar(): ExpressionWrapper<DB, TB, any> {
    return undefined as any
  }
}
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 2416),
        0,
        "identical leading args + any tail arg must relate; got: {:?}",
        codes(&diags)
    );
}

/// Generic interface member (default type parameter) — the literal kysely
/// `$asScalar<K extends keyof O = keyof O>()` shape. tsc-clean.
#[test]
fn implements_member_generic_target_signature_any_arg_no_ts2416() {
    let source = r#"
type SqlBool = boolean
interface KyselyTypeError<E extends string> {
  readonly error: E
}
declare class AndWrapper<T> {
  get expressionType(): T | undefined
}
declare class ExpressionWrapper<T> {
  get expressionType(): T | undefined
  and(
    rhs: string,
  ): T extends SqlBool ? AndWrapper<SqlBool> : KyselyTypeError<'nope'>
}
interface SQB<O> {
  $asScalar<K extends keyof O = keyof O>(): ExpressionWrapper<O[K]>
}
class SQBImpl<O> implements SQB<O> {
  $asScalar(): ExpressionWrapper<any> {
    return undefined as any
  }
}
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 2416),
        0,
        "non-generic impl must satisfy the generic interface member; got: {:?}",
        codes(&diags)
    );
}

/// Genuinely incompatible negative control: a concrete non-`any` argument
/// must keep failing (tsc errors here too).
#[test]
fn implements_member_concrete_mismatched_arg_still_ts2416() {
    let source = r#"
type SqlBool = boolean
interface KyselyTypeError<E extends string> {
  readonly error: E
}
declare class AndWrapper<T> {
  get expressionType(): T | undefined
}
declare class ExpressionWrapper<T> {
  get expressionType(): T | undefined
  and(
    rhs: string,
  ): T extends SqlBool ? AndWrapper<SqlBool> : KyselyTypeError<'nope'>
}
interface SQB<O> {
  $asScalar(): ExpressionWrapper<O[keyof O]>
}
class SQBImpl<O> implements SQB<O> {
  $asScalar(): ExpressionWrapper<{ marker: number }> {
    return undefined as any
  }
}
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 2416),
        1,
        "a concrete mismatched type argument must keep erroring; got: {:?}",
        codes(&diags)
    );
}

/// Unrelated-class negative control: same shape but DIFFERENT class on the
/// impl side must not be accepted by the same-definition shortcut.
#[test]
fn implements_member_different_class_any_arg_still_errors() {
    let source = r#"
type SqlBool = boolean
declare class WrapperA<T> {
  get expressionType(): T | undefined
  onlyA(x: T): void
}
declare class WrapperB<T> {
  get expressionType(): T | undefined
  onlyB(x: T): void
}
interface SQB<O> {
  $asScalar(): WrapperA<O[keyof O]>
}
class SQBImpl<O> implements SQB<O> {
  $asScalar(): WrapperB<any> {
    return undefined as any
  }
}
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 2416),
        1,
        "different classes must not relate through the any-arg shortcut; got: {:?}",
        codes(&diags)
    );
}
