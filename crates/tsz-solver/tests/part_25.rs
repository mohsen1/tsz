use super::*;
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
