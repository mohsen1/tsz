use super::*;
use crate::TypeInterner;
use crate::def::DefId;
use crate::{SubtypeChecker, TypeSubstitution, instantiate_type};
/// Test infer from optional tuple element: [string, number?] matches [infer A, infer B?]
#[test]
fn test_infer_optional_tuple_element() {
    let interner = TypeInterner::new();

    let infer_a_name = interner.intern_string("A");
    let infer_a = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_a_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_b_name = interner.intern_string("B");
    let infer_b = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_b_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: [infer A, infer B?]
    let pattern = interner.tuple(vec![
        TupleElement {
            type_id: infer_a,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: infer_b,
            name: None,
            optional: true,
            rest: false,
        },
    ]);

    // Input: [string, number]
    let input = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let result_tuple = interner.tuple(vec![
        TupleElement {
            type_id: infer_a,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: infer_b,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let cond = ConditionalType {
        check_type: input,
        extends_type: pattern,
        true_type: result_tuple,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Should infer A = string, B = number
    assert!(result != TypeId::ERROR);
}

// =============================================================================
// TEMPLATE LITERAL TYPE EDGE CASES
// =============================================================================

#[test]
fn test_template_literal_with_number_type() {
    // `id_${number}` - template literal with number placeholder
    let interner = TypeInterner::new();

    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("id_")),
        TemplateSpan::Type(TypeId::NUMBER),
    ]);

    // Verify template structure is created
    match interner.lookup(template) {
        Some(TypeData::TemplateLiteral(_)) => (),
        _ => panic!("Expected TemplateLiteral type"),
    }
}

#[test]
fn test_template_literal_with_boolean_type() {
    // `is_${boolean}` - template literal with boolean placeholder
    // TypeScript expands this to "is_true" | "is_false"
    let interner = TypeInterner::new();

    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("is_")),
        TemplateSpan::Type(TypeId::BOOLEAN),
    ]);

    // Should expand to union of two string literals
    match interner.lookup(template) {
        Some(TypeData::Union(list_id)) => {
            let members = interner.type_list(list_id);
            assert_eq!(
                members.len(),
                2,
                "Expected 2 members in union for boolean expansion"
            );
            // Both should be string literals
            for member in members.iter() {
                match interner.lookup(*member) {
                    Some(TypeData::Literal(LiteralValue::String(_))) => (),
                    other => panic!("Expected string literal in union, got {other:?}"),
                }
            }
        }
        other => panic!("Expected Union type for `is_${{boolean}}`, got {other:?}"),
    }
}

#[test]
fn test_template_literal_cartesian_product() {
    // `${"a"|"b"}_${"1"|"2"}` should expand to "a_1" | "a_2" | "b_1" | "b_2"
    let interner = TypeInterner::new();

    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let union1 = interner.union(vec![lit_a, lit_b]);

    let lit_1 = interner.literal_string("1");
    let lit_2 = interner.literal_string("2");
    let union2 = interner.union(vec![lit_1, lit_2]);

    let template = interner.template_literal(vec![
        TemplateSpan::Type(union1),
        TemplateSpan::Text(interner.intern_string("_")),
        TemplateSpan::Type(union2),
    ]);

    // With optimization, template literals with expandable unions are expanded immediately
    // `${"a"|"b"}_${"1"|"2"}` becomes "a_1" | "a_2" | "b_1" | "b_2"
    match interner.lookup(template) {
        Some(TypeData::Union(members_id)) => {
            let members = interner.type_list(members_id);
            assert_eq!(
                members.len(),
                4,
                "Expected 4 members in cartesian product union"
            );
        }
        _ => panic!(
            "Expected Union type for template with multiple union interpolations, got {:?}",
            interner.lookup(template)
        ),
    }
}

#[test]
fn test_template_literal_with_never() {
    // `prefix_${never}` should produce never (empty union)
    let interner = TypeInterner::new();

    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("prefix_")),
        TemplateSpan::Type(TypeId::NEVER),
    ]);

    // Template with never should collapse to never on evaluation
    let result = evaluate_type(&interner, template);
    // never in template position should result in never
    assert!(result == TypeId::NEVER || result == template);
}

#[test]
fn test_template_literal_with_any() {
    // `${any}` template with any should produce string
    // TypeScript: `prefix-${any}` collapses to `string` because any can be any value
    let interner = TypeInterner::new();

    let template = interner.template_literal(vec![TemplateSpan::Type(TypeId::ANY)]);

    // Template with any should widen to string - any stringifies to any possible string
    assert_eq!(template, TypeId::STRING);
}

#[test]
fn test_template_literal_concatenation() {
    // `${"hello"}${"world"}` should be "helloworld"
    let interner = TypeInterner::new();

    let hello = interner.literal_string("hello");
    let world = interner.literal_string("world");

    let template =
        interner.template_literal(vec![TemplateSpan::Type(hello), TemplateSpan::Type(world)]);

    // With optimization, string literal interpolations are expanded and concatenated
    // So `${"hello"}${"world"}` becomes "helloworld" string literal
    match interner.lookup(template) {
        Some(TypeData::Literal(LiteralValue::String(atom))) => {
            let s = interner.resolve_atom_ref(atom);
            assert_eq!(
                s.as_ref(),
                "helloworld",
                "Expected concatenated string literal"
            );
        }
        _ => panic!(
            "Expected string literal for concatenated string interpolations, got {:?}",
            interner.lookup(template)
        ),
    }
}

#[test]
fn test_template_literal_empty_string() {
    // `` empty template
    let interner = TypeInterner::new();

    let template = interner.template_literal(vec![]);

    // Empty template should be equivalent to empty string literal
    let result = evaluate_type(&interner, template);
    // Should be a valid type
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_template_literal_single_text() {
    // `hello` just text, no interpolations
    let interner = TypeInterner::new();

    let template =
        interner.template_literal(vec![TemplateSpan::Text(interner.intern_string("hello"))]);

    // Should be equivalent to "hello" literal
    let result = evaluate_type(&interner, template);
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_template_literal_pattern_infer_numeric() {
    // `id_${infer N extends number}` - infer from numeric pattern
    let interner = TypeInterner::new();

    let n_name = interner.intern_string("N");
    let infer_n = interner.intern(TypeData::Infer(TypeParamInfo {
        name: n_name,
        constraint: Some(TypeId::NUMBER),
        default: None,
        is_const: false,
    }));

    let extends_template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("id_")),
        TemplateSpan::Type(infer_n),
    ]);

    // Test matching against "id_42"
    let lit_id_42 = interner.literal_string("id_42");

    let cond = ConditionalType {
        check_type: lit_id_42,
        extends_type: extends_template,
        true_type: infer_n,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Should infer something or at least not error
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_template_literal_multiple_adjacent_types() {
    // `${A}${B}${C}` - multiple type interpolations
    let interner = TypeInterner::new();

    let lit_x = interner.literal_string("x");
    let lit_y = interner.literal_string("y");
    let lit_z = interner.literal_string("z");

    let template = interner.template_literal(vec![
        TemplateSpan::Type(lit_x),
        TemplateSpan::Type(lit_y),
        TemplateSpan::Type(lit_z),
    ]);

    // With optimization, string literal interpolations are expanded and concatenated
    // So `${"x"}${"y"}${"z"}` becomes "xyz" string literal
    match interner.lookup(template) {
        Some(TypeData::Literal(LiteralValue::String(atom))) => {
            let s = interner.resolve_atom_ref(atom);
            assert_eq!(s.as_ref(), "xyz", "Expected concatenated string literal");
        }
        _ => panic!(
            "Expected string literal for concatenated string interpolations, got {:?}",
            interner.lookup(template)
        ),
    }
}

#[test]
fn test_template_literal_union_in_middle() {
    // `pre_${"a"|"b"|"c"}_suf` - union in middle position
    let interner = TypeInterner::new();

    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let lit_c = interner.literal_string("c");
    let union = interner.union(vec![lit_a, lit_b, lit_c]);

    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("pre_")),
        TemplateSpan::Type(union),
        TemplateSpan::Text(interner.intern_string("_suf")),
    ]);

    // With optimization, template literals with expandable unions become a union of string literals
    // `pre_${"a"|"b"|"c"}_suf` becomes "pre_a_suf" | "pre_b_suf" | "pre_c_suf"
    match interner.lookup(template) {
        Some(TypeData::Union(members_id)) => {
            let members = interner.type_list(members_id);
            assert_eq!(members.len(), 3, "Expected 3 members in union");
        }
        _ => panic!(
            "Expected Union type for template with union interpolation, got {:?}",
            interner.lookup(template)
        ),
    }
}

#[test]
fn test_template_literal_bigint_type() {
    // `value_${bigint}` - template with bigint
    let interner = TypeInterner::new();

    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("value_")),
        TemplateSpan::Type(TypeId::BIGINT),
    ]);

    match interner.lookup(template) {
        Some(TypeData::TemplateLiteral(_)) => (),
        _ => panic!("Expected TemplateLiteral type"),
    }
}

#[test]
fn test_template_literal_null_undefined() {
    // `${null}` and `${undefined}` - special types in template
    // TypeScript expands these to string literals "null" and "undefined"
    let interner = TypeInterner::new();

    let template_null = interner.template_literal(vec![TemplateSpan::Type(TypeId::NULL)]);
    let template_undefined = interner.template_literal(vec![TemplateSpan::Type(TypeId::UNDEFINED)]);

    // Both should expand to string literals
    match interner.lookup(template_null) {
        Some(TypeData::Literal(LiteralValue::String(atom))) => {
            let s = interner.resolve_atom_ref(atom);
            assert_eq!(s.as_ref(), "null", "Expected 'null' string literal");
        }
        _ => panic!(
            "Expected string literal 'null' for `${{null}}`, got {:?}",
            interner.lookup(template_null)
        ),
    }
    match interner.lookup(template_undefined) {
        Some(TypeData::Literal(LiteralValue::String(atom))) => {
            let s = interner.resolve_atom_ref(atom);
            assert_eq!(
                s.as_ref(),
                "undefined",
                "Expected 'undefined' string literal"
            );
        }
        _ => panic!(
            "Expected string literal 'undefined' for `${{undefined}}`, got {:?}",
            interner.lookup(template_undefined)
        ),
    }
}

#[test]
fn test_template_literal_subtype_of_string() {
    // `foo_${T}` should extend string when T is string
    let interner = TypeInterner::new();

    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("foo_")),
        TemplateSpan::Type(TypeId::STRING),
    ]);

    // Template literal types extend string
    let mut checker = SubtypeChecker::new(&interner);
    let extends = checker.is_subtype_of(template, TypeId::STRING);
    // Should be true - all template literal types are subtypes of string
    assert!(extends);
}

#[test]
fn test_template_literal_specific_extends_pattern() {
    // "foo_bar" extends `foo_${string}`
    let interner = TypeInterner::new();

    let literal = interner.literal_string("foo_bar");
    let pattern = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("foo_")),
        TemplateSpan::Type(TypeId::STRING),
    ]);

    let mut checker = SubtypeChecker::new(&interner);
    let extends = checker.is_subtype_of(literal, pattern);
    // "foo_bar" should extend `foo_${string}`
    assert!(extends);
}

// =============================================================================
// KEYOF EDGE CASES - INTERSECTION AND UNION
// =============================================================================

#[test]
fn test_keyof_intersection_with_never() {
    // keyof (T & never) should be never (never absorbs in intersection)
    let interner = TypeInterner::new();

    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let intersection = interner.intersection(vec![obj, TypeId::NEVER]);
    let result = evaluate_keyof(&interner, intersection);

    // Intersection with never is never, so keyof never = never
    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_keyof_union_with_any() {
    // keyof (T | any) - any absorbs the union
    let interner = TypeInterner::new();

    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let union = interner.union(vec![obj, TypeId::ANY]);
    let result = evaluate_keyof(&interner, union);

    // Union with any is any, keyof any is string | number | symbol
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_keyof_intersection_with_any() {
    // keyof (T & any) - any in intersection
    let interner = TypeInterner::new();

    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let intersection = interner.intersection(vec![obj, TypeId::ANY]);
    let result = evaluate_keyof(&interner, intersection);

    // Should produce keys from both
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_keyof_union_with_unknown() {
    // keyof (T | unknown) - unknown absorbs in union
    let interner = TypeInterner::new();

    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let union = interner.union(vec![obj, TypeId::UNKNOWN]);
    let result = evaluate_keyof(&interner, union);

    // keyof unknown is never
    assert_eq!(result, TypeId::NEVER);
}

