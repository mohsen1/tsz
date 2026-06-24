//! Tests that an object-literal property matched against a target **index
//! signature** (`{ [k: string]: T }` / `{ [k: number]: T }`) elaborates a
//! mismatching **fresh** object/array-literal value down to its deepest leaf —
//! exactly as a named-property target does — instead of also reporting the
//! property-level aggregate.
//!
//! Structural rule: `tsc`'s `elaborateElementwise` descends into a fresh
//! object/array literal and anchors the single deepest leaf mismatch. tsz's
//! index-signature value check (`try_union_index_signature_value_check`)
//! previously reported the relation at the property name via
//! `check_assignable_or_report_at_exact_anchor_without_source_elaboration`,
//! which does not drill into the source literal. For `{ [k: string]: { n:
//! number } } = { foo: { n: "x" } }` that produced a redundant `{ n: string }`
//! is-not-assignable-to `{ n: number }` at `foo` *in addition to* (or, for an
//! interface index signature, *instead of*) the leaf `string`→`number` at `n`.
//! `tsc` reports just the leaf.
//!
//! Non-fresh values (a reference, a call result) cannot be drilled into, so the
//! property-level aggregate is kept — matching `tsc`. The default test lib does
//! not expose `Record`, so these fixtures spell the index signatures directly;
//! `Record<string, T>` lowers to the same `{ [k: string]: T }` shape.

use crate::test_utils::{
    check_source_diagnostics, diagnostic_count, diagnostic_line_column, diagnostics_with_code,
};

const TS2322: u32 = 2322;
const TS2353: u32 = 2353;

/// A string index signature whose value is an object type, initialized with a
/// nested object literal whose only mismatch is a leaf property: exactly one
/// TS2322, anchored at the inner `n` value, with no property-level aggregate at
/// `foo`.
#[test]
fn string_index_value_object_literal_reports_only_deepest_leaf() {
    let source = r#"
type R = { [k: string]: { n: number } };
const r: R = { foo: { n: "x" } };
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        diagnostic_count(&diags, TS2322),
        1,
        "expected exactly one TS2322 (deepest leaf), got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
    let diag = diagnostics_with_code(&diags, TS2322)[0];
    let (_line, col) = diagnostic_line_column(source, diag);
    assert!(
        diag.message_text.contains("'string'") && diag.message_text.contains("'number'"),
        "leaf message should be string->number, got: {}",
        diag.message_text
    );
    assert!(
        !diag.message_text.contains("{ n: string"),
        "must not be the property-level aggregate, got: {}",
        diag.message_text
    );
    // `{ foo: { n: "x" } }` — the `"x"` value sits well past column 20.
    assert!(col > 20, "leaf should anchor at the inner value, col={col}");
}

/// Same shape via an explicit interface index signature. Before the fix this
/// emitted *only* the aggregate at `foo` (no leaf at all); now it matches the
/// type-alias form: one TS2322 at the leaf.
#[test]
fn interface_index_signature_value_object_literal_reports_only_deepest_leaf() {
    let source = r#"
interface R { [k: string]: { n: number }; }
const r: R = { foo: { n: "x" } };
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        diagnostic_count(&diags, TS2322),
        1,
        "expected exactly one TS2322 (deepest leaf), got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
    let diag = diagnostics_with_code(&diags, TS2322)[0];
    assert!(
        diag.message_text.contains("'string'") && diag.message_text.contains("'number'"),
        "leaf message should be string->number, got: {}",
        diag.message_text
    );
}

/// Renamed binders / property names must not change the behavior — the gate is
/// purely structural (fresh literal matched through an index signature), never
/// keyed on identifiers.
#[test]
fn renamed_property_and_binder_still_report_only_deepest_leaf() {
    let source = r#"
type Bag = { [key: string]: { count: number } };
const store: Bag = { widget: { count: "lots" } };
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        diagnostic_count(&diags, TS2322),
        1,
        "renamed shape should still report a single leaf TS2322, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

/// A doubly-nested string index signature drills all the way to the single
/// deepest leaf, not once per nesting level.
#[test]
fn deeply_nested_index_signatures_report_only_deepest_leaf() {
    let source = r#"
type Inner = { [k: string]: { n: number } };
type R = { [k: string]: Inner };
const r: R = { a: { b: { n: "x" } } };
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        diagnostic_count(&diags, TS2322),
        1,
        "expected one TS2322 at the deepest leaf across two index levels, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

/// Multiple mismatching leaves in the same nested literal each get their own
/// leaf TS2322 (tsc reports each incompatible property), with no aggregate.
#[test]
fn multiple_mismatching_leaves_each_report_at_the_leaf() {
    let source = r#"
type R = { [k: string]: { n: number; s: string } };
const r: R = { foo: { n: "x", s: 1 } };
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        diagnostic_count(&diags, TS2322),
        2,
        "expected one TS2322 per mismatching leaf, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
    for diag in diagnostics_with_code(&diags, TS2322) {
        assert!(
            !diag.message_text.contains("{ n:"),
            "no property-level aggregate expected, got: {}",
            diag.message_text
        );
    }
}

/// An array-literal value matched through the index signature also drills into
/// the offending element rather than reporting an aggregate at the property.
#[test]
fn array_literal_value_drills_into_offending_element() {
    let source = r#"
type R = { [k: string]: { n: number }[] };
const r: R = { foo: [{ n: "x" }] };
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        diagnostic_count(&diags, TS2322),
        1,
        "array-element value should drill to the leaf, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

/// A fresh function value (unannotated params) matched through the index
/// signature anchors at its return expression, exactly like a named-property
/// target — not at the whole `() => T` function type on the property.
#[test]
fn function_value_drills_into_return_expression() {
    let source = r#"
type R = { [k: string]: () => number };
const r: R = { foo: () => "x" };
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        diagnostic_count(&diags, TS2322),
        1,
        "function value should anchor a single TS2322 at its return, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
    let diag = diagnostics_with_code(&diags, TS2322)[0];
    assert!(
        !diag.message_text.contains("=>"),
        "must anchor at the return leaf, not the function type, got: {}",
        diag.message_text
    );
}

/// A function value whose parameter IS annotated keeps the function-type
/// mismatch (tsc's `elaborateArrowFunction` does not drill into a body when any
/// parameter carries an explicit type) — the shared gateway preserves that.
#[test]
fn annotated_param_function_value_keeps_function_type_mismatch() {
    let source = r#"
type R = { [k: string]: (x: number) => void };
const r: R = { foo: (x: string) => {} };
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        diagnostic_count(&diags, TS2322),
        1,
        "annotated-param function value should report one TS2322, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

/// A **non-fresh** value (a plain reference) cannot be drilled into, so the
/// property-level aggregate is preserved — matching `tsc`.
#[test]
fn reference_value_keeps_property_level_aggregate() {
    let source = r#"
type R = { [k: string]: { n: number } };
const v: { n: string } = { n: "x" } as { n: string };
const r: R = { foo: v };
"#;
    let diags = check_source_diagnostics(source);
    let aggregate = diagnostics_with_code(&diags, TS2322)
        .into_iter()
        .find(|d| d.message_text.contains("{ n: string"));
    assert!(
        aggregate.is_some(),
        "non-fresh reference value should keep the property-level aggregate, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

/// An excess property inside the nested literal is still reported as TS2353,
/// not displaced by a property-level aggregate.
#[test]
fn nested_excess_property_reports_ts2353_not_aggregate() {
    let source = r#"
type R = { [k: string]: { n: number } };
const r: R = { foo: { n: 1, extra: 2 } };
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        diagnostic_count(&diags, TS2353),
        1,
        "nested excess property should be TS2353, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        diagnostic_count(&diags, TS2322),
        0,
        "no TS2322 aggregate expected alongside the nested excess TS2353"
    );
}
