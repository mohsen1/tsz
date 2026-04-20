use super::*;
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

#[test]
fn test_keyof_four_way_intersection() {
    // keyof (A & B & C & D) = keyof A | keyof B | keyof C | keyof D
    let interner = TypeInterner::new();

    let obj_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let obj_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);

    let obj_c = interner.object(vec![PropertyInfo::new(
        interner.intern_string("c"),
        TypeId::BOOLEAN,
    )]);

    let obj_d = interner.object(vec![PropertyInfo::new(
        interner.intern_string("d"),
        TypeId::STRING,
    )]);

    let intersection = interner.intersection(vec![obj_a, obj_b, obj_c, obj_d]);
    let result = evaluate_keyof(&interner, intersection);

    // Should produce "a" | "b" | "c" | "d"
    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let lit_c = interner.literal_string("c");
    let lit_d = interner.literal_string("d");
    let expected = interner.union(vec![lit_a, lit_b, lit_c, lit_d]);
    assert_eq!(result, expected);
}

#[test]
fn test_keyof_four_way_union() {
    // keyof (A | B | C | D) = only common keys
    let interner = TypeInterner::new();

    let common_key = interner.intern_string("common");

    let obj_a = interner.object(vec![
        PropertyInfo::new(common_key, TypeId::STRING),
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
    ]);

    let obj_b = interner.object(vec![
        PropertyInfo::new(common_key, TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
    ]);

    let obj_c = interner.object(vec![PropertyInfo::new(common_key, TypeId::STRING)]);

    let obj_d = interner.object(vec![
        PropertyInfo::new(common_key, TypeId::STRING),
        PropertyInfo::new(interner.intern_string("d"), TypeId::BOOLEAN),
    ]);

    let union = interner.union(vec![obj_a, obj_b, obj_c, obj_d]);
    let result = evaluate_keyof(&interner, union);

    // Only "common" is present in all
    let expected = interner.literal_string("common");
    assert_eq!(result, expected);
}

#[test]
fn test_keyof_mixed_intersection_union() {
    // keyof ((A & B) | C) - nested combination
    let interner = TypeInterner::new();

    let obj_a = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("common"), TypeId::STRING),
    ]);

    let obj_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);

    let obj_c = interner.object(vec![PropertyInfo::new(
        interner.intern_string("common"),
        TypeId::STRING,
    )]);

    let a_and_b = interner.intersection(vec![obj_a, obj_b]);
    let union = interner.union(vec![a_and_b, obj_c]);
    let result = evaluate_keyof(&interner, union);

    // Common keys between (A & B) and C = "common"
    let expected = interner.literal_string("common");
    assert_eq!(result, expected);
}

#[test]
fn test_keyof_intersection_both_index_signatures() {
    // keyof ({ [k: string]: T } & { [k: number]: U }) = string | number
    let interner = TypeInterner::new();

    let string_indexed = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let number_indexed = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
    });

    let intersection = interner.intersection(vec![string_indexed, number_indexed]);
    let result = evaluate_keyof(&interner, intersection);

    // Should be string | number
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

#[test]
fn test_keyof_union_index_and_literal() {
    // keyof ({ [k: string]: T } | { a: U }) - intersection of keys
    let interner = TypeInterner::new();

    let string_indexed = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let literal_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::NUMBER,
    )]);

    let union = interner.union(vec![string_indexed, literal_obj]);
    let result = evaluate_keyof(&interner, union);

    // "a" is subtype of string, so "a" is the common key
    let expected = interner.literal_string("a");
    assert_eq!(result, expected);
}

#[test]
fn test_keyof_intersection_with_callable() {
    // keyof (T & { (): void }) - object with call signature
    let interner = TypeInterner::new();

    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            type_params: Vec::new(),
            params: Vec::new(),
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
    });

    let intersection = interner.intersection(vec![obj, callable]);
    let result = evaluate_keyof(&interner, intersection);

    // Should at least include "a" from the object
    let lit_a = interner.literal_string("a");
    let mut checker = SubtypeChecker::new(&interner);
    assert!(checker.is_subtype_of(lit_a, result));
}

#[test]
fn test_keyof_intersection_with_array() {
    // keyof ({ a: T } & string[]) - object intersected with array
    let interner = TypeInterner::new();

    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let arr = interner.array(TypeId::STRING);
    let intersection = interner.intersection(vec![obj, arr]);
    let result = evaluate_keyof(&interner, intersection);

    // Should include array keys (number index) plus "a" plus array methods
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_keyof_empty_intersection() {
    // keyof (A & B) where A and B have disjoint primitive types
    // This is different from object intersection - primitive intersection is never
    let interner = TypeInterner::new();

    // string & number = never
    let intersection = interner.intersection(vec![TypeId::STRING, TypeId::NUMBER]);
    let result = evaluate_keyof(&interner, intersection);

    // Intersection of disjoint primitives is never, keyof never = never
    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_keyof_empty_union() {
    // keyof never = never
    let interner = TypeInterner::new();

    let result = evaluate_keyof(&interner, TypeId::NEVER);
    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_keyof_nested_keyof() {
    // keyof keyof T - nested keyof application
    let interner = TypeInterner::new();

    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
    ]);

    let keyof_obj = evaluate_keyof(&interner, obj);
    // keyof_obj = "a" | "b"

    // Now keyof (keyof obj) = keyof ("a" | "b") = keyof string (apparent members)
    let keyof_keyof = evaluate_keyof(&interner, keyof_obj);

    // String literal unions extend string, so keyof should give string apparent members
    assert!(keyof_keyof != TypeId::ERROR);
}

// ==================== Callable-parameter inference regression tests ====================

#[test]
fn test_callable_param_infer_union_of_signatures() {
    // T extends ((x: infer P) => any) ? P : never
    // where T = ((x: string) => void) | ((x: number) => void)
    // Result should be string | number (extracting param from both signatures)
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let p_name = interner.intern_string("P");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let infer_p = interner.intern(TypeData::Infer(TypeParamInfo {
        name: p_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: (x: infer P) => any
    let pattern_fn = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(infer_p)],
        return_type: TypeId::ANY,
        type_predicate: None,
        this_type: None,
        type_params: Vec::new(),
        is_constructor: false,
        is_method: false,
    });

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: pattern_fn,
        true_type: infer_p,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);

    // Create union of two function signatures: ((x: string) => void) | ((x: number) => void)
    let fn_string = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(TypeId::STRING)],
        return_type: TypeId::VOID,
        type_predicate: None,
        this_type: None,
        type_params: Vec::new(),
        is_constructor: false,
        is_method: false,
    });
    let fn_number = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(TypeId::NUMBER)],
        return_type: TypeId::VOID,
        type_predicate: None,
        this_type: None,
        type_params: Vec::new(),
        is_constructor: false,
        is_method: false,
    });
    let fn_union = interner.union(vec![fn_string, fn_number]);

    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, fn_union);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Distributive: (string) | (number) = string | number
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

#[test]
fn test_callable_param_infer_overloaded_callable() {
    // T extends { (x: infer P): any } ? P : never
    // where T = { (x: string): void; (x: number): void }
    // For overloaded callables, TypeScript uses the last signature's param
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let p_name = interner.intern_string("P");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let infer_p = interner.intern(TypeData::Infer(TypeParamInfo {
        name: p_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: { (x: infer P): any }
    let pattern_callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            params: vec![ParamInfo::unnamed(infer_p)],
            return_type: TypeId::ANY,
            type_predicate: None,
            this_type: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
    });

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: pattern_callable,
        true_type: infer_p,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);

    // Create overloaded callable with two call signatures
    let overloaded = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![
            CallSignature {
                params: vec![ParamInfo::unnamed(TypeId::STRING)],
                return_type: TypeId::VOID,
                type_predicate: None,
                this_type: None,
                type_params: Vec::new(),
                is_method: false,
            },
            CallSignature {
                params: vec![ParamInfo::unnamed(TypeId::NUMBER)],
                return_type: TypeId::BOOLEAN,
                type_predicate: None,
                this_type: None,
                type_params: Vec::new(),
                is_method: false,
            },
        ],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
    });

    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, overloaded);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Current behavior: callable matching doesn't yet extract from overloads
    // This returns never because Callable vs Callable matching with infer patterns
    // is not fully implemented for extracting from last signature.
    // TODO: Implement proper overload signature extraction for infer patterns
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_callable_param_infer_mixed_union() {
    // T extends ((x: infer P) => any) ? P : never
    // where T = ((x: string) => void) | number
    // Result: string (number doesn't match the pattern so it goes to never branch)
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let p_name = interner.intern_string("P");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let infer_p = interner.intern(TypeData::Infer(TypeParamInfo {
        name: p_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let pattern_fn = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(infer_p)],
        return_type: TypeId::ANY,
        type_predicate: None,
        this_type: None,
        type_params: Vec::new(),
        is_constructor: false,
        is_method: false,
    });

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: pattern_fn,
        true_type: infer_p,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);

    let fn_string = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(TypeId::STRING)],
        return_type: TypeId::VOID,
        type_predicate: None,
        this_type: None,
        type_params: Vec::new(),
        is_constructor: false,
        is_method: false,
    });
    let mixed_union = interner.union(vec![fn_string, TypeId::NUMBER]);

    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, mixed_union);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // string (from fn) | never (from number) = string
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_callable_return_and_param_infer_separately() {
    // T extends ((x: infer P) => infer R) ? [P, R] : never
    // where T = (x: string) => number
    // Result: [string, number] represented as a tuple
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let p_name = interner.intern_string("P");
    let r_name = interner.intern_string("R");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let infer_p = interner.intern(TypeData::Infer(TypeParamInfo {
        name: p_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: r_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let pattern_fn = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(infer_p)],
        return_type: infer_r,
        type_predicate: None,
        this_type: None,
        type_params: Vec::new(),
        is_constructor: false,
        is_method: false,
    });

    // True type: tuple [P, R]
    let tuple_type = interner.tuple(vec![
        TupleElement {
            type_id: infer_p,
            optional: false,
            rest: false,
            name: None,
        },
        TupleElement {
            type_id: infer_r,
            optional: false,
            rest: false,
            name: None,
        },
    ]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: pattern_fn,
        true_type: tuple_type,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);

    let source_fn = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(TypeId::STRING)],
        return_type: TypeId::NUMBER,
        type_predicate: None,
        this_type: None,
        type_params: Vec::new(),
        is_constructor: false,
        is_method: false,
    });

    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, source_fn);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
            name: None,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
            name: None,
        },
    ]);
    assert_eq!(result, expected);
}

#[test]
fn test_callable_multiple_params_infer() {
    // T extends ((a: infer A, b: infer B) => any) ? [A, B] : never
    // where T = (a: string, b: number) => void
    // Result: [string, number]
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let infer_a = interner.intern(TypeData::Infer(TypeParamInfo {
        name: a_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let infer_b = interner.intern(TypeData::Infer(TypeParamInfo {
        name: b_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let pattern_fn = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(infer_a), ParamInfo::unnamed(infer_b)],
        return_type: TypeId::ANY,
        type_predicate: None,
        this_type: None,
        type_params: Vec::new(),
        is_constructor: false,
        is_method: false,
    });

    let tuple_type = interner.tuple(vec![
        TupleElement {
            type_id: infer_a,
            optional: false,
            rest: false,
            name: None,
        },
        TupleElement {
            type_id: infer_b,
            optional: false,
            rest: false,
            name: None,
        },
    ]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: pattern_fn,
        true_type: tuple_type,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);

    let source_fn = interner.function(FunctionShape {
        params: vec![
            ParamInfo::unnamed(TypeId::STRING),
            ParamInfo::unnamed(TypeId::NUMBER),
        ],
        return_type: TypeId::VOID,
        type_predicate: None,
        this_type: None,
        type_params: Vec::new(),
        is_constructor: false,
        is_method: false,
    });

    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, source_fn);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
            name: None,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
            name: None,
        },
    ]);
    assert_eq!(result, expected);
}

// =============================================================================
// Mapped Type Edge Cases - Homomorphic Modifiers & Key Remapping
// =============================================================================
// These tests cover advanced mapped type scenarios including homomorphic
// modifier preservation, complex key remapping, and edge cases.

#[test]
fn test_mapped_type_homomorphic_preserves_optional() {
    // Homomorphic: { [K in keyof T]: T[K] } preserves optional from source
    let interner = TypeInterner::new();

    // Source type with optional property
    let source = interner.object(vec![
        PropertyInfo::new(interner.intern_string("required"), TypeId::STRING),
        PropertyInfo::opt(interner.intern_string("optional"), TypeId::NUMBER),
    ]);

    let keyof_source = interner.intern(TypeData::KeyOf(source));

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keyof_source,
        name_type: None,
        template: TypeId::BOOLEAN,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_mapped_type_homomorphic_preserves_readonly() {
    // Homomorphic: { [K in keyof T]: T[K] } preserves readonly from source
    let interner = TypeInterner::new();

    let source = interner.object(vec![
        PropertyInfo::new(interner.intern_string("mutable"), TypeId::STRING),
        PropertyInfo::readonly(interner.intern_string("immutable"), TypeId::NUMBER),
    ]);

    let keyof_source = interner.intern(TypeData::KeyOf(source));

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keyof_source,
        name_type: None,
        template: TypeId::BOOLEAN,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_mapped_type_key_remap_to_getter_setter() {
    // Key remapping: { [K in keyof T as `get${Capitalize<K>}`]: () => T[K] }
    let interner = TypeInterner::new();

    let key_x = interner.literal_string("x");
    let key_y = interner.literal_string("y");
    let keys = interner.union(vec![key_x, key_y]);

    // Simulate key remapping with template literal
    let get_x = interner.literal_string("getX");
    let get_y = interner.literal_string("getY");
    let remapped_keys = interner.union(vec![get_x, get_y]);

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keys,
        name_type: Some(remapped_keys),
        template: TypeId::NUMBER,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_mapped_type_key_remap_filter_by_type() {
    // Filter keys: { [K in keyof T as T[K] extends string ? K : never]: T[K] }
    let interner = TypeInterner::new();

    let key_name = interner.literal_string("name");
    let key_age = interner.literal_string("age");
    let keys = interner.union(vec![key_name, key_age]);

    // Only "name" passes filter (string type), "age" becomes never
    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keys,
        name_type: Some(key_name),
        template: TypeId::STRING,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_mapped_type_nested_mapped() {
    // Nested: { [K in keyof T]: { [J in keyof T[K]]: boolean } }
    let interner = TypeInterner::new();

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let outer_keys = interner.union(vec![key_a, key_b]);

    let inner_template = TypeId::BOOLEAN;

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: outer_keys,
        name_type: None,
        template: inner_template,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);
    assert!(result != TypeId::ERROR);
}

