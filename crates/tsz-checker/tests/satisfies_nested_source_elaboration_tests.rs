//! Regression tests: `satisfies` elaboration drills into nested literal / array
//! / arrow-function property values the same way a direct assignment does.
//!
//! Structural rule: tsc runs the *same* `elaborateError` for a `satisfies`
//! operand as for an assignment (only the outer error code / keyword anchor
//! differ). When a property value is itself a fresh object literal, an array
//! literal, or an expression-bodied arrow, tsc's `elaborateElementwise` /
//! `elaborateArrowFunction` recurse and anchor the `TS2322` at the innermost
//! mismatch — a nested property name, an array element, or the arrow's returned
//! expression — instead of the coarse whole-property (or whole-expression
//! `TS1360`) frame. tsz previously stopped one level in for `satisfies`, so it
//! reported the whole property-value type mismatch while assignment drilled.
//!
//! Owner layer: checker assignability elaboration
//! (`elaborate_satisfies_object_literal` /
//! `check_satisfies_assignable_or_report` routing through the shared
//! `try_elaborate_assignment_source_error` boundary).
//!
//! All fixtures vary binder names and pair every drilled case against the
//! divergence witnessed with `tsc` 6.0.2; positive controls (leaf primitive at
//! property name, excess property TS2353, direct-primitive TS1360,
//! annotated-param / method function-level frame) and valid no-error cases are
//! included so the drill cannot over-reach.

use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::check_source;

fn diagnostics(source: &str) -> Vec<Diagnostic> {
    check_source(source, "test.ts", CheckerOptions::default())
}

fn with_code(source: &str, code: u32) -> Vec<Diagnostic> {
    diagnostics(source)
        .into_iter()
        .filter(|diag| diag.code == code)
        .collect()
}

fn line_col(source: &str, start: u32) -> (usize, usize) {
    let start = start as usize;
    let line = source[..start].bytes().filter(|b| *b == b'\n').count() + 1;
    let line_start = source[..start].rfind('\n').map_or(0, |idx| idx + 1);
    (line, start - line_start + 1)
}

/// The `TS2322` must anchor exactly at `needle` in `source` (0-based byte
/// offset of the innermost mismatch), with no residual `TS1360`.
fn assert_single_ts2322_at(source: &str, needle: &str) {
    let ts2322 = with_code(source, 2322);
    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly one TS2322, got: {ts2322:?}"
    );
    let expected = source.find(needle).expect("fixture contains the anchor");
    assert_eq!(
        ts2322[0].start as usize,
        expected,
        "TS2322 anchored at {:?}, expected the innermost node at {:?}",
        line_col(source, ts2322[0].start),
        line_col(source, expected as u32),
    );
    assert!(
        with_code(source, 1360).is_empty(),
        "nested drill-in must suppress the coarse TS1360"
    );
}

#[test]
fn nested_object_property_value_drills_to_inner_property() {
    // `{ a: { b: 1 } } satisfies { a: { b: string } }` — tsc anchors at inner `b`.
    let source = "const cfg = { outer: { leaf: 1 } } satisfies { outer: { leaf: string } };\n";
    assert_single_ts2322_at(source, "leaf: 1");
    // The anchor is the inner property name, not the outer one.
    let ts2322 = with_code(source, 2322);
    assert_eq!(line_col(source, ts2322[0].start), (1, 24));
}

#[test]
fn three_level_nested_object_drills_to_deepest_property() {
    let source = "const deep = { p: { q: { r: 1 } } } satisfies { p: { q: { r: string } } };\n";
    assert_single_ts2322_at(source, "r: 1");
}

#[test]
fn arrow_property_value_drills_to_returned_expression() {
    // `{ handler: () => 7 } satisfies { handler: () => string }`.
    let source = "const svc = { handler: () => 7 } satisfies { handler: () => string };\n";
    assert_single_ts2322_at(source, "7");
    assert!(
        with_code(source, 2322)[0].message_text.contains("'number'"),
        "message drills to the returned number, not the function type"
    );
}

#[test]
fn renamed_arrow_property_value_uses_same_path() {
    // Varied binder names — the drill is structural, not name-driven.
    let source = "const widget = { onTick: () => true } satisfies { onTick: () => string };\n";
    assert_single_ts2322_at(source, "true");
    assert!(
        with_code(source, 2322)[0]
            .message_text
            .contains("'boolean'")
    );
}

#[test]
fn nested_arrow_property_value_drills_two_levels() {
    let source =
        "const shell = { inner: { run: () => 1 } } satisfies { inner: { run: () => string } };\n";
    assert_single_ts2322_at(source, "1");
}

#[test]
fn array_literal_property_element_drills_to_offending_element() {
    let source = "const bag = { items: [3, \"z\"] } satisfies { items: number[] };\n";
    assert_single_ts2322_at(source, "\"z\"");
    assert!(
        with_code(source, 2322)[0].message_text.contains("'string'"),
        "drills to the string element, not the whole array"
    );
}

#[test]
fn direct_parenthesized_arrow_satisfies_drills_to_body() {
    // Non-object-literal source: the whole `satisfies` operand is an arrow.
    let source = "const fn = (() => 5) satisfies (() => string);\n";
    assert_single_ts2322_at(source, "5");
}

// ---- controls: leaf anchoring and coarse frames that must NOT change ----

#[test]
fn primitive_property_value_anchors_at_property_name() {
    // Leaf primitive value: tsc anchors at the property name, still TS2322.
    let source = "const okish = { count: 1 } satisfies { count: string };\n";
    let ts2322 = with_code(source, 2322);
    assert_eq!(ts2322.len(), 1, "got: {ts2322:?}");
    assert_eq!(
        ts2322[0].start as usize,
        source.find("count: 1").expect("fixture"),
        "primitive leaf stays anchored at the property name"
    );
    assert!(with_code(source, 1360).is_empty());
}

#[test]
fn excess_property_still_reports_ts2353() {
    let source = "const extra = { x: 1, y: 2 } satisfies { x: number };\n";
    assert_eq!(
        with_code(source, 2353).len(),
        1,
        "excess property TS2353 preserved"
    );
    assert!(
        with_code(source, 2322).is_empty() && with_code(source, 1360).is_empty(),
        "TS2353 suppresses the assignability frames"
    );
}

#[test]
fn direct_primitive_satisfies_keeps_ts1360() {
    // No nested structure to drill into: the coarse TS1360 must remain.
    let source = "const s = \"hello\" satisfies number;\n";
    assert_eq!(
        with_code(source, 1360).len(),
        1,
        "primitive satisfies stays TS1360"
    );
    assert!(with_code(source, 2322).is_empty());
}

#[test]
fn index_signature_reports_each_property() {
    // Inline index signature (no lib dependency) — each property value is
    // checked against the string index value type and drills to its own TS2322.
    let source = "const rec = { a: 1, b: 2 } satisfies { [k: string]: string };\n";
    assert_eq!(
        with_code(source, 2322).len(),
        2,
        "each index-signature property value mismatches"
    );
    assert!(with_code(source, 1360).is_empty());
}

#[test]
fn annotated_param_arrow_keeps_function_level_frame() {
    // tsc's `elaborateArrowFunction` bails when a parameter is annotated, so the
    // mismatch stays at the function-type level anchored at the property name.
    let source = "const a = { fn: (n: number) => 1 } satisfies { fn: (n: number) => string };\n";
    let ts2322 = with_code(source, 2322);
    assert_eq!(ts2322.len(), 1, "got: {ts2322:?}");
    assert_eq!(
        ts2322[0].start as usize,
        source.find("fn: (n").expect("fixture"),
        "annotated-param arrow anchors at the property name, function-level"
    );
    assert!(
        ts2322[0].message_text.contains("=>"),
        "message is the whole function type, not the drilled return"
    );
}

#[test]
fn method_shorthand_keeps_function_level_frame() {
    // Method (block body) — tsc keeps the function-level frame at the method name.
    let source = "const o = { m() { return 1; } } satisfies { m: () => string };\n";
    let ts2322 = with_code(source, 2322);
    assert_eq!(ts2322.len(), 1, "got: {ts2322:?}");
    assert_eq!(
        ts2322[0].start as usize,
        source.find("m()").expect("fixture"),
        "method anchors at the method name"
    );
}

#[test]
fn block_bodied_arrow_property_keeps_function_level_frame() {
    // tsc's `elaborateArrowFunction` only elaborates *expression* bodies; a block
    // body keeps the function-level TS2322 anchored at the property name (with the
    // nested return chain), never drilling to the `return` statement.
    let source = "const o = { fn: () => { return 1; } } satisfies { fn: () => string };\n";
    let ts2322 = with_code(source, 2322);
    assert_eq!(ts2322.len(), 1, "got: {ts2322:?}");
    assert_eq!(
        ts2322[0].start as usize,
        source.find("fn: () =>").expect("fixture"),
        "block-bodied arrow anchors at the property name, function-level"
    );
    assert!(
        ts2322[0].message_text.contains("=>"),
        "message is the whole function type, not a drilled return"
    );
    assert!(with_code(source, 1360).is_empty());
}

#[test]
fn direct_block_bodied_arrow_satisfies_keeps_ts1360() {
    // A direct block-bodied arrow does not drill, so the coarse TS1360 remains.
    let source = "const f = (() => { return 1; }) satisfies (() => string);\n";
    assert_eq!(
        with_code(source, 1360).len(),
        1,
        "block-body direct arrow stays TS1360"
    );
    assert!(with_code(source, 2322).is_empty());
}

// ---- valid cases: must stay completely error-free ----

#[test]
fn valid_nested_object_satisfies_reports_nothing() {
    let source = "const good = { a: { b: \"x\" } } satisfies { a: { b: string } };\n";
    assert!(
        diagnostics(source).is_empty(),
        "got: {:?}",
        diagnostics(source)
    );
}

#[test]
fn valid_arrow_property_satisfies_reports_nothing() {
    let source = "const good = { fn: () => \"x\" } satisfies { fn: () => string };\n";
    assert!(
        diagnostics(source).is_empty(),
        "got: {:?}",
        diagnostics(source)
    );
}

#[test]
fn valid_array_property_satisfies_reports_nothing() {
    let source = "const good = { items: [1, 2, 3] } satisfies { items: number[] };\n";
    assert!(
        diagnostics(source).is_empty(),
        "got: {:?}",
        diagnostics(source)
    );
}
