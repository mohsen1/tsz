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
fn test_constrained_type_parameter_template_projects_to_literal_union_target() {
    // For T extends "foo" | "bar", `get${Capitalize<T>}` is bounded by
    // "getFoo" | "getBar" and is therefore assignable to that union.
    let interner = TypeInterner::new();

    let foo = interner.literal_string("foo");
    let bar = interner.literal_string("bar");
    let constraint = interner.union2(foo, bar);
    let t = interner.type_param(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(constraint),
        default: None,
        is_const: false,
    });
    let cap_t = interner.string_intrinsic(StringIntrinsicKind::Capitalize, t);
    let source = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("get")),
        TemplateSpan::Type(cap_t),
    ]);
    let target = interner.union2(
        interner.literal_string("getFoo"),
        interner.literal_string("getBar"),
    );

    let mut checker = SubtypeChecker::new(&interner);
    assert!(checker.is_subtype_of(source, target));

    let too_narrow = interner.literal_string("getFoo");
    assert!(!checker.is_subtype_of(source, too_narrow));
}

#[test]
fn test_numeric_literal_union_template_expands_for_assignability() {
    let interner = TypeInterner::new();

    let digits = (1..=20)
        .map(|n| interner.literal_number(n as f64))
        .collect();
    let suffixes = interner.union(digits);
    let spacing = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("s")),
        TemplateSpan::Type(suffixes),
    ]);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(checker.is_subtype_of(interner.literal_string("s12"), spacing));
}

#[test]
fn test_literal_union_members_are_not_absorbed_by_unrelated_templates() {
    let interner = TypeInterner::new();

    let number_px = interner.template_literal(vec![
        TemplateSpan::Type(TypeId::NUMBER),
        TemplateSpan::Text(interner.intern_string("px")),
    ]);
    let number_rem = interner.template_literal(vec![
        TemplateSpan::Type(TypeId::NUMBER),
        TemplateSpan::Text(interner.intern_string("rem")),
    ]);
    let s12 = interner.literal_string("s12");
    let spacing = interner.union(vec![number_px, number_rem, s12]);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(checker.is_subtype_of(interner.literal_string("s12"), spacing));
}

#[test]
fn test_number_template_does_not_match_prefixed_string_without_digits() {
    let interner = TypeInterner::new();
    let number_px = interner.template_literal(vec![
        TemplateSpan::Type(TypeId::NUMBER),
        TemplateSpan::Text(interner.intern_string("px")),
    ]);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(!checker.is_subtype_of(interner.literal_string("s12"), number_px));
}

#[test]
fn test_prefixed_number_template_not_subtype_of_suffixed_number_template() {
    let interner = TypeInterner::new();
    let s_number = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("s")),
        TemplateSpan::Type(TypeId::NUMBER),
    ]);
    let number_rem = interner.template_literal(vec![
        TemplateSpan::Type(TypeId::NUMBER),
        TemplateSpan::Text(interner.intern_string("rem")),
    ]);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(!checker.is_subtype_of(s_number, number_rem));
}

#[test]
fn test_large_template_cross_product_sets_too_complex_flag() {
    let interner = TypeInterner::new();
    let digits = (0..=9).map(|n| interner.literal_number(n as f64)).collect();
    let digits = interner.union(digits);

    let _ = interner.take_union_too_complex();
    let result = interner.template_literal(vec![
        TemplateSpan::Type(digits),
        TemplateSpan::Type(digits),
        TemplateSpan::Type(digits),
        TemplateSpan::Type(digits),
        TemplateSpan::Type(digits),
    ]);

    assert_eq!(result, TypeId::STRING);
    assert!(interner.take_union_too_complex());
}

#[test]
fn test_large_template_cross_product_sets_flag_after_lazy_resolution() {
    use crate::def::DefId;
    use crate::{TypeEnvironment, TypeEvaluator};

    let interner = TypeInterner::new();
    let digits = (0..=9).map(|n| interner.literal_number(n as f64)).collect();
    let digits = interner.union(digits);

    let mut env = TypeEnvironment::new();
    let digits_def = DefId(1);
    env.insert_def(digits_def, digits);
    let lazy_digits = interner.lazy(digits_def);
    let template = interner.template_literal(vec![
        TemplateSpan::Type(lazy_digits),
        TemplateSpan::Type(lazy_digits),
        TemplateSpan::Type(lazy_digits),
        TemplateSpan::Type(lazy_digits),
        TemplateSpan::Type(lazy_digits),
    ]);

    let _ = interner.take_union_too_complex();
    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(template);

    assert_eq!(result, TypeId::STRING);
    assert!(interner.take_union_too_complex());
}

#[test]
fn test_mixed_template_union_counts_lazy_template_alternatives_for_complexity() {
    use crate::def::DefId;
    use crate::{TypeEnvironment, TypeEvaluator};

    let interner = TypeInterner::new();
    let zero = interner.literal_string("0");
    let number_px = interner.template_literal(vec![
        TemplateSpan::Type(TypeId::NUMBER),
        TemplateSpan::Text(interner.intern_string("px")),
    ]);
    let number_rem = interner.template_literal(vec![
        TemplateSpan::Type(TypeId::NUMBER),
        TemplateSpan::Text(interner.intern_string("rem")),
    ]);
    let suffixes = interner.union(
        (1..=20)
            .map(|n| interner.literal_number(n as f64))
            .collect(),
    );
    let scaled = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("s")),
        TemplateSpan::Type(suffixes),
    ]);
    let spacing = interner.union(vec![zero, number_px, number_rem, scaled]);

    let mut env = TypeEnvironment::new();
    let spacing_def = DefId(1);
    env.insert_def(spacing_def, spacing);
    let lazy_spacing = interner.lazy(spacing_def);
    let shorthand = interner.template_literal(vec![
        TemplateSpan::Type(lazy_spacing),
        TemplateSpan::Text(interner.intern_string(" ")),
        TemplateSpan::Type(lazy_spacing),
        TemplateSpan::Text(interner.intern_string(" ")),
        TemplateSpan::Type(lazy_spacing),
        TemplateSpan::Text(interner.intern_string(" ")),
        TemplateSpan::Type(lazy_spacing),
    ]);

    let _ = interner.take_union_too_complex();
    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(shorthand);

    assert_eq!(result, TypeId::STRING);
    assert!(interner.take_union_too_complex());
}

#[test]
fn test_tuple_spread_cross_product_sets_flag_after_lazy_resolution() {
    use crate::def::DefId;
    use crate::{TypeEnvironment, TypeEvaluator};

    let interner = TypeInterner::new();
    let tuple_members = (0..=9)
        .map(|n| {
            interner.tuple(vec![TupleElement {
                type_id: interner.literal_number(n as f64),
                name: None,
                optional: false,
                rest: false,
            }])
        })
        .collect();
    let tuple_digits = interner.union(tuple_members);

    let mut env = TypeEnvironment::new();
    let tuple_digits_def = DefId(1);
    env.insert_def(tuple_digits_def, tuple_digits);
    let lazy_tuple_digits = interner.lazy(tuple_digits_def);
    let spread = TupleElement {
        type_id: lazy_tuple_digits,
        name: None,
        optional: false,
        rest: true,
    };
    let tuple = interner.tuple(vec![spread, spread, spread, spread, spread]);

    let _ = interner.take_union_too_complex();
    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(tuple);

    assert_eq!(result, TypeId::ERROR);
    assert!(interner.take_union_too_complex());
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
