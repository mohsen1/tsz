//! A `symbol`-keyed computed property whose `[k: symbol]` index value type is
//! an object shape drills into its nested object-literal value, exactly like a
//! `[k: string]` index does: a nested excess property is `TS2353` and a nested
//! type mismatch is `TS2322` at the deepest leaf — never the flat `TS2418`
//! computed-property-value aggregate.
//!
//! Regression for #16649. #16651 landed the nested *excess-property* drill-in
//! (the `TS2353` rows below), but the nested *type-mismatch* case still fell to
//! the flat `TS2418` because the source elaboration ran only after the
//! computed-property `TS2418` branch had already fired and `continue`d, and it
//! read `object_literal_property_initializer(report_idx)` — `None` for a symbol
//! key, whose `report_idx` is the whole literal. The elaboration now runs on the
//! resolved `nested_value_idx` ahead of the `TS2418` branch, so an elaboratable
//! object/array/function value anchors its nested leaf (`TS2322`/`TS2353`) while
//! a non-elaboratable primitive value keeps `TS2418`.
//!
//! Oracle: `typescript@7.0.2`, `--noEmit --strict --pretty false --lib es2024
//! --target es2022`. Binder names (symbol const, value interface, index value
//! types, member names) are varied across the rows so no fixture-name string
//! can drive the decision.

use tsz_checker::context::CheckerOptions;

fn check_strict(source: &str) -> Vec<(u32, String)> {
    let options = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..Default::default()
    };
    tsz_checker::test_utils::check_source(source, "test.ts", options)
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

fn codes(diags: &[(u32, String)]) -> Vec<u32> {
    diags.iter().map(|(c, _)| *c).collect()
}

// ---- The issue's own repro: excess property in the nested literal ----------

#[test]
fn symbol_index_object_value_nested_excess_is_ts2353_not_ts2418() {
    // `[sym]: { a: 1 }` is clean; `[sym]: { a: 1, b: 2 }` reports TS2353 on the
    // nested `b`, not TS2418 on the outer computed key.
    let source = r#"
declare const sym: unique symbol;
interface Val { a: number; }
interface I { [k: string]: number; [k: symbol]: Val; }
const ok: I = { [sym]: { a: 1 } };
const bad: I = { [sym]: { a: 1, b: 2 } };
"#;
    let diags = check_strict(source);
    assert_eq!(
        codes(&diags),
        vec![2353],
        "expected exactly one TS2353: {diags:?}"
    );
    assert!(
        diags[0].1.contains("'b'") && diags[0].1.contains("'Val'"),
        "TS2353 should name the nested excess property 'b' and type 'Val': {diags:?}"
    );
}

#[test]
fn symbol_index_present_member_only_is_clean() {
    // Isolated positive control with renamed binders.
    let source = r#"
declare const tag: unique symbol;
interface Payload { id: number; }
interface Bag { [s: string]: number; [s: symbol]: Payload; }
const bag: Bag = { [tag]: { id: 7 } };
"#;
    let diags = check_strict(source);
    assert!(
        diags.is_empty(),
        "present symbol member must be clean: {diags:?}"
    );
}

// ---- Nested type mismatch (not excess) elaborates to TS2322, not TS2418 ----

#[test]
fn symbol_index_object_value_nested_type_mismatch_is_ts2322_not_ts2418() {
    // The nested `a: "x"` mismatches `number`; tsc anchors TS2322 at the leaf,
    // the same as the string-keyed sibling, not the flat TS2418 aggregate.
    let source = r#"
declare const marker: unique symbol;
interface Cell { a: number; }
interface Store { [p: string]: number; [p: symbol]: Cell; }
const store: Store = { [marker]: { a: "x" } };
"#;
    let diags = check_strict(source);
    assert_eq!(
        codes(&diags),
        vec![2322],
        "expected exactly one TS2322: {diags:?}"
    );
    assert!(
        diags[0].1.contains("'string'") && diags[0].1.contains("'number'"),
        "TS2322 should be the nested string->number leaf mismatch: {diags:?}"
    );
}

// ---- Regression guard: string-key sibling was already correct --------------

#[test]
fn string_index_object_value_nested_excess_is_ts2353_regression_guard() {
    let source = r#"
interface Val { a: number; }
interface I { [k: string]: Val; }
const bad: I = { foo: { a: 1, b: 2 } };
"#;
    let diags = check_strict(source);
    assert_eq!(
        codes(&diags),
        vec![2353],
        "string-key nested excess is TS2353: {diags:?}"
    );
}

#[test]
fn string_index_object_value_nested_type_mismatch_is_ts2322_regression_guard() {
    let source = r#"
interface Val { a: number; }
interface I { [k: string]: Val; }
const bad: I = { foo: { a: "x" } };
"#;
    let diags = check_strict(source);
    assert_eq!(
        codes(&diags),
        vec![2322],
        "string-key nested mismatch is TS2322: {diags:?}"
    );
}

// ---- Symbol-only interface (no string index) drills in the same way --------

#[test]
fn symbol_only_index_object_value_nested_excess_is_ts2353() {
    let source = r#"
declare const key: unique symbol;
interface Val { a: number; }
interface J { [k: symbol]: Val; }
const bad: J = { [key]: { a: 1, b: 2 } };
"#;
    let diags = check_strict(source);
    assert_eq!(
        codes(&diags),
        vec![2353],
        "symbol-only nested excess is TS2353: {diags:?}"
    );
}

#[test]
fn symbol_only_index_object_value_nested_type_mismatch_is_ts2322() {
    let source = r#"
declare const key: unique symbol;
interface Val { a: number; }
interface J { [k: symbol]: Val; }
const bad: J = { [key]: { a: "x" } };
"#;
    let diags = check_strict(source);
    assert_eq!(
        codes(&diags),
        vec![2322],
        "symbol-only nested mismatch is TS2322: {diags:?}"
    );
}

// ---- Primitive symbol value keeps the flat TS2418 (object-valued only) ------

#[test]
fn symbol_index_primitive_value_keeps_ts2418() {
    // The `[k: symbol]` value type is a primitive, so there is no nested literal
    // to drill into; the flat TS2418 computed-property aggregate is correct.
    let source = r#"
declare const sym: unique symbol;
interface I { [k: string]: number; [k: symbol]: number; }
const bad: I = { [sym]: "x" };
"#;
    let diags = check_strict(source);
    assert_eq!(
        codes(&diags),
        vec![2418],
        "primitive symbol value stays TS2418: {diags:?}"
    );
    assert!(
        diags[0].1.contains("computed") && diags[0].1.contains("'number'"),
        "TS2418 should be the computed-property-value aggregate: {diags:?}"
    );
}
