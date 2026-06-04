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
