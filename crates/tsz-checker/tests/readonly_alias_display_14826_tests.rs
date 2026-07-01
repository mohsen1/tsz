//! A `readonly` tuple/array type referenced through a non-generic type alias
//! keeps its `aliasSymbol` in `tsc`, so when it is the TS2322 assignment
//! *target* the diagnostic names the outer alias (`R`, `RA`, `RT`), not the
//! structural `readonly [...]` / `readonly T[]` form.
//!
//! Before the fix, tsz's TS2322 target-display path had no provenance-aware
//! recovery for `readonly` aliases, so it fell through to structural formatting
//! and dropped the outer alias. Worse, when a *mutable* tuple alias of the same
//! element shape existed anywhere in scope, the recursive format of the inner
//! tuple resolved that coincidentally-shaped alias (tuples/arrays are
//! content-interned), yielding the syntactically-invalid `readonly M`.
//!
//! The fix (1) recovers the outer `readonly` alias by name and (2) renders the
//! synthetic inner array/tuple structurally — `readonly` applies only to
//! array/tuple literals, so the inner node never carries an independent alias
//! symbol. Binder names are varied across cases so the behavior is proven
//! structural, not keyed on a fixture identifier.

use tsz_checker::context::CheckerOptions;
use tsz_common::diagnostics::Diagnostic;

fn check_strict(source: &str) -> Vec<Diagnostic> {
    let options = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..Default::default()
    };
    tsz_checker::test_utils::check_source(source, "test.ts", options)
}

fn message_for(diags: &[Diagnostic], code: u32) -> String {
    let matches: Vec<&Diagnostic> = diags.iter().filter(|d| d.code == code).collect();
    assert!(
        !matches.is_empty(),
        "expected a TS{code} diagnostic, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
    matches[0].message_text.clone()
}

/// The headline repro: a `readonly` tuple alias `R` is the TS2322 target while a
/// mutable twin alias `M` of the same shape exists. tsc names `R`; tsz must not
/// emit the structural form nor the nonsensical `readonly M`.
#[test]
fn readonly_tuple_alias_target_names_alias_not_mutable_twin() {
    let diags = check_strict(
        "type M = [number, string];\ntype R = readonly [number, string];\nconst b: R = \"wrong\";\n",
    );
    let msg = message_for(&diags, 2322);
    assert!(
        msg.contains("is not assignable to type 'R'"),
        "readonly tuple alias target must render as 'R'; got: {msg}"
    );
    assert!(
        !msg.contains("readonly M"),
        "must never resolve the inner tuple to a coincidental mutable alias; got: {msg}"
    );
    assert!(
        !msg.contains("readonly [number, string]"),
        "the aliased target must not drop to the structural form; got: {msg}"
    );
}

/// No mutable twin in scope: the readonly tuple alias still names itself.
#[test]
fn readonly_tuple_alias_target_names_alias_without_twin() {
    let diags = check_strict("type Ro = readonly [number, string];\nconst b: Ro = \"wrong\";\n");
    let msg = message_for(&diags, 2322);
    assert!(
        msg.contains("is not assignable to type 'Ro'"),
        "readonly tuple alias target must render as 'Ro'; got: {msg}"
    );
    assert!(
        !msg.contains("readonly ["),
        "aliased readonly tuple must not render structurally; got: {msg}"
    );
}

/// A `readonly` array alias as the TS2322 target names the alias.
#[test]
fn readonly_array_alias_target_names_alias() {
    let diags = check_strict("type Arr = readonly number[];\nconst a: Arr = 1;\n");
    let msg = message_for(&diags, 2322);
    assert!(
        msg.contains("is not assignable to type 'Arr'"),
        "readonly array alias target must render as 'Arr'; got: {msg}"
    );
    assert!(
        !msg.contains("readonly number[]"),
        "aliased readonly array must not render structurally; got: {msg}"
    );
}

/// Distinct binder names prove the fix is structural, not keyed on `R`/`M`.
#[test]
fn readonly_tuple_alias_target_names_alias_renamed_binders() {
    let diags = check_strict(
        "type Pair = [number, string];\ntype Frozen = readonly [number, string];\nconst v: Frozen = \"wrong\";\n",
    );
    let msg = message_for(&diags, 2322);
    assert!(
        msg.contains("is not assignable to type 'Frozen'"),
        "renamed readonly tuple alias must render as 'Frozen'; got: {msg}"
    );
    assert!(
        !msg.contains("readonly Pair"),
        "must never resolve inner tuple to the renamed mutable twin; got: {msg}"
    );
}

/// An *un-aliased* readonly tuple target with a mutable twin alias in scope must
/// render structurally as `readonly [number, string]` — never `readonly Twin`.
/// This exercises the inner-node suppression directly (there is no outer alias
/// to recover, so formatting reaches the structural `ReadonlyType` arm).
#[test]
fn unaliased_readonly_tuple_target_renders_structurally_not_mutable_twin() {
    let diags = check_strict(
        "type Twin = [number, string];\nconst b: readonly [number, string] = \"wrong\";\n",
    );
    let msg = message_for(&diags, 2322);
    assert!(
        msg.contains("is not assignable to type 'readonly [number, string]'"),
        "un-aliased readonly tuple must render structurally; got: {msg}"
    );
    assert!(
        !msg.contains("readonly Twin"),
        "must never repaint the inner tuple with a coincidental mutable alias; got: {msg}"
    );
}
