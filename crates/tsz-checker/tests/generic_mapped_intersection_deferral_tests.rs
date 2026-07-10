//! Generic homomorphic mapped types keyed by `keyof (T & X)` must stay
//! deferred while `T` is free (#10663).
//!
//! Structural rule: when a mapped type's key source is `keyof S` and `S` is a
//! composite (intersection/union) containing a free type parameter, its key
//! set includes `keyof T`, which has no concrete expansion — tsc's
//! `isGenericIndexType` keeps the mapped type generic and relates
//! `Readonly<T & X>` to `Readonly<T>` through the mapped-to-mapped rule.
//! tsz used to eagerly materialize only the concrete member's keys, dropping
//! every key `T` contributes, which produced false `TS2322`/`TS2345` for
//! kysely's freeze-factory pattern (`QueryNode.cloneWith*`).
//!
//! Owner layer: solver mapped-type evaluation
//! (`evaluation/evaluate_rules/mapped.rs` deferral predicate backed by
//! `type_queries::mapped::keyof_operand_is_generic`).

use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::{check_source_with_libs, load_default_lib_files};

fn strict_diagnostics(source: &str) -> Vec<Diagnostic> {
    let lib_files = load_default_lib_files();
    check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            no_implicit_any: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
        &lib_files,
    )
    .into_iter()
    .filter(|diagnostic| diagnostic.code != 2318)
    .collect()
}

fn format_codes(diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .map(|d| format!("TS{}: {}", d.code, d.message_text))
        .collect()
}

/// The kysely freeze-factory witness: an unannotated method shorthand whose
/// contextual signature is generic returns an inner `freeze({ ...node, prop })`
/// — `Readonly<T & { prop }>` must be assignable to `Readonly<T>`.
#[test]
fn freeze_factory_generic_clone_method_is_clean() {
    let source = r#"
declare function freeze<T>(obj: T): Readonly<T>
interface WhereNode { readonly where: string }
declare function createWhere(op: string): WhereNode
type HasWhere = { where?: WhereNode }
type Factory = Readonly<{
  cloneWithWhere<T extends HasWhere>(node: T, op: string): Readonly<T>
}>
export const QueryNode: Factory = freeze<Factory>({
  cloneWithWhere(node, op) {
    return freeze({ ...node, where: createWhere(op) })
  },
})
"#;
    let diagnostics = strict_diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "freeze-factory clone method must be clean, got {:?}",
        format_codes(&diagnostics)
    );
}

/// Renamed binders: same shape with every user identifier renamed.
#[test]
fn freeze_factory_generic_clone_method_is_clean_renamed_binders() {
    let source = r#"
declare function lock<Z>(value: Z): Readonly<Z>
interface Clause { readonly text: string }
declare function mkClause(raw: string): Clause
type Carrier = { clause?: Clause }
type Maker = Readonly<{
  withClause<Q extends Carrier>(item: Q, raw: string): Readonly<Q>
}>
export const Maker: Maker = lock<Maker>({
  withClause(item, raw) {
    return lock({ ...item, clause: mkClause(raw) })
  },
})
"#;
    let diagnostics = strict_diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "renamed freeze-factory clone method must be clean, got {:?}",
        format_codes(&diagnostics)
    );
}

/// The bare relation, generic-to-generic: `Readonly<T & { p }> <: Readonly<T>`
/// (lib alias form) and a user-authored homomorphic mapped alias form.
#[test]
fn readonly_of_intersection_relates_to_readonly_of_param() {
    let source = r#"
type HasWhere = { where?: string }
declare function target<T extends HasWhere>(node: T): Readonly<T>
export const viaLibAlias: typeof target =
  null as any as (<T extends HasWhere>(node: T) => Readonly<T & { where: string }>)

type Box<T> = { readonly [K in keyof T]: T[K] }
declare function boxed<T extends HasWhere>(node: T): Box<T>
export const viaUserMapped: typeof boxed =
  null as any as (<T extends HasWhere>(node: T) => Box<T & { where: string }>)
"#;
    let diagnostics = strict_diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "Readonly<T & X> must relate to Readonly<T>, got {:?}",
        format_codes(&diagnostics)
    );
}

/// Negative direction: `Readonly<T>` is NOT assignable to
/// `Readonly<T & { p }>` (the target promises the extra member).
#[test]
fn readonly_of_param_does_not_relate_to_readonly_of_intersection() {
    let source = r#"
type HasWhere = { where?: string }
declare function target<T extends HasWhere>(node: T): Readonly<T & { where: string }>
export const g: typeof target =
  null as any as (<T extends HasWhere>(node: T) => Readonly<T>)
"#;
    let diagnostics = strict_diagnostics(source);
    assert!(
        diagnostics.iter().any(|d| d.code == 2322),
        "reversed direction must still report TS2322, got {:?}",
        format_codes(&diagnostics)
    );
}

/// Negative modifier case: an optionality-adding mapped source is not
/// assignable to the plain generic target.
#[test]
fn optional_adding_mapped_of_intersection_still_rejected() {
    let source = r#"
type Opt<T> = { [K in keyof T]?: T[K] }
type HasWhere = { where?: string }
declare function target<T extends HasWhere>(node: T): T
export const g: typeof target =
  null as any as (<T extends HasWhere>(node: T) => Opt<T & { where: string }>)
"#;
    let diagnostics = strict_diagnostics(source);
    assert!(
        diagnostics.iter().any(|d| d.code == 2322),
        "optionality-adding mapped source must still report TS2322, got {:?}",
        format_codes(&diagnostics)
    );
}

/// Property access through the deferred form still resolves: the concrete
/// member's key from the intersection and a key reached through `T`'s
/// constraint.
#[test]
fn property_access_on_deferred_mapped_intersection_resolves() {
    let source = r#"
type Box<T> = { readonly [K in keyof T]: T[K] }
export function readWhere<T>(x: Box<T & { where: string }>): string {
  return x.where
}
export function readId<T extends { id: number }>(x: Box<T & { where: string }>): number {
  return x.id
}
"#;
    let diagnostics = strict_diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "property access on deferred generic mapped types must resolve, got {:?}",
        format_codes(&diagnostics)
    );
}

/// Concrete composite operands keep materializing: assignability in both
/// directions works, and a genuinely missing member is still reported.
#[test]
fn concrete_intersection_mapped_still_materializes() {
    let source = r#"
export const x: Readonly<{ a: 1 } & { b: 2 }> = { a: 1, b: 2 }
export const y: { readonly a: 1; readonly b: 2 } = x
export const bad: Readonly<{ a: 1 } & { b: 2 }> = (null as any as { a: 1 })
"#;
    let diagnostics = strict_diagnostics(source);
    assert_eq!(
        diagnostics.len(),
        1,
        "exactly the deliberately-bad assignment must error, got {:?}",
        format_codes(&diagnostics)
    );
    // tsc reports the single missing property as TS2741.
    assert_eq!(diagnostics[0].code, 2741);
}
