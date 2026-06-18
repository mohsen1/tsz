//! Regression tests for #13618: an object literal whose contextual type is an
//! *inline* generic application (e.g. ts-essentials `DeepPick<Type, Filter>`
//! used directly as a variable annotation) must have that application reduced
//! through the authoritative resolver before its per-property contextual types
//! are extracted.
//!
//! Structural rule: when an object literal is contextually typed by an
//! unevaluated complex type — an instantiated generic `Application`, or a
//! deferred `Conditional`/`IndexAccess`/`KeyOf` — `tsc` evaluates that type to
//! its structural object form and contextually types each property against the
//! reduced member. tsz's per-property contextual extraction runs with a
//! non-resolving resolver, so a recursive instantiated body left as an opaque
//! `Application` could not expand: each property's expected type degraded to the
//! un-reduced source (`DeepPick<User, { id: true }>` resolving to `User` instead
//! of the picked `{ id: string }`), producing spurious `TS2741`/`TS2322`.
//!
//! The divergence is purely single-file and position-sensitive (a `satisfies`
//! operand, a parameter/return annotation, or a `type Alias = DeepPick<...>`
//! wrapper already reach the literal in evaluated form and were always clean —
//! it is only the inline-application-in-annotation form that degraded), so this
//! is distinct from the cross-arena `error`/`never`-in-type-argument family
//! (#13044/#13484). The fix reduces an unevaluated complex contextual type via
//! the authoritative resolver in the object-literal request setup.
//!
//! Binder names are varied from the original ts-essentials witness so the
//! behavior follows the type shape, not a spelling (anti-hardcoding gate). The
//! matrix covers the object-only and array-bearing branches, single and multiple
//! keys, a named-alias control that was already correct, and negative controls
//! proving the picked leaf is the concrete reduced type (not `any`/`error`).

use tsz_checker::diagnostics::Diagnostic;

fn check_source(source: &str) -> Vec<Diagnostic> {
    let libs = tsz_checker::test_utils::load_default_lib_files();
    tsz_checker::test_utils::check_source_with_libs(
        source,
        "test.ts",
        tsz_checker::context::CheckerOptions {
            strict: true,
            ..Default::default()
        },
        &libs,
    )
}

/// A faithful reconstruction of the ts-essentials `DeepPick` chain with renamed
/// binders. `Project<Source, Spec>` keeps each key listed in `Spec`, recursing
/// into nested objects/arrays and treating a `true` leaf as "take the whole
/// source property".
const PROJECT_DEF: &str = r#"
type Prim = string | number | boolean | bigint | symbol | undefined | null;
type Native = Prim | Function | Date | Error | RegExp;
type AnyRec<T = any> = Record<keyof any, T>;
type Project<Source, Spec> = Source extends Native
  ? Source
  : Source extends Array<infer Item>
    ? Spec extends Array<infer SpecItem>
      ? Array<Project<Item, SpecItem>>
      : Source
    : Spec extends AnyRec
      ? {
          [K in keyof Source as K extends keyof Spec ? K : never]: Spec[K &
            keyof Spec] extends true
            ? Source[K]
            : K extends keyof Spec
              ? Project<Source[K], Spec[K]>
              : never;
        }
      : never;
"#;

fn project_source(body: &str) -> String {
    format!("{PROJECT_DEF}\n{body}\n")
}

fn assignment_errors(diagnostics: &[Diagnostic]) -> Vec<&Diagnostic> {
    diagnostics
        .iter()
        .filter(|d| d.code == 2322 || d.code == 2741 || d.code == 2353)
        .collect()
}

fn assert_no_assignment_error(body: &str, label: &str) {
    let diagnostics = check_source(&project_source(body));
    let errors = assignment_errors(&diagnostics);
    assert!(
        errors.is_empty(),
        "[{label}] expected no TS2322/TS2741/TS2353; got:\n{errors:#?}\nall: {diagnostics:#?}"
    );
}

fn assert_has_assignment_error(body: &str, label: &str) {
    let diagnostics = check_source(&project_source(body));
    let errors = assignment_errors(&diagnostics);
    assert!(
        !errors.is_empty(),
        "[{label}] expected a TS2322/TS2741/TS2353 (picked leaf is the concrete \
         reduced type, not any/error); got none. All: {diagnostics:#?}"
    );
}

/// The #13618 witness: an inline `Project<...>` application used directly as a
/// const annotation, over a multi-key object/array mix with a *named* filter
/// alias. Before the fix each picked property degraded to the full source type
/// (`Widget`, requiring `mode`), so the picked-shape literal false-failed
/// `TS2741`.
#[test]
fn inline_project_application_multi_key_object_and_array() {
    let body = r#"
type Widget = { id: string; mode: 'a' | 'b' };
type Plan = { lead: Widget; crew: Widget[]; backups: Widget[] };
type PickSpec = { lead: { id: true }; crew: { id: true }[]; backups: { id: true }[] };
const out: Project<Plan, PickSpec> = {
  lead: { id: 'lead_id' },
  crew: [{ id: 'crew_id' }],
  backups: [{ id: 'backup_id' }],
};
"#;
    assert_no_assignment_error(
        body,
        "inline application, multi-key object+array, named filter",
    );
}

/// Minimal trigger: a single object key with a *named* filter alias. The inline
/// `Project<{ box: Cell }, OneSpec>` annotation must reduce to
/// `{ box: { id: string } }` so the picked-shape literal is accepted.
#[test]
fn inline_project_application_single_key_named_filter() {
    let body = r#"
type Cell = { id: string; mode: 'a' | 'b' };
type Wrap = { box: Cell };
type OneSpec = { box: { id: true } };
const w: Project<Wrap, OneSpec> = { box: { id: 'k' } };
"#;
    assert_no_assignment_error(body, "single key, named filter alias");
}

/// An inline filter (no named alias) was always clean; keep it as a control so
/// a future change to the reduction path cannot regress the common case.
#[test]
fn inline_project_application_inline_filter_control() {
    let body = r#"
type Cell = { id: string; mode: 'a' | 'b' };
const w: Project<{ box: Cell }, { box: { id: true } }> = { box: { id: 'k' } };
"#;
    assert_no_assignment_error(body, "inline filter control");
}

/// A `type Alias = Project<...>` wrapper already reached the literal in
/// evaluated form (and was always correct). Locking it guards the parity floor.
#[test]
fn named_result_alias_control() {
    let body = r#"
type Cell = { id: string; mode: 'a' | 'b' };
type OneSpec = { box: { id: true } };
type Picked = Project<{ box: Cell }, OneSpec>;
const w: Picked = { box: { id: 'k' } };
"#;
    assert_no_assignment_error(body, "named result alias control");
}

/// Negative control: the picked leaf is the concrete reduced type, not silenced
/// to `any`/`error`. A genuinely mismatched leaf value (`id` should be `string`)
/// must still report.
#[test]
fn inline_project_application_rejects_genuine_leaf_mismatch() {
    let body = r#"
type Cell = { id: string; mode: 'a' | 'b' };
type OneSpec = { box: { id: true } };
const bad: Project<{ box: Cell }, OneSpec> = { box: { id: 123 } };
"#;
    assert_has_assignment_error(body, "genuine leaf mismatch still reported");
}

/// Negative control: an excess property not present in the picked result is
/// still rejected (the reduced contextual type is a real object, not `any`).
#[test]
fn inline_project_application_rejects_excess_picked_property() {
    let body = r#"
type Cell = { id: string; mode: 'a' | 'b' };
type OneSpec = { box: { id: true } };
const bad: Project<{ box: Cell }, OneSpec> = { box: { id: 'k', mode: 'a' } };
"#;
    assert_has_assignment_error(body, "excess property on picked leaf rejected");
}
