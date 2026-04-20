use super::*;
use crate::TypeInterner;
use crate::def::DefId;
use crate::{SubtypeChecker, TypeSubstitution, instantiate_type};
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
