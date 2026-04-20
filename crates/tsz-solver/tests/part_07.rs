use super::*;
#[test]
fn test_mapped_type_number_index_signature() {
    let interner = TypeInterner::new();

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: TypeId::NUMBER,
        name_type: None,
        template: TypeId::STRING,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);
    let key = interner.lookup(result).expect("Expected object type");

    match key {
        TypeData::ObjectWithIndex(shape_id) => {
            let shape = interner.object_shape(shape_id);
            assert!(shape.properties.is_empty());
            assert!(shape.string_index.is_none());

            let number_index = shape
                .number_index
                .as_ref()
                .expect("expected number index signature");
            assert_eq!(number_index.key_type, TypeId::NUMBER);
            assert_eq!(number_index.value_type, TypeId::STRING);
            assert!(!number_index.readonly);
        }
        other => panic!("Expected object type, got {other:?}"),
    }
}

#[test]
fn test_mapped_type_single_key() {
    let interner = TypeInterner::new();

    // { [K in "foo"]: string }
    // Should produce { foo: string }
    let key_foo = interner.literal_string("foo");

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: key_foo,
        name_type: None,
        template: TypeId::STRING,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);

    let expected = interner.object(vec![PropertyInfo::new(
        interner.intern_string("foo"),
        TypeId::STRING,
    )]);
    assert_eq!(result, expected);
}

#[test]
fn test_mapped_type_with_optional_modifier() {
    let interner = TypeInterner::new();

    // { [K in "x" | "y"]?: number }
    // Should produce { x?: number, y?: number }
    let key_x = interner.literal_string("x");
    let key_y = interner.literal_string("y");
    let keys = interner.union(vec![key_x, key_y]);

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keys,
        name_type: None,
        template: TypeId::NUMBER,
        readonly_modifier: None,
        optional_modifier: Some(MappedModifier::Add),
    };

    let result = evaluate_mapped(&interner, &mapped);

    // Result should be { x?: number, y?: number }
    let expected = interner.object(vec![
        PropertyInfo::opt(interner.intern_string("x"), TypeId::NUMBER),
        PropertyInfo::opt(interner.intern_string("y"), TypeId::NUMBER),
    ]);
    assert_eq!(result, expected);
}

#[test]
fn test_mapped_type_with_readonly_modifier() {
    let interner = TypeInterner::new();

    // { readonly [K in "x"]: number }
    // Should produce { readonly x: number }
    let key_x = interner.literal_string("x");

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: key_x,
        name_type: None,
        template: TypeId::NUMBER,
        readonly_modifier: Some(MappedModifier::Add),
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);

    let expected = interner.object(vec![PropertyInfo::readonly(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);
    assert_eq!(result, expected);
}

#[test]
fn test_mapped_type_with_template_substitution() {
    let interner = TypeInterner::new();

    // { [K in "x" | "y"]: K }
    // Should produce { x: "x", y: "y" }
    let key_x = interner.literal_string("x");
    let key_y = interner.literal_string("y");
    let keys = interner.union(vec![key_x, key_y]);

    // Template is the type parameter K itself
    let type_param_k = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keys,
        name_type: None,
        template: type_param_k,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);

    // Result should be { x: "x", y: "y" }
    let expected = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), key_x),
        PropertyInfo::new(interner.intern_string("y"), key_y),
    ]);
    assert_eq!(result, expected);
}

#[test]
fn test_mapped_type_key_remap_filters_keys() {
    let interner = TypeInterner::new();

    let prop_a = PropertyInfo::new(interner.intern_string("a"), TypeId::STRING);
    let prop_b = PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER);
    let obj = interner.object(vec![prop_a, prop_b.clone()]);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let keys = interner.union(vec![key_a, key_b]);

    let key_param = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(keys),
        default: None,
        is_const: false,
    };
    let key_param_id = interner.intern(TypeData::TypeParameter(key_param));

    let name_type = interner.conditional(ConditionalType {
        check_type: key_param_id,
        extends_type: key_a,
        true_type: TypeId::NEVER,
        false_type: key_param_id,
        is_distributive: true,
    });
    let template = interner.intern(TypeData::IndexAccess(obj, key_param_id));

    let mapped = MappedType {
        type_param: key_param,
        constraint: keys,
        name_type: Some(name_type),
        template,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);

    let expected = interner.object(vec![PropertyInfo {
        name: prop_b.name,
        type_id: prop_b.type_id,
        write_type: prop_b.write_type,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    assert_eq!(result, expected);
}

#[test]
fn test_mapped_type_deferred() {
    let interner = TypeInterner::new();

    // { [K in T]: number } where T is a type parameter
    // Should remain as mapped type (deferred)
    let type_param_t = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: type_param_t,
        name_type: None,
        template: TypeId::NUMBER,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let mapped_type = interner.mapped(mapped);
    let result = evaluate_mapped(&interner, &mapped);

    // Should return the same mapped type (deferred)
    assert_eq!(result, mapped_type);
}

/// Test mapped type with remove readonly modifier (-readonly).
///
/// `{ -readonly [K in keyof T]: T[K] }` should remove readonly from properties.
#[test]
fn test_mapped_type_remove_readonly_modifier() {
    let interner = TypeInterner::new();

    // Iterate over "a" | "b" keys with -readonly modifier
    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let keys = interner.union(vec![key_a, key_b]);

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keys,
        name_type: None,
        template: TypeId::STRING,
        readonly_modifier: Some(MappedModifier::Remove), // -readonly
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);

    // Expected: { a: string; b: string } with readonly removed
    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");
    let expected = interner.object(vec![
        PropertyInfo {
            name: a_name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false, // readonly removed
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        },
        PropertyInfo::new(b_name, TypeId::STRING),
    ]);

    assert_eq!(result, expected);
}

/// Test mapped type with remove optional modifier (-?).
///
/// `{ [K in keyof T]-?: T[K] }` should remove optional from properties.
#[test]
fn test_mapped_type_remove_optional_modifier() {
    let interner = TypeInterner::new();

    // Iterate over "a" | "b" keys with -? modifier
    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let keys = interner.union(vec![key_a, key_b]);

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keys,
        name_type: None,
        template: TypeId::NUMBER,
        readonly_modifier: None,
        optional_modifier: Some(MappedModifier::Remove), // -?
    };

    let result = evaluate_mapped(&interner, &mapped);

    // Expected: { a: number; b: number } with optional removed
    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");
    let expected = interner.object(vec![
        PropertyInfo {
            name: a_name,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false, // optional removed
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        },
        PropertyInfo::new(b_name, TypeId::NUMBER),
    ]);

    assert_eq!(result, expected);
}

/// Test mapped type with add readonly modifier (+readonly).
///
/// `{ +readonly [K in keyof T]: T[K] }` should add readonly to properties.
#[test]
fn test_mapped_type_add_readonly_modifier() {
    let interner = TypeInterner::new();

    // Iterate over "x" | "y" keys with +readonly modifier
    let key_x = interner.literal_string("x");
    let key_y = interner.literal_string("y");
    let keys = interner.union(vec![key_x, key_y]);

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keys,
        name_type: None,
        template: TypeId::BOOLEAN,
        readonly_modifier: Some(MappedModifier::Add), // +readonly
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);

    // Expected: { readonly x: boolean; readonly y: boolean }
    let x_name = interner.intern_string("x");
    let y_name = interner.intern_string("y");
    let expected = interner.object(vec![
        PropertyInfo {
            name: x_name,
            type_id: TypeId::BOOLEAN,
            write_type: TypeId::BOOLEAN,
            optional: false,
            readonly: true, // readonly added
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        },
        PropertyInfo::readonly(y_name, TypeId::BOOLEAN),
    ]);

    assert_eq!(result, expected);
}

/// Test mapped type with add optional modifier (+?).
///
/// `{ [K in keyof T]+?: T[K] }` should add optional to properties.
#[test]
fn test_mapped_type_add_optional_modifier() {
    let interner = TypeInterner::new();

    // Iterate over "foo" | "bar" keys with +? modifier
    let key_foo = interner.literal_string("foo");
    let key_bar = interner.literal_string("bar");
    let keys = interner.union(vec![key_foo, key_bar]);

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keys,
        name_type: None,
        template: TypeId::STRING,
        readonly_modifier: None,
        optional_modifier: Some(MappedModifier::Add), // +?
    };

    let result = evaluate_mapped(&interner, &mapped);

    // Expected: { foo?: string; bar?: string }
    let foo_name = interner.intern_string("foo");
    let bar_name = interner.intern_string("bar");
    let expected = interner.object(vec![
        PropertyInfo {
            name: bar_name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: true, // optional added
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        },
        PropertyInfo::opt(foo_name, TypeId::STRING),
    ]);

    assert_eq!(result, expected);
}

/// Test mapped type with both readonly and optional modifiers.
///
/// `{ +readonly [K in keyof T]+?: T[K] }` should add both modifiers.
#[test]
fn test_mapped_type_both_modifiers() {
    let interner = TypeInterner::new();

    // Iterate over "id" key with both +readonly and +? modifiers
    let key_id = interner.literal_string("id");

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: key_id,
        name_type: None,
        template: TypeId::NUMBER,
        readonly_modifier: Some(MappedModifier::Add), // +readonly
        optional_modifier: Some(MappedModifier::Add), // +?
    };

    let result = evaluate_mapped(&interner, &mapped);

    // Expected: { readonly id?: number }
    let id_name = interner.intern_string("id");
    let expected = interner.object(vec![PropertyInfo {
        name: id_name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: true,
        readonly: true,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    assert_eq!(result, expected);
}

/// Test mapped type with both remove modifiers (-readonly -?).
///
/// `{ -readonly [K in keyof T]-?: T[K] }` should remove both readonly and optional.
#[test]
fn test_mapped_type_both_remove_modifiers() {
    let interner = TypeInterner::new();

    // Iterate over "data" key with both -readonly and -? modifiers
    let key_data = interner.literal_string("data");

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: key_data,
        name_type: None,
        template: TypeId::STRING,
        readonly_modifier: Some(MappedModifier::Remove), // -readonly
        optional_modifier: Some(MappedModifier::Remove), // -?
    };

    let result = evaluate_mapped(&interner, &mapped);

    // Expected: { data: string } with both readonly and optional removed
    let data_name = interner.intern_string("data");
    let expected = interner.object(vec![PropertyInfo {
        name: data_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false, // optional removed
        readonly: false, // readonly removed
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    assert_eq!(result, expected);
}

/// Test mapped type with mixed modifiers (+readonly -?).
///
/// `{ +readonly [K in keyof T]-?: T[K] }` should add readonly and remove optional.
#[test]
fn test_mapped_type_add_readonly_remove_optional() {
    let interner = TypeInterner::new();

    // Iterate over "value" key with +readonly and -? modifiers
    let key_value = interner.literal_string("value");

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: key_value,
        name_type: None,
        template: TypeId::NUMBER,
        readonly_modifier: Some(MappedModifier::Add), // +readonly
        optional_modifier: Some(MappedModifier::Remove), // -?
    };

    let result = evaluate_mapped(&interner, &mapped);

    // Expected: { readonly value: number } (readonly added, optional removed)
    let value_name = interner.intern_string("value");
    let expected = interner.object(vec![PropertyInfo {
        name: value_name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false, // optional removed
        readonly: true,  // readonly added
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    assert_eq!(result, expected);
}

/// Test mapped type with mixed modifiers (-readonly +?).
///
/// `{ -readonly [K in keyof T]+?: T[K] }` should remove readonly and add optional.
#[test]
fn test_mapped_type_remove_readonly_add_optional() {
    let interner = TypeInterner::new();

    // Iterate over "config" key with -readonly and +? modifiers
    let key_config = interner.literal_string("config");

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: key_config,
        name_type: None,
        template: TypeId::BOOLEAN,
        readonly_modifier: Some(MappedModifier::Remove), // -readonly
        optional_modifier: Some(MappedModifier::Add),    // +?
    };

    let result = evaluate_mapped(&interner, &mapped);

    // Expected: { config?: boolean } (readonly removed, optional added)
    let config_name = interner.intern_string("config");
    let expected = interner.object(vec![PropertyInfo {
        name: config_name,
        type_id: TypeId::BOOLEAN,
        write_type: TypeId::BOOLEAN,
        optional: true,  // optional added
        readonly: false, // readonly removed
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    assert_eq!(result, expected);
}

// =============================================================================
// MAPPED TYPE MODIFIER ADVANCED TESTS
// =============================================================================

/// Test mapped type -readonly removes readonly from source object properties.
///
/// Given keys with -readonly modifier, properties should have readonly: false.
#[test]
fn test_mapped_type_minus_readonly_on_readonly_source() {
    let interner = TypeInterner::new();

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let keys = interner.union(vec![key_a, key_b]);

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keys,
        name_type: None,
        template: TypeId::STRING,
        readonly_modifier: Some(MappedModifier::Remove), // -readonly
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);

    // Expected: { a: string; b: string } with readonly: false
    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");
    let expected = interner.object(vec![
        PropertyInfo {
            name: a_name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false, // removed
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        },
        PropertyInfo::new(b_name, TypeId::STRING),
    ]);

    assert_eq!(result, expected);
}

/// Test mapped type +optional adds optional to all properties.
///
/// Given keys with +? modifier, properties should have optional: true.
#[test]
fn test_mapped_type_plus_optional_on_required_source() {
    let interner = TypeInterner::new();

    let key_x = interner.literal_string("x");
    let key_y = interner.literal_string("y");
    let keys = interner.union(vec![key_x, key_y]);

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keys,
        name_type: None,
        template: TypeId::NUMBER,
        readonly_modifier: None,
        optional_modifier: Some(MappedModifier::Add), // +?
    };

    let result = evaluate_mapped(&interner, &mapped);

    // Expected: { x?: number; y?: number }
    let x_name = interner.intern_string("x");
    let y_name = interner.intern_string("y");
    let expected = interner.object(vec![
        PropertyInfo {
            name: x_name,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: true, // added
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        },
        PropertyInfo::opt(y_name, TypeId::NUMBER),
    ]);

    assert_eq!(result, expected);
}

/// Test key remapping to uppercase using as clause.
///
/// { [K in "a" | "b" as Uppercase<K>]: string } should produce { A: string; B: string }.
#[test]
fn test_mapped_type_key_remap_uppercase() {
    let interner = TypeInterner::new();

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let keys = interner.union(vec![key_a, key_b]);

    let key_upper_a = interner.literal_string("A");
    let key_upper_b = interner.literal_string("B");

    let key_param = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(keys),
        default: None,
        is_const: false,
    };
    let key_param_id = interner.intern(TypeData::TypeParameter(key_param));

    // Create a conditional that maps "a" -> "A", "b" -> "B"
    let inner_cond = interner.conditional(ConditionalType {
        check_type: key_param_id,
        extends_type: key_b,
        true_type: key_upper_b,
        false_type: TypeId::NEVER,
        is_distributive: false,
    });
    let name_type = interner.conditional(ConditionalType {
        check_type: key_param_id,
        extends_type: key_a,
        true_type: key_upper_a,
        false_type: inner_cond,
        is_distributive: false,
    });

    let mapped = MappedType {
        type_param: key_param,
        constraint: keys,
        name_type: Some(name_type),
        template: TypeId::STRING,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);

    // Expected: { A: string; B: string }
    let a_upper_name = interner.intern_string("A");
    let b_upper_name = interner.intern_string("B");
    let expected = interner.object(vec![
        PropertyInfo::new(a_upper_name, TypeId::STRING),
        PropertyInfo::new(b_upper_name, TypeId::STRING),
    ]);

    assert_eq!(result, expected);
}

/// Test key remapping with prefix using as clause.
///
/// { [K in "name" | "age" as `get_${K}`]: string } should produce { `get_name`: string; `get_age`: string }.
#[test]
fn test_mapped_type_key_remap_with_prefix() {
    let interner = TypeInterner::new();

    let key_name = interner.literal_string("name");
    let key_age = interner.literal_string("age");
    let keys = interner.union(vec![key_name, key_age]);

    let key_get_name = interner.literal_string("get_name");
    let key_get_age = interner.literal_string("get_age");

    let key_param = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(keys),
        default: None,
        is_const: false,
    };
    let key_param_id = interner.intern(TypeData::TypeParameter(key_param));

    // Create conditional: K extends "name" ? "get_name" : K extends "age" ? "get_age" : never
    let inner_cond = interner.conditional(ConditionalType {
        check_type: key_param_id,
        extends_type: key_age,
        true_type: key_get_age,
        false_type: TypeId::NEVER,
        is_distributive: false,
    });
    let name_type = interner.conditional(ConditionalType {
        check_type: key_param_id,
        extends_type: key_name,
        true_type: key_get_name,
        false_type: inner_cond,
        is_distributive: false,
    });

    let mapped = MappedType {
        type_param: key_param,
        constraint: keys,
        name_type: Some(name_type),
        template: TypeId::STRING,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);

    // Expected: { get_age: string; get_name: string }
    let get_age_name = interner.intern_string("get_age");
    let get_name_name = interner.intern_string("get_name");
    let expected = interner.object(vec![
        PropertyInfo::new(get_age_name, TypeId::STRING),
        PropertyInfo::new(get_name_name, TypeId::STRING),
    ]);

    assert_eq!(result, expected);
}

/// Test modifier combination: +readonly +optional.
///
/// { +readonly [K in keys]+?: T[K] } should add both modifiers.
#[test]
fn test_mapped_type_add_both_modifiers_on_source() {
    let interner = TypeInterner::new();

    let key_value = interner.literal_string("value");

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: key_value,
        name_type: None,
        template: TypeId::STRING,
        readonly_modifier: Some(MappedModifier::Add), // +readonly
        optional_modifier: Some(MappedModifier::Add), // +?
    };

    let result = evaluate_mapped(&interner, &mapped);

    // Expected: { readonly value?: string }
    let value_name = interner.intern_string("value");
    let expected = interner.object(vec![PropertyInfo {
        name: value_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: true,
        readonly: true,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    assert_eq!(result, expected);
}

/// Test modifier combination: -readonly -optional (Required<T> pattern).
///
/// { -readonly [K in keys]-?: T[K] } should remove both modifiers.
#[test]
fn test_mapped_type_remove_both_modifiers_required_pattern() {
    let interner = TypeInterner::new();

    let key_data = interner.literal_string("data");

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: key_data,
        name_type: None,
        template: TypeId::STRING,
        readonly_modifier: Some(MappedModifier::Remove), // -readonly
        optional_modifier: Some(MappedModifier::Remove), // -?
    };

    let result = evaluate_mapped(&interner, &mapped);

    // Expected: { data: string } with both modifiers removed
    let data_name = interner.intern_string("data");
    let expected = interner.object(vec![PropertyInfo::new(data_name, TypeId::STRING)]);

    assert_eq!(result, expected);
}

/// Test key remapping that filters out keys (produces never).
///
/// { [K in "a" | "b" | "c" as K extends "b" ? never : K]: string }
/// should produce { a: string; c: string } (b filtered out).
#[test]
fn test_mapped_type_key_remap_filter_out_key() {
    let interner = TypeInterner::new();

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let key_c = interner.literal_string("c");
    let keys = interner.union(vec![key_a, key_b, key_c]);

    let key_param = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(keys),
        default: None,
        is_const: false,
    };
    let key_param_id = interner.intern(TypeData::TypeParameter(key_param));

    // K extends "b" ? never : K
    let name_type = interner.conditional(ConditionalType {
        check_type: key_param_id,
        extends_type: key_b,
        true_type: TypeId::NEVER,
        false_type: key_param_id,
        is_distributive: true,
    });

    let mapped = MappedType {
        type_param: key_param,
        constraint: keys,
        name_type: Some(name_type),
        template: TypeId::STRING,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);

    // Expected: { a: string; c: string } (b filtered out)
    let a_name = interner.intern_string("a");
    let c_name = interner.intern_string("c");
    let expected = interner.object(vec![
        PropertyInfo::new(a_name, TypeId::STRING),
        PropertyInfo::new(c_name, TypeId::STRING),
    ]);

    assert_eq!(result, expected);
}

/// Test mapped type with multiple keys preserves all properties.
///
/// { [K in "str" | "num" | "bool"]: K } should produce 3 properties.
#[test]
fn test_mapped_type_preserves_source_types() {
    let interner = TypeInterner::new();

    let key_str = interner.literal_string("str");
    let key_num = interner.literal_string("num");
    let key_bool = interner.literal_string("bool");
    let keys = interner.union(vec![key_str, key_num, key_bool]);

    let key_param = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let key_param_id = interner.intern(TypeData::TypeParameter(key_param));

    let mapped = MappedType {
        type_param: key_param,
        constraint: keys,
        name_type: None,
        template: key_param_id, // Template is the key itself
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);

    // Expected: { bool: "bool"; num: "num"; str: "str" }
    let str_name = interner.intern_string("str");
    let num_name = interner.intern_string("num");
    let bool_name = interner.intern_string("bool");
    let expected = interner.object(vec![
        PropertyInfo::new(bool_name, key_bool),
        PropertyInfo::new(num_name, key_num),
        PropertyInfo::new(str_name, key_str),
    ]);

    assert_eq!(result, expected);
}

// =============================================================================
// KEY REMAPPING (AS CLAUSE) TESTS
// =============================================================================

/// Test basic as clause with simple key transformation.
///
/// { [K in "a" | "b" as `${K}_key`]: string } should produce { `a_key`: string; `b_key`: string }.
#[test]
fn test_mapped_type_basic_as_clause() {
    let interner = TypeInterner::new();

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let keys = interner.union(vec![key_a, key_b]);

    // Create transformed key names
    let key_a_key = interner.literal_string("a_key");
    let key_b_key = interner.literal_string("b_key");

    let key_param = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(keys),
        default: None,
        is_const: false,
    };
    let key_param_id = interner.intern(TypeData::TypeParameter(key_param));

    // Create conditional: K extends "a" ? "a_key" : K extends "b" ? "b_key" : never
    let inner_cond = interner.conditional(ConditionalType {
        check_type: key_param_id,
        extends_type: key_b,
        true_type: key_b_key,
        false_type: TypeId::NEVER,
        is_distributive: false,
    });
    let name_type = interner.conditional(ConditionalType {
        check_type: key_param_id,
        extends_type: key_a,
        true_type: key_a_key,
        false_type: inner_cond,
        is_distributive: false,
    });

    let mapped = MappedType {
        type_param: key_param,
        constraint: keys,
        name_type: Some(name_type),
        template: TypeId::STRING,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);

    // Expected: { a_key: string; b_key: string }
    let a_key_name = interner.intern_string("a_key");
    let b_key_name = interner.intern_string("b_key");
    let expected = interner.object(vec![
        PropertyInfo::new(a_key_name, TypeId::STRING),
        PropertyInfo::new(b_key_name, TypeId::STRING),
    ]);

    assert_eq!(result, expected);
}

/// Test as clause with Extract-like filtering using specific keys.
///
/// { [K in "a" | "b" | "c" as K extends "a" | "c" ? K : never]: number }
/// should produce { a: number; c: number } (b filtered out).
#[test]
fn test_mapped_type_as_extract_specific_keys() {
    let interner = TypeInterner::new();

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let key_c = interner.literal_string("c");
    let keys = interner.union(vec![key_a, key_b, key_c]);

    // Allowed keys for Extract
    let allowed_keys = interner.union(vec![key_a, key_c]);

    let key_param = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(keys),
        default: None,
        is_const: false,
    };
    let key_param_id = interner.intern(TypeData::TypeParameter(key_param));

    // K extends "a" | "c" ? K : never (Extract<K, "a" | "c">)
    let name_type = interner.conditional(ConditionalType {
        check_type: key_param_id,
        extends_type: allowed_keys,
        true_type: key_param_id,
        false_type: TypeId::NEVER,
        is_distributive: true,
    });

    let mapped = MappedType {
        type_param: key_param,
        constraint: keys,
        name_type: Some(name_type),
        template: TypeId::NUMBER,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);

    // Expected: { a: number; c: number } (b filtered out)
    let a_name = interner.intern_string("a");
    let c_name = interner.intern_string("c");
    let expected = interner.object(vec![
        PropertyInfo::new(a_name, TypeId::NUMBER),
        PropertyInfo::new(c_name, TypeId::NUMBER),
    ]);

    assert_eq!(result, expected);
}

/// Test as clause with template literal key remapping.
///
/// { [K in "name" | "value" as `on${Capitalize<K>}Change`]: () => void }
/// simulated as { [K in keys as transformedK]: () => void }
#[test]
fn test_mapped_type_as_template_literal() {
    let interner = TypeInterner::new();

    let key_name = interner.literal_string("name");
    let key_value = interner.literal_string("value");
    let keys = interner.union(vec![key_name, key_value]);

    // Template literal results: "onNameChange", "onValueChange"
    let on_name_change = interner.literal_string("onNameChange");
    let on_value_change = interner.literal_string("onValueChange");

    let key_param = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(keys),
        default: None,
        is_const: false,
    };
    let key_param_id = interner.intern(TypeData::TypeParameter(key_param));

    // Simulate template literal with conditional
    let inner_cond = interner.conditional(ConditionalType {
        check_type: key_param_id,
        extends_type: key_value,
        true_type: on_value_change,
        false_type: TypeId::NEVER,
        is_distributive: false,
    });
    let name_type = interner.conditional(ConditionalType {
        check_type: key_param_id,
        extends_type: key_name,
        true_type: on_name_change,
        false_type: inner_cond,
        is_distributive: false,
    });

    // Create a void function type
    let void_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let mapped = MappedType {
        type_param: key_param,
        constraint: keys,
        name_type: Some(name_type),
        template: void_fn,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);

    // Expected: { onNameChange: () => void; onValueChange: () => void }
    let on_name_change_name = interner.intern_string("onNameChange");
    let on_value_change_name = interner.intern_string("onValueChange");
    let expected = interner.object(vec![
        PropertyInfo::new(on_name_change_name, void_fn),
        PropertyInfo::new(on_value_change_name, void_fn),
    ]);

    assert_eq!(result, expected);
}

/// Test as clause with conditional key transformation based on type.
///
/// { [K in "id" | "name" as K extends "id" ? `${K}_number` : `${K}_string`]: K }
/// should produce { `id_number`: "id"; `name_string`: "name" }
#[test]
fn test_mapped_type_as_conditional_transformation() {
    let interner = TypeInterner::new();

    let key_id = interner.literal_string("id");
    let key_name = interner.literal_string("name");
    let keys = interner.union(vec![key_id, key_name]);

    // Transformed keys
    let id_number = interner.literal_string("id_number");
    let name_string = interner.literal_string("name_string");

    let key_param = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(keys),
        default: None,
        is_const: false,
    };
    let key_param_id = interner.intern(TypeData::TypeParameter(key_param));

    // K extends "id" ? "id_number" : K extends "name" ? "name_string" : never
    let inner_cond = interner.conditional(ConditionalType {
        check_type: key_param_id,
        extends_type: key_name,
        true_type: name_string,
        false_type: TypeId::NEVER,
        is_distributive: false,
    });
    let name_type = interner.conditional(ConditionalType {
        check_type: key_param_id,
        extends_type: key_id,
        true_type: id_number,
        false_type: inner_cond,
        is_distributive: false,
    });

    let mapped = MappedType {
        type_param: key_param,
        constraint: keys,
        name_type: Some(name_type),
        template: key_param_id, // Template is the original key
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);

    // Expected: { id_number: "id"; name_string: "name" }
    let id_number_name = interner.intern_string("id_number");
    let name_string_name = interner.intern_string("name_string");
    let expected = interner.object(vec![
        PropertyInfo::new(id_number_name, key_id),
        PropertyInfo::new(name_string_name, key_name),
    ]);

    assert_eq!(result, expected);
}

/// Test as clause that excludes specific keys using Exclude pattern.
///
/// { [K in "a" | "b" | "c" as Exclude<K, "b">]: boolean }
/// should produce { a: boolean; c: boolean }
#[test]
fn test_mapped_type_as_exclude_key() {
    let interner = TypeInterner::new();

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let key_c = interner.literal_string("c");
    let keys = interner.union(vec![key_a, key_b, key_c]);

    let key_param = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(keys),
        default: None,
        is_const: false,
    };
    let key_param_id = interner.intern(TypeData::TypeParameter(key_param));

    // Exclude<K, "b"> = K extends "b" ? never : K
    let name_type = interner.conditional(ConditionalType {
        check_type: key_param_id,
        extends_type: key_b,
        true_type: TypeId::NEVER,
        false_type: key_param_id,
        is_distributive: true,
    });

    let mapped = MappedType {
        type_param: key_param,
        constraint: keys,
        name_type: Some(name_type),
        template: TypeId::BOOLEAN,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);

    // Expected: { a: boolean; c: boolean }
    let a_name = interner.intern_string("a");
    let c_name = interner.intern_string("c");
    let expected = interner.object(vec![
        PropertyInfo::new(a_name, TypeId::BOOLEAN),
        PropertyInfo::new(c_name, TypeId::BOOLEAN),
    ]);

    assert_eq!(result, expected);
}

/// Test as clause with identity transformation (as K keeps original keys).
///
/// { [K in "x" | "y" as K]: number } should produce { x: number; y: number }
#[test]
fn test_mapped_type_as_identity() {
    let interner = TypeInterner::new();

    let key_x = interner.literal_string("x");
    let key_y = interner.literal_string("y");
    let keys = interner.union(vec![key_x, key_y]);

    let key_param = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(keys),
        default: None,
        is_const: false,
    };
    let key_param_id = interner.intern(TypeData::TypeParameter(key_param));

    // as K (identity)
    let mapped = MappedType {
        type_param: key_param,
        constraint: keys,
        name_type: Some(key_param_id), // Identity: as K
        template: TypeId::NUMBER,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);

    // Expected: { x: number; y: number }
    let x_name = interner.intern_string("x");
    let y_name = interner.intern_string("y");
    let expected = interner.object(vec![
        PropertyInfo::new(x_name, TypeId::NUMBER),
        PropertyInfo::new(y_name, TypeId::NUMBER),
    ]);

    assert_eq!(result, expected);
}

/// Test as clause producing never for all keys results in empty object.
///
/// { [K in "a" | "b" as never]: string } should produce {}
#[test]
fn test_mapped_type_as_never_all_keys() {
    let interner = TypeInterner::new();

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let keys = interner.union(vec![key_a, key_b]);

    let key_param = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(keys),
        default: None,
        is_const: false,
    };

    // as never (filter out all keys)
    let mapped = MappedType {
        type_param: key_param,
        constraint: keys,
        name_type: Some(TypeId::NEVER),
        template: TypeId::STRING,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);

    // Expected: {} (empty object)
    let expected = interner.object(vec![]);

    assert_eq!(result, expected);
}

/// Test as clause with single key produces single property.
///
/// { [K in "only" as `prefix_${K}`]: K } should produce { `prefix_only`: "only" }
#[test]
fn test_mapped_type_as_single_key() {
    let interner = TypeInterner::new();

    let key_only = interner.literal_string("only");
    let prefix_only = interner.literal_string("prefix_only");

    let key_param = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(key_only),
        default: None,
        is_const: false,
    };
    let key_param_id = interner.intern(TypeData::TypeParameter(key_param));

    // as "prefix_only"
    let name_type = interner.conditional(ConditionalType {
        check_type: key_param_id,
        extends_type: key_only,
        true_type: prefix_only,
        false_type: TypeId::NEVER,
        is_distributive: false,
    });

    let mapped = MappedType {
        type_param: key_param,
        constraint: key_only,
        name_type: Some(name_type),
        template: key_param_id,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);

    // Expected: { prefix_only: "only" }
    let prefix_only_name = interner.intern_string("prefix_only");
    let expected = interner.object(vec![PropertyInfo::new(prefix_only_name, key_only)]);

    assert_eq!(result, expected);
}

/// Test conditional with void check type.
///
/// `void extends undefined ? true : false` should be false.
#[test]
fn test_conditional_void_check_type() {
    let interner = TypeInterner::new();

    let lit_true = interner.literal_boolean(true);
    let lit_false = interner.literal_boolean(false);

    // void extends undefined ? true : false
    let cond = ConditionalType {
        check_type: TypeId::VOID,
        extends_type: TypeId::UNDEFINED,
        true_type: lit_true,
        false_type: lit_false,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);

    // void is not assignable to undefined (they are different types)
    assert_eq!(result, lit_false, "void extends undefined should be false");
}

/// Test conditional with null check type.
///
/// `null extends object ? true : false` should be false.
#[test]
fn test_conditional_null_check_type() {
    let interner = TypeInterner::new();

    let lit_true = interner.literal_boolean(true);
    let lit_false = interner.literal_boolean(false);

    // null extends object ? true : false
    let cond = ConditionalType {
        check_type: TypeId::NULL,
        extends_type: TypeId::OBJECT,
        true_type: lit_true,
        false_type: lit_false,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);

    // null is not assignable to object in strict mode
    assert_eq!(result, lit_false, "null extends object should be false");
}

/// Test conditional with function extends function.
///
/// `() => void extends () => void ? true : false` should be true.
#[test]
fn test_conditional_function_extends_function() {
    let interner = TypeInterner::new();

    let lit_true = interner.literal_boolean(true);
    let lit_false = interner.literal_boolean(false);

    // () => void
    let void_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // () => void extends () => void ? true : false
    let cond = ConditionalType {
        check_type: void_fn,
        extends_type: void_fn,
        true_type: lit_true,
        false_type: lit_false,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);

    assert_eq!(
        result, lit_true,
        "() => void extends () => void should be true"
    );
}

/// Test conditional with array extends array.
///
/// `string[] extends any[] ? true : false` should be true.
#[test]
fn test_conditional_array_extends_array() {
    let interner = TypeInterner::new();

    let lit_true = interner.literal_boolean(true);
    let lit_false = interner.literal_boolean(false);

    let string_array = interner.array(TypeId::STRING);
    let any_array = interner.array(TypeId::ANY);

    // string[] extends any[] ? true : false
    let cond = ConditionalType {
        check_type: string_array,
        extends_type: any_array,
        true_type: lit_true,
        false_type: lit_false,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);

    assert_eq!(result, lit_true, "string[] extends any[] should be true");
}

/// Test conditional with tuple extends array.
///
/// `[string, number] extends any[] ? true : false` should be true.
#[test]
fn test_conditional_tuple_extends_array() {
    let interner = TypeInterner::new();

    let lit_true = interner.literal_boolean(true);
    let lit_false = interner.literal_boolean(false);

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
    let any_array = interner.array(TypeId::ANY);

    // [string, number] extends any[] ? true : false
    let cond = ConditionalType {
        check_type: tuple,
        extends_type: any_array,
        true_type: lit_true,
        false_type: lit_false,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);

    assert_eq!(
        result, lit_true,
        "[string, number] extends any[] should be true"
    );
}

/// Test conditional with object structural subtyping.
///
/// `{a: string, b: number} extends {a: string} ? true : false` should be true.
#[test]
fn test_conditional_object_structural_subtype() {
    let interner = TypeInterner::new();

    let lit_true = interner.literal_boolean(true);
    let lit_false = interner.literal_boolean(false);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");

    // {a: string, b: number}
    let obj_ab = interner.object(vec![
        PropertyInfo::new(a_name, TypeId::STRING),
        PropertyInfo::new(b_name, TypeId::NUMBER),
    ]);

    // {a: string}
    let obj_a = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    // {a: string, b: number} extends {a: string} ? true : false
    let cond = ConditionalType {
        check_type: obj_ab,
        extends_type: obj_a,
        true_type: lit_true,
        false_type: lit_false,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);

    assert_eq!(
        result, lit_true,
        "{{a: string, b: number}} extends {{a: string}} should be true"
    );
}

/// Test conditional with bigint type.
///
/// `bigint extends number ? true : false` should be false.
#[test]
fn test_conditional_bigint_extends_number() {
    let interner = TypeInterner::new();

    let lit_true = interner.literal_boolean(true);
    let lit_false = interner.literal_boolean(false);

    // bigint extends number ? true : false
    let cond = ConditionalType {
        check_type: TypeId::BIGINT,
        extends_type: TypeId::NUMBER,
        true_type: lit_true,
        false_type: lit_false,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);

    assert_eq!(result, lit_false, "bigint extends number should be false");
}

/// Test conditional with symbol type.
///
/// `symbol extends string ? true : false` should be false.
#[test]
fn test_conditional_symbol_extends_string() {
    let interner = TypeInterner::new();

    let lit_true = interner.literal_boolean(true);
    let lit_false = interner.literal_boolean(false);

    // symbol extends string ? true : false
    let cond = ConditionalType {
        check_type: TypeId::SYMBOL,
        extends_type: TypeId::STRING,
        true_type: lit_true,
        false_type: lit_false,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);

    assert_eq!(result, lit_false, "symbol extends string should be false");
}

// ExtractState/ExtractAction pattern tests (Redux-style utility types)
// These test conditional infer patterns like:
//   type ExtractState<R> = R extends Reducer<infer S, AnyAction> ? S : never;
//   type ExtractAction<R> = R extends Reducer<any, infer A> ? A : never;

#[test]
fn test_conditional_infer_extract_state_pattern() {
    let interner = TypeInterner::new();

    // Simulates: type ExtractState<R> = R extends Reducer<infer S, AnyAction> ? S : never;
    // Where Reducer<S, A> is represented as a function type: (state: S | undefined, action: A) => S

    let infer_s_name = interner.intern_string("S");
    let infer_s = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_s_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // AnyAction = { type: string }
    let any_action = interner.object(vec![PropertyInfo::new(
        interner.intern_string("type"),
        TypeId::STRING,
    )]);

    // Pattern to match: Reducer<infer S, AnyAction> represented as a function
    // (state: S | undefined, action: AnyAction) => S
    let state_param = interner.union(vec![infer_s, TypeId::UNDEFINED]);
    let extends_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("state")),
                type_id: state_param,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("action")),
                type_id: any_action,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: infer_s,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // The concrete Reducer type: (state: number | undefined, action: AnyAction) => number
    let concrete_state = TypeId::NUMBER;
    let concrete_state_param = interner.union(vec![concrete_state, TypeId::UNDEFINED]);
    let concrete_reducer = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("state")),
                type_id: concrete_state_param,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("action")),
                type_id: any_action,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: concrete_state,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Conditional: concrete_reducer extends extends_fn ? S : never
    let cond = ConditionalType {
        check_type: concrete_reducer,
        extends_type: extends_fn,
        true_type: infer_s,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);

    // Function infer pattern matching with union parameter types now works correctly.
    // Expected behavior: should extract the state type: number
    // With Application type expansion working, we can now correctly extract the state type.
    assert_eq!(result, concrete_state);
}

