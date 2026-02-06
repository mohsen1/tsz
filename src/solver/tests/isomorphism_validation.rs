//! Isomorphism Validation Tests for O(1) Equality
//!
//! This test suite validates the North Star goal: "Type equality check: O(1) via interning"
//!
//! The invariant we must maintain: **Structural identity implies TypeId equality**
//!
//! These tests verify that different evaluation paths producing the same structure
//! result in the exact same TypeId. If a test fails, it means canonicalization is
//! broken and O(1) equality is not achieved.

use crate::solver::intern::TypeInterner;
use crate::solver::types::*;

/// Helper to create a test interner
fn create_test_interner() -> TypeInterner {
    TypeInterner::new()
}

#[test]
fn test_union_order_independence_basic() {
    // "a" | "b" should equal "b" | "a"
    let interner = create_test_interner();
    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");

    let union1 = interner.union2(lit_a, lit_b);
    let union2 = interner.union2(lit_b, lit_a);

    assert_eq!(
        union1, union2,
        "Union order independence failed: different TypeIds for same members"
    );
}

#[test]
fn test_union_redundancy_elimination() {
    // "a" | "a" should equal "a"
    let interner = create_test_interner();
    let lit_a = interner.literal_string("a");

    let union = interner.union2(lit_a, lit_a);

    assert_eq!(
        union, lit_a,
        "Union redundancy elimination failed: a | a should reduce to a"
    );
}

#[test]
fn test_union_literal_absorption() {
    // string | "a" should equal string
    let interner = create_test_interner();
    let lit_a = interner.literal_string("a");

    let union = interner.union2(TypeId::STRING, lit_a);

    assert_eq!(
        union,
        TypeId::STRING,
        "Union literal absorption failed: string | literal should reduce to string"
    );
}

#[test]
fn test_intersection_order_independence() {
    // {a: number} & {b: string} should equal {b: string} & {a: number}
    let interner = create_test_interner();

    let prop_a = PropertyInfo {
        name: interner.intern_string("a"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
        visibility: Visibility::Public,
        parent_id: None,
    };

    let prop_b = PropertyInfo {
        name: interner.intern_string("b"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
        visibility: Visibility::Public,
        parent_id: None,
    };

    let obj1 = interner.object(vec![prop_a.clone(), prop_b.clone()]);
    let obj2 = interner.object(vec![prop_b.clone(), prop_a.clone()]);

    assert_eq!(obj1, obj2, "Object property order independence failed");
}

#[test]
fn test_template_literal_adjacent_text_merging() {
    // Template with empty string interpolation should merge
    let interner = create_test_interner();
    let empty = interner.literal_string("");

    // a + "" + b
    let spans1 = vec![
        TemplateSpan::Text(interner.intern_string("a")),
        TemplateSpan::Type(empty),
        TemplateSpan::Text(interner.intern_string("b")),
    ];
    let template1 = interner.template_literal(spans1);

    // ab
    let spans2 = vec![
        TemplateSpan::Text(interner.intern_string("a")),
        TemplateSpan::Text(interner.intern_string("b")),
    ];
    let template2 = interner.template_literal(spans2);

    assert_eq!(
        template1, template2,
        "Adjacent text merging failed with empty string interpolation"
    );
}

#[test]
fn test_template_literal_nested_flattening() {
    // Nested templates should flatten
    let interner = create_test_interner();

    // First create "b" as a nested template
    let nested_spans = vec![TemplateSpan::Text(interner.intern_string("b"))];
    let nested_template = interner.template_literal(nested_spans);

    // a + "b" + c (with "b" as nested template)
    let outer_spans = vec![
        TemplateSpan::Text(interner.intern_string("a")),
        TemplateSpan::Type(nested_template),
        TemplateSpan::Text(interner.intern_string("c")),
    ];
    let outer_template = interner.template_literal(outer_spans);

    // abc (flat)
    let flat_spans = vec![
        TemplateSpan::Text(interner.intern_string("a")),
        TemplateSpan::Text(interner.intern_string("b")),
        TemplateSpan::Text(interner.intern_string("c")),
    ];
    let flat_template = interner.template_literal(flat_spans);

    assert_eq!(
        outer_template, flat_template,
        "Nested template flattening failed"
    );
}

#[test]
fn test_template_literal_expansion_to_union() {
    // Template with union interpolation should expand
    let interner = create_test_interner();
    let lit_b = interner.literal_string("b");
    let lit_c = interner.literal_string("c");
    let union_bc = interner.union2(lit_b, lit_c);

    let spans = vec![
        TemplateSpan::Text(interner.intern_string("a")),
        TemplateSpan::Type(union_bc),
        TemplateSpan::Text(interner.intern_string("d")),
    ];
    let template = interner.template_literal(spans);

    let expected_abd = interner.literal_string("abd");
    let expected_acd = interner.literal_string("acd");
    let expected_union = interner.union2(expected_abd, expected_acd);

    assert_eq!(
        template, expected_union,
        "Template literal expansion to union failed"
    );
}

#[test]
fn test_union_duplication_elimination() {
    // "a" | "b" | "a" should equal "a" | "b"
    let interner = create_test_interner();
    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");

    let union1 = interner.union(vec![lit_a, lit_b, lit_a]);
    let union2 = interner.union2(lit_a, lit_b);

    assert_eq!(union1, union2, "Union duplication elimination failed");
}

#[test]
fn test_intersection_duplication_elimination() {
    // {a: 1} & {b: 2} & {a: 1} should equal {a: 1} & {b: 2}
    let interner = create_test_interner();

    let prop_a = PropertyInfo {
        name: interner.intern_string("a"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
        visibility: Visibility::Public,
        parent_id: None,
    };

    let prop_b = PropertyInfo {
        name: interner.intern_string("b"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
        visibility: Visibility::Public,
        parent_id: None,
    };

    let obj_a = interner.object(vec![prop_a.clone()]);
    let obj_b = interner.object(vec![prop_b.clone()]);

    let intersection1 = interner.intersection(vec![obj_a, obj_b, obj_a]);
    let intersection2 = interner.intersection2(obj_a, obj_b);

    assert_eq!(
        intersection1, intersection2,
        "Intersection duplication elimination failed"
    );
}

#[test]
fn test_never_absorption_in_union() {
    // never | "a" | "b" should equal "a" | "b"
    let interner = create_test_interner();
    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");

    let union = interner.union(vec![TypeId::NEVER, lit_a, lit_b]);
    let expected = interner.union2(lit_a, lit_b);

    assert_eq!(union, expected, "Never absorption in union failed");
}

#[test]
fn test_empty_string_removal_in_template() {
    // Template with empty string should remove it
    let interner = create_test_interner();
    let empty = interner.literal_string("");

    let spans1 = vec![
        TemplateSpan::Text(interner.intern_string("a")),
        TemplateSpan::Type(empty),
    ];
    let template1 = interner.template_literal(spans1);

    let spans2 = vec![TemplateSpan::Text(interner.intern_string("a"))];
    let template2 = interner.template_literal(spans2);

    assert_eq!(
        template1, template2,
        "Empty string removal in template failed"
    );
}

#[test]
fn test_null_stringification_in_template() {
    // null in template should become "null" text
    let interner = create_test_interner();

    let spans = vec![
        TemplateSpan::Text(interner.intern_string("a")),
        TemplateSpan::Type(TypeId::NULL),
        TemplateSpan::Text(interner.intern_string("b")),
    ];
    let template = interner.template_literal(spans);

    let expected_spans = vec![
        TemplateSpan::Text(interner.intern_string("a")),
        TemplateSpan::Text(interner.intern_string("null")),
        TemplateSpan::Text(interner.intern_string("b")),
    ];
    let expected = interner.template_literal(expected_spans);

    assert_eq!(
        template, expected,
        "Null stringification in template failed"
    );
}

#[test]
fn test_undefined_stringification_in_template() {
    // undefined in template should become "undefined" text
    let interner = create_test_interner();

    let spans = vec![
        TemplateSpan::Text(interner.intern_string("a")),
        TemplateSpan::Type(TypeId::UNDEFINED),
        TemplateSpan::Text(interner.intern_string("b")),
    ];
    let template = interner.template_literal(spans);

    let expected_spans = vec![
        TemplateSpan::Text(interner.intern_string("a")),
        TemplateSpan::Text(interner.intern_string("undefined")),
        TemplateSpan::Text(interner.intern_string("b")),
    ];
    let expected = interner.template_literal(expected_spans);

    assert_eq!(
        template, expected,
        "Undefined stringification in template failed"
    );
}

#[test]
#[ignore = "TODO: Boolean expansion in templates - requires deeper investigation of expansion logic"]
fn test_boolean_expansion_in_template() {
    // boolean in template should expand to "true" | "false"
    let interner = create_test_interner();

    let spans = vec![
        TemplateSpan::Text(interner.intern_string("a")),
        TemplateSpan::Type(TypeId::BOOLEAN),
        TemplateSpan::Text(interner.intern_string("b")),
    ];
    let template = interner.template_literal(spans);

    let expected_true = interner.literal_string("atrueb");
    let expected_false = interner.literal_string("afalseb");
    let expected = interner.union2(expected_true, expected_false);

    assert_eq!(template, expected, "Boolean expansion in template failed");
}

#[test]
fn test_any_widening_in_template() {
    // any in template should widen to string
    let interner = create_test_interner();

    let spans = vec![
        TemplateSpan::Text(interner.intern_string("a")),
        TemplateSpan::Type(TypeId::ANY),
        TemplateSpan::Text(interner.intern_string("b")),
    ];
    let template = interner.template_literal(spans);

    assert_eq!(template, TypeId::STRING, "Any widening in template failed");
}

#[test]
fn test_unknown_widening_in_template() {
    // unknown in template should widen to string
    let interner = create_test_interner();

    let spans = vec![
        TemplateSpan::Text(interner.intern_string("a")),
        TemplateSpan::Type(TypeId::UNKNOWN),
        TemplateSpan::Text(interner.intern_string("b")),
    ];
    let template = interner.template_literal(spans);

    assert_eq!(
        template,
        TypeId::STRING,
        "Unknown widening in template failed"
    );
}

#[test]
fn test_never_absorption_in_template() {
    // never in template should absorb to never
    let interner = create_test_interner();

    let spans = vec![
        TemplateSpan::Text(interner.intern_string("a")),
        TemplateSpan::Type(TypeId::NEVER),
        TemplateSpan::Text(interner.intern_string("b")),
    ];
    let template = interner.template_literal(spans);

    assert_eq!(
        template,
        TypeId::NEVER,
        "Never absorption in template failed"
    );
}
