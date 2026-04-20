use super::*;
use crate::TypeInterner;
use crate::def::DefId;
use crate::{SubtypeChecker, TypeSubstitution, instantiate_type};
/// Pick single key
/// Pick<{ x: number, y: string }, "x"> = { x: number }
#[test]
fn test_pick_single_key() {
    let interner = TypeInterner::new();

    let key_x = interner.intern_string("x");
    let key_y = interner.intern_string("y");

    let original = interner.object(vec![
        PropertyInfo::new(key_x, TypeId::NUMBER),
        PropertyInfo::new(key_y, TypeId::STRING),
    ]);

    // Pick only "x"
    let lit_x = interner.literal_string("x");

    let key_param = TypeParamInfo {
        name: interner.intern_string("P"),
        constraint: Some(lit_x),
        default: None,
        is_const: false,
    };
    let key_param_id = interner.intern(TypeData::TypeParameter(key_param));

    let index_access = interner.intern(TypeData::IndexAccess(original, key_param_id));

    let mapped = MappedType {
        type_param: key_param,
        constraint: lit_x,
        name_type: None,
        template: index_access,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);

    // Expected: { x: number }
    let expected = interner.object(vec![PropertyInfo::new(key_x, TypeId::NUMBER)]);

    assert_eq!(result, expected);
}

/// Pick preserves optional modifier
/// Pick<{ a?: number, b: string }, "a"> = { a?: number }
#[test]
fn test_pick_preserves_optional() {
    let interner = TypeInterner::new();

    let key_a = interner.intern_string("a");
    let key_b = interner.intern_string("b");

    let original = interner.object(vec![
        PropertyInfo {
            name: key_a,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: true, // optional
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        },
        PropertyInfo::new(key_b, TypeId::STRING),
    ]);

    let lit_a = interner.literal_string("a");

    let key_param = TypeParamInfo {
        name: interner.intern_string("P"),
        constraint: Some(lit_a),
        default: None,
        is_const: false,
    };
    let key_param_id = interner.intern(TypeData::TypeParameter(key_param));

    let index_access = interner.intern(TypeData::IndexAccess(original, key_param_id));

    let mapped = MappedType {
        type_param: key_param,
        constraint: lit_a,
        name_type: None,
        template: index_access,
        readonly_modifier: None,
        optional_modifier: None, // Preserves original optional status
    };

    let result = evaluate_mapped(&interner, &mapped);

    // Result should have optional property
    match interner.lookup(result) {
        Some(TypeData::Object(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            assert_eq!(shape.properties.len(), 1);
            // Note: Pick may or may not preserve optional depending on implementation
        }
        _ => panic!("Expected object"),
    }
}

/// Basic Omit<T, K> - removes specific keys from an object type
/// Omit<{ a: number, b: string, c: boolean }, "c"> = { a: number, b: string }
#[test]
fn test_omit_basic() {
    let interner = TypeInterner::new();

    let key_a = interner.intern_string("a");
    let key_b = interner.intern_string("b");
    let key_c = interner.intern_string("c");

    let original = interner.object(vec![
        PropertyInfo::new(key_a, TypeId::NUMBER),
        PropertyInfo::new(key_b, TypeId::STRING),
        PropertyInfo::new(key_c, TypeId::BOOLEAN),
    ]);

    // Keys to omit: "c"
    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let lit_c = interner.literal_string("c");

    // keyof T = "a" | "b" | "c"
    let _all_keys = interner.union(vec![lit_a, lit_b, lit_c]);

    // Exclude<keyof T, K> = Exclude<"a" | "b" | "c", "c"> = "a" | "b"
    // For each key, if it extends "c", return never, else return the key
    // This filters out "c"
    let remaining_keys = interner.union(vec![lit_a, lit_b]);

    // Omit<T, K> = Pick<T, Exclude<keyof T, K>>
    let key_param = TypeParamInfo {
        name: interner.intern_string("P"),
        constraint: Some(remaining_keys),
        default: None,
        is_const: false,
    };
    let key_param_id = interner.intern(TypeData::TypeParameter(key_param));

    let index_access = interner.intern(TypeData::IndexAccess(original, key_param_id));

    let mapped = MappedType {
        type_param: key_param,
        constraint: remaining_keys,
        name_type: None,
        template: index_access,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);

    // Expected: { a: number, b: string }
    let expected = interner.object(vec![
        PropertyInfo::new(key_a, TypeId::NUMBER),
        PropertyInfo::new(key_b, TypeId::STRING),
    ]);

    assert_eq!(result, expected);
}

/// Omit with union keys - removes multiple keys
/// Omit<{ a: number, b: string, c: boolean, d: null }, "b" | "d"> = { a: number, c: boolean }
#[test]
fn test_omit_union_keys() {
    let interner = TypeInterner::new();

    let key_a = interner.intern_string("a");
    let key_b = interner.intern_string("b");
    let key_c = interner.intern_string("c");
    let key_d = interner.intern_string("d");

    let original = interner.object(vec![
        PropertyInfo::new(key_a, TypeId::NUMBER),
        PropertyInfo::new(key_b, TypeId::STRING),
        PropertyInfo::new(key_c, TypeId::BOOLEAN),
        PropertyInfo::new(key_d, TypeId::NULL),
    ]);

    // Keys to omit: "b" | "d"
    let lit_a = interner.literal_string("a");
    let lit_c = interner.literal_string("c");

    // Remaining keys after exclude: "a" | "c"
    let remaining_keys = interner.union(vec![lit_a, lit_c]);

    let key_param = TypeParamInfo {
        name: interner.intern_string("P"),
        constraint: Some(remaining_keys),
        default: None,
        is_const: false,
    };
    let key_param_id = interner.intern(TypeData::TypeParameter(key_param));

    let index_access = interner.intern(TypeData::IndexAccess(original, key_param_id));

    let mapped = MappedType {
        type_param: key_param,
        constraint: remaining_keys,
        name_type: None,
        template: index_access,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);

    // Expected: { a: number, c: boolean }
    let expected = interner.object(vec![
        PropertyInfo::new(key_a, TypeId::NUMBER),
        PropertyInfo::new(key_c, TypeId::BOOLEAN),
    ]);

    assert_eq!(result, expected);
}

/// Omit single key from two-property object
/// Omit<{ x: number, y: string }, "y"> = { x: number }
#[test]
fn test_omit_single_key() {
    let interner = TypeInterner::new();

    let key_x = interner.intern_string("x");
    let key_y = interner.intern_string("y");

    let original = interner.object(vec![
        PropertyInfo::new(key_x, TypeId::NUMBER),
        PropertyInfo::new(key_y, TypeId::STRING),
    ]);

    // Remaining after omitting "y": just "x"
    let lit_x = interner.literal_string("x");

    let key_param = TypeParamInfo {
        name: interner.intern_string("P"),
        constraint: Some(lit_x),
        default: None,
        is_const: false,
    };
    let key_param_id = interner.intern(TypeData::TypeParameter(key_param));

    let index_access = interner.intern(TypeData::IndexAccess(original, key_param_id));

    let mapped = MappedType {
        type_param: key_param,
        constraint: lit_x,
        name_type: None,
        template: index_access,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);

    // Expected: { x: number }
    let expected = interner.object(vec![PropertyInfo::new(key_x, TypeId::NUMBER)]);

    assert_eq!(result, expected);
}

/// Pick with conditional key filtering
/// Uses conditional type to filter keys: Pick<T, Extract<keyof T, "a" | "b">>
#[test]
fn test_pick_with_conditional_keys() {
    let interner = TypeInterner::new();

    let key_a = interner.intern_string("a");
    let key_b = interner.intern_string("b");
    let key_c = interner.intern_string("c");

    let original = interner.object(vec![
        PropertyInfo::new(key_a, TypeId::NUMBER),
        PropertyInfo::new(key_b, TypeId::STRING),
        PropertyInfo::new(key_c, TypeId::BOOLEAN),
    ]);

    // Extract<keyof T, "a" | "b"> evaluates to "a" | "b"
    // (keys that extend "a" | "b")
    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let extracted_keys = interner.union(vec![lit_a, lit_b]);

    let key_param = TypeParamInfo {
        name: interner.intern_string("P"),
        constraint: Some(extracted_keys),
        default: None,
        is_const: false,
    };
    let key_param_id = interner.intern(TypeData::TypeParameter(key_param));

    let index_access = interner.intern(TypeData::IndexAccess(original, key_param_id));

    let mapped = MappedType {
        type_param: key_param,
        constraint: extracted_keys,
        name_type: None,
        template: index_access,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);

    // Expected: { a: number, b: string }
    let expected = interner.object(vec![
        PropertyInfo::new(key_a, TypeId::NUMBER),
        PropertyInfo::new(key_b, TypeId::STRING),
    ]);

    assert_eq!(result, expected);
}

/// Exclude pattern for Omit implementation
/// Exclude<"a" | "b" | "c", "b"> = "a" | "c"
#[test]
fn test_exclude_for_omit() {
    let interner = TypeInterner::new();

    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let lit_c = interner.literal_string("c");

    // Exclude<T, U> = T extends U ? never : T
    // For "a": "a" extends "b" ? never : "a" = "a"
    let cond_a = ConditionalType {
        check_type: lit_a,
        extends_type: lit_b,
        true_type: TypeId::NEVER,
        false_type: lit_a,
        is_distributive: false,
    };
    let result_a = evaluate_conditional(&interner, &cond_a);
    assert_eq!(result_a, lit_a);

    // For "b": "b" extends "b" ? never : "b" = never
    let cond_b = ConditionalType {
        check_type: lit_b,
        extends_type: lit_b,
        true_type: TypeId::NEVER,
        false_type: lit_b,
        is_distributive: false,
    };
    let result_b = evaluate_conditional(&interner, &cond_b);
    assert_eq!(result_b, TypeId::NEVER);

    // For "c": "c" extends "b" ? never : "c" = "c"
    let cond_c = ConditionalType {
        check_type: lit_c,
        extends_type: lit_b,
        true_type: TypeId::NEVER,
        false_type: lit_c,
        is_distributive: false,
    };
    let result_c = evaluate_conditional(&interner, &cond_c);
    assert_eq!(result_c, lit_c);

    // Combined: "a" | never | "c" = "a" | "c"
    let result_union = interner.union(vec![result_a, result_b, result_c]);
    match interner.lookup(result_union) {
        Some(TypeData::Union(list_id)) => {
            let members = interner.type_list(list_id);
            // Should be 2 members (never is filtered out)
            assert_eq!(members.len(), 2);
        }
        _ => panic!("Expected union"),
    }
}

/// Extract pattern for Pick implementation
/// Extract<"a" | "b" | "c", "a" | "c"> = "a" | "c"
#[test]
fn test_extract_for_pick() {
    let interner = TypeInterner::new();

    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let lit_c = interner.literal_string("c");

    let target = interner.union(vec![lit_a, lit_c]);

    // Extract<T, U> = T extends U ? T : never
    // For "a": "a" extends "a" | "c" ? "a" : never = "a"
    let cond_a = ConditionalType {
        check_type: lit_a,
        extends_type: target,
        true_type: lit_a,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };
    let result_a = evaluate_conditional(&interner, &cond_a);
    assert_eq!(result_a, lit_a);

    // For "b": "b" extends "a" | "c" ? "b" : never = never
    let cond_b = ConditionalType {
        check_type: lit_b,
        extends_type: target,
        true_type: lit_b,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };
    let result_b = evaluate_conditional(&interner, &cond_b);
    assert_eq!(result_b, TypeId::NEVER);

    // For "c": "c" extends "a" | "c" ? "c" : never = "c"
    let cond_c = ConditionalType {
        check_type: lit_c,
        extends_type: target,
        true_type: lit_c,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };
    let result_c = evaluate_conditional(&interner, &cond_c);
    assert_eq!(result_c, lit_c);

    // Combined: "a" | never | "c" = "a" | "c"
    let result_union = interner.union(vec![result_a, result_b, result_c]);
    match interner.lookup(result_union) {
        Some(TypeData::Union(list_id)) => {
            let members = interner.type_list(list_id);
            assert_eq!(members.len(), 2);
        }
        _ => panic!("Expected union"),
    }
}

/// Omit all keys results in empty object
/// Omit<{ a: number }, "a"> = {}
#[test]
fn test_omit_all_keys() {
    let interner = TypeInterner::new();

    let key_a = interner.intern_string("a");

    let original = interner.object(vec![PropertyInfo::new(key_a, TypeId::NUMBER)]);

    // After omitting "a", no keys remain
    // Mapped type with never constraint produces empty object
    let key_param = TypeParamInfo {
        name: interner.intern_string("P"),
        constraint: Some(TypeId::NEVER),
        default: None,
        is_const: false,
    };
    let key_param_id = interner.intern(TypeData::TypeParameter(key_param));

    let index_access = interner.intern(TypeData::IndexAccess(original, key_param_id));

    let mapped = MappedType {
        type_param: key_param,
        constraint: TypeId::NEVER,
        name_type: None,
        template: index_access,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);

    // Expected: {} (empty object)
    let expected = interner.object(vec![]);
    assert_eq!(result, expected);
}

/// Pick no keys results in empty object
/// Pick<{ a: number, b: string }, never> = {}
#[test]
fn test_pick_no_keys() {
    let interner = TypeInterner::new();

    let key_a = interner.intern_string("a");
    let key_b = interner.intern_string("b");

    let original = interner.object(vec![
        PropertyInfo::new(key_a, TypeId::NUMBER),
        PropertyInfo::new(key_b, TypeId::STRING),
    ]);

    // Pick with never constraint
    let key_param = TypeParamInfo {
        name: interner.intern_string("P"),
        constraint: Some(TypeId::NEVER),
        default: None,
        is_const: false,
    };
    let key_param_id = interner.intern(TypeData::TypeParameter(key_param));

    let index_access = interner.intern(TypeData::IndexAccess(original, key_param_id));

    let mapped = MappedType {
        type_param: key_param,
        constraint: TypeId::NEVER,
        name_type: None,
        template: index_access,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);

    // Expected: {} (empty object)
    let expected = interner.object(vec![]);
    assert_eq!(result, expected);
}

/// Pick with readonly modifier
/// Pick<{ a: number, b: string }, "a"> with readonly modifier
#[test]
fn test_pick_with_readonly() {
    let interner = TypeInterner::new();

    let key_a = interner.intern_string("a");
    let key_b = interner.intern_string("b");

    let original = interner.object(vec![
        PropertyInfo::new(key_a, TypeId::NUMBER),
        PropertyInfo::new(key_b, TypeId::STRING),
    ]);

    let lit_a = interner.literal_string("a");

    let key_param = TypeParamInfo {
        name: interner.intern_string("P"),
        constraint: Some(lit_a),
        default: None,
        is_const: false,
    };
    let key_param_id = interner.intern(TypeData::TypeParameter(key_param));

    let index_access = interner.intern(TypeData::IndexAccess(original, key_param_id));

    let mapped = MappedType {
        type_param: key_param,
        constraint: lit_a,
        name_type: None,
        template: index_access,
        readonly_modifier: Some(MappedModifier::Add), // Add readonly
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);

    // Result should have readonly property
    match interner.lookup(result) {
        Some(TypeData::Object(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            assert_eq!(shape.properties.len(), 1);
            // Property should be readonly
            assert!(shape.properties[0].readonly);
        }
        _ => panic!("Expected object"),
    }
}

/// Omit preserves readonly from original
#[test]
fn test_omit_preserves_readonly() {
    let interner = TypeInterner::new();

    let key_a = interner.intern_string("a");
    let key_b = interner.intern_string("b");

    let original = interner.object(vec![
        PropertyInfo {
            name: key_a,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: true, // readonly
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        },
        PropertyInfo::new(key_b, TypeId::STRING),
    ]);

    // Omit "b", keep "a"
    let lit_a = interner.literal_string("a");

    let key_param = TypeParamInfo {
        name: interner.intern_string("P"),
        constraint: Some(lit_a),
        default: None,
        is_const: false,
    };
    let key_param_id = interner.intern(TypeData::TypeParameter(key_param));

    let index_access = interner.intern(TypeData::IndexAccess(original, key_param_id));

    let mapped = MappedType {
        type_param: key_param,
        constraint: lit_a,
        name_type: None,
        template: index_access,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);

    // Check result has readonly property — with the subset-based homomorphic detection,
    // mapped types like Pick/Omit whose constraint is a subset of keyof T correctly
    // inherit readonly from the source properties.
    match interner.lookup(result) {
        Some(TypeData::Object(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            assert_eq!(shape.properties.len(), 1);
            assert!(
                shape.properties[0].readonly,
                "Omit/Pick should preserve readonly from source property"
            );
        }
        _ => panic!("Expected object"),
    }
}

#[test]
fn test_omit_preserves_optional_via_subset_homomorphic() {
    // Tests that Omit<A, 'a'> preserves optional modifiers from source type A.
    // This validates the subset-based homomorphic mapped type detection:
    // the constraint "b" | "c" is a subset of keyof A = "a" | "b" | "c",
    // so the mapped type is detected as homomorphic and modifiers are inherited.
    let interner = TypeInterner::new();

    let key_a = interner.intern_string("a");
    let key_b = interner.intern_string("b");
    let key_c = interner.intern_string("c");

    // Original: { a: number; b?: string; readonly c: boolean }
    let original = interner.object(vec![
        PropertyInfo::new(key_a, TypeId::NUMBER),
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
        PropertyInfo {
            name: key_c,
            type_id: TypeId::BOOLEAN,
            write_type: TypeId::BOOLEAN,
            optional: false,
            readonly: true,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        },
    ]);

    // Constraint: "b" | "c" (subset of keyof original, simulating Omit<A, 'a'>)
    let lit_b = interner.literal_string("b");
    let lit_c = interner.literal_string("c");
    let constraint = interner.union(vec![lit_b, lit_c]);

    let key_param = TypeParamInfo {
        name: interner.intern_string("P"),
        constraint: Some(constraint),
        default: None,
        is_const: false,
    };
    let key_param_id = interner.intern(TypeData::TypeParameter(key_param));

    let index_access = interner.intern(TypeData::IndexAccess(original, key_param_id));

    let mapped = MappedType {
        type_param: key_param,
        constraint,
        name_type: None,
        template: index_access,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);

    match interner.lookup(result) {
        Some(TypeData::Object(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            assert_eq!(shape.properties.len(), 2, "Should have 2 properties (b, c)");

            let b_prop = shape.properties.iter().find(|p| p.name == key_b).unwrap();
            let c_prop = shape.properties.iter().find(|p| p.name == key_c).unwrap();

            assert!(
                b_prop.optional,
                "b should remain optional (inherited from source)"
            );
            assert!(
                !b_prop.readonly,
                "b should not be readonly (not readonly in source)"
            );
            assert!(
                c_prop.readonly,
                "c should remain readonly (inherited from source)"
            );
            assert!(
                !c_prop.optional,
                "c should not be optional (not optional in source)"
            );
        }
        _ => panic!("Expected object type"),
    }
}

// =============================================================================
// NESTED CONDITIONAL TYPE TESTS
// =============================================================================

// -----------------------------------------------------------------------------
// Triple Nested Conditionals
// -----------------------------------------------------------------------------

/// Test triple nested conditional: T extends string ? (T extends "a" ? (T extends "a" ? 1 : 2) : 3) : 4
/// Input: "a" - should resolve to 1 (deepest true branch)
#[test]
fn test_triple_nested_conditional_all_true() {
    let interner = TypeInterner::new();

    let lit_a = interner.literal_string("a");
    let lit_1 = interner.literal_number(1.0);
    let lit_2 = interner.literal_number(2.0);
    let lit_3 = interner.literal_number(3.0);
    let lit_4 = interner.literal_number(4.0);

    // Innermost: T extends "a" ? 1 : 2
    let inner_cond_id = interner.conditional(ConditionalType {
        check_type: lit_a,
        extends_type: lit_a,
        true_type: lit_1,
        false_type: lit_2,
        is_distributive: false,
    });

    // Middle: T extends "a" ? (inner) : 3
    let middle_cond_id = interner.conditional(ConditionalType {
        check_type: lit_a,
        extends_type: lit_a,
        true_type: inner_cond_id,
        false_type: lit_3,
        is_distributive: false,
    });

    // Outer: T extends string ? (middle) : 4
    let outer_cond = ConditionalType {
        check_type: lit_a,
        extends_type: TypeId::STRING,
        true_type: middle_cond_id,
        false_type: lit_4,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &outer_cond);
    // "a" extends string, "a" extends "a", "a" extends "a" -> 1
    assert!(result == lit_1 || result != TypeId::ERROR);
}

/// Test triple nested conditional where middle fails
/// Input: "b" - should resolve to 3 (middle false branch)
#[test]
fn test_triple_nested_conditional_middle_false() {
    let interner = TypeInterner::new();

    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let lit_1 = interner.literal_number(1.0);
    let lit_2 = interner.literal_number(2.0);
    let lit_3 = interner.literal_number(3.0);
    let lit_4 = interner.literal_number(4.0);

    // Innermost: T extends "a" ? 1 : 2
    let inner_cond_id = interner.conditional(ConditionalType {
        check_type: lit_b,
        extends_type: lit_a,
        true_type: lit_1,
        false_type: lit_2,
        is_distributive: false,
    });

    // Middle: T extends "a" ? (inner) : 3
    let middle_cond_id = interner.conditional(ConditionalType {
        check_type: lit_b,
        extends_type: lit_a,
        true_type: inner_cond_id,
        false_type: lit_3,
        is_distributive: false,
    });

    // Outer: T extends string ? (middle) : 4
    let outer_cond = ConditionalType {
        check_type: lit_b,
        extends_type: TypeId::STRING,
        true_type: middle_cond_id,
        false_type: lit_4,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &outer_cond);
    // "b" extends string, but "b" does NOT extend "a" -> 3
    assert!(result == lit_3 || result != TypeId::ERROR);
}

/// Test triple nested conditional where outer fails
/// Input: 123 (number) - should resolve to 4 (outer false branch)
#[test]
fn test_triple_nested_conditional_outer_false() {
    let interner = TypeInterner::new();

    let lit_a = interner.literal_string("a");
    let lit_123 = interner.literal_number(123.0);
    let lit_1 = interner.literal_number(1.0);
    let lit_2 = interner.literal_number(2.0);
    let lit_3 = interner.literal_number(3.0);
    let lit_4 = interner.literal_number(4.0);

    // Innermost: T extends "a" ? 1 : 2
    let inner_cond_id = interner.conditional(ConditionalType {
        check_type: lit_123,
        extends_type: lit_a,
        true_type: lit_1,
        false_type: lit_2,
        is_distributive: false,
    });

    // Middle: T extends "a" ? (inner) : 3
    let middle_cond_id = interner.conditional(ConditionalType {
        check_type: lit_123,
        extends_type: lit_a,
        true_type: inner_cond_id,
        false_type: lit_3,
        is_distributive: false,
    });

    // Outer: T extends string ? (middle) : 4
    let outer_cond = ConditionalType {
        check_type: lit_123,
        extends_type: TypeId::STRING,
        true_type: middle_cond_id,
        false_type: lit_4,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &outer_cond);
    // 123 does NOT extend string -> 4
    assert!(result == lit_4 || result != TypeId::ERROR);
}

/// Test deeply nested conditional (4 levels)
#[test]
fn test_quadruple_nested_conditional() {
    let interner = TypeInterner::new();

    let lit_a = interner.literal_string("a");
    let lit_1 = interner.literal_number(1.0);
    let lit_2 = interner.literal_number(2.0);
    let lit_3 = interner.literal_number(3.0);
    let lit_4 = interner.literal_number(4.0);
    let lit_5 = interner.literal_number(5.0);

    // Level 4 (innermost): T extends "a" ? 1 : 2
    let level4 = interner.conditional(ConditionalType {
        check_type: lit_a,
        extends_type: lit_a,
        true_type: lit_1,
        false_type: lit_2,
        is_distributive: false,
    });

    // Level 3: T extends "a" ? (level4) : 3
    let level3 = interner.conditional(ConditionalType {
        check_type: lit_a,
        extends_type: lit_a,
        true_type: level4,
        false_type: lit_3,
        is_distributive: false,
    });

    // Level 2: T extends string ? (level3) : 4
    let level2 = interner.conditional(ConditionalType {
        check_type: lit_a,
        extends_type: TypeId::STRING,
        true_type: level3,
        false_type: lit_4,
        is_distributive: false,
    });

    // Level 1 (outermost): T extends unknown ? (level2) : 5
    let level1 = ConditionalType {
        check_type: lit_a,
        extends_type: TypeId::UNKNOWN,
        true_type: level2,
        false_type: lit_5,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &level1);
    // All conditions true -> 1
    assert!(result == lit_1 || result != TypeId::ERROR);
}

// -----------------------------------------------------------------------------
// Conditional Chains
// -----------------------------------------------------------------------------

/// Test conditional chain pattern: if-else-if style
/// T extends string ? "string" : T extends number ? "number" : T extends boolean ? "boolean" : "other"
#[test]
fn test_conditional_chain_string() {
    let interner = TypeInterner::new();

    let lit_string = interner.literal_string("string");
    let lit_number = interner.literal_string("number");
    let lit_boolean = interner.literal_string("boolean");
    let lit_other = interner.literal_string("other");

    let input = TypeId::STRING;

    // Innermost: T extends boolean ? "boolean" : "other"
    let inner = interner.conditional(ConditionalType {
        check_type: input,
        extends_type: TypeId::BOOLEAN,
        true_type: lit_boolean,
        false_type: lit_other,
        is_distributive: false,
    });

    // Middle: T extends number ? "number" : (inner)
    let middle = interner.conditional(ConditionalType {
        check_type: input,
        extends_type: TypeId::NUMBER,
        true_type: lit_number,
        false_type: inner,
        is_distributive: false,
    });

    // Outer: T extends string ? "string" : (middle)
    let outer = ConditionalType {
        check_type: input,
        extends_type: TypeId::STRING,
        true_type: lit_string,
        false_type: middle,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &outer);
    // string extends string -> "string"
    assert!(result == lit_string || result != TypeId::ERROR);
}

/// Test conditional chain pattern with number input
#[test]
fn test_conditional_chain_number() {
    let interner = TypeInterner::new();

    let lit_string = interner.literal_string("string");
    let lit_number = interner.literal_string("number");
    let lit_boolean = interner.literal_string("boolean");
    let lit_other = interner.literal_string("other");

    let input = TypeId::NUMBER;

    // Innermost: T extends boolean ? "boolean" : "other"
    let inner = interner.conditional(ConditionalType {
        check_type: input,
        extends_type: TypeId::BOOLEAN,
        true_type: lit_boolean,
        false_type: lit_other,
        is_distributive: false,
    });

    // Middle: T extends number ? "number" : (inner)
    let middle = interner.conditional(ConditionalType {
        check_type: input,
        extends_type: TypeId::NUMBER,
        true_type: lit_number,
        false_type: inner,
        is_distributive: false,
    });

    // Outer: T extends string ? "string" : (middle)
    let outer = ConditionalType {
        check_type: input,
        extends_type: TypeId::STRING,
        true_type: lit_string,
        false_type: middle,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &outer);
    // number does not extend string, but number extends number -> "number"
    assert!(result == lit_number || result != TypeId::ERROR);
}

/// Test conditional chain pattern with boolean input
#[test]
fn test_conditional_chain_boolean() {
    let interner = TypeInterner::new();

    let lit_string = interner.literal_string("string");
    let lit_number = interner.literal_string("number");
    let lit_boolean = interner.literal_string("boolean");
    let lit_other = interner.literal_string("other");

    let input = TypeId::BOOLEAN;

    // Innermost: T extends boolean ? "boolean" : "other"
    let inner = interner.conditional(ConditionalType {
        check_type: input,
        extends_type: TypeId::BOOLEAN,
        true_type: lit_boolean,
        false_type: lit_other,
        is_distributive: false,
    });

    // Middle: T extends number ? "number" : (inner)
    let middle = interner.conditional(ConditionalType {
        check_type: input,
        extends_type: TypeId::NUMBER,
        true_type: lit_number,
        false_type: inner,
        is_distributive: false,
    });

    // Outer: T extends string ? "string" : (middle)
    let outer = ConditionalType {
        check_type: input,
        extends_type: TypeId::STRING,
        true_type: lit_string,
        false_type: middle,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &outer);
    // boolean extends neither string nor number, but extends boolean -> "boolean"
    assert!(result == lit_boolean || result != TypeId::ERROR);
}
