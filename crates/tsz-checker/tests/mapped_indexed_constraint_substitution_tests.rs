//! Tests for indexing a *generic* mapped type by its own constraint:
//! `{ [T in TB]: F(T) }[TB]` where `TB` is still generic.
//!
//! Structural rule (tsc `substituteIndexedMappedType` parity): when the
//! object of an indexed access is a generic mapped type (its constraint
//! still contains type variables) and the index is that constraint, the
//! result is the template instantiated with the mapped binder replaced by
//! the constraint — `F(TB)` — not the template over a fresh parameter
//! `T extends TB`. The fresh-parameter form fails identity with the
//! simplified form on the other side of a relation, producing false
//! TS2322/TS2345/TS2416 (kysely `AnyColumn`/`AnyColumnWithTable` family).
//!
//! Owner layer: solver evaluation
//! (`evaluate_rules/index_access.rs::instantiate_mapped_template_with_constraint_param`).

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

/// kysely `AnyColumn` shape: `{ [T in TB]: keyof DB[T] }[TB] & string` must be
/// mutually assignable with `keyof DB[TB] & string` in a generic context.
#[test]
fn keyof_template_mapped_indexed_by_constraint_both_directions() {
    let source = r#"
type AnyCol<DB, TB extends keyof DB> = { [T in TB]: keyof DB[T] }[TB] & string
function h<DB, TB extends keyof DB>(a: AnyCol<DB, TB>, b: keyof DB[TB] & string) {
    const x: keyof DB[TB] & string = a
    const y: AnyCol<DB, TB> = b
}
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 2322),
        0,
        "AnyCol<DB, TB> and keyof DB[TB] & string must be mutually assignable; got: {:?}",
        codes(&diags)
    );
}

/// Renamed binders (anti-hardcoding): the rule must not key on `T`/`TB`/`DB`.
#[test]
fn keyof_template_mapped_indexed_by_constraint_renamed_binders() {
    let source = r#"
type Cols<Schema, Tables extends keyof Schema> = {
    [Tbl in Tables]: keyof Schema[Tbl]
}[Tables] &
    string
function probe<Schema, Tables extends keyof Schema>(
    a: Cols<Schema, Tables>,
    b: keyof Schema[Tables] & string,
) {
    const x: keyof Schema[Tables] & string = a
    const y: Cols<Schema, Tables> = b
}
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 2322),
        0,
        "renamed-binder form must behave identically; got: {:?}",
        codes(&diags)
    );
}

/// Template-literal template (kysely `AnyColumnWithTable` shape).
#[test]
fn template_literal_mapped_indexed_by_constraint_both_directions() {
    let source = r#"
type ColT<DB, TB extends keyof DB> = {
    [T in TB]: `${T & string}.${keyof DB[T] & string}`
}[TB]
function h<DB, TB extends keyof DB>(
    a: ColT<DB, TB>,
    b: `${TB & string}.${keyof DB[TB] & string}`,
) {
    const x: `${TB & string}.${keyof DB[TB] & string}` = a
    const y: ColT<DB, TB> = b
}
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 2322),
        0,
        "template-literal mapped-indexed alias must collapse to the substituted form; got: {:?}",
        codes(&diags)
    );
}

/// Correlated-function template: `{ [K in keyof T]: (x: T[K]) => void }[keyof T]`
/// relates to `(x: T[keyof T]) => void` (tsc simplification identity).
#[test]
fn function_template_mapped_indexed_by_keyof_constraint() {
    let source = r#"
type FnMap<T> = { [K in keyof T]: (x: T[K]) => void }[keyof T]
function h<T>(a: FnMap<T>, b: (x: T[keyof T]) => void) {
    const x: (x: T[keyof T]) => void = a
    const y: FnMap<T> = b
}
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 2322),
        0,
        "generic mapped fn template indexed by its constraint must substitute the binder; got: {:?}",
        codes(&diags)
    );
}

/// Through an interface member and impl-vs-interface TS2416 (kysely witness
/// shape, locally mocked).
#[test]
fn impl_vs_interface_member_using_mapped_indexed_alias_no_ts2416() {
    let source = r#"
type AnyCol<DB, TB extends keyof DB> = { [T in TB]: keyof DB[T] }[TB] & string
interface Builder<DB, TB extends keyof DB> {
    refTuple(refs: ReadonlyArray<AnyCol<DB, TB>>): keyof DB[TB] & string
}
class BuilderImpl<DB, TB extends keyof DB> implements Builder<DB, TB> {
    refTuple(refs: ReadonlyArray<keyof DB[TB] & string>): AnyCol<DB, TB> {
        return refs[0] as AnyCol<DB, TB>
    }
}
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 2416),
        0,
        "impl member typed via the alias must satisfy the interface member typed via the substituted form; got: {:?}",
        codes(&diags)
    );
}

/// Concrete control: a fully concrete key space stays a per-key union — the
/// collapse must not fire for literal constraints (distributive semantics).
#[test]
fn concrete_key_space_stays_per_key_union() {
    let source = r#"
type Conc = { [K in 'a' | 'b']: (x: K) => void }['a' | 'b']
declare const c1: Conc
const c2: ((x: 'a') => void) | ((x: 'b') => void) = c1
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 2322),
        0,
        "concrete mapped-indexed access must remain the per-key union; got: {:?}",
        codes(&diags)
    );
}

/// Concrete negative control: the collapsed (non-distributive) function-of-union
/// form is NOT a supertype of the per-key union — tsc errors here and so must we.
#[test]
fn concrete_key_space_collapse_shape_still_errors() {
    let source = r#"
type Conc = { [K in 'a' | 'b']: (x: K) => void }['a' | 'b']
declare const c1: Conc
const c3: (x: 'a' | 'b') => void = c1
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 2322),
        1,
        "per-key union of functions must not be assignable to the function over the key union; got: {:?}",
        codes(&diags)
    );
}

/// Genuinely incompatible negative control: `AnyCol<DB, TB>` is the keys of
/// `DB[TB]`, not `TB` itself — must still error.
#[test]
fn mapped_indexed_alias_not_assignable_to_unrelated_param() {
    let source = r#"
type AnyCol<DB, TB extends keyof DB> = { [T in TB]: keyof DB[T] }[TB] & string
function h<DB, TB extends keyof DB>(a: AnyCol<DB, TB>) {
    const x: TB = a
}
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 2322),
        1,
        "keys of DB[TB] are unrelated to TB; assignment must keep erroring; got: {:?}",
        codes(&diags)
    );
}

/// Infer-bearing-conditional control (mapped deferral boundary, see #13004):
/// per-key filtering aliases like `FunctionKeys` must keep working through
/// generic `Pick` without false diagnostics.
#[test]
fn function_keys_filter_through_generic_pick_still_clean() {
    let source = r#"
type LocalPick<T, K extends keyof T> = { [P in K]: T[P] }
type NonUndefined<A> = A extends undefined ? never : A
type FunctionKeys<T extends object> = {
    [K in keyof T]-?: NonUndefined<T[K]> extends (...args: any[]) => unknown ? K : never
}[keyof T]
type FunctionProps<T extends object> = LocalPick<T, FunctionKeys<T>>
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        diags.len(),
        0,
        "generic Pick over a per-key filter alias must stay clean; got: {:?}",
        codes(&diags)
    );
}

/// Concrete instantiation of the filter alias still distributes per key.
#[test]
fn function_keys_filter_concrete_instantiation_distributes() {
    let source = r#"
type NonUndefined<A> = A extends undefined ? never : A
type FunctionKeys<T extends object> = {
    [K in keyof T]-?: NonUndefined<T[K]> extends (...args: any[]) => unknown ? K : never
}[keyof T]
interface Mixed {
    go(): void
    name: string
}
const k: FunctionKeys<Mixed> = 'go'
const bad: FunctionKeys<Mixed> = 'name'
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 2322),
        1,
        "FunctionKeys<Mixed> must be exactly 'go' ('name' rejected, 'go' accepted); got: {:?}",
        codes(&diags)
    );
}
