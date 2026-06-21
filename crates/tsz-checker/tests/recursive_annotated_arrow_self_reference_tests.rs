//! Regression matrix for a self-referential `const`/`let`/`var` bound to a
//! fully-annotated arrow or function expression (issue #14234).
//!
//! When such a binding references itself inside its own body (for example
//! `const f = (p: number[] | number): string => { ...; const r = p.map(f); ... }`),
//! `tsc` resolves the in-body self-reference to the binding's *declared*
//! signature — computable from the parameter and return annotations without
//! analyzing the body. tsz previously collapsed the self-reference to
//! `unknown` during the resolution cycle (the variable is not a `FUNCTION`
//! symbol, so the existing function cycle-break did not apply), so
//! `p.map(f)` degraded to `unknown[]` and produced a false `TS2571`/`TS18046`.
//!
//! These tests lock the parity. Per the anti-hardcoding gate, the binder name
//! is varied across cases so a fix keyed on a specific spelling would fail.

use tsz_checker::test_utils::{check_source_strict, diagnostic_count};

const TS2571_OBJECT_IS_UNKNOWN: u32 = 2571;
const TS18046_IS_OF_TYPE_UNKNOWN: u32 = 18046;

/// Total `unknown`-object element-access diagnostics on a snippet (the two
/// forms are mutually exclusive per access site).
fn unknown_object_diags(source: &str) -> usize {
    let diags = check_source_strict(source);
    diagnostic_count(&diags, TS2571_OBJECT_IS_UNKNOWN)
        + diagnostic_count(&diags, TS18046_IS_OF_TYPE_UNKNOWN)
}

/// The issue witness: a fully-annotated recursive const arrow whose body maps
/// over itself. The mapped result must be `string[]`, so the trailing
/// `.toUpperCase()` is well typed.
#[test]
fn recursive_const_arrow_self_reference_resolves_declared_signature() {
    let source = r#"
const consumer = (param: number[] | number): string => {
  if (typeof param === 'number') return String(param);
  const r = param.map(consumer);
  return r[0]!.toUpperCase();
};
"#;
    assert_eq!(
        unknown_object_diags(source),
        0,
        "in-body self-reference must resolve to the declared `(param) => string` signature"
    );
}

/// Same shape via a `function` expression rather than an arrow.
#[test]
fn recursive_const_function_expression_self_reference_resolves_declared_signature() {
    let source = r#"
const handler = function (items: number[] | number): string {
  if (typeof items === 'number') return String(items);
  const mapped = items.map(handler);
  return mapped[0]!.toUpperCase();
};
"#;
    assert_eq!(
        unknown_object_diags(source),
        0,
        "function-expression form must behave identically to the arrow form"
    );
}

/// Renamed binder — the fix must be structural, not keyed on a name.
#[test]
fn recursive_const_arrow_self_reference_is_not_name_keyed() {
    let source = r#"
const walkValue = (entry: number[] | number): string => {
  if (typeof entry === 'number') return String(entry);
  const collected = entry.map(walkValue);
  return collected[0]!.toUpperCase();
};
"#;
    assert_eq!(
        unknown_object_diags(source),
        0,
        "the declared-signature recovery must not depend on the binder spelling"
    );
}

/// `let`-bound recursive annotated arrow.
#[test]
fn recursive_let_arrow_self_reference_resolves_declared_signature() {
    let source = r#"
let visit = (node: string[] | string): string => {
  if (typeof node === 'string') return node;
  const parts = node.map(visit);
  return parts[0]!.toUpperCase();
};
"#;
    assert_eq!(
        unknown_object_diags(source),
        0,
        "a `let`-bound recursive annotated arrow resolves to its declared signature"
    );
}

/// Negative control: a genuinely `unknown` indexed access still errors, proving
/// the recovery is scoped to the self-reference cycle and does not blanket-
/// suppress the `unknown`-object diagnostics.
#[test]
fn unknown_object_index_still_reports() {
    let source = r#"
const reader = (value: unknown): string => {
  const slot = (value as unknown)["k"];
  return slot;
};
"#;
    assert_eq!(
        diagnostic_count(&check_source_strict(source), TS2571_OBJECT_IS_UNKNOWN),
        1,
        "indexing a genuine `unknown` base must still report TS2571"
    );
}
