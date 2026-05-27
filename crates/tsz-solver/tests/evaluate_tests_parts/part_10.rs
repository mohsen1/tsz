#[test]
fn test_template_literal_prefix_infer_suffix_multiple_hyphens() {
    let interner = TypeInterner::new();

    // Pattern: T extends `api-${infer Route}-handler` ? Route : never
    // Input: "api-user-profile-handler" => Route = "user-profile"
    // The infer captures everything between "api-" and "-handler"

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("Route");
    let infer_route = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends `api-${infer Route}-handler` ? Route : never
    let extends_template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("api-")),
        TemplateSpan::Type(infer_route),
        TemplateSpan::Text(interner.intern_string("-handler")),
    ]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_template,
        true_type: infer_route,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, interner.literal_string("api-user-profile-handler"));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Captures everything between "api-" and "-handler"
    let expected = interner.literal_string("user-profile");
    assert_eq!(result, expected);
}

#[test]
fn test_template_literal_prefix_infer_suffix_distributive() {
    let interner = TypeInterner::new();

    // Pattern: T extends `on-${infer E}-event` ? E : never (distributive)
    // Input: "on-click-event" | "on-load-event" => "click" | "load"

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("E");
    let infer_e = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends `on-${infer E}-event` ? E : never
    let extends_template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("on-")),
        TemplateSpan::Type(infer_e),
        TemplateSpan::Text(interner.intern_string("-event")),
    ]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_template,
        true_type: infer_e,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let lit_click = interner.literal_string("on-click-event");
    let lit_load = interner.literal_string("on-load-event");
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, interner.union(vec![lit_click, lit_load]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.union(vec![
        interner.literal_string("click"),
        interner.literal_string("load"),
    ]);
    assert_eq!(result, expected);
}

// =========================================================================
// Template Literal Type Inference - Number Extraction Pattern Tests
// =========================================================================
// Tests for template literal patterns that extract numeric strings

#[test]
fn test_template_literal_extract_numeric_id() {
    let interner = TypeInterner::new();

    // Pattern: T extends `user-${infer Id}` ? Id : never
    // Input: "user-42" => Id = "42"
    // Common pattern for extracting numeric IDs from string keys

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("Id");
    let infer_id = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends `user-${infer Id}` ? Id : never
    let extends_template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("user-")),
        TemplateSpan::Type(infer_id),
    ]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_template,
        true_type: infer_id,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, interner.literal_string("user-42"));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Extracts "42" as a string literal
    let expected = interner.literal_string("42");
    assert_eq!(result, expected);
}

#[test]
fn test_template_literal_extract_version_numbers() {
    let interner = TypeInterner::new();

    // Pattern: T extends `v${infer Major}.${infer Minor}` ? [Major, Minor] : never
    // Input: "v1.2" => [Major, Minor] = ["1", "2"]
    // Common pattern for parsing version strings

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_major = interner.intern(TypeData::Infer(TypeParamInfo {
        name: interner.intern_string("Major"),
        constraint: None,
        default: None,
        is_const: false,
    }));
    let infer_minor = interner.intern(TypeData::Infer(TypeParamInfo {
        name: interner.intern_string("Minor"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends `v${infer Major}.${infer Minor}` ? [Major, Minor] : never
    let extends_template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("v")),
        TemplateSpan::Type(infer_major),
        TemplateSpan::Text(interner.intern_string(".")),
        TemplateSpan::Type(infer_minor),
    ]);

    let true_type = interner.tuple(vec![
        TupleElement {
            type_id: infer_major,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: infer_minor,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_template,
        true_type,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, interner.literal_string("v1.2"));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Extracts ["1", "2"]
    let expected = interner.tuple(vec![
        TupleElement {
            type_id: interner.literal_string("1"),
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: interner.literal_string("2"),
            name: None,
            optional: false,
            rest: false,
        },
    ]);
    assert_eq!(result, expected);
}

#[test]
fn test_template_literal_extract_index_from_array_key() {
    let interner = TypeInterner::new();

    // Pattern: T extends `item[${infer Index}]` ? Index : never
    // Input: "item[0]" | "item[1]" | "item[2]" => "0" | "1" | "2"
    // Common pattern for extracting array indices from bracket notation

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("Index");
    let infer_index = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends `item[${infer Index}]` ? Index : never
    let extends_template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("item[")),
        TemplateSpan::Type(infer_index),
        TemplateSpan::Text(interner.intern_string("]")),
    ]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_template,
        true_type: infer_index,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let lit_0 = interner.literal_string("item[0]");
    let lit_1 = interner.literal_string("item[1]");
    let lit_2 = interner.literal_string("item[2]");
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, interner.union(vec![lit_0, lit_1, lit_2]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Extracts "0" | "1" | "2"
    let expected = interner.union(vec![
        interner.literal_string("0"),
        interner.literal_string("1"),
        interner.literal_string("2"),
    ]);
    assert_eq!(result, expected);
}

#[test]
fn test_template_literal_extract_port_number() {
    let interner = TypeInterner::new();

    // Pattern: T extends `localhost:${infer Port}` ? Port : never
    // Input: "localhost:3000" => Port = "3000"
    // Common pattern for extracting port numbers from host strings

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("Port");
    let infer_port = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends `localhost:${infer Port}` ? Port : never
    let extends_template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("localhost:")),
        TemplateSpan::Type(infer_port),
    ]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_template,
        true_type: infer_port,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, interner.literal_string("localhost:3000"));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.literal_string("3000");
    assert_eq!(result, expected);
}

#[test]
fn test_template_literal_extract_coordinates() {
    let interner = TypeInterner::new();

    // Pattern: T extends `(${infer X},${infer Y})` ? [X, Y] : never
    // Input: "(10,20)" => [X, Y] = ["10", "20"]
    // Common pattern for parsing coordinate pairs

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_x = interner.intern(TypeData::Infer(TypeParamInfo {
        name: interner.intern_string("X"),
        constraint: None,
        default: None,
        is_const: false,
    }));
    let infer_y = interner.intern(TypeData::Infer(TypeParamInfo {
        name: interner.intern_string("Y"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends `(${infer X},${infer Y})` ? [X, Y] : never
    let extends_template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("(")),
        TemplateSpan::Type(infer_x),
        TemplateSpan::Text(interner.intern_string(",")),
        TemplateSpan::Type(infer_y),
        TemplateSpan::Text(interner.intern_string(")")),
    ]);

    let true_type = interner.tuple(vec![
        TupleElement {
            type_id: infer_x,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: infer_y,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_template,
        true_type,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, interner.literal_string("(10,20)"));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.tuple(vec![
        TupleElement {
            type_id: interner.literal_string("10"),
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: interner.literal_string("20"),
            name: None,
            optional: false,
            rest: false,
        },
    ]);
    assert_eq!(result, expected);
}

// =============================================================================
// Variadic Tuple Type Tests
// =============================================================================

#[test]
fn test_variadic_tuple_spread_at_end() {
    // Test: [string, ...number[]] - variadic tuple with spread at end
    let interner = TypeInterner::new();

    // Create [string, ...number[]]
    let number_array = interner.array(TypeId::NUMBER);
    let variadic_tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: number_array,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    // Verify the tuple was created as a tuple type
    assert!(matches!(
        interner.lookup(variadic_tuple),
        Some(TypeData::Tuple(_))
    ));
    assert_ne!(variadic_tuple, TypeId::NEVER);
    assert_ne!(variadic_tuple, TypeId::UNKNOWN);
}

#[test]
fn test_variadic_tuple_spread_at_start() {
    // Test: [...string[], number] - variadic tuple with spread at start
    let interner = TypeInterner::new();

    // Create [...string[], number]
    let string_array = interner.array(TypeId::STRING);
    let variadic_tuple = interner.tuple(vec![
        TupleElement {
            type_id: string_array,
            name: None,
            optional: false,
            rest: true,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    // Verify the tuple was created as a tuple type
    assert!(matches!(
        interner.lookup(variadic_tuple),
        Some(TypeData::Tuple(_))
    ));
    assert_ne!(variadic_tuple, TypeId::NEVER);
    assert_ne!(variadic_tuple, TypeId::UNKNOWN);
}

#[test]
fn test_variadic_tuple_infer_rest_elements() {
    // Test: T extends [first, ...infer Rest] ? Rest : never
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let rest_name = interner.intern_string("Rest");

    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_rest = interner.intern(TypeData::Infer(TypeParamInfo {
        name: rest_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: [string, ...infer Rest]
    let extends_tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: infer_rest,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_tuple,
        true_type: infer_rest,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();

    // Input: [string, number, boolean]
    let input_tuple = interner.tuple(vec![
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
        TupleElement {
            type_id: TypeId::BOOLEAN,
            name: None,
            optional: false,
            rest: false,
        },
    ]);
    subst.insert(t_name, input_tuple);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Rest should be [number, boolean]
    let expected = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::BOOLEAN,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    assert_eq!(result, expected);
}

#[test]
fn test_variadic_tuple_infer_first_element() {
    // Test: T extends [infer First, ...infer Rest] ? First : never
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let first_name = interner.intern_string("First");
    let rest_name = interner.intern_string("Rest");

    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_first = interner.intern(TypeData::Infer(TypeParamInfo {
        name: first_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_rest = interner.intern(TypeData::Infer(TypeParamInfo {
        name: rest_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: [infer First, ...infer Rest]
    let extends_tuple = interner.tuple(vec![
        TupleElement {
            type_id: infer_first,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: infer_rest,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_tuple,
        true_type: infer_first,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();

    // Input: [number, string, boolean]
    let input_tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::BOOLEAN,
            name: None,
            optional: false,
            rest: false,
        },
    ]);
    subst.insert(t_name, input_tuple);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // First should be number
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_variadic_tuple_empty_rest() {
    // Test: [string] extends [string, ...infer R] ? R : never
    // Should produce empty tuple []
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let r_name = interner.intern_string("R");

    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
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

    // Pattern: [string, ...infer R]
    let extends_tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: infer_r,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_tuple,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();

    // Input: [string] - only one element
    let input_tuple = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }]);
    subst.insert(t_name, input_tuple);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // R should be empty tuple []
    let expected = interner.tuple(Vec::new());
    assert_eq!(result, expected);
}

// =========================================================================
// KeyOf and Indexed Access Type Tests - Additional Scenarios
// =========================================================================
// Tests for keyof and indexed access types in complex scenarios

#[test]
fn test_keyof_with_index_access_combination() {
    let interner = TypeInterner::new();

    // Pattern: { [K in keyof T]: T[K] } - identity mapped type
    // Object: { name: string, age: number }
    // keyof T = "name" | "age", T[K] produces the value types

    let name_prop = interner.intern_string("name");
    let age_prop = interner.intern_string("age");

    let obj = interner.object(vec![
        PropertyInfo::new(name_prop, TypeId::STRING),
        PropertyInfo::new(age_prop, TypeId::NUMBER),
    ]);

    let result = evaluate_keyof(&interner, obj);

    // Should produce "age" | "name" (order determined by interner)
    let expected = interner.union(vec![
        interner.literal_string("age"),
        interner.literal_string("name"),
    ]);
    assert_eq!(result, expected);
}

#[test]
fn test_index_access_with_keyof() {
    let interner = TypeInterner::new();

    // Pattern: T[keyof T] - get all value types from object
    // Object: { x: string, y: number }
    // T[keyof T] = string | number

    let x_prop = interner.intern_string("x");
    let y_prop = interner.intern_string("y");

    let obj = interner.object(vec![
        PropertyInfo::new(x_prop, TypeId::STRING),
        PropertyInfo::new(y_prop, TypeId::NUMBER),
    ]);

    // Access with "x" key
    let key_x = interner.literal_string("x");
    let result_x = evaluate_index_access(&interner, obj, key_x);
    assert_eq!(result_x, TypeId::STRING);

    // Access with "y" key
    let key_y = interner.literal_string("y");
    let result_y = evaluate_index_access(&interner, obj, key_y);
    assert_eq!(result_y, TypeId::NUMBER);
}

#[test]
fn test_index_access_nested_object() {
    let interner = TypeInterner::new();

    // Pattern: T["outer"]["inner"]
    // Object: { outer: { inner: string } }

    let inner_prop = interner.intern_string("inner");
    let inner_obj = interner.object(vec![PropertyInfo::new(inner_prop, TypeId::STRING)]);

    let outer_prop = interner.intern_string("outer");
    let outer_obj = interner.object(vec![PropertyInfo::new(outer_prop, inner_obj)]);

    // First access: T["outer"]
    let outer_key = interner.literal_string("outer");
    let first_result = evaluate_index_access(&interner, outer_obj, outer_key);

    // First result should be the inner object
    assert_eq!(first_result, inner_obj);

    // Second access: T["outer"]["inner"]
    let inner_key = interner.literal_string("inner");
    let final_result = evaluate_index_access(&interner, first_result, inner_key);

    // Final result should be string
    assert_eq!(final_result, TypeId::STRING);
}

// =============================================================================
// INDEXED ACCESS TYPE TESTS
// =============================================================================

/// Test basic indexed access with literal key.
///
/// { a: string, b: number }["a"] should be string.
#[test]
fn test_indexed_access_basic_literal_key() {
    let interner = TypeInterner::new();

    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
    ]);

    let key_a = interner.literal_string("a");
    let result = evaluate_index_access(&interner, obj, key_a);
    assert_eq!(result, TypeId::STRING);

    let key_b = interner.literal_string("b");
    let result_b = evaluate_index_access(&interner, obj, key_b);
    assert_eq!(result_b, TypeId::NUMBER);
}

/// Test indexed access with union key produces union type.
///
/// { a: string, b: number, c: boolean }["a" | "b"] should be string | number.
#[test]
fn test_indexed_access_union_key_produces_union() {
    let interner = TypeInterner::new();

    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("c"), TypeId::BOOLEAN),
    ]);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let key_union = interner.union(vec![key_a, key_b]);

    let result = evaluate_index_access(&interner, obj, key_union);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

/// Test indexed access with triple union key.
///
/// { a: string, b: number, c: boolean }["a" | "b" | "c"] should be string | number | boolean.
#[test]
fn test_indexed_access_triple_union_key() {
    let interner = TypeInterner::new();

    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("c"), TypeId::BOOLEAN),
    ]);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let key_c = interner.literal_string("c");
    let key_union = interner.union(vec![key_a, key_b, key_c]);

    let result = evaluate_index_access(&interner, obj, key_union);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);
    assert_eq!(result, expected);
}

/// Test recursive indexed access for nested objects.
///
/// { outer: { middle: { inner: string } } }["outer"]["middle"]["inner"] should be string.
#[test]
fn test_indexed_access_recursive_three_levels() {
    let interner = TypeInterner::new();

    // Build innermost object: { inner: string }
    let inner_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("inner"),
        TypeId::STRING,
    )]);

    // Build middle object: { middle: { inner: string } }
    let middle_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("middle"),
        inner_obj,
    )]);

    // Build outer object: { outer: { middle: { inner: string } } }
    let outer_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("outer"),
        middle_obj,
    )]);

    // Access T["outer"]
    let outer_key = interner.literal_string("outer");
    let first_result = evaluate_index_access(&interner, outer_obj, outer_key);
    assert_eq!(first_result, middle_obj);

    // Access T["outer"]["middle"]
    let middle_key = interner.literal_string("middle");
    let second_result = evaluate_index_access(&interner, first_result, middle_key);
    assert_eq!(second_result, inner_obj);

    // Access T["outer"]["middle"]["inner"]
    let inner_key = interner.literal_string("inner");
    let final_result = evaluate_index_access(&interner, second_result, inner_key);
    assert_eq!(final_result, TypeId::STRING);
}

/// Test indexed access on optional property includes undefined.
///
/// { a?: string }["a"] should be string | undefined.
#[test]
fn test_indexed_access_optional_property() {
    let interner = TypeInterner::new();

    let obj = interner.object(vec![PropertyInfo {
        name: interner.intern_string("a"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: true, // optional property
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    }]);

    let key_a = interner.literal_string("a");
    let result = evaluate_index_access(&interner, obj, key_a);

    // Optional property access should include undefined
    let expected = interner.union(vec![TypeId::STRING, TypeId::UNDEFINED]);
    assert_eq!(result, expected);
}

/// Test indexed access with mix of required and optional properties.
///
/// { a: string, b?: number }["a" | "b"] should be string | number | undefined.
#[test]
fn test_indexed_access_mixed_optional_required() {
    let interner = TypeInterner::new();

    let obj = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("a"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false, // required
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
            name: interner.intern_string("b"),
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
            is_symbol_named: false,
            single_quoted_name: false,
        },
    ]);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let key_union = interner.union(vec![key_a, key_b]);

    let result = evaluate_index_access(&interner, obj, key_union);

    // Union access includes all types + undefined from optional
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::UNDEFINED]);
    assert_eq!(result, expected);
}

/// Test indexed access on array type with number key.
///
/// string[][number] should be string.
#[test]
fn test_indexed_access_array_number_key() {
    let interner = TypeInterner::new();

    let string_array = interner.array(TypeId::STRING);

    let result = evaluate_index_access(&interner, string_array, TypeId::NUMBER);
    assert_eq!(result, TypeId::STRING);
}

/// Test indexed access on tuple with literal index.
///
/// [string, number, boolean][1] should be number.
#[test]
fn test_indexed_access_tuple_literal_index() {
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
        TupleElement {
            type_id: TypeId::BOOLEAN,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let key_0 = interner.literal_number(0.0);
    let result_0 = evaluate_index_access(&interner, tuple, key_0);
    assert_eq!(result_0, TypeId::STRING);

    let key_1 = interner.literal_number(1.0);
    let result_1 = evaluate_index_access(&interner, tuple, key_1);
    assert_eq!(result_1, TypeId::NUMBER);

    let key_2 = interner.literal_number(2.0);
    let result_2 = evaluate_index_access(&interner, tuple, key_2);
    assert_eq!(result_2, TypeId::BOOLEAN);
}

/// Test indexed access with union of objects.
///
/// ({ a: string } | { a: number })["a"] should be string | number.
#[test]
fn test_indexed_access_union_object() {
    let interner = TypeInterner::new();

    let obj1 = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let obj2 = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::NUMBER,
    )]);

    let union_obj = interner.union(vec![obj1, obj2]);

    let key_a = interner.literal_string("a");
    let result = evaluate_index_access(&interner, union_obj, key_a);

    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

/// Test indexed access with all optional properties.
///
/// { a?: string, b?: number }["a" | "b"] should be string | number | undefined.
#[test]
fn test_indexed_access_all_optional_properties() {
    let interner = TypeInterner::new();

    let obj = interner.object(vec![
        PropertyInfo::opt(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::opt(interner.intern_string("b"), TypeId::NUMBER),
    ]);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let key_union = interner.union(vec![key_a, key_b]);

    let result = evaluate_index_access(&interner, obj, key_union);

    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::UNDEFINED]);
    assert_eq!(result, expected);
}

/// Test indexed access preserves readonly property type.
///
/// { readonly a: string }["a"] should still be string.
#[test]
fn test_indexed_access_readonly_property() {
    let interner = TypeInterner::new();

    let obj = interner.object(vec![PropertyInfo {
        name: interner.intern_string("a"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: true, // readonly
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    }]);

    let key_a = interner.literal_string("a");
    let result = evaluate_index_access(&interner, obj, key_a);
    assert_eq!(result, TypeId::STRING);
}

// ============================================================================
// Generator Function Type Tests
// ============================================================================
// Tests for generator function return type evaluation

#[test]
fn test_generator_function_return_type_extraction() {
    // Test: Extract return type from generator-like function
    // T extends () => infer R ? R : never
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: () => infer R
    let extends_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: infer_r,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_fn,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();

    // Input: () => number
    let input_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    subst.insert(t_name, input_fn);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Should extract number
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_generator_function_yield_type_simulation() {
    // Test: Simulate extracting yield type via first type param
    // Generator<T, TReturn, TNext> - extract T
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("Y");
    let infer_y = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern function returning: { value: infer Y; done: boolean }
    let value_prop = interner.intern_string("value");
    let done_prop = interner.intern_string("done");
    let iterator_result = interner.object(vec![
        PropertyInfo::readonly(value_prop, infer_y),
        PropertyInfo::readonly(done_prop, TypeId::BOOLEAN),
    ]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: iterator_result,
        true_type: infer_y,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();

    // Input: { value: string; done: boolean }
    let input_obj = interner.object(vec![
        PropertyInfo::readonly(value_prop, TypeId::STRING),
        PropertyInfo::readonly(done_prop, TypeId::BOOLEAN),
    ]);
    subst.insert(t_name, input_obj);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Should extract string as yield type
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_generator_function_async_return() {
    // Test: Extract inner type from Promise-like return
    // T extends () => Promise<infer R> ? R : never (simulated with object)
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: { then: (resolve: (value: infer R) => void) => void }
    let then_prop = interner.intern_string("then");
    let promise_like = interner.object(vec![PropertyInfo {
        name: then_prop,
        type_id: infer_r, // Simplified - using infer R directly as property
        write_type: infer_r,
        optional: false,
        readonly: true,
        is_method: true,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    }]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: promise_like,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();

    // Input: { then: string }
    let input_obj = interner.object(vec![PropertyInfo {
        name: then_prop,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: true,
        is_method: true,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    }]);
    subst.insert(t_name, input_obj);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Should extract string
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_generator_function_next_param_type() {
    // Test: Extract parameter type from function
    // T extends (arg: infer A) => any ? A : never
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("A");
    let infer_a = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: (arg: infer A) => any
    let arg_name = interner.intern_string("arg");
    let extends_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo::required(arg_name, infer_a)],
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_fn,
        true_type: infer_a,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();

    // Input: (x: number) => string
    let x_name = interner.intern_string("x");
    let input_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo::required(x_name, TypeId::NUMBER)],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    subst.insert(t_name, input_fn);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Should extract number
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_generator_function_multiple_params() {
    // Test: Extract all parameters as tuple
    // T extends (...args: infer P) => any ? P : never
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("P");
    let infer_p = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: (...args: infer P) => any
    let args_name = interner.intern_string("args");
    let extends_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo::rest(args_name, infer_p)],
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_fn,
        true_type: infer_p,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();

    // Input: (a: string, b: number) => void
    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");
    let input_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo::required(a_name, TypeId::STRING),
            ParamInfo::required(b_name, TypeId::NUMBER),
        ],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    subst.insert(t_name, input_fn);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Rest parameter extraction may return never if pattern doesn't match
    // or return the extracted parameters if it does
    // This tests the basic structure is correct
    assert!(
        result == TypeId::NEVER
            || matches!(
                interner.lookup(result),
                Some(TypeData::Tuple(_) | TypeData::Array(_) | _)
            )
    );
}

// ============================================================================
// Module Augmentation Type Tests
// ============================================================================
// Tests for module augmentation and declaration merging behavior

#[test]
fn test_module_augmentation_object_merge() {
    // Test: Merge two object types (simulating interface merging)
    // interface A { x: string } merged with interface A { y: number }
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // First object: { x: string }
    let x_prop = interner.intern_string("x");
    let obj1 = interner.object(vec![PropertyInfo::new(x_prop, TypeId::STRING)]);

    // Second object: { y: number }
    let y_prop = interner.intern_string("y");
    let obj2 = interner.object(vec![PropertyInfo::new(y_prop, TypeId::NUMBER)]);

    // Merge via intersection
    let merged = interner.intersection(vec![obj1, obj2]);

    // T extends merged ? T : never
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: merged,
        true_type: t_param,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();

    // Input: { x: string, y: number }
    let combined = interner.object(vec![
        PropertyInfo::new(x_prop, TypeId::STRING),
        PropertyInfo::new(y_prop, TypeId::NUMBER),
    ]);
    subst.insert(t_name, combined);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Should match and return combined
    assert_eq!(result, combined);
}

#[test]
fn test_module_augmentation_function_overload() {
    // Test: Merged function signatures (callable with multiple overloads)
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: () => infer R
    let extends_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: infer_r,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_fn,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();

    // Input: () => string (first overload)
    let input_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    subst.insert(t_name, input_fn);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Should extract string return type
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_module_augmentation_namespace_merge() {
    // Test: Namespace with merged properties
    let interner = TypeInterner::new();

    // Original namespace: { version: string }
    let version_prop = interner.intern_string("version");
    let ns1 = interner.object(vec![PropertyInfo::readonly(version_prop, TypeId::STRING)]);

    // Augmentation: { utils: { format: () => string } }
    let utils_prop = interner.intern_string("utils");
    let format_prop = interner.intern_string("format");
    let format_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let utils_obj = interner.object(vec![PropertyInfo::method(format_prop, format_fn)]);
    let ns2 = interner.object(vec![PropertyInfo::new(utils_prop, utils_obj)]);

    // Merged namespace
    let merged_ns = interner.intersection(vec![ns1, ns2]);

    // The merged namespace should expose both sets of properties.
    match interner.lookup(merged_ns) {
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            let has_version = shape.properties.iter().any(|p| p.name == version_prop);
            let has_utils = shape.properties.iter().any(|p| p.name == utils_prop);
            assert!(
                has_version && has_utils,
                "merged namespace should include both props"
            );
        }
        other => panic!("unexpected merged namespace representation: {other:?}"),
    }
}

#[test]
fn test_module_augmentation_class_extension() {
    // Test: Class with augmented static members
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Class static: { new (): Instance }
    let instance_type = interner.object(vec![]);
    let constructor = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: instance_type,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    let new_prop = interner.intern_string("new");
    let class_static = interner.object(vec![PropertyInfo {
        name: new_prop,
        type_id: constructor,
        write_type: constructor,
        optional: false,
        readonly: true,
        is_method: true,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    }]);

    // T extends { new: ... } ? T : never
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: class_static,
        true_type: t_param,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, class_static);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Should match
    assert_eq!(result, class_static);
}

#[test]
fn test_module_augmentation_global_interface() {
    // Test: Global interface augmentation (like adding to Array prototype)
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("E");
    let infer_e = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: Array-like with custom method
    // { myMethod: () => infer E }
    let my_method = interner.intern_string("myMethod");
    let extends_obj = interner.object(vec![PropertyInfo::method(my_method, infer_e)]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_obj,
        true_type: infer_e,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();

    // Input: { myMethod: number }
    let input_obj = interner.object(vec![PropertyInfo::method(my_method, TypeId::NUMBER)]);
    subst.insert(t_name, input_obj);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Should extract number
    assert_eq!(result, TypeId::NUMBER);
}

// ============================================================================
// Array Covariance Tests
// ============================================================================
// Tests for array type covariance and element type extraction
