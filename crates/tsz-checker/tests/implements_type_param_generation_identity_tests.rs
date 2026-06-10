//! Tests for type-parameter `TypeId` generation identity across checker passes.
//!
//! Structural rule: every resolution of the same type-parameter declaration
//! (class member checking, heritage-clause type-argument resolution, interface
//! member rebuilding for the `implements` check) must converge on a single
//! `TypeId` per `TypeParamInfo`. `tsc` identifies type parameters by symbol;
//! `tsz` approximates that with declaration-scoped fresh `TypeId`s deduped in
//! `intern_type_param_for_decl` — plus a *separate* structurally-interned
//! scheme used by the lowering for class member shapes. The two schemes do
//! not converge: trace evidence shows the impl-side member embedding
//! lowered/structural param ids while the heritage-clause substitution
//! produces checker-fresh ids for the same declarations. Member types
//! embedding those ids inside *deferred* types (conditional `extends`
//! clauses, which relate by `TypeId` identity) then fail against each other:
//! false TS2416 on `implements` members (the dominant kysely builder family).
//!
//! The convergence: `intern_type_param_for_decl` interns class/interface/
//! type-alias type parameters *structurally* (`TypeInterner::type_param`),
//! matching the lowering's scheme, so every resolution of a type-level
//! declaration's parameter produces one `TypeId`. Function-like declarations
//! keep declaration-scoped fresh ids so distinct same-named, same-constrained
//! function params stay distinct (`S[K_outer]` vs `S[K_inner]` keeps
//! erroring). All witnesses are tsc-5.5-verified.
//!
//! The remaining ignored test pins a separate residue: a deferred
//! conditional instantiated with concrete type arguments must relate to its
//! generic origin (tsc's same-conditional-root rule), which id convergence
//! alone does not provide.
//!
//! Owner layer: checker type-parameter identity
//! (`state/type_analysis/core.rs::intern_type_param_for_decl` and the
//! lowering's structural `type_param` interning).

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

/// Minimal kysely-shaped witness: a generic method whose deferred conditional
/// mixes its own type param with an outer dependent-constrained interface
/// param, reached through an interface-typed parameter of an implemented
/// member. tsc-clean; tsz emitted a false TS2416 before the fix.
#[test]
fn implements_member_with_dependent_constraint_conditional_no_ts2416() {
    let source = r#"
interface FM<DB, TB extends keyof DB> {
    any<RE>(expr: RE): RE extends ReadonlyArray<TB> ? number : never
}
interface B7<DB, TB extends keyof DB> {
    m(lhs: FM<DB, TB>): void
}
class B7Impl<DB, TB extends keyof DB> implements B7<DB, TB> {
    m(lhs: FM<DB, TB>): void {}
}
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 2416),
        0,
        "identical member types must relate across passes; got: {:?}",
        codes(&diags)
    );
}

/// Renamed binders (anti-hardcoding): interface/class params named differently
/// from the inner interface's own params.
#[test]
fn implements_member_dependent_constraint_conditional_renamed_binders() {
    let source = r#"
interface Mod<SchemaT, TblT extends keyof SchemaT> {
    probe<Ref>(expr: Ref): Ref extends ReadonlyArray<TblT> ? number : never
}
interface Host<Db2, Tb2 extends keyof Db2> {
    member(lhs: Mod<Db2, Tb2>): void
}
class HostImpl<Db2, Tb2 extends keyof Db2> implements Host<Db2, Tb2> {
    member(lhs: Mod<Db2, Tb2>): void {}
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

/// Mapped-indexed alias in the constraint chain (closer to kysely's
/// `StringReference`/`ExtractColumnType` shape), with self-recursion through
/// a `Pick`-like local alias.
#[test]
fn implements_member_recursive_builder_with_mapped_indexed_alias() {
    let source = r#"
type LocalPick<T, K extends keyof T> = { [P in K]: T[P] }
type AnyCol<DB, TB extends keyof DB> = { [T in TB]: keyof DB[T] }[TB] & string
type ExtractCol<DB, TB extends keyof DB, C> = {
    [T in TB]: C extends keyof DB[T] ? DB[T][C] : never
}[TB]
interface Expression<T> {
    readonly expressionType?: T
}
interface EW<DB, TB extends keyof DB, T> extends Expression<T> {
    dummy?: [DB, TB]
}
interface FM<DB, TB extends keyof DB> {
    any<RE extends AnyCol<DB, TB>>(
        expr: RE,
    ): Exclude<ExtractCol<DB, TB, RE>, null> extends ReadonlyArray<infer I>
        ? EW<DB, TB, I>
        : never
    any<T>(expr: Expression<ReadonlyArray<T>>): EW<DB, TB, T>
}
interface EB<DB, TB extends keyof DB> {
    get eb(): EB<DB, TB>
    get fn(): FM<DB, TB>
}
type EBSub<DB, TB extends keyof DB> = LocalPick<EB<DB, TB>, 'eb'>
interface B7<DB, TB extends keyof DB> {
    m(lhs: EBSub<DB, TB>): void
}
class B7Impl<DB, TB extends keyof DB> implements B7<DB, TB> {
    m(lhs: EBSub<DB, TB>): void {}
}
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 2416),
        0,
        "recursive builder member must relate to itself; got: {:?}",
        codes(&diags)
    );
}

/// Unconstrained-params control (passed before the fix; must keep passing).
#[test]
fn implements_member_unconstrained_params_still_clean() {
    let source = r#"
interface FM<DB, TB> {
    any<RE>(expr: RE): RE extends ReadonlyArray<TB> ? number : never
}
interface B7<DB, TB> {
    m(lhs: FM<TB, DB>): void
}
class B7Impl<DB, TB> implements B7<DB, TB> {
    m(lhs: FM<TB, DB>): void {}
}
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(count(&diags, 2416), 0, "got: {:?}", codes(&diags));
}

/// Genuinely incompatible negative control: the impl member narrows the
/// parameter type — must still error (TS2416).
#[test]
fn implements_member_genuinely_incompatible_still_ts2416() {
    let source = r#"
interface FM<DB, TB extends keyof DB> {
    any<RE>(expr: RE): RE extends ReadonlyArray<TB> ? number : never
}
interface B7<DB, TB extends keyof DB> {
    m(lhs: FM<DB, TB>): void
}
class B7Impl<DB, TB extends keyof DB> implements B7<DB, TB> {
    m(lhs: string): void {}
}
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 2416),
        1,
        "narrowed impl parameter must keep failing; got: {:?}",
        codes(&diags)
    );
}

/// Method-bivariance control: a concretely instantiated impl parameter is
/// accepted bivariantly by tsc (method declarations relate bivariantly);
/// verified against `tsc 5.5` (clean).
#[test]
#[ignore = "pinned: relating a deferred conditional instantiated with concrete args against its generic origin still requires conditional-root identity (tsc relates same-root conditional instantiations); the generation-identity convergence fixed the generic-vs-generic family but not this concrete bivariant form"]
fn implements_member_concrete_param_bivariant_accepted() {
    let source = r#"
interface FM<DB, TB extends keyof DB> {
    any<RE>(expr: RE): RE extends ReadonlyArray<TB> ? number : never
}
interface B7<DB, TB extends keyof DB> {
    m(lhs: FM<DB, TB>): void
}
class B7Impl<DB, TB extends keyof DB> implements B7<DB, TB> {
    m(lhs: FM<{ a: 1 }, 'a'>): void {}
}
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 2416),
        0,
        "tsc accepts this bivariantly; got: {:?}",
        codes(&diags)
    );
}
