//! Regression fence: a failed generic call whose parameter is constrained by a
//! **type-alias application** (`V extends Alias<…, Ref>`) must report the
//! argument against the constraint's *alias name*, not its structurally
//! expanded body — matching `tsc`'s `aliasSymbol` / `aliasTypeArguments`
//! retention on instantiated types.
//!
//! ## Why this file exists
//!
//! It fences the **working neighbours** of the still-failing driver row
//! `tsz-cli::driver_tests::cross_file_dependent_operand_aliases_accept_scalars_and_literal_arrays`
//! (#15983 — the last unfixed row of that batch).
//!
//! ### Root cause of the driver row (measured on `main` `e2f5f9e`)
//!
//! In the driver fixture, three deliberately-invalid operands each produce the
//! expected `TS2345` at the expected offset, but the *rendered parameter type*
//! diverges by which field the constraint's `Ref` selects:
//!
//! * `'category'` (a union-literal field) renders by alias —
//!   `DependentOperand<Registry, keyof Registry, "category">`;
//! * `'title'` (a `string` field) renders as the **fully expanded union**
//!   `string | Expr<…> | ScalarBuilder<…> | ((scope: …) => …) | readonly …[] | null`,
//!   dropping the `DependentOperand<…>` alias name.
//!
//! The displayed parameter type is the generic parameter `Value`'s constraint,
//! *clamped and evaluated* during overload resolution
//! (`get_parameter_type_for_call` → the assignability target). For `'category'`
//! the clamp keeps the constraint as a deferred `Application` (so the printer
//! shows the alias); for `'title'` the inner `FieldOutput<…,"title">`
//! materialises to `string`, the whole alias body reduces to a plain `Union`,
//! and that union is interned **outside** the solver's display-alias
//! provenance machinery (`record_application_evaluation_display_aliases` /
//! `store_display_alias` in `tsz-solver`), so no `evaluated → Application`
//! reverse entry exists and the printer has no alias to recover.
//!
//! That is the unmaterialised-`Lazy`/`Application`-operand family (#15396):
//! the fix belongs to the materialise-or-defer gateway at the
//! constraint-clamp/display boundary, and needs the project corpus gate — it is
//! **not** attempted here. The divergence does not reduce below the driver
//! fixture's deep `DependentOperand` nesting (multi-arm union of functions,
//! readonly arrays, and generic sub-aliases): the shallow constraint
//! applications pinned below all render by alias correctly today.
//!
//! ### What these controls guard
//!
//! They pin that a shallow type-alias-application constraint keeps its alias
//! name in the `TS2345` parameter render for **both** a `string`-valued and a
//! union-literal-valued inner field — the two shapes the driver row's
//! `'title'` / `'category'` operands exercise. When the materialise-or-defer
//! fix lands and closes the driver row, these must stay green so the fix cannot
//! regress the already-correct shallow cases. Binder names deliberately vary
//! between cases to keep the assertion structural, not spelling-keyed
//! (`.claude/CLAUDE.md` anti-hardcoding gate).

use tsz_checker::test_utils::check_source_strict;

/// Return the `TS2345` "not assignable to parameter of type '…'" parameter
/// renders produced by `source`, in diagnostic order.
fn ts2345_parameter_renders(source: &str) -> Vec<String> {
    check_source_strict(source)
        .iter()
        .filter(|d| d.code == 2345)
        .map(|d| d.message_text.clone())
        .collect()
}

/// A `string`-valued inner field: the constraint
/// `Dep<DB, "row", "title">` must render by its alias name (the shallow analogue
/// of the driver row's `'title'` operand, which loses the alias only under the
/// deep `DependentOperand` nesting).
#[test]
fn string_valued_field_constraint_renders_by_alias() {
    let renders = ts2345_parameter_renders(
        r#"
interface Row { title: string; kind: 'a' | 'b' }
interface DB { row: Row }
type FieldOut<D, T extends keyof D, N> = { [P in T]: N extends keyof D[P] ? D[P][N] : never }[T]
type Dep<D, T extends keyof D, R> = FieldOut<D, T, R> | { readonly tag: 1 }
declare function acc<R extends keyof Row, V extends Dep<DB, 'row', R>>(r: R, v: V): void
acc('title', 0)
"#,
    );
    assert_eq!(renders.len(), 1, "expected one TS2345: {renders:#?}");
    assert!(
        renders[0].contains(r#"Dep<DB, "row", "title">"#),
        "string-valued field constraint must render by alias, got: {}",
        renders[0]
    );
}

/// A union-literal-valued inner field: the constraint
/// `Dep<DB, "row", "kind">` must render by its alias name (the shallow analogue
/// of the driver row's `'category'` operand, which already renders by alias).
#[test]
fn union_valued_field_constraint_renders_by_alias() {
    let renders = ts2345_parameter_renders(
        r#"
interface Row { title: string; kind: 'a' | 'b' }
interface DB { row: Row }
type FieldOut<D, T extends keyof D, N> = { [P in T]: N extends keyof D[P] ? D[P][N] : never }[T]
type Dep<D, T extends keyof D, R> = FieldOut<D, T, R> | { readonly tag: 1 }
declare function acc<R extends keyof Row, V extends Dep<DB, 'row', R>>(r: R, v: V): void
acc('kind', 0)
"#,
    );
    assert_eq!(renders.len(), 1, "expected one TS2345: {renders:#?}");
    assert!(
        renders[0].contains(r#"Dep<DB, "row", "kind">"#),
        "union-valued field constraint must render by alias, got: {}",
        renders[0]
    );
}

/// Name-agnostic: the same shape with different binder/identifier names keeps
/// the alias render, proving the fence is structural rather than keyed on the
/// `Dep`/`DB`/`Row` spellings.
#[test]
fn renamed_binders_constraint_renders_by_alias() {
    let renders = ts2345_parameter_renders(
        r#"
interface Record0 { label: string; state: 'x' | 'y' }
interface Catalog { entry: Record0 }
type Cell<C, K extends keyof C, N> = { [P in K]: N extends keyof C[P] ? C[P][N] : never }[K]
type Operand<C, K extends keyof C, S> = Cell<C, K, S> | { readonly marker: 0 }
declare function feed<S extends keyof Record0, W extends Operand<Catalog, 'entry', S>>(s: S, w: W): void
feed('label', 0)
feed('state', 0)
"#,
    );
    assert_eq!(renders.len(), 2, "expected two TS2345: {renders:#?}");
    assert!(
        renders[0].contains(r#"Operand<Catalog, "entry", "label">"#),
        "renamed string-valued constraint must render by alias, got: {}",
        renders[0]
    );
    assert!(
        renders[1].contains(r#"Operand<Catalog, "entry", "state">"#),
        "renamed union-valued constraint must render by alias, got: {}",
        renders[1]
    );
}
