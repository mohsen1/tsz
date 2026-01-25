//! Tests for Rule #22: Template String Expansion Limits
//!
//! This module tests the 100k item limit for template literal expansion.

use crate::solver::intern::TypeInterner;
use crate::solver::types::*;

#[test]
fn test_template_literal_small_expansion_works() {
    let interner = TypeInterner::new();

    // Create a small union that should expand successfully
    let mut members = Vec::with_capacity(10);
    for idx in 0..10 {
        let literal = interner.literal_string(&format!("k{idx}"));
        members.push(literal);
    }
    let union = interner.union(members);
    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("get")),
        TemplateSpan::Type(union),
    ]);

    // Should expand to a union of string literals, not widen to string
    assert_ne!(template, TypeId::STRING);

    // Verify it's a union with the expected members
    if let Some(TypeKey::Union(members)) = interner.lookup(template) {
        let members = interner.type_list(members);
        assert_eq!(members.len(), 10);
    } else {
        panic!("Expected a union type");
    }
}

#[test]
fn test_template_literal_at_limit_boundary() {
    let interner = TypeInterner::new();

    // Test exactly at the limit (100,000)
    // Using 100 members each with 1000 combinations = 100,000 total
    let count = 100;
    let mut members = Vec::with_capacity(count);
    for idx in 0..count {
        let literal = interner.literal_string(&format!("k{idx}"));
        members.push(literal);
    }
    let union = interner.union(members);

    // Create 10 spans, each with the same 100-member union
    // Total combinations: 100^10 = way over limit, but should abort early
    let mut spans = Vec::new();
    for _ in 0..10 {
        spans.push(TemplateSpan::Type(union));
    }

    let template = interner.template_literal(spans);

    // Should widen to string due to limit
    assert_eq!(template, TypeId::STRING);
}

#[test]
fn test_template_literal_cartesian_product_limit() {
    let interner = TypeInterner::new();

    // Create two large unions where Cartesian product would exceed limit
    // 500 * 500 = 250,000 > 100,000
    let mut members1 = Vec::new();
    let mut members2 = Vec::new();
    for idx in 0..500 {
        members1.push(interner.literal_string(&format!("a{idx}")));
        members2.push(interner.literal_string(&format!("b{idx}")));
    }

    let union1 = interner.union(members1);
    let union2 = interner.union(members2);

    let template = interner.template_literal(vec![
        TemplateSpan::Type(union1),
        TemplateSpan::Text(interner.intern_string("-")),
        TemplateSpan::Type(union2),
    ]);

    // Should widen to string
    assert_eq!(template, TypeId::STRING);
}

#[test]
fn test_template_literal_nested_template_limit() {
    let interner = TypeInterner::new();

    // Create a union that would cause nested expansion
    let mut members = Vec::new();
    for idx in 0..1000 {
        members.push(interner.literal_string(&format!("k{idx}")));
    }
    let union = interner.union(members);

    // Nested template: `${`${T}`}-${`${T}`}`
    // Each nested template would have 1000 combinations
    // The outer template would have 1000 * 1000 = 1,000,000 combinations
    let inner_template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("prefix_")),
        TemplateSpan::Type(union),
    ]);

    let outer_template = interner.template_literal(vec![
        TemplateSpan::Type(inner_template),
        TemplateSpan::Text(interner.intern_string("-")),
        TemplateSpan::Type(inner_template),
    ]);

    // Should widen to string
    assert_eq!(outer_template, TypeId::STRING);
}

#[test]
fn test_template_literal_exactly_at_limit() {
    let interner = TypeInterner::new();

    // Test with exactly 100,000 combinations
    // Using 316 members (316^2 ≈ 99,856, 317^2 ≈ 100,489)
    let count = 317;
    let mut members = Vec::with_capacity(count);
    for idx in 0..count {
        let literal = interner.literal_string(&format!("k{idx}"));
        members.push(literal);
    }
    let union = interner.union(members);

    let template = interner.template_literal(vec![
        TemplateSpan::Type(union),
        TemplateSpan::Text(interner.intern_string("-")),
        TemplateSpan::Type(union),
    ]);

    // Should widen to string (317 * 317 = 100,489 > 100,000)
    assert_eq!(template, TypeId::STRING);
}

#[test]
fn test_template_literal_just_under_limit() {
    let interner = TypeInterner::new();

    // Test with just under 100,000 combinations
    // Using 316 members (316^2 = 99,856 < 100,000)
    let count = 316;
    let mut members = Vec::with_capacity(count);
    for idx in 0..count {
        let literal = interner.literal_string(&format!("k{idx}"));
        members.push(literal);
    }
    let union = interner.union(members);

    let template = interner.template_literal(vec![
        TemplateSpan::Type(union),
        TemplateSpan::Text(interner.intern_string("-")),
        TemplateSpan::Type(union),
    ]);

    // Should NOT widen to string (316 * 316 = 99,856 < 100,000)
    // But should be a union of literals
    assert_ne!(template, TypeId::STRING);

    if let Some(TypeKey::Union(members)) = interner.lookup(template) {
        let members = interner.type_list(members);
        // Should have expanded to 99,856 combinations
        assert_eq!(members.len(), 99856);
    } else {
        panic!("Expected a union type");
    }
}

#[test]
fn test_template_literal_non_literal_types() {
    let interner = TypeInterner::new();

    // Template with non-literal types should not expand
    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("get")),
        TemplateSpan::Type(TypeId::STRING),
    ]);

    // Should remain as template literal (can't expand `string` to literals)
    if let Some(TypeKey::TemplateLiteral(_)) = interner.lookup(template) {
        // Expected
    } else {
        panic!("Expected a template literal type");
    }
}

#[test]
fn test_template_literal_single_union_member() {
    let interner = TypeInterner::new();

    // Single member union should work fine
    let literal = interner.literal_string("key");
    let union = interner.union(vec![literal]);

    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("get")),
        TemplateSpan::Type(union),
    ]);

    // Should expand to a single literal
    assert_ne!(template, TypeId::STRING);
    assert_ne!(template, literal);

    // Should be the literal string "getkey"
    if let Some(TypeKey::Literal(LiteralValue::String(atom))) = interner.lookup(template) {
        let s = interner.resolve_atom_ref(atom);
        assert_eq!(s.as_str(), "getkey");
    } else {
        panic!("Expected a literal string");
    }
}
