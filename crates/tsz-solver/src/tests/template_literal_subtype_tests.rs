//! Template literal type subtyping tests (Task #30)
//!
//! Tests for:
//! 1. String literal to template literal (already implemented)
//! 2. Template to template subtyping
//! 3. Template literal disjointness detection
//! 4. Intrinsic coercion in template literals

use crate::intern::TypeInterner;
use crate::subtype::SubtypeChecker;
use crate::types::*;

#[test]
fn test_string_literal_matches_template_literal() {
    // "foo_bar" should be subtype of `foo_${string}`
    let interner = TypeInterner::new();

    let literal = interner.literal_string("foo_bar");
    let pattern = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("foo_")),
        TemplateSpan::Type(TypeId::STRING),
    ]);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(checker.is_subtype_of(literal, pattern));
}

#[test]
fn test_string_literal_does_not_match_template_literal() {
    // "bar_baz" should NOT be subtype of `foo_${string}`
    let interner = TypeInterner::new();

    let literal = interner.literal_string("bar_baz");
    let pattern = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("foo_")),
        TemplateSpan::Type(TypeId::STRING),
    ]);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(!checker.is_subtype_of(literal, pattern));
}

#[test]
fn test_template_literal_subtype_of_string() {
    // `foo_${string}` should be subtype of `string`
    let interner = TypeInterner::new();

    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("foo_")),
        TemplateSpan::Type(TypeId::STRING),
    ]);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(checker.is_subtype_of(template, TypeId::STRING));
}

#[test]
fn test_specific_template_subtype_of_generic_template() {
    // `foo_bar` should be subtype of `foo_${string}`
    let interner = TypeInterner::new();

    // Source: `foo_bar` (specific literal)
    let source = interner.literal_string("foo_bar");

    // Target: `foo_${string}` (generic pattern)
    let target = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("foo_")),
        TemplateSpan::Type(TypeId::STRING),
    ]);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(checker.is_subtype_of(source, target));
}

#[test]
fn test_template_to_template_subtype_same_structure() {
    // `foo_${string}` should be subtype of `foo_${string}` (identical)
    let interner = TypeInterner::new();

    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("foo_")),
        TemplateSpan::Type(TypeId::STRING),
    ]);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(checker.is_subtype_of(template, template));
}

#[test]
fn test_template_to_template_subtype_with_literal_types() {
    // `foo_${'bar'}` should be subtype of `foo_${string}`
    // because 'bar' <: string
    let interner = TypeInterner::new();

    // Source: `foo_${'bar'}` (more specific type)
    let source = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("foo_")),
        TemplateSpan::Type(interner.literal_string("bar")),
    ]);

    // Target: `foo_${string}` (more general type)
    let target = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("foo_")),
        TemplateSpan::Type(TypeId::STRING),
    ]);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(checker.is_subtype_of(source, target));
}

#[test]
fn test_template_not_subtype_different_structure() {
    // `foo_${string}` should NOT be subtype of `${string}`
    // because they have different numbers of spans
    let interner = TypeInterner::new();

    // Source: `foo_${string}` (2 spans)
    let source = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("foo_")),
        TemplateSpan::Type(TypeId::STRING),
    ]);

    // Target: `${string}` (1 span)
    let target = interner.template_literal(vec![TemplateSpan::Type(TypeId::STRING)]);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(!checker.is_subtype_of(source, target));
}

#[test]
fn test_template_to_template_not_subtype_different_prefix() {
    // `foo_${string}` should NOT be subtype of `bar_${string}`
    let interner = TypeInterner::new();

    // Source: `foo_${string}`
    let source = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("foo_")),
        TemplateSpan::Type(TypeId::STRING),
    ]);

    // Target: `bar_${string}`
    let target = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("bar_")),
        TemplateSpan::Type(TypeId::STRING),
    ]);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(!checker.is_subtype_of(source, target));
}

#[test]
fn test_template_to_template_subtype_same_pattern() {
    // `foo_${string}` should be subtype of `foo_${string}`
    let interner = TypeInterner::new();

    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("foo_")),
        TemplateSpan::Type(TypeId::STRING),
    ]);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(checker.is_subtype_of(template, template));
}

#[test]
fn test_template_with_intrinsic_coercion() {
    // `get${number}` should match "get123"
    let interner = TypeInterner::new();

    let literal = interner.literal_string("get123");
    let pattern = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("get")),
        TemplateSpan::Type(TypeId::NUMBER),
    ]);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(checker.is_subtype_of(literal, pattern));
}

#[test]
fn test_template_with_boolean_coercion() {
    // `is${boolean}` should match "istrue"
    let interner = TypeInterner::new();

    let literal = interner.literal_string("istrue");
    let pattern = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("is")),
        TemplateSpan::Type(TypeId::BOOLEAN),
    ]);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(checker.is_subtype_of(literal, pattern));
}

#[test]
fn test_template_literal_disjointness_detection() {
    // `foo${string}` and `bar${string}` should be detected as disjoint
    let interner = TypeInterner::new();

    let template1 = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("foo")),
        TemplateSpan::Type(TypeId::STRING),
    ]);

    let template2 = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("bar")),
        TemplateSpan::Type(TypeId::STRING),
    ]);

    let checker = SubtypeChecker::new(&interner);
    assert!(!checker.are_types_overlapping(template1, template2));
}

#[test]
fn test_template_literal_overlap_detection() {
    // `foo${string}` and `foo${number}` should overlap
    // because both can produce "foo1" (string and number both coerce to "1")
    let interner = TypeInterner::new();

    let template1 = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("foo")),
        TemplateSpan::Type(TypeId::STRING),
    ]);

    let template2 = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("foo")),
        TemplateSpan::Type(TypeId::NUMBER),
    ]);

    let checker = SubtypeChecker::new(&interner);
    assert!(checker.are_types_overlapping(template1, template2));
}

#[test]
fn test_template_literal_disjointness_different_suffix() {
    // `a${string}b` and `a${string}c` should be disjoint
    let interner = TypeInterner::new();

    let template1 = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("a")),
        TemplateSpan::Type(TypeId::STRING),
        TemplateSpan::Text(interner.intern_string("b")),
    ]);

    let template2 = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("a")),
        TemplateSpan::Type(TypeId::STRING),
        TemplateSpan::Text(interner.intern_string("c")),
    ]);

    let checker = SubtypeChecker::new(&interner);
    assert!(!checker.are_types_overlapping(template1, template2));
}
