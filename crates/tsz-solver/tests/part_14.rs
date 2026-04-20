use super::*;
use crate::TypeInterner;
use crate::def::DefId;
use crate::{SubtypeChecker, TypeSubstitution, instantiate_type};
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
