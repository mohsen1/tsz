//! Template literal type subtyping tests (Task #30)
//!
//! Tests for:
//! 1. String literal to template literal (already implemented)
//! 2. Template to template subtyping
//! 3. Template literal disjointness detection
//! 4. Intrinsic coercion in template literals

use crate::intern::TypeInterner;
use crate::relations::subtype::SubtypeChecker;
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
fn test_template_subtype_different_structure_string_absorbs() {
    // `foo_${string}` IS a subtype of `${string}` because `${string}` matches any string
    // and all strings matching `foo_${string}` are also strings.
    let interner = TypeInterner::new();

    // Source: `foo_${string}` (2 spans)
    let source = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("foo_")),
        TemplateSpan::Type(TypeId::STRING),
    ]);

    // Target: `${string}` (1 span) — equivalent to `string`
    let target = interner.template_literal(vec![TemplateSpan::Type(TypeId::STRING)]);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(checker.is_subtype_of(source, target));
}

#[test]
fn test_template_not_subtype_number_rejects_text() {
    // `foo_${string}` is NOT a subtype of `${number}` because
    // source produces strings like "foo_abc" which are not valid numbers.
    let interner = TypeInterner::new();

    let source = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("foo_")),
        TemplateSpan::Type(TypeId::STRING),
    ]);

    let target = interner.template_literal(vec![TemplateSpan::Type(TypeId::NUMBER)]);

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
fn test_template_literal_leading_hole_overlap_is_conservative() {
    // `foo-${string}` and `${string}-bar` overlap because the leading string
    // hole can absorb the fixed prefix from the other template.
    let interner = TypeInterner::new();

    let template1 = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("foo-")),
        TemplateSpan::Type(TypeId::STRING),
    ]);

    let template2 = interner.template_literal(vec![
        TemplateSpan::Type(TypeId::STRING),
        TemplateSpan::Text(interner.intern_string("-bar")),
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

// =========================================================================
// Template-to-template subtype matching with different span structures
// =========================================================================

#[test]
fn test_template_to_template_text_matches_type_holes() {
    // `1.1.${number}` should be a subtype of `${number}.${number}.${number}`
    // because source text "1.1." can be parsed by target's number.dot.number.dot pattern
    let interner = TypeInterner::new();

    let source = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("1.1.")),
        TemplateSpan::Type(TypeId::NUMBER),
    ]);

    let target = interner.template_literal(vec![
        TemplateSpan::Type(TypeId::NUMBER),
        TemplateSpan::Text(interner.intern_string(".")),
        TemplateSpan::Type(TypeId::NUMBER),
        TemplateSpan::Text(interner.intern_string(".")),
        TemplateSpan::Type(TypeId::NUMBER),
    ]);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(checker.is_subtype_of(source, target));
}

#[test]
fn test_template_to_template_type_then_text_matches() {
    // `${number}.1.1` should be a subtype of `${number}.${number}.${number}`
    let interner = TypeInterner::new();

    let source = interner.template_literal(vec![
        TemplateSpan::Type(TypeId::NUMBER),
        TemplateSpan::Text(interner.intern_string(".1.1")),
    ]);

    let target = interner.template_literal(vec![
        TemplateSpan::Type(TypeId::NUMBER),
        TemplateSpan::Text(interner.intern_string(".")),
        TemplateSpan::Type(TypeId::NUMBER),
        TemplateSpan::Text(interner.intern_string(".")),
        TemplateSpan::Type(TypeId::NUMBER),
    ]);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(checker.is_subtype_of(source, target));
}

#[test]
fn test_template_to_template_string_absorbs_spans() {
    // `${number}.${number}` should be a subtype of `${string}`
    // because `${string}` matches any string
    let interner = TypeInterner::new();

    let source = interner.template_literal(vec![
        TemplateSpan::Type(TypeId::NUMBER),
        TemplateSpan::Text(interner.intern_string(".")),
        TemplateSpan::Type(TypeId::NUMBER),
    ]);

    let target = interner.template_literal(vec![TemplateSpan::Type(TypeId::STRING)]);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(checker.is_subtype_of(source, target));
}

#[test]
fn test_template_to_template_number_to_string_in_context() {
    // `${number}` should be a subtype of `${string}` in template context
    let interner = TypeInterner::new();

    let source = interner.template_literal(vec![TemplateSpan::Type(TypeId::NUMBER)]);
    let target = interner.template_literal(vec![TemplateSpan::Type(TypeId::STRING)]);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(checker.is_subtype_of(source, target));
}

#[test]
fn test_template_to_template_string_not_subtype_of_number() {
    // `${string}` should NOT be a subtype of `${number}`
    let interner = TypeInterner::new();

    let source = interner.template_literal(vec![TemplateSpan::Type(TypeId::STRING)]);
    let target = interner.template_literal(vec![TemplateSpan::Type(TypeId::NUMBER)]);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(!checker.is_subtype_of(source, target));
}

#[test]
fn test_template_literal_with_prefixed_any_keeps_fixed_text() {
    // `a${any}` remains a pattern with an `a` prefix; only bare `${any}`
    // collapses to `string`.
    let interner = TypeInterner::new();

    let pattern = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("a")),
        TemplateSpan::Type(TypeId::ANY),
    ]);

    assert!(
        matches!(interner.lookup(pattern), Some(TypeData::TemplateLiteral(_))),
        "prefixed any template should remain a template literal pattern"
    );

    let mut checker = SubtypeChecker::new(&interner);
    assert!(checker.is_subtype_of(interner.literal_string("aok"), pattern));
    assert!(!checker.is_subtype_of(interner.literal_string("bno"), pattern));
}

#[test]
fn test_template_literal_hole_accepts_intersection_pattern_prefix() {
    // In `` `${`a${string}` & `${string}a`}Test` ``, the interpolation can
    // consume "aba" because it satisfies both intersected template patterns.
    let interner = TypeInterner::new();

    let starts_with_a = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("a")),
        TemplateSpan::Type(TypeId::STRING),
    ]);
    let ends_with_a = interner.template_literal(vec![
        TemplateSpan::Type(TypeId::STRING),
        TemplateSpan::Text(interner.intern_string("a")),
    ]);
    let intersection = interner.intersection(vec![starts_with_a, ends_with_a]);
    let pattern = interner.template_literal(vec![
        TemplateSpan::Type(intersection),
        TemplateSpan::Text(interner.intern_string("Test")),
    ]);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(checker.is_subtype_of(interner.literal_string("abaTest"), pattern));
    assert!(!checker.is_subtype_of(interner.literal_string("abcTest"), pattern));
}

// ==========================================================================
// Hex/Octal/Binary literal matching for ${bigint} and ${number} patterns
// ==========================================================================

#[test]
fn test_hex_literal_matches_bigint_pattern() {
    // "0x1" should be subtype of `${bigint}` (hex is valid bigint syntax)
    let interner = TypeInterner::new();
    let literal = interner.literal_string("0x1");
    let pattern = interner.template_literal(vec![TemplateSpan::Type(TypeId::BIGINT)]);
    let mut checker = SubtypeChecker::new(&interner);
    assert!(checker.is_subtype_of(literal, pattern));
}

#[test]
fn test_octal_literal_matches_bigint_pattern() {
    // "0o1" should be subtype of `${bigint}` (octal is valid bigint syntax)
    let interner = TypeInterner::new();
    let literal = interner.literal_string("0o1");
    let pattern = interner.template_literal(vec![TemplateSpan::Type(TypeId::BIGINT)]);
    let mut checker = SubtypeChecker::new(&interner);
    assert!(checker.is_subtype_of(literal, pattern));
}

#[test]
fn test_binary_literal_matches_bigint_pattern() {
    // "0b1" should be subtype of `${bigint}` (binary is valid bigint syntax)
    let interner = TypeInterner::new();
    let literal = interner.literal_string("0b1");
    let pattern = interner.template_literal(vec![TemplateSpan::Type(TypeId::BIGINT)]);
    let mut checker = SubtypeChecker::new(&interner);
    assert!(checker.is_subtype_of(literal, pattern));
}

#[test]
fn test_hex_literal_matches_number_pattern() {
    // "0x1" should be subtype of `${number}` (hex is valid number syntax in JS)
    let interner = TypeInterner::new();
    let literal = interner.literal_string("0x1");
    let pattern = interner.template_literal(vec![TemplateSpan::Type(TypeId::NUMBER)]);
    let mut checker = SubtypeChecker::new(&interner);
    assert!(checker.is_subtype_of(literal, pattern));
}

#[test]
fn test_octal_literal_matches_number_pattern() {
    // "0o1" should be subtype of `${number}` (octal is valid number syntax in JS)
    let interner = TypeInterner::new();
    let literal = interner.literal_string("0o1");
    let pattern = interner.template_literal(vec![TemplateSpan::Type(TypeId::NUMBER)]);
    let mut checker = SubtypeChecker::new(&interner);
    assert!(checker.is_subtype_of(literal, pattern));
}

#[test]
fn test_binary_literal_matches_number_pattern() {
    // "0b1" should be subtype of `${number}` (binary is valid number syntax in JS)
    let interner = TypeInterner::new();
    let literal = interner.literal_string("0b1");
    let pattern = interner.template_literal(vec![TemplateSpan::Type(TypeId::NUMBER)]);
    let mut checker = SubtypeChecker::new(&interner);
    assert!(checker.is_subtype_of(literal, pattern));
}

#[test]
fn test_invalid_hex_does_not_match_bigint_pattern() {
    // "0xGG" should NOT be subtype of `${bigint}` (invalid hex)
    let interner = TypeInterner::new();
    let literal = interner.literal_string("0xGG");
    let pattern = interner.template_literal(vec![TemplateSpan::Type(TypeId::BIGINT)]);
    let mut checker = SubtypeChecker::new(&interner);
    assert!(!checker.is_subtype_of(literal, pattern));
}
