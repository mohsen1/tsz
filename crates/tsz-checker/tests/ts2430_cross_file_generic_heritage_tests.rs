//! TS2430 ("interface incorrectly extends interface") cross-file generic
//! method heritage.
//!
//! When a base interface lives in another file, its declarations are not
//! resolved into the in-arena interface path, so `check_interface_extension_compatibility`
//! falls back to comparing each derived member against the base's *resolved*
//! type. That fallback used the strict no-erase-generics relation, which
//! rejects alpha-equivalent generic method signatures (the base member is a
//! `Callable` lowered from the interface type while the derived member is a
//! fresh AST `Function`) and never substituted the base interface's type
//! parameters with the heritage arguments. Both produced false TS2430 reports
//! on valid generic-method overrides (observed in the Kysely row:
//! `SelectQueryBuilder`/`RawBuilder`).
//!
//! The fix recognises a generic method override that is assignable under fresh
//! method-local generic instantiation (single or overloaded), and substitutes
//! the base interface's type parameters before comparison. These tests guard
//! the rule across renamed type parameters, overload sets, base-parameter
//! return types, and the negative cases that must still report.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_multi_file;

fn cross_file_codes(files: &[(&str, &str)], entry: &str) -> Vec<u32> {
    check_multi_file(files, entry, CheckerOptions::default())
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn has_2430(files: &[(&str, &str)], entry: &str) -> bool {
    cross_file_codes(files, entry).contains(&2430)
}

/// Reported repro: a generic method whose return type does not mention the
/// base interface's type parameter, overridden across a file boundary with a
/// renamed method-local type parameter, must not report TS2430.
#[test]
fn cross_file_generic_method_renamed_type_param_no_false_ts2430() {
    let base = r#"
export type Fmt = 'a' | 'b';
export interface Explainable {
  explain<O extends Record<string, unknown> = Record<string, unknown>>(fmt?: Fmt): Promise<O[]>;
}
"#;
    let derived = r#"
import type { Explainable, Fmt } from './base.js';
export interface Builder<DB, O> extends Explainable {
  explain<ER extends Record<string, unknown> = Record<string, unknown>>(fmt?: Fmt): Promise<ER[]>;
}
"#;
    assert!(
        !has_2430(
            &[("./base.ts", base), ("./derived.ts", derived)],
            "./derived.ts"
        ),
        "alpha-equivalent generic method override across files must not report TS2430"
    );
}

/// The same rule must hold regardless of the method-local type parameter name
/// the derived side happens to choose (`K` here instead of `ER`).
#[test]
fn cross_file_generic_method_alt_name_no_false_ts2430() {
    let base = r#"
export interface Source<T> {
  pick<P extends keyof T>(key: P): Source<T>;
}
"#;
    let derived = r#"
import type { Source } from './base.js';
export interface View<T> extends Source<T> {
  pick<K extends keyof T>(key: K): View<T>;
}
"#;
    assert!(
        !has_2430(
            &[("./base.ts", base), ("./derived.ts", derived)],
            "./derived.ts"
        ),
        "renamed method-local type parameter must not change the decision"
    );
}

/// Overloaded generic method override across files: each derived overload is
/// compared against the base's combined overload set, so the override must be
/// evaluated as a whole, not signature-by-signature.
#[test]
fn cross_file_overloaded_generic_method_no_false_ts2430() {
    let base = r#"
export interface WhereInterface<DB, TB extends keyof DB> {
  where<K extends keyof DB>(key: K, value: DB[K]): WhereInterface<DB, TB>;
  where(raw: string): WhereInterface<DB, TB>;
}
"#;
    let derived = r#"
import type { WhereInterface } from './base.js';
export interface SelectQueryBuilder<DB, TB extends keyof DB, O> extends WhereInterface<DB, TB> {
  where<K extends keyof DB>(key: K, value: DB[K]): SelectQueryBuilder<DB, TB, O>;
  where(raw: string): SelectQueryBuilder<DB, TB, O>;
}
"#;
    assert!(
        !has_2430(
            &[("./base.ts", base), ("./derived.ts", derived)],
            "./derived.ts"
        ),
        "overloaded generic method override across files must not report TS2430"
    );
}

/// Base member return type mentions the base interface's own type parameter
/// (`AliasedExpression<T, A>`). The heritage argument must be substituted into
/// the base member before comparison; the derived override returns a subtype
/// (`AliasedRawBuilder<O, A>`), which is a valid specialization.
#[test]
fn cross_file_base_param_return_type_no_false_ts2430() {
    let base = r#"
export interface AliasedExpression<T, A extends string> { readonly alias: A; readonly expr: T; }
export interface AliasableExpression<T> {
  as<A extends string>(alias: A): AliasedExpression<T, A>;
  as<A extends string>(alias: number): AliasedExpression<T, A>;
}
"#;
    let derived = r#"
import type { AliasableExpression, AliasedExpression } from './base.js';
export interface AliasedRawBuilder<O, A extends string> extends AliasedExpression<O, A> {
  readonly raw: true;
}
export interface RawBuilder<O> extends AliasableExpression<O> {
  as<A extends string>(alias: A): AliasedRawBuilder<O, A>;
  as<A extends string>(alias: number): AliasedRawBuilder<O, A>;
}
"#;
    assert!(
        !has_2430(
            &[("./base.ts", base), ("./derived.ts", derived)],
            "./derived.ts"
        ),
        "base interface type parameter must be substituted before comparison"
    );
}

/// Negative: a genuinely incompatible generic method override (the derived
/// return type is not assignable to the base's) must still report TS2430.
#[test]
fn cross_file_generic_method_incompatible_return_still_ts2430() {
    let base = r#"
export interface Source<T> {
  wrap<A extends string>(key: A): ReadonlyArray<T>;
}
"#;
    let derived = r#"
import type { Source } from './base.js';
export interface View<T> extends Source<T> {
  wrap<A extends string>(key: A): T;
}
"#;
    assert!(
        has_2430(
            &[("./base.ts", base), ("./derived.ts", derived)],
            "./derived.ts"
        ),
        "an incompatible generic method override must still report TS2430"
    );
}

/// Negative: a non-generic property override with an incompatible type must
/// still report TS2430 across files (the relaxation is gated on generic
/// callables only).
#[test]
fn cross_file_non_generic_incompatible_still_ts2430() {
    let base = r#"
export interface Base { value: number; }
"#;
    let derived = r#"
import type { Base } from './base.js';
export interface Derived extends Base { value: string; }
"#;
    assert!(
        has_2430(
            &[("./base.ts", base), ("./derived.ts", derived)],
            "./derived.ts"
        ),
        "an incompatible non-generic property override must still report TS2430"
    );
}
