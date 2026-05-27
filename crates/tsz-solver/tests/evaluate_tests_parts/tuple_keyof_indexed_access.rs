/// Test template literal with union of unions
/// `prefix${("a" | "b") | ("c" | "d")}` should handle nested unions
#[test]
fn test_template_literal_nested_union_interpolation() {
    let interner = TypeInterner::new();

    // Create nested unions: ("a" | "b") | ("c" | "d")
    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let union_ab = interner.union(vec![lit_a, lit_b]);

    let lit_c = interner.literal_string("c");
    let lit_d = interner.literal_string("d");
    let union_cd = interner.union(vec![lit_c, lit_d]);

    let nested_union = interner.union(vec![union_ab, union_cd]);

    // Template with nested union interpolation
    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("prefix")),
        TemplateSpan::Type(nested_union),
    ]);

    // With optimization, nested unions in template literals should be expanded
    // The nested union is flattened to "a" | "b" | "c" | "d" and template expands to
    // "prefixa" | "prefixb" | "prefixc" | "prefixd"
    match interner.lookup(template) {
        Some(TypeData::Union(members_id)) => {
            let members = interner.type_list(members_id);
            assert_eq!(members.len(), 4, "Expected 4 members in expanded union");
        }
        _ => panic!(
            "Expected Union type for template with nested union interpolation, got {:?}",
            interner.lookup(template)
        ),
    }
}

/// Test template literal matching against another template literal
/// `foo${string}` extends `foo${infer R}` ? R : never
#[test]
fn test_template_literal_matches_template_literal() {
    let interner = TypeInterner::new();

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: `foo${infer R}`
    let pattern = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("foo")),
        TemplateSpan::Type(infer_r),
    ]);

    // Check type: `foo${string}`
    let check_template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("foo")),
        TemplateSpan::Type(TypeId::STRING),
    ]);

    let cond = ConditionalType {
        check_type: check_template,
        extends_type: pattern,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Should infer string
    assert_eq!(result, TypeId::STRING);
}

/// Test keyof with template literal that expands to multiple literals
/// keyof `item${0 | 1 | 2}` should return keyof string (apparent keys)
#[test]
fn test_keyof_template_literal_number_union_interpolation() {
    let interner = TypeInterner::new();

    // Create 0 | 1 | 2 union
    let lit_0 = interner.literal_number(0.0);
    let lit_1 = interner.literal_number(1.0);
    let lit_2 = interner.literal_number(2.0);
    let union_012 = interner.union(vec![lit_0, lit_1, lit_2]);

    // Create template literal: `item${0 | 1 | 2}`
    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("item")),
        TemplateSpan::Type(union_012),
    ]);

    // keyof returns apparent keys of string (same as keyof string)
    let result = evaluate_keyof(&interner, template);
    let expected = evaluate_keyof(&interner, TypeId::STRING);
    assert_eq!(result, expected);
}

/// Test conditional with template literal in both check and extends
/// `prefix${string}` extends `prefix${string}` ? true : false
#[test]
fn test_template_literal_conditional_same_pattern() {
    let interner = TypeInterner::new();

    let template1 = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("prefix")),
        TemplateSpan::Type(TypeId::STRING),
    ]);

    let template2 = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("prefix")),
        TemplateSpan::Type(TypeId::STRING),
    ]);

    let cond = ConditionalType {
        check_type: template1,
        extends_type: template2,
        true_type: TypeId::STRING,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Should match and return true branch
    assert_eq!(result, TypeId::STRING);
}

/// Test tail-recursion elimination for conditional types.
///
/// This test verifies that tail-recursive conditional types can recurse
/// up to `MAX_TAIL_RECURSION_DEPTH` (1000) instead of being limited by
/// `MAX_EVALUATE_DEPTH` (50).
#[test]
fn test_tail_recursive_conditional() {
    let interner = TypeInterner::new();

    // Build a chain of 60 nested conditionals
    // Each conditional: `string extends number ? never : string`
    // This will take the false branch each time

    let mut current_type = TypeId::STRING;

    for _ in 0..60 {
        let cond = ConditionalType {
            check_type: TypeId::STRING,
            extends_type: TypeId::NUMBER,
            true_type: TypeId::NEVER,
            false_type: current_type,
            is_distributive: false,
        };

        current_type = interner.conditional(cond);
    }

    let mut evaluator = TypeEvaluator::new(&interner);
    let result = evaluator.evaluate(current_type);

    // The result should be STRING (the false branch all the way down)
    // Without tail-recursion elimination, this would hit MAX_EVALUATE_DEPTH (50)
    assert_eq!(result, TypeId::STRING);
}

/// Test intersection reduction for disjoint primitive types.
#[test]
fn test_intersection_reduction_disjoint_primitives() {
    let interner = TypeInterner::new();
    let intersection = interner.intersection(vec![TypeId::STRING, TypeId::NUMBER]);
    let mut evaluator = TypeEvaluator::new(&interner);
    let result = evaluator.evaluate(intersection);
    assert_eq!(result, TypeId::NEVER);
}

/// Test intersection reduction with any.
#[test]
fn test_intersection_reduction_any() {
    let interner = TypeInterner::new();
    let intersection = interner.intersection(vec![TypeId::STRING, TypeId::ANY]);
    let mut evaluator = TypeEvaluator::new(&interner);
    let result = evaluator.evaluate(intersection);
    assert_eq!(result, TypeId::ANY);
}

/// Test union reduction for duplicate types.
#[test]
fn test_union_reduction_duplicates() {
    let interner = TypeInterner::new();
    let union = interner.union(vec![TypeId::STRING, TypeId::STRING]);
    let mut evaluator = TypeEvaluator::new(&interner);
    let result = evaluator.evaluate(union);
    assert_eq!(result, TypeId::STRING);
}

/// Test union reduction for literal and base type.
#[test]
fn test_union_reduction_literal_into_base() {
    let interner = TypeInterner::new();
    let hello = interner.literal_string("hello");
    let union = interner.union(vec![hello, TypeId::STRING]);
    let mut evaluator = TypeEvaluator::new(&interner);
    let result = evaluator.evaluate(union);
    assert_eq!(result, TypeId::STRING);
}

/// Homomorphic mapped type with `keyof` constraint preserves optional modifiers.
///
/// This tests the core fix for Pick<TP, keyof TP> where TP has optional properties.
/// The mapped type `{ [P in keyof TP]: TP[P] }` should produce the same type as TP,
/// preserving optional/readonly modifiers from the source.
///
/// Previously, the evaluator would produce `{ a: number | undefined, b: string | undefined }`
/// instead of `{ a?: number, b?: string }` because:
/// 1. `IndexAccess` on optional properties adds `| undefined`
/// 2. The homomorphic detection failed when the source object was extracted from the
///    template vs the constraint (different `TypeIds` for the same logical type)
#[test]
fn test_homomorphic_mapped_keyof_preserves_optional() {
    let interner = TypeInterner::new();

    let key_a = interner.intern_string("a");
    let key_b = interner.intern_string("b");

    // Source: { a?: number, b?: string }
    let source = interner.object(vec![
        PropertyInfo {
            name: key_a,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: true,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
            is_symbol_named: false,
            single_quoted_name: false,
        },
        PropertyInfo {
            name: key_b,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: true,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
            is_symbol_named: false,
            single_quoted_name: false,
        },
    ]);

    // Constraint: keyof source
    let keyof_source = interner.keyof(source);

    let key_param = TypeParamInfo {
        name: interner.intern_string("P"),
        constraint: Some(keyof_source),
        default: None,
        is_const: false,
    };
    let key_param_id = interner.intern(TypeData::TypeParameter(key_param));

    // Template: source[P]
    let index_access = interner.intern(TypeData::IndexAccess(source, key_param_id));

    // Mapped: { [P in keyof source]: source[P] }
    let mapped = MappedType {
        type_param: key_param,
        constraint: keyof_source,
        name_type: None,
        template: index_access,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);

    // Result should be identical to source: { a?: number, b?: string }
    assert_eq!(result, source);
}

/// Homomorphic mapped type with post-instantiation keyof (union constraint).
///
/// After generic instantiation, `keyof T` may be eagerly evaluated to a literal union.
/// The mapped type should still be detected as homomorphic via Method 2 (comparing
/// `keyof obj` with the constraint) and preserve optional modifiers.
#[test]
fn test_homomorphic_mapped_post_instantiation_preserves_optional() {
    let interner = TypeInterner::new();

    let key_a = interner.intern_string("a");
    let key_b = interner.intern_string("b");

    // Source: { a?: number, b?: string }
    let source = interner.object(vec![
        PropertyInfo {
            name: key_a,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: true,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
            is_symbol_named: false,
            single_quoted_name: false,
        },
        PropertyInfo {
            name: key_b,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: true,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
            is_symbol_named: false,
            single_quoted_name: false,
        },
    ]);

    // Constraint: "a" | "b" (post-instantiation form of keyof source)
    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let union_constraint = interner.union(vec![lit_a, lit_b]);

    let key_param = TypeParamInfo {
        name: interner.intern_string("P"),
        constraint: Some(union_constraint),
        default: None,
        is_const: false,
    };
    let key_param_id = interner.intern(TypeData::TypeParameter(key_param));

    // Template: source[P] (uses the concrete object, not type param)
    let index_access = interner.intern(TypeData::IndexAccess(source, key_param_id));

    // Mapped: { [P in "a" | "b"]: source[P] } — post-instantiation form
    let mapped = MappedType {
        type_param: key_param,
        constraint: union_constraint,
        name_type: None,
        template: index_access,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);

    // Result should preserve optional: { a?: number, b?: string }
    assert_eq!(result, source);
}

/// Homomorphic mapped type preserves readonly in the same way as optional.
#[test]
fn test_homomorphic_mapped_keyof_preserves_readonly() {
    let interner = TypeInterner::new();

    let key_a = interner.intern_string("a");

    // Source: { readonly a: number }
    let source = interner.object(vec![PropertyInfo {
        name: key_a,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: true,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    }]);

    let keyof_source = interner.keyof(source);
    let key_param = TypeParamInfo {
        name: interner.intern_string("P"),
        constraint: Some(keyof_source),
        default: None,
        is_const: false,
    };
    let key_param_id = interner.intern(TypeData::TypeParameter(key_param));
    let index_access = interner.intern(TypeData::IndexAccess(source, key_param_id));

    let mapped = MappedType {
        type_param: key_param,
        constraint: keyof_source,
        name_type: None,
        template: index_access,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);
    assert_eq!(result, source);
}

/// Test: Homomorphic mapped type alias applied to a primitive passes through.
/// `Partial<number>` should evaluate to `number` (not expand the mapped type).
/// This matches tsc's `instantiateMappedType` logic:
///   const typeVariable = getHomomorphicTypeVariable(type);
///   if (typeVariable && !(instantiateType(typeVariable, mapper).flags & TypeFlags.Object))
///     return instantiateType(typeVariable, mapper);
#[test]
fn test_application_homomorphic_mapped_type_primitive_passthrough() {
    use crate::evaluation::evaluate::TypeEvaluator;
    use crate::relations::subtype::TypeEnvironment;

    let interner = TypeInterner::new();

    // Define type parameter T
    let t_name = interner.intern_string("T");
    let t_param = TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    // Define: type Partial<T> = { [K in keyof T]?: T[K] }
    let k_name = interner.intern_string("K");
    let k_param = TypeParamInfo {
        name: k_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let k_type = interner.intern(TypeData::TypeParameter(k_param));
    let keyof_t = interner.intern(TypeData::KeyOf(t_type));
    let index_access = interner.intern(TypeData::IndexAccess(t_type, k_type));

    let partial_body = MappedType {
        type_param: k_param,
        constraint: keyof_t,
        name_type: None,
        template: index_access,
        readonly_modifier: None,
        optional_modifier: Some(MappedModifier::Add),
    };
    let partial_body_id = interner.mapped(partial_body);

    // Create Lazy(DefId(1)) for Partial type alias
    let partial_ref = interner.lazy(DefId(1));

    // Test: Partial<number> should pass through to number
    let partial_number = interner.application(partial_ref, vec![TypeId::NUMBER]);

    let mut env = TypeEnvironment::new();
    env.insert_def_with_params(DefId(1), partial_body_id, vec![t_param]);

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(partial_number);
    assert_eq!(
        result,
        TypeId::NUMBER,
        "Partial<number> should pass through to number"
    );

    // Test: Partial<string> should pass through to string
    let partial_string = interner.application(partial_ref, vec![TypeId::STRING]);
    let mut evaluator2 = TypeEvaluator::with_resolver(&interner, &env);
    let result2 = evaluator2.evaluate(partial_string);
    assert_eq!(
        result2,
        TypeId::STRING,
        "Partial<string> should pass through to string"
    );

    // Test: Partial<boolean> should pass through to boolean
    let partial_boolean = interner.application(partial_ref, vec![TypeId::BOOLEAN]);
    let mut evaluator3 = TypeEvaluator::with_resolver(&interner, &env);
    let result3 = evaluator3.evaluate(partial_boolean);
    assert_eq!(
        result3,
        TypeId::BOOLEAN,
        "Partial<boolean> should pass through to boolean"
    );
}

/// Test: Homomorphic mapped type alias applied to an object expands normally.
/// `Partial<{ a: number }>` should expand to `{ a?: number }`, NOT pass through.
#[test]
fn test_application_homomorphic_mapped_type_object_expands() {
    use crate::evaluation::evaluate::TypeEvaluator;
    use crate::relations::subtype::TypeEnvironment;

    let interner = TypeInterner::new();

    // Define type parameter T
    let t_name = interner.intern_string("T");
    let t_param = TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    // Define: type Partial<T> = { [K in keyof T]?: T[K] }
    let k_name = interner.intern_string("K");
    let k_param = TypeParamInfo {
        name: k_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let k_type = interner.intern(TypeData::TypeParameter(k_param));
    let keyof_t = interner.intern(TypeData::KeyOf(t_type));
    let index_access = interner.intern(TypeData::IndexAccess(t_type, k_type));

    let partial_body = MappedType {
        type_param: k_param,
        constraint: keyof_t,
        name_type: None,
        template: index_access,
        readonly_modifier: None,
        optional_modifier: Some(MappedModifier::Add),
    };
    let partial_body_id = interner.mapped(partial_body);

    // Create source: { a: number }
    let a_name = interner.intern_string("a");
    let source = interner.object(vec![PropertyInfo::new(a_name, TypeId::NUMBER)]);

    // Create Lazy(DefId(1)) for Partial type alias
    let partial_ref = interner.lazy(DefId(1));

    // Test: Partial<{ a: number }> should expand to { a?: number }
    let partial_obj = interner.application(partial_ref, vec![source]);

    let mut env = TypeEnvironment::new();
    env.insert_def_with_params(DefId(1), partial_body_id, vec![t_param]);

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(partial_obj);

    // Should NOT be the source object (should be expanded)
    assert_ne!(
        result, source,
        "Partial<{{ a: number }}> should NOT pass through, should expand"
    );

    // Should be an object type with optional property 'a'
    match interner.lookup(result) {
        Some(TypeData::Object(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            assert_eq!(shape.properties.len(), 1, "Should have 1 property");
            assert_eq!(shape.properties[0].name, a_name);
            assert!(
                shape.properties[0].optional,
                "Property 'a' should be optional"
            );
        }
        other => panic!("Expected Object type, got {other:?}"),
    }
}

// =============================================================================
// Lazy type as valid index type (TS2538 fix)
// =============================================================================

#[test]
fn test_lazy_type_not_invalid_for_indexing() {
    // A Lazy(DefId) type (e.g., `type SS1 = string`) should NOT be
    // flagged as invalid for indexing. Only concrete invalid types
    // (objects, arrays, void, etc.) should be invalid.
    let interner = TypeInterner::new();

    let def_id = DefId(100);
    let lazy_type = interner.lazy(def_id);

    let result = crate::type_queries::get_invalid_index_type_member(&interner, lazy_type);
    assert!(
        result.is_none(),
        "Lazy type should not be flagged as invalid index type"
    );
}

#[test]
fn test_concrete_invalid_types_still_flagged() {
    // Verify that actual invalid types (objects, arrays, void) are still caught.
    let interner = TypeInterner::new();

    // Object type is invalid for indexing
    let obj = interner.object(vec![]);
    assert!(
        crate::type_queries::get_invalid_index_type_member(&interner, obj).is_some(),
        "Object type should be invalid for indexing"
    );

    // Array type is invalid for indexing
    let arr = interner.array(TypeId::NUMBER);
    assert!(
        crate::type_queries::get_invalid_index_type_member(&interner, arr).is_some(),
        "Array type should be invalid for indexing"
    );

    // string is valid for indexing
    assert!(
        crate::type_queries::get_invalid_index_type_member(&interner, TypeId::STRING).is_none(),
        "string should be valid for indexing"
    );

    // number is valid for indexing
    assert!(
        crate::type_queries::get_invalid_index_type_member(&interner, TypeId::NUMBER).is_none(),
        "number should be valid for indexing"
    );
}

// =============================================================================
// Tuple element evaluation (visit_tuple)
// =============================================================================

#[test]
fn test_tuple_evaluates_index_access_element() {
    // A tuple with an IndexAccess element should evaluate the element.
    let interner = TypeInterner::new();

    // Create mapped type: { [K in string]: number }
    let mapped = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: Some(TypeId::STRING),
            default: None,
            is_const: false,
        },
        constraint: TypeId::STRING,
        template: TypeId::NUMBER,
        name_type: None,
        optional_modifier: None,
        readonly_modifier: None,
    });

    // Create IndexAccess: MappedType[string]
    let index_access = interner.index_access(mapped, TypeId::STRING);

    // Create tuple: [string, IndexAccess]
    let tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: index_access,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let result = evaluate_type(&interner, tuple);

    // The tuple should have been evaluated — the IndexAccess element
    // should be simplified
    match interner.lookup(result) {
        Some(TypeData::Tuple(list_id)) => {
            let elements = interner.tuple_list(list_id);
            assert_eq!(elements.len(), 2, "Tuple should still have 2 elements");
            assert_eq!(elements[0].type_id, TypeId::STRING);
            assert_ne!(
                elements[1].type_id, index_access,
                "IndexAccess element should have been evaluated"
            );
        }
        _ => panic!("Expected evaluated Tuple type"),
    }
}

#[test]
fn test_tuple_preserves_concrete_elements() {
    // A tuple with only concrete elements should pass through unchanged.
    let interner = TypeInterner::new();

    let tuple = interner.tuple(vec![
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

    let result = evaluate_type(&interner, tuple);
    // Should be the same tuple — no evaluation needed
    assert_eq!(result, tuple, "Concrete tuple should be unchanged");
}

#[test]
fn test_index_access_with_keyof_type_as_index() {
    let interner = TypeInterner::new();

    // Pattern: T[keyof T] where T = { a: number, b: string }
    // keyof T = "a" | "b"
    // T[keyof T] = T["a" | "b"] = number | string
    let a_prop = interner.intern_string("a");
    let b_prop = interner.intern_string("b");

    let obj = interner.object(vec![
        PropertyInfo::new(a_prop, TypeId::NUMBER),
        PropertyInfo::new(b_prop, TypeId::STRING),
    ]);

    // Create keyof T as the index
    let keyof_obj = interner.keyof(obj);

    // Create IndexAccess(T, keyof T)
    let index_access = interner.index_access(obj, keyof_obj);

    // Evaluate should resolve to number | string
    let result = evaluate_type(&interner, index_access);

    // The result should be a union of number and string
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::STRING]);
    assert_eq!(
        result,
        expected,
        "T[keyof T] should evaluate to number | string, got {:?}",
        interner.lookup(result)
    );
}

#[test]
fn intermediate_application_alias_skips_preexisting_application_occurrence() {
    let interner = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();
    let type_param = |name: &str| TypeParamInfo {
        name: interner.intern_string(name),
        constraint: None,
        default: None,
        is_const: false,
    };

    let inner_def = def_store.register(crate::def::DefinitionInfo::type_alias(
        interner.intern_string("Inner"),
        vec![type_param("T")],
        TypeId::UNKNOWN,
    ));
    let outer_def = def_store.register(crate::def::DefinitionInfo::type_alias(
        interner.intern_string("Outer"),
        vec![type_param("T")],
        TypeId::UNKNOWN,
    ));
    let one = interner.literal_number(1.0);

    // Simulate a user-authored Inner<1> that predates evaluating Outer<1>.
    let inner_app = interner.application(interner.lazy(inner_def), vec![one]);
    let outer_app = interner.application(interner.lazy(outer_def), vec![one]);
    let evaluated = interner.object(vec![PropertyInfo::new(
        interner.intern_string("p"),
        TypeId::NUMBER,
    )]);

    let evaluator = TypeEvaluator::new(&interner);
    evaluator.store_intermediate_application_display_alias(inner_app, outer_app, evaluated, &[one]);

    assert_eq!(
        interner.get_display_alias(inner_app),
        None,
        "Pre-existing instantiated applications should not be globally repainted"
    );
}

#[test]
fn intermediate_application_alias_preserves_newly_introduced_intermediate() {
    let interner = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();
    let type_param = |name: &str| TypeParamInfo {
        name: interner.intern_string(name),
        constraint: None,
        default: None,
        is_const: false,
    };

    let inner_def = def_store.register(crate::def::DefinitionInfo::type_alias(
        interner.intern_string("Inner"),
        vec![type_param("T")],
        TypeId::UNKNOWN,
    ));
    let outer_def = def_store.register(crate::def::DefinitionInfo::type_alias(
        interner.intern_string("Outer"),
        vec![type_param("T")],
        TypeId::UNKNOWN,
    ));
    let one = interner.literal_number(1.0);

    // Outer exists first; Inner<1> is introduced later as an intermediate.
    let outer_app = interner.application(interner.lazy(outer_def), vec![one]);
    let inner_app = interner.application(interner.lazy(inner_def), vec![one]);
    let evaluated = interner.object(vec![PropertyInfo::new(
        interner.intern_string("p"),
        TypeId::NUMBER,
    )]);

    let evaluator = TypeEvaluator::new(&interner);
    evaluator.store_intermediate_application_display_alias(inner_app, outer_app, evaluated, &[one]);

    assert_eq!(
        interner.get_display_alias(inner_app),
        Some(outer_app),
        "Fresh intermediate applications should still carry the forward alias"
    );
}

/// When `store_intermediate_application_display_alias` is called with a
/// freshly-allocated Mapped type as `evaluated`, the display alias must be
/// stored even when the Application's args contain generic type parameters.
///
/// Structural rule: a generic type alias whose body evaluates to a fresh
/// `MappedType` (constraint baked into interned key → each instantiation
/// produces a distinct node) gets its `Application → MappedType` alias stored
/// so that `IndexAccess(MappedType, idx)` formats as `Alias<K>[idx]`.
#[test]
fn intermediate_application_alias_stores_for_fresh_generic_mapped_type() {
    let interner = TypeInterner::new();

    let k = interner.type_param(TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    });

    // Application is allocated first (as it would be when `Alias<K>` appears in source).
    let app = interner.application(interner.lazy(DefId(9901)), vec![k]);

    // MappedType is allocated second — simulating `instantiate_generic` producing a fresh node.
    let p = interner.type_param(TypeParamInfo::simple(interner.intern_string("P")));
    let prefix = interner.intern_string("get");
    let name_type = interner.template_literal(vec![
        crate::types::TemplateSpan::Text(prefix),
        crate::types::TemplateSpan::Type(p),
    ]);
    let prop = interner.intern_string("a");
    let template = interner.object(vec![PropertyInfo::new(prop, p)]);
    let mapped = interner.mapped(MappedType {
        type_param: TypeParamInfo::simple(interner.intern_string("P")),
        constraint: k,
        template,
        name_type: Some(name_type),
        readonly_modifier: None,
        optional_modifier: None,
    });

    // Call the function under test directly — no pre-seeding via store_display_alias.
    let evaluator = TypeEvaluator::new(&interner);
    evaluator.store_intermediate_application_display_alias(mapped, app, mapped, &[k]);

    assert_eq!(
        interner.get_display_alias(mapped),
        Some(app),
        "Generic alias evaluating to a fresh Mapped type must have its display alias stored"
    );
}

/// Non-Mapped structural types (Object, Intersection) must NEVER receive a
/// display alias when the Application's args contain generic type parameters.
/// Only freshly-allocated Mapped types are safe because their constraint is
/// baked into the interned key (guaranteeing per-instantiation uniqueness).
/// Both shapes are tested so a future change that widens alias storage to
/// generic Objects or Intersections trips the boundary assertion.
#[test]
fn intermediate_application_alias_skips_generic_args_for_non_mapped_structural_type() {
    let interner = TypeInterner::new();

    let k = interner.type_param(TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: None,
        default: None,
        is_const: false,
    });

    // --- Object shape ---
    let app_obj = interner.application(interner.lazy(DefId(9902)), vec![k]);
    let obj = interner.object(vec![PropertyInfo::new(interner.intern_string("x"), k)]);

    let evaluator = TypeEvaluator::new(&interner);
    evaluator.store_intermediate_application_display_alias(obj, app_obj, obj, &[k]);

    assert_eq!(
        interner.get_display_alias(obj),
        None,
        "Object with generic args must not receive a display alias"
    );

    // --- Intersection shape ---
    let app_int = interner.application(interner.lazy(DefId(9909)), vec![k]);
    let intersect = interner.intersection(vec![TypeId::STRING, k]);

    evaluator.store_intermediate_application_display_alias(intersect, app_int, intersect, &[k]);

    assert_eq!(
        interner.get_display_alias(intersect),
        None,
        "Intersection with generic args must not receive a display alias"
    );
}

/// Prove the formatter uses the evaluator-stored alias (no manual pre-seeding).
/// `store_intermediate_application_display_alias` stores `mapped → app`, and
/// then `format(IndexAccess(mapped, idx))` must use the Application form.
#[test]
fn evaluator_stored_mapped_alias_appears_in_index_access_format() {
    use crate::diagnostics::format::TypeFormatter;

    let interner = TypeInterner::new();

    let k = interner.type_param(TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    });

    // Allocation order: Application first, then fresh Mapped.
    let app = interner.application(interner.lazy(DefId(9903)), vec![k]);

    let p = interner.type_param(TypeParamInfo::simple(interner.intern_string("P")));
    let prefix = interner.intern_string("get");
    let name_type = interner.template_literal(vec![
        crate::types::TemplateSpan::Text(prefix),
        crate::types::TemplateSpan::Type(p),
    ]);
    let prop = interner.intern_string("a");
    let template = interner.object(vec![PropertyInfo::new(prop, p)]);
    let mapped = interner.mapped(MappedType {
        type_param: TypeParamInfo::simple(interner.intern_string("P")),
        constraint: k,
        template,
        name_type: Some(name_type),
        readonly_modifier: None,
        optional_modifier: None,
    });

    // Store via the evaluator path — not via store_display_alias_preferring_application.
    let evaluator = TypeEvaluator::new(&interner);
    evaluator.store_intermediate_application_display_alias(mapped, app, mapped, &[k]);

    // Build the IndexAccess and format it.
    let idx = interner.template_literal(vec![
        crate::types::TemplateSpan::Text(prefix),
        crate::types::TemplateSpan::Type(k),
    ]);
    let access = interner.index_access(mapped, idx);

    let mut fmt = TypeFormatter::new(&interner);
    let result = fmt.format(access);

    // The alias was stored by the evaluator, so the formatter must not show the
    // expanded `{ [P in K as ...]: ... }[...]` structural form.
    assert!(
        !result.contains("[P in K as"),
        "formatter must use the evaluator-stored alias, not the expanded mapped form; got: {result}"
    );
}

/// Tests for distributive conditional instantiation over union type parameters.
///
/// Structural rule: when a distributive conditional `K extends K ? K : never`
/// is instantiated with K=1|2, the `TypeInstantiator` distributes K over the
/// union members, producing a union of evaluated conditionals.
#[test]
fn test_distributive_conditional_over_union_evaluates_correctly() {
    let interner = TypeInterner::new();

    let k_name = interner.intern_string("K");

    let k_param = interner.type_param(TypeParamInfo {
        name: k_name,
        constraint: None,
        default: None,
        is_const: false,
    });

    let lit1 = interner.literal_number(1.0);
    let lit2 = interner.literal_number(2.0);
    let union_1_2 = interner.union(vec![lit1, lit2]);

    // Conditional: K extends K ? K : never  (distributive, trivially true)
    let k_extends_k = interner.conditional(ConditionalType {
        check_type: k_param,
        extends_type: k_param,
        true_type: k_param,
        false_type: TypeId::NEVER,
        is_distributive: true,
    });

    // Instantiate with {K → 1|2} — should distribute and produce 1|2
    let k_type_params = vec![TypeParamInfo {
        name: k_name,
        constraint: None,
        default: None,
        is_const: false,
    }];
    let k_args = vec![union_1_2];

    let result = instantiate_generic(&interner, k_extends_k, &k_type_params, &k_args);
    let evaluated = evaluate_type(&interner, result);

    // Each union member passes `K extends K`, so the evaluated result is the original union
    assert!(
        matches!(interner.lookup(evaluated), Some(TypeData::Union(_))),
        "Expected union from distributive K extends K ? K : never with K=1|2, got: {:?}",
        interner.lookup(evaluated)
    );
}

/// Tests with renamed type parameter (X instead of K) to prove the fix
/// is structural, not dependent on parameter name spelling.
#[test]
fn test_distributive_conditional_renamed_param_evaluates_correctly() {
    let interner = TypeInterner::new();

    let x_name = interner.intern_string("X");
    let x_param = interner.type_param(TypeParamInfo {
        name: x_name,
        constraint: None,
        default: None,
        is_const: false,
    });

    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let union_ab = interner.union(vec![lit_a, lit_b]);

    // Conditional: X extends X ? X : never  (distributive)
    let x_extends_x = interner.conditional(ConditionalType {
        check_type: x_param,
        extends_type: x_param,
        true_type: x_param,
        false_type: TypeId::NEVER,
        is_distributive: true,
    });

    let x_type_params = vec![TypeParamInfo {
        name: x_name,
        constraint: None,
        default: None,
        is_const: false,
    }];
    let x_args = vec![union_ab];

    let result = instantiate_generic(&interner, x_extends_x, &x_type_params, &x_args);
    let evaluated = evaluate_type(&interner, result);

    assert!(
        matches!(interner.lookup(evaluated), Some(TypeData::Union(_))),
        "Expected union from distributive X extends X ? X : never with X='a'|'b', got: {:?}",
        interner.lookup(evaluated)
    );
}

// --- Spreading a union of tuples into a tuple distributes (issue #9764) ---
//
// Structural rule: when a tuple type contains a spread (rest) element whose
// operand is a union of array-like types, the tuple normalizer distributes
// over the union — `[a, ...(X | Y), b]` becomes `[a, ...X, b] | [a, ...Y, b]`
// — producing one concrete tuple per union member. Single (non-union) tuple
// spreads already flatten; this extends that to union operands.

fn fixed_elem(type_id: TypeId) -> TupleElement {
    TupleElement {
        type_id,
        name: None,
        optional: false,
        rest: false,
    }
}

fn rest_elem(type_id: TypeId) -> TupleElement {
    TupleElement {
        type_id,
        name: None,
        optional: false,
        rest: true,
    }
}

#[test]
fn test_tuple_spread_union_distributes_with_trailing() {
    // [0, ...([2] | [3, 4]), 1] -> [0, 2, 1] | [0, 3, 4, 1]
    let interner = TypeInterner::new();
    let (l0, l1, l2, l3, l4) = (
        interner.literal_number(0.0),
        interner.literal_number(1.0),
        interner.literal_number(2.0),
        interner.literal_number(3.0),
        interner.literal_number(4.0),
    );

    let t_a = interner.tuple(vec![fixed_elem(l2)]);
    let t_b = interner.tuple(vec![fixed_elem(l3), fixed_elem(l4)]);
    let union = interner.union(vec![t_a, t_b]);

    let src = interner.tuple(vec![fixed_elem(l0), rest_elem(union), fixed_elem(l1)]);
    let result = evaluate_type(&interner, src);

    let exp_a = interner.tuple(vec![fixed_elem(l0), fixed_elem(l2), fixed_elem(l1)]);
    let exp_b = interner.tuple(vec![
        fixed_elem(l0),
        fixed_elem(l3),
        fixed_elem(l4),
        fixed_elem(l1),
    ]);
    let expected = interner.union(vec![exp_a, exp_b]);

    assert_eq!(
        result,
        expected,
        "expected [0,2,1] | [0,3,4,1], got {:?}",
        interner.lookup(result)
    );
}

#[test]
fn test_tuple_spread_union_distributes_no_trailing_keeps_literal() {
    // [0, ...([2] | [3, 4])] -> [0, 2] | [0, 3, 4]; leading `0` stays literal.
    let interner = TypeInterner::new();
    let (l0, l2, l3, l4) = (
        interner.literal_number(0.0),
        interner.literal_number(2.0),
        interner.literal_number(3.0),
        interner.literal_number(4.0),
    );

    let t_a = interner.tuple(vec![fixed_elem(l2)]);
    let t_b = interner.tuple(vec![fixed_elem(l3), fixed_elem(l4)]);
    let union = interner.union(vec![t_a, t_b]);

    let src = interner.tuple(vec![fixed_elem(l0), rest_elem(union)]);
    let result = evaluate_type(&interner, src);

    let exp_a = interner.tuple(vec![fixed_elem(l0), fixed_elem(l2)]);
    let exp_b = interner.tuple(vec![fixed_elem(l0), fixed_elem(l3), fixed_elem(l4)]);
    let expected = interner.union(vec![exp_a, exp_b]);

    assert_eq!(
        result,
        expected,
        "expected [0,2] | [0,3,4] with literal 0 preserved, got {:?}",
        interner.lookup(result)
    );
}

#[test]
fn test_tuple_spread_single_tuple_still_flattens_control() {
    // CONTROL: [0, ...[2, 3], 1] -> [0, 2, 3, 1] (single tuple, not a union).
    let interner = TypeInterner::new();
    let (l0, l1, l2, l3) = (
        interner.literal_number(0.0),
        interner.literal_number(1.0),
        interner.literal_number(2.0),
        interner.literal_number(3.0),
    );

    let inner = interner.tuple(vec![fixed_elem(l2), fixed_elem(l3)]);
    let src = interner.tuple(vec![fixed_elem(l0), rest_elem(inner), fixed_elem(l1)]);
    let result = evaluate_type(&interner, src);

    let expected = interner.tuple(vec![
        fixed_elem(l0),
        fixed_elem(l2),
        fixed_elem(l3),
        fixed_elem(l1),
    ]);
    assert_eq!(
        result,
        expected,
        "expected single tuple [0,2,3,1], got {:?}",
        interner.lookup(result)
    );
}

#[test]
fn test_tuple_spread_union_member_with_rest_preserved() {
    // [0, ...(["x"] | [number, ...string[]])] ->
    //   [0, "x"] | [0, number, ...string[]]
    // A union member that itself carries a rest keeps its rest after the
    // spread distributes. (The leading element of the first member is a
    // string literal so neither alternative is absorbed by the other.)
    let interner = TypeInterner::new();
    let l0 = interner.literal_number(0.0);
    let lx = interner.literal_string("x");
    let string_array = interner.array(TypeId::STRING);

    let t_a = interner.tuple(vec![fixed_elem(lx)]);
    let t_b = interner.tuple(vec![fixed_elem(TypeId::NUMBER), rest_elem(string_array)]);
    let union = interner.union(vec![t_a, t_b]);

    let src = interner.tuple(vec![fixed_elem(l0), rest_elem(union)]);
    let result = evaluate_type(&interner, src);

    let exp_a = interner.tuple(vec![fixed_elem(l0), fixed_elem(lx)]);
    let exp_b = interner.tuple(vec![
        fixed_elem(l0),
        fixed_elem(TypeId::NUMBER),
        rest_elem(string_array),
    ]);
    let expected = interner.union(vec![exp_a, exp_b]);

    assert_eq!(
        result,
        expected,
        "expected [0, \"x\"] | [0, number, ...string[]], got {:?}",
        interner.lookup(result)
    );
}

#[test]
fn test_tuple_two_union_spreads_cartesian_product() {
    // [...([0] | [1]), ...([2] | [3])] ->
    //   [0, 2] | [0, 3] | [1, 2] | [1, 3]
    let interner = TypeInterner::new();
    let (l0, l1, l2, l3) = (
        interner.literal_number(0.0),
        interner.literal_number(1.0),
        interner.literal_number(2.0),
        interner.literal_number(3.0),
    );

    let u_left = interner.union(vec![
        interner.tuple(vec![fixed_elem(l0)]),
        interner.tuple(vec![fixed_elem(l1)]),
    ]);
    let u_right = interner.union(vec![
        interner.tuple(vec![fixed_elem(l2)]),
        interner.tuple(vec![fixed_elem(l3)]),
    ]);

    let src = interner.tuple(vec![rest_elem(u_left), rest_elem(u_right)]);
    let result = evaluate_type(&interner, src);

    let expected = interner.union(vec![
        interner.tuple(vec![fixed_elem(l0), fixed_elem(l2)]),
        interner.tuple(vec![fixed_elem(l0), fixed_elem(l3)]),
        interner.tuple(vec![fixed_elem(l1), fixed_elem(l2)]),
        interner.tuple(vec![fixed_elem(l1), fixed_elem(l3)]),
    ]);

    assert_eq!(
        result,
        expected,
        "expected cartesian [0,2]|[0,3]|[1,2]|[1,3], got {:?}",
        interner.lookup(result)
    );
}

#[test]
fn test_tuple_spread_array_union_is_not_distributed() {
    // NEGATIVE/FALLBACK: [string, number, ...(string[] | boolean[])] is left
    // undistributed (a single tuple). tsc keeps a union-of-arrays rest as one
    // rest element — an unbounded rest already encodes the union without
    // fanning out into a union of tuples. Distributing it (as an earlier
    // iteration did) broke reverse-mapping inference through variadic tuples
    // (TypeScript conformance `variadicTuples1.ts`).
    let interner = TypeInterner::new();
    let string_array = interner.array(TypeId::STRING);
    let boolean_array = interner.array(TypeId::BOOLEAN);
    let union = interner.union(vec![string_array, boolean_array]);

    let src = interner.tuple(vec![
        fixed_elem(TypeId::STRING),
        fixed_elem(TypeId::NUMBER),
        rest_elem(union),
    ]);
    let result = evaluate_type(&interner, src);

    assert_eq!(
        result,
        src,
        "array-union spread must stay undistributed, got {:?}",
        interner.lookup(result)
    );
    assert!(
        matches!(interner.lookup(result), Some(TypeData::Tuple(_))),
        "expected a single tuple, got {:?}",
        interner.lookup(result)
    );
}

#[test]
fn test_tuple_spread_generic_union_is_not_distributed() {
    // NEGATIVE/FALLBACK: [0, ...(T | U)] with T, U generic type parameters is
    // left undistributed (a single tuple), matching tsc, which keeps generic
    // spreads lazy until instantiation. Only concrete array-like unions fan out.
    let interner = TypeInterner::new();
    let l0 = interner.literal_number(0.0);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let u_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let union = interner.union(vec![t_param, u_param]);

    let src = interner.tuple(vec![fixed_elem(l0), rest_elem(union)]);
    let result = evaluate_type(&interner, src);

    assert_eq!(
        result,
        src,
        "generic type-parameter union spread must stay undistributed, got {:?}",
        interner.lookup(result)
    );
    assert!(
        matches!(interner.lookup(result), Some(TypeData::Tuple(_))),
        "expected a single tuple, got {:?}",
        interner.lookup(result)
    );
}
