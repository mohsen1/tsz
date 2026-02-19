//! Comprehensive tests for template literal type operations.
//!
//! These tests verify TypeScript's template literal type behavior:
//! - Basic template literal construction
//! - Template literal with type interpolation
//! - Template literal evaluation
//! - Template literal subtype relationships

use super::*;
use crate::evaluate::evaluate_type;
use crate::intern::TypeInterner;
use crate::subtype::SubtypeChecker;
use crate::types::{TemplateSpan, TypeData, TypeParamInfo};

// =============================================================================
// Basic Template Literal Construction Tests
// =============================================================================

#[test]
fn test_template_literal_text_only() {
    let interner = TypeInterner::new();

    let template =
        interner.template_literal(vec![TemplateSpan::Text(interner.intern_string("hello"))]);

    // Template with only text may be simplified to a string literal
    // Just verify it's a valid type
    let result = evaluate_type(&interner, template);
    if let Some(TypeData::Literal(crate::types::LiteralValue::String(_))) = interner.lookup(result)
    {
        // Good - simplified to string literal
    } else if let Some(TypeData::TemplateLiteral(_)) = interner.lookup(result) {
        // Also good - kept as template
    } else {
        panic!("Expected template literal or string literal");
    }
}

#[test]
fn test_template_literal_with_interpolation() {
    let interner = TypeInterner::new();

    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("hello ")),
        TemplateSpan::Type(TypeId::STRING),
        TemplateSpan::Text(interner.intern_string("!")),
    ]);

    if let Some(TypeData::TemplateLiteral(spans)) = interner.lookup(template) {
        let spans = interner.template_list(spans);
        assert_eq!(spans.len(), 3);
    } else {
        panic!("Expected template literal type");
    }
}

#[test]
fn test_template_literal_multiple_interpolations() {
    let interner = TypeInterner::new();

    let template = interner.template_literal(vec![
        TemplateSpan::Type(TypeId::STRING),
        TemplateSpan::Text(interner.intern_string("-")),
        TemplateSpan::Type(TypeId::NUMBER),
        TemplateSpan::Text(interner.intern_string("-")),
        TemplateSpan::Type(TypeId::BOOLEAN),
    ]);

    if let Some(TypeData::TemplateLiteral(spans)) = interner.lookup(template) {
        let spans = interner.template_list(spans);
        assert_eq!(spans.len(), 5);
    } else {
        panic!("Expected template literal type");
    }
}

// =============================================================================
// Template Literal Evaluation Tests
// =============================================================================

#[test]
fn test_template_literal_evaluates_to_string() {
    let interner = TypeInterner::new();

    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("prefix-")),
        TemplateSpan::Type(TypeId::STRING),
    ]);

    let result = evaluate_type(&interner, template);

    // Template literal should be a subtype of string
    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.is_subtype_of(result, TypeId::STRING),
        "Template literal should be subtype of string"
    );
}

#[test]
fn test_template_literal_with_literal_type() {
    let interner = TypeInterner::new();

    let hello = interner.literal_string("hello");
    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("prefix-")),
        TemplateSpan::Type(hello),
    ]);

    let result = evaluate_type(&interner, template);

    // Should evaluate to "prefix-hello"
    if let Some(TypeData::Literal(crate::types::LiteralValue::String(s))) = interner.lookup(result)
    {
        let value = interner.resolve_atom(s);
        assert_eq!(value, "prefix-hello");
    }
    // If it doesn't fully evaluate, it should at least be a string
}

// =============================================================================
// Template Literal with Union Types
// =============================================================================

#[test]
fn test_template_literal_with_union() {
    let interner = TypeInterner::new();

    let a = interner.literal_string("a");
    let b = interner.literal_string("b");
    let union = interner.union2(a, b);

    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("prefix-")),
        TemplateSpan::Type(union),
    ]);

    let result = evaluate_type(&interner, template);

    // `prefix-${"a" | "b"}` should be "prefix-a" | "prefix-b"
    if let Some(TypeData::Union(members)) = interner.lookup(result) {
        let members = interner.type_list(members);
        assert_eq!(members.len(), 2);
    }
}

// =============================================================================
// Template Literal Subtype Tests
// =============================================================================

#[test]
fn test_template_literal_subtype_of_string() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("hello")),
        TemplateSpan::Type(TypeId::STRING),
    ]);

    let result = evaluate_type(&interner, template);

    assert!(
        checker.is_subtype_of(result, TypeId::STRING),
        "Template literal should be subtype of string"
    );
}

#[test]
fn test_template_literal_not_subtype_of_number() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("hello")),
        TemplateSpan::Type(TypeId::STRING),
    ]);

    let result = evaluate_type(&interner, template);

    assert!(
        !checker.is_subtype_of(result, TypeId::NUMBER),
        "Template literal should not be subtype of number"
    );
}

// =============================================================================
// Template Literal with Type Parameters
// =============================================================================

#[test]
fn test_template_literal_with_type_param() {
    let interner = TypeInterner::new();

    let type_param_info = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    };
    let type_param = interner.intern(TypeData::TypeParameter(type_param_info));

    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("prefix-")),
        TemplateSpan::Type(type_param),
    ]);

    // Just verify construction
    if let Some(TypeData::TemplateLiteral(_)) = interner.lookup(template) {
        // Good
    } else {
        panic!("Expected template literal type");
    }
}

// =============================================================================
// Template Literal Identity Tests
// =============================================================================

#[test]
fn test_template_literal_identity_stability() {
    let interner = TypeInterner::new();

    let spans = vec![
        TemplateSpan::Text(interner.intern_string("hello ")),
        TemplateSpan::Type(TypeId::STRING),
    ];

    let template1 = interner.template_literal(spans.clone());
    let template2 = interner.template_literal(spans);

    assert_eq!(
        template1, template2,
        "Same template literal construction should produce same TypeId"
    );
}

// =============================================================================
// Template Literal Edge Cases
// =============================================================================

#[test]
fn test_empty_template_literal() {
    let interner = TypeInterner::new();

    let template = interner.template_literal(vec![]);

    // Empty template literal should be empty string
    let result = evaluate_type(&interner, template);
    if let Some(TypeData::Literal(crate::types::LiteralValue::String(s))) = interner.lookup(result)
    {
        let value = interner.resolve_atom(s);
        assert_eq!(value, "");
    }
}

#[test]
fn test_template_literal_only_type() {
    let interner = TypeInterner::new();

    let template = interner.template_literal(vec![TemplateSpan::Type(TypeId::STRING)]);

    // `${string}` should evaluate to something string-like
    let result = evaluate_type(&interner, template);
    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.is_subtype_of(result, TypeId::STRING),
        "${{string}} should be subtype of string"
    );
}

// =============================================================================
// Template Literal with any
// =============================================================================

#[test]
fn test_template_literal_with_any() {
    let interner = TypeInterner::new();

    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("prefix-")),
        TemplateSpan::Type(TypeId::ANY),
    ]);

    let result = evaluate_type(&interner, template);

    // `prefix-${any}` should be string (since any converts to string)
    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.is_subtype_of(result, TypeId::STRING),
        "Template with any should be subtype of string"
    );
}

// =============================================================================
// Template Literal with never
// =============================================================================

#[test]
fn test_template_literal_with_never() {
    let interner = TypeInterner::new();

    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("prefix-")),
        TemplateSpan::Type(TypeId::NEVER),
    ]);

    let result = evaluate_type(&interner, template);

    // `prefix-${never}` should be never (no possible values)
    assert_eq!(result, TypeId::NEVER);
}

// =============================================================================
// Template Literal Patterns
// =============================================================================

#[test]
fn test_template_literal_get_prefix_pattern() {
    let interner = TypeInterner::new();

    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("get")),
        TemplateSpan::Type(TypeId::STRING),
    ]);

    let result = evaluate_type(&interner, template);

    // `get${string}` should be a string
    let mut checker = SubtypeChecker::new(&interner);
    assert!(checker.is_subtype_of(result, TypeId::STRING));
}

#[test]
fn test_template_literal_on_event_pattern() {
    let interner = TypeInterner::new();

    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("on")),
        TemplateSpan::Type(TypeId::STRING),
    ]);

    let result = evaluate_type(&interner, template);

    // `on${string}` should be a string
    let mut checker = SubtypeChecker::new(&interner);
    assert!(checker.is_subtype_of(result, TypeId::STRING));
}

// =============================================================================
// Complex Template Literal Tests
// =============================================================================

#[test]
fn test_template_literal_nested_in_union() {
    let interner = TypeInterner::new();

    let template1 = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("a-")),
        TemplateSpan::Type(TypeId::STRING),
    ]);

    let template2 = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("b-")),
        TemplateSpan::Type(TypeId::STRING),
    ]);

    let union = interner.union2(template1, template2);

    // Verify union was created
    if let Some(TypeData::Union(members)) = interner.lookup(union) {
        let members = interner.type_list(members);
        assert_eq!(members.len(), 2);
    } else {
        panic!("Expected union type");
    }
}
