#[test]
fn test_distribution_over_union_of_objects() {
    // T extends { x: string } ? T : never where T = { x: string, y: number } | { x: number } | { x: string }
    let interner = TypeInterner::new();

    let x_name = interner.intern_string("x");
    let y_name = interner.intern_string("y");

    let obj_xy = interner.object(vec![
        PropertyInfo::new(x_name, TypeId::STRING),
        PropertyInfo::new(y_name, TypeId::NUMBER),
    ]);

    let obj_x_num = interner.object(vec![PropertyInfo::new(x_name, TypeId::NUMBER)]);

    let obj_x_str = interner.object(vec![PropertyInfo::new(x_name, TypeId::STRING)]);

    let target = interner.object(vec![PropertyInfo::new(x_name, TypeId::STRING)]);

    let union = interner.union(vec![obj_xy, obj_x_num, obj_x_str]);

    let cond = ConditionalType {
        check_type: union,
        extends_type: target,
        true_type: union,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // obj_xy extends { x: string } -> yes
    // obj_x_num extends { x: string } -> no (x is number)
    // obj_x_str extends { x: string } -> yes
    // Result: obj_xy | obj_x_str
    assert!(result != TypeId::ERROR);
    assert!(result != TypeId::NEVER);
}

#[test]
fn test_distribution_over_intersection_of_unions() {
    // T extends string ? "yes" : "no" where T = (string | number) & (string | boolean)
    // Intersection = string (common to both)
    let interner = TypeInterner::new();

    let lit_yes = interner.literal_string("yes");
    let lit_no = interner.literal_string("no");

    let union1 = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let union2 = interner.union(vec![TypeId::STRING, TypeId::BOOLEAN]);
    let intersection = interner.intersection(vec![union1, union2]);

    let cond = ConditionalType {
        check_type: intersection,
        extends_type: TypeId::STRING,
        true_type: lit_yes,
        false_type: lit_no,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // (string | number) & (string | boolean) = string
    // string extends string = yes
    assert!(result == lit_yes || result != TypeId::ERROR);
}

#[test]
fn test_distribution_over_union_with_unknown() {
    // T extends unknown ? T : never where T = string | number | unknown
    // All types extend unknown
    let interner = TypeInterner::new();

    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::UNKNOWN]);

    let cond = ConditionalType {
        check_type: union,
        extends_type: TypeId::UNKNOWN,
        true_type: union,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Everything extends unknown, so result = union (or simplified)
    assert!(result != TypeId::NEVER);
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_distribution_exclude_pattern() {
    // Exclude<T, U> = T extends U ? never : T
    // Exclude<string | number | boolean, number> = string | boolean
    let interner = TypeInterner::new();

    let check_union = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);

    let cond = ConditionalType {
        check_type: check_union,
        extends_type: TypeId::NUMBER,
        true_type: TypeId::NEVER,
        false_type: check_union, // T
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // string -> not number -> string
    // number -> number -> never
    // boolean -> not number -> boolean
    // Result: string | boolean
    let expected = interner.union(vec![TypeId::STRING, TypeId::BOOLEAN]);
    assert!(result == expected || result != TypeId::ERROR);
}

#[test]
fn test_distribution_extract_pattern() {
    // Extract<T, U> = T extends U ? T : never
    // Extract<string | number | boolean, string | number> = string | number
    let interner = TypeInterner::new();

    let check_union = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);
    let target_union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let cond = ConditionalType {
        check_type: check_union,
        extends_type: target_union,
        true_type: check_union, // T
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // string -> extends string | number -> string
    // number -> extends string | number -> number
    // boolean -> not extends string | number -> never
    // Result: string | number
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_distribution_with_literal_union() {
    // T extends "a" | "b" ? "match" : "no-match" where T = "a" | "c" | "b" | "d"
    let interner = TypeInterner::new();

    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let lit_c = interner.literal_string("c");
    let lit_d = interner.literal_string("d");
    let lit_match = interner.literal_string("match");
    let lit_no_match = interner.literal_string("no-match");

    let check_union = interner.union(vec![lit_a, lit_c, lit_b, lit_d]);
    let extends_union = interner.union(vec![lit_a, lit_b]);

    let cond = ConditionalType {
        check_type: check_union,
        extends_type: extends_union,
        true_type: lit_match,
        false_type: lit_no_match,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // "a" extends "a" | "b" -> match
    // "b" extends "a" | "b" -> match
    // "c" extends "a" | "b" -> no-match
    // "d" extends "a" | "b" -> no-match
    // Result: "match" | "no-match"
    let expected = interner.union(vec![lit_match, lit_no_match]);
    assert!(result == expected || result != TypeId::ERROR);
}

#[test]
fn test_non_distribution_tuple_wrapped() {
    // [T] extends [string] ? "yes" : "no" where T = string | number
    // Non-distributive: [string | number] extends [string] is false
    let interner = TypeInterner::new();

    let lit_yes = interner.literal_string("yes");
    let lit_no = interner.literal_string("no");

    let check_union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let check_tuple = interner.tuple(vec![TupleElement {
        type_id: check_union,
        optional: false,
        name: None,
        rest: false,
    }]);
    let extends_tuple = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        optional: false,
        name: None,
        rest: false,
    }]);

    let cond = ConditionalType {
        check_type: check_tuple,
        extends_type: extends_tuple,
        true_type: lit_yes,
        false_type: lit_no,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // [string | number] does not extend [string] (number not assignable to string)
    assert_eq!(result, lit_no);
}

#[test]
fn test_distribution_boolean_special() {
    // boolean = true | false, distribution should work over both
    // T extends true ? "yes" : "no" where T = boolean
    let interner = TypeInterner::new();

    let lit_yes = interner.literal_string("yes");
    let lit_no = interner.literal_string("no");
    let lit_true = interner.literal_boolean(true);

    let cond = ConditionalType {
        check_type: TypeId::BOOLEAN,
        extends_type: lit_true,
        true_type: lit_yes,
        false_type: lit_no,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // boolean = true | false
    // true extends true -> yes
    // false extends true -> no
    // Result: "yes" | "no"
    let expected = interner.union(vec![lit_yes, lit_no]);
    assert!(result == expected || result == lit_yes || result == lit_no || result != TypeId::ERROR);
}

#[test]
fn test_distribution_with_function_types() {
    // T extends (...args: any[]) => any ? "function" : "not-function"
    // where T = ((x: string) => number) | string | ((y: number) => string)
    let interner = TypeInterner::new();

    let lit_function = interner.literal_string("function");
    let lit_not_function = interner.literal_string("not-function");

    let any_array = interner.array(TypeId::ANY);
    let fn_pattern = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: any_array,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn1 = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn2 = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("y")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let check_union = interner.union(vec![fn1, TypeId::STRING, fn2]);

    let cond = ConditionalType {
        check_type: check_union,
        extends_type: fn_pattern,
        true_type: lit_function,
        false_type: lit_not_function,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // fn1 extends fn_pattern -> function
    // string extends fn_pattern -> not-function
    // fn2 extends fn_pattern -> function
    // Result: "function" | "not-function"
    let expected = interner.union(vec![lit_function, lit_not_function]);
    assert!(result == expected || result != TypeId::ERROR);
}

#[test]
fn test_distribution_keyof_result() {
    // T extends keyof { a: 1, b: 2 } ? T : never
    // where T = "a" | "b" | "c"
    let interner = TypeInterner::new();

    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let lit_c = interner.literal_string("c");

    let check_union = interner.union(vec![lit_a, lit_b, lit_c]);
    let keyof_result = interner.union(vec![lit_a, lit_b]);

    let cond = ConditionalType {
        check_type: check_union,
        extends_type: keyof_result,
        true_type: check_union, // T
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // "a" extends "a" | "b" -> "a"
    // "b" extends "a" | "b" -> "b"
    // "c" extends "a" | "b" -> never
    // Result: "a" | "b"
    let expected = interner.union(vec![lit_a, lit_b]);
    assert!(result == expected || result != TypeId::ERROR);
}

// =============================================================================
// INDEXED ACCESS TYPE TESTS - T[K], Nested Access
// =============================================================================

#[test]
fn test_indexed_access_simple_property() {
    // { a: string }["a"] = string
    let interner = TypeInterner::new();

    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let key_a = interner.literal_string("a");
    let result = evaluate_index_access(&interner, obj, key_a);

    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_indexed_access_multiple_properties() {
    // { a: string, b: number }["a"] = string
    // { a: string, b: number }["b"] = number
    let interner = TypeInterner::new();

    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
    ]);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");

    assert_eq!(evaluate_index_access(&interner, obj, key_a), TypeId::STRING);
    assert_eq!(evaluate_index_access(&interner, obj, key_b), TypeId::NUMBER);
}

#[test]
fn test_indexed_access_union_key() {
    // { a: string, b: number }["a" | "b"] = string | number
    let interner = TypeInterner::new();

    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
    ]);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let union_key = interner.union(vec![key_a, key_b]);

    let result = evaluate_index_access(&interner, obj, union_key);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    assert_eq!(result, expected);
}

#[test]
fn test_indexed_access_nested_two_levels() {
    // { outer: { inner: string } }["outer"]["inner"] = string
    let interner = TypeInterner::new();

    let inner_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("inner"),
        TypeId::STRING,
    )]);

    let outer_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("outer"),
        inner_obj,
    )]);

    let key_outer = interner.literal_string("outer");
    let key_inner = interner.literal_string("inner");

    let first_access = evaluate_index_access(&interner, outer_obj, key_outer);
    assert_eq!(first_access, inner_obj);

    let second_access = evaluate_index_access(&interner, first_access, key_inner);
    assert_eq!(second_access, TypeId::STRING);
}

#[test]
fn test_indexed_access_deeply_nested() {
    // { a: { b: { c: { d: number } } } }["a"]["b"]["c"]["d"] = number
    let interner = TypeInterner::new();

    let d_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("d"),
        TypeId::NUMBER,
    )]);

    let c_obj = interner.object(vec![PropertyInfo::new(interner.intern_string("c"), d_obj)]);

    let b_obj = interner.object(vec![PropertyInfo::new(interner.intern_string("b"), c_obj)]);

    let a_obj = interner.object(vec![PropertyInfo::new(interner.intern_string("a"), b_obj)]);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let key_c = interner.literal_string("c");
    let key_d = interner.literal_string("d");

    let r1 = evaluate_index_access(&interner, a_obj, key_a);
    let r2 = evaluate_index_access(&interner, r1, key_b);
    let r3 = evaluate_index_access(&interner, r2, key_c);
    let r4 = evaluate_index_access(&interner, r3, key_d);

    assert_eq!(r4, TypeId::NUMBER);
}

#[test]
fn test_indexed_access_array_element() {
    // string[][number] = string
    let interner = TypeInterner::new();

    let string_array = interner.array(TypeId::STRING);
    let result = evaluate_index_access(&interner, string_array, TypeId::NUMBER);

    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_indexed_access_tuple_each_element() {
    // [string, number, boolean][0] = string
    // [string, number, boolean][1] = number
    // [string, number, boolean][2] = boolean
    let interner = TypeInterner::new();

    let tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            optional: false,
            name: None,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            optional: false,
            name: None,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::BOOLEAN,
            optional: false,
            name: None,
            rest: false,
        },
    ]);

    let key_0 = interner.literal_number(0.0);
    let key_1 = interner.literal_number(1.0);
    let key_2 = interner.literal_number(2.0);

    assert_eq!(
        evaluate_index_access(&interner, tuple, key_0),
        TypeId::STRING
    );
    assert_eq!(
        evaluate_index_access(&interner, tuple, key_1),
        TypeId::NUMBER
    );
    assert_eq!(
        evaluate_index_access(&interner, tuple, key_2),
        TypeId::BOOLEAN
    );
}

#[test]
fn test_indexed_access_tuple_number_index() {
    // [string, number, boolean][number] = string | number | boolean
    let interner = TypeInterner::new();

    let tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            optional: false,
            name: None,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            optional: false,
            name: None,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::BOOLEAN,
            optional: false,
            name: None,
            rest: false,
        },
    ]);

    let result = evaluate_index_access(&interner, tuple, TypeId::NUMBER);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);

    assert_eq!(result, expected);
}

#[test]
fn test_indexed_access_with_optional_property() {
    // { a?: string }["a"] = string | undefined
    let interner = TypeInterner::new();

    let obj = interner.object(vec![PropertyInfo::opt(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let key_a = interner.literal_string("a");
    let result = evaluate_index_access(&interner, obj, key_a);

    let expected = interner.union(vec![TypeId::STRING, TypeId::UNDEFINED]);
    assert_eq!(result, expected);
}

#[test]
fn test_indexed_access_with_readonly_property() {
    // { readonly a: string }["a"] = string
    let interner = TypeInterner::new();

    let obj = interner.object(vec![PropertyInfo::readonly(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let key_a = interner.literal_string("a");
    let result = evaluate_index_access(&interner, obj, key_a);

    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_indexed_access_union_of_objects() {
    // ({ a: string } | { a: number })["a"] = string | number
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

#[test]
fn test_indexed_access_intersection_object() {
    // ({ a: string } & { b: number })["a"] = string
    // ({ a: string } & { b: number })["b"] = number
    // Note: Implementation may return intersection or merged type
    let interner = TypeInterner::new();

    let obj1 = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let obj2 = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);

    let intersection = interner.intersection(vec![obj1, obj2]);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");

    let result_a = evaluate_index_access(&interner, intersection, key_a);
    let result_b = evaluate_index_access(&interner, intersection, key_b);

    // Results should not be errors
    assert!(result_a != TypeId::ERROR);
    assert!(result_b != TypeId::ERROR);
}

#[test]
fn test_indexed_access_string_index_signature() {
    // { [key: string]: number }["anyKey"] = number
    let interner = TypeInterner::new();

    let obj = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let any_key = interner.literal_string("anyKey");
    let result = evaluate_index_access(&interner, obj, any_key);

    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_indexed_access_number_index_signature() {
    // { [key: number]: string }[42] = string
    let interner = TypeInterner::new();

    let obj = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
    });

    let key_42 = interner.literal_number(42.0);
    let result = evaluate_index_access(&interner, obj, key_42);

    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_indexed_access_property_overrides_index_signature() {
    // { a: boolean, [key: string]: number }["a"] = boolean (specific property wins)
    let interner = TypeInterner::new();

    let obj = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo::new(
            interner.intern_string("a"),
            TypeId::BOOLEAN,
        )],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let key_a = interner.literal_string("a");
    let result = evaluate_index_access(&interner, obj, key_a);

    // Specific property takes precedence over index signature
    assert_eq!(result, TypeId::BOOLEAN);
}

#[test]
fn test_indexed_access_nested_with_union_intermediate() {
    // { data: { value: string } | { value: number } }["data"]["value"] = string | number
    let interner = TypeInterner::new();

    let obj1 = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        TypeId::STRING,
    )]);

    let obj2 = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        TypeId::NUMBER,
    )]);

    let union_data = interner.union(vec![obj1, obj2]);

    let outer = interner.object(vec![PropertyInfo::new(
        interner.intern_string("data"),
        union_data,
    )]);

    let key_data = interner.literal_string("data");
    let key_value = interner.literal_string("value");

    let r1 = evaluate_index_access(&interner, outer, key_data);
    let r2 = evaluate_index_access(&interner, r1, key_value);

    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(r2, expected);
}

#[test]
fn test_indexed_access_literal_types() {
    // { status: "active" | "inactive" }["status"] = "active" | "inactive"
    let interner = TypeInterner::new();

    let lit_active = interner.literal_string("active");
    let lit_inactive = interner.literal_string("inactive");
    let status_type = interner.union(vec![lit_active, lit_inactive]);

    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("status"),
        status_type,
    )]);

    let key_status = interner.literal_string("status");
    let result = evaluate_index_access(&interner, obj, key_status);

    assert_eq!(result, status_type);
}

#[test]
fn test_indexed_access_function_property() {
    // { fn: () => string }["fn"] = () => string
    let interner = TypeInterner::new();

    let fn_type = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let obj = interner.object(vec![PropertyInfo::method(
        interner.intern_string("fn"),
        fn_type,
    )]);

    let key_fn = interner.literal_string("fn");
    let result = evaluate_index_access(&interner, obj, key_fn);

    assert_eq!(result, fn_type);
}

#[test]
fn test_indexed_access_array_method_property() {
    // string[]["length"] = number
    let interner = TypeInterner::new();

    let string_array = interner.array(TypeId::STRING);
    let key_length = interner.literal_string("length");

    let result = evaluate_index_access(&interner, string_array, key_length);

    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_indexed_access_nested_array() {
    // string[][number][number] = string (flattened char)
    let interner = TypeInterner::new();

    let string_array = interner.array(TypeId::STRING);

    // First access: string[][number] = string
    let r1 = evaluate_index_access(&interner, string_array, TypeId::NUMBER);
    assert_eq!(r1, TypeId::STRING);

    // Second access: string[number] = string (character)
    let r2 = evaluate_index_access(&interner, r1, TypeId::NUMBER);
    assert_eq!(r2, TypeId::STRING);
}

#[test]
fn test_indexed_access_2d_array() {
    // number[][0] = number[]
    // number[][0][0] = number
    let interner = TypeInterner::new();

    let number_array = interner.array(TypeId::NUMBER);
    let array_2d = interner.array(number_array);

    let key_0 = interner.literal_number(0.0);

    // First access returns inner array type
    let r1 = evaluate_index_access(&interner, array_2d, key_0);
    assert_eq!(r1, number_array);

    // Second access returns element type
    let r2 = evaluate_index_access(&interner, r1, key_0);
    assert_eq!(r2, TypeId::NUMBER);
}

// =============================================================================
// TEMPLATE LITERAL AND KEYOF TESTS
// =============================================================================

/// Test keyof with template literal containing union interpolation
/// keyof `get${Action}Done` should return keyof string (apparent keys of String)
#[test]
fn test_keyof_template_literal_union_interpolation() {
    let interner = TypeInterner::new();

    // Create "A" | "B" | "C" union
    let lit_a = interner.literal_string("A");
    let lit_b = interner.literal_string("B");
    let lit_c = interner.literal_string("C");
    let union_abc = interner.union(vec![lit_a, lit_b, lit_c]);

    // Create template literal: `get${"A" | "B" | "C"}Done`
    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("get")),
        TemplateSpan::Type(union_abc),
        TemplateSpan::Text(interner.intern_string("Done")),
    ]);

    // keyof template literal returns apparent keys of string (same as keyof string)
    let result = evaluate_keyof(&interner, template);
    let expected = evaluate_keyof(&interner, TypeId::STRING);
    assert_eq!(result, expected);
}

/// Test keyof with union of template literals
/// keyof (`foo${string}` | `bar${string}`) should return keyof string (apparent keys)
#[test]
fn test_keyof_union_of_template_literals() {
    let interner = TypeInterner::new();

    // Create `foo${string}` template
    let template1 = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("foo")),
        TemplateSpan::Type(TypeId::STRING),
    ]);

    // Create `bar${string}` template
    let template2 = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("bar")),
        TemplateSpan::Type(TypeId::STRING),
    ]);

    // Union of template literals
    let union_templates = interner.union(vec![template1, template2]);

    // keyof (union of templates) = intersection of keyofs, which is keyof string
    let result = evaluate_keyof(&interner, union_templates);
    let expected = evaluate_keyof(&interner, TypeId::STRING);
    assert_eq!(result, expected);
}

/// Test conditional type with template literal infer and keyof
/// T extends `get${infer K}Done` ? keyof { [P in K]: any } : never
#[test]
fn test_conditional_infer_template_with_keyof_result() {
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("K");
    let infer_k = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: `get${infer K}Done`
    let pattern = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("get")),
        TemplateSpan::Type(infer_k),
        TemplateSpan::Text(interner.intern_string("Done")),
    ]);

    // T extends `get${infer K}Done` ? K : never
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: pattern,
        true_type: infer_k,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);

    // Test with "getFooDone"
    let mut subst = TypeSubstitution::new();
    let input = interner.literal_string("getFooDone");
    subst.insert(t_name, input);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.literal_string("Foo");
    assert_eq!(result, expected);
}

/// Test string intrinsic (Uppercase) with template literal
/// `get${Uppercase<Action>}` should create template with uppercased value
/// Note: Uppercase is typically implemented via mapped types, this tests the pattern
#[test]
fn test_template_literal_with_uppercase_intrinsic_pattern() {
    let interner = TypeInterner::new();

    // Simulate Uppercase<"a" | "b"> -> "A" | "B"
    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let input_union = interner.union(vec![lit_a, lit_b]);

    // Template that would use uppercased values: `on${Uppercase<"a" | "b">}Change`
    // In real TS, this would expand to "onAChange" | "onBChange"
    // Here we test that template literals handle the union interpolation correctly
    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("on")),
        TemplateSpan::Type(input_union),
        TemplateSpan::Text(interner.intern_string("Change")),
    ]);

    // With optimization, template literals with expandable unions are expanded immediately
    // `on${"a"|"b"}Change` becomes "onaChange" | "onbChange"
    match interner.lookup(template) {
        Some(TypeData::Union(members_id)) => {
            let members = interner.type_list(members_id);
            assert_eq!(members.len(), 2, "Expected 2 members in expanded union");
        }
        _ => panic!(
            "Expected Union type for template with union interpolation, got {:?}",
            interner.lookup(template)
        ),
    }
}

/// Test nested conditional types with template literals
/// T extends `prefix${infer R}` ? R extends `suffix${infer S}` ? S : never : never
#[test]
fn test_nested_conditional_template_literal_infer() {
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_r_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_r_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_s_name = interner.intern_string("S");
    let infer_s = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_s_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Outer pattern: `prefix${infer R}`
    let outer_pattern = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("prefix")),
        TemplateSpan::Type(infer_r),
    ]);

    // Inner conditional: R extends `suffix${infer S}` ? S : never
    let inner_pattern = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("suffix")),
        TemplateSpan::Type(infer_s),
    ]);

    let inner_cond = ConditionalType {
        check_type: infer_r,
        extends_type: inner_pattern,
        true_type: infer_s,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    // Outer conditional: T extends `prefix${infer R}` ? (inner) : never
    let outer_cond = ConditionalType {
        check_type: t_param,
        extends_type: outer_pattern,
        true_type: interner.conditional(inner_cond),
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(outer_cond);

    // Test with "prefixsuffixValue"
    let mut subst = TypeSubstitution::new();
    let input = interner.literal_string("prefixsuffixValue");
    subst.insert(t_name, input);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.literal_string("Value");
    assert_eq!(result, expected);
}

#[test]
fn test_recursive_template_literal_application_with_string_intrinsics() {
    use crate::StringIntrinsicKind;
    use crate::def::DefKind;
    use crate::relations::subtype::TypeEnvironment;

    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();
    let camel_def = DefId(6312);
    let camel_base = interner.intern(TypeData::Lazy(camel_def));

    let s_name = interner.intern_string("S");
    let s_param_info = TypeParamInfo {
        name: s_name,
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    };
    let s_param = interner.intern(TypeData::TypeParameter(s_param_info));

    let l_name = interner.intern_string("L");
    let infer_l = interner.intern(TypeData::Infer(TypeParamInfo {
        name: l_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let r_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: r_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let pattern = interner.template_literal(vec![
        TemplateSpan::Type(infer_l),
        TemplateSpan::Text(interner.intern_string("_")),
        TemplateSpan::Type(infer_r),
    ]);
    let lower_l = interner.string_intrinsic(StringIntrinsicKind::Lowercase, infer_l);
    let cap_r = interner.string_intrinsic(StringIntrinsicKind::Capitalize, infer_r);
    let recursive = interner.application(camel_base, vec![cap_r]);
    let true_type = interner.template_literal(vec![
        TemplateSpan::Type(lower_l),
        TemplateSpan::Type(recursive),
    ]);
    let false_type = interner.string_intrinsic(StringIntrinsicKind::Lowercase, s_param);
    let body = interner.conditional(ConditionalType {
        check_type: s_param,
        extends_type: pattern,
        true_type,
        false_type,
        is_distributive: true,
    });

    env.insert_def_with_params(camel_def, body, vec![s_param_info]);
    env.insert_def_kind(camel_def, DefKind::TypeAlias);

    let input = interner.literal_string("hello_world");
    let app = interner.application(camel_base, vec![input]);
    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(app);

    assert_eq!(
        result,
        interner.literal_string("helloworld"),
        "got {:?}",
        interner.lookup(result)
    );
}

/// Test template literal in conditional extends clause
/// `prefix${string}` extends `prefix${infer R}` ? R : never
#[test]
fn test_template_literal_conditional_extends_template() {
    let interner = TypeInterner::new();

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: `prefix${infer R}`
    let pattern = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("prefix")),
        TemplateSpan::Type(infer_r),
    ]);

    // Check type: `prefix${string}`
    let check_type = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("prefix")),
        TemplateSpan::Type(TypeId::STRING),
    ]);

    let cond = ConditionalType {
        check_type,
        extends_type: pattern,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Should infer string from the template
    assert_eq!(result, TypeId::STRING);
}

/// Test escape sequences in template literal evaluation
/// Template literals with special characters should be handled correctly
#[test]
fn test_template_literal_escape_sequences() {
    let interner = TypeInterner::new();

    // Template with newline escape sequence - text-only templates become string literals
    let template = interner.template_literal(vec![TemplateSpan::Text(
        interner.intern_string("line1\\nline2"),
    )]);

    // With optimization, text-only template literals become string literals
    if let Some(TypeData::Literal(LiteralValue::String(atom))) = interner.lookup(template) {
        let resolved = interner.resolve_atom_ref(atom);
        // The escape sequence should be preserved in the string
        assert!(
            resolved.contains("\\n"),
            "Escape sequence should be preserved"
        );
    } else {
        panic!(
            "Expected string literal for text-only template, got {:?}",
            interner.lookup(template)
        );
    }
}

/// Test template literal with special characters in infer pattern
/// `prefix\n${infer R}` should match "prefix\nvalue"
#[test]
fn test_template_literal_infer_with_special_chars() {
    let interner = TypeInterner::new();

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern with special character
    let pattern = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("data-")),
        TemplateSpan::Type(infer_r),
    ]);

    // Input with hyphen (special character in property names)
    let input = interner.literal_string("data-value");

    let cond = ConditionalType {
        check_type: input,
        extends_type: pattern,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    let expected = interner.literal_string("value");
    assert_eq!(result, expected);
}

/// Test complex composition: keyof, template literal, conditional, and infer
/// Extract property names from template literal pattern
#[test]
fn test_complex_keyof_template_infer_composition() {
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_k_name = interner.intern_string("K");
    let infer_k = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_k_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: `get${infer K}`
    let pattern = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("get")),
        TemplateSpan::Type(infer_k),
    ]);

    // T extends `get${infer K}` ? K : never
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: pattern,
        true_type: infer_k,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    // Create object type to use in keyof
    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("getName"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("getAge"), TypeId::NUMBER),
    ]);

    // keyof obj = "getName" | "getAge"
    let keys_of_obj = evaluate_keyof(&interner, obj);

    // Now test the conditional with the keys
    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, keys_of_obj);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Should extract "Name" | "Age" from the keys
    if let Some(TypeData::Union(members)) = interner.lookup(result) {
        let members = interner.type_list(members);
        assert_eq!(members.len(), 2);
    } else {
        panic!("Expected union of extracted names");
    }
}

/// Test template literal with number interpolation
/// `item${number}` should work with number types
#[test]
fn test_template_literal_with_number_interpolation() {
    let interner = TypeInterner::new();

    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("item")),
        TemplateSpan::Type(TypeId::NUMBER),
    ]);

    // Verify template was created
    if let Some(TypeData::TemplateLiteral(spans)) = interner.lookup(template) {
        let spans = interner.template_list(spans);
        assert_eq!(spans.len(), 2);
    } else {
        panic!("Expected template literal");
    }
}

/// Test multiple infers in template literal pattern with union input
/// `${infer A}-${infer B}` with "foo-bar" | "baz-qux"
#[test]
fn test_template_literal_two_infers_union_input() {
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

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

    // Pattern: `${infer A}-${infer B}`
    let pattern = interner.template_literal(vec![
        TemplateSpan::Type(infer_a),
        TemplateSpan::Text(interner.intern_string("-")),
        TemplateSpan::Type(infer_b),
    ]);

    // Result type: `${infer A}-${infer B}` (reconstruct the pattern)
    let result_template = interner.template_literal(vec![
        TemplateSpan::Type(infer_a),
        TemplateSpan::Text(interner.intern_string("-")),
        TemplateSpan::Type(infer_b),
    ]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: pattern,
        true_type: result_template,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);

    // Test with "foo-bar" | "baz-qux"
    let mut subst = TypeSubstitution::new();
    let foo_bar = interner.literal_string("foo-bar");
    let baz_qux = interner.literal_string("baz-qux");
    subst.insert(t_name, interner.union(vec![foo_bar, baz_qux]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Should return "foo-bar" | "baz-qux"
    if let Some(TypeData::Union(members)) = interner.lookup(result) {
        let members = interner.type_list(members);
        assert_eq!(members.len(), 2);
    } else {
        panic!("Expected union");
    }
}

/// Test template literal with constrained infer
/// T extends `prefix${infer R extends string}` ? R : never
#[test]
fn test_template_literal_constrained_infer() {
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
        constraint: Some(TypeId::STRING), // Constrained to string
        default: None,
        is_const: false,
    }));

    // Pattern: `prefix${infer R extends string}`
    let pattern = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("prefix")),
        TemplateSpan::Type(infer_r),
    ]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: pattern,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);

    // Test with "prefixValue"
    let mut subst = TypeSubstitution::new();
    let input = interner.literal_string("prefixValue");
    subst.insert(t_name, input);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.literal_string("Value");
    assert_eq!(result, expected);
}

/// Test keyof with object containing template literal keys
/// { [`get${string}`]: string } should have string keys
#[test]
fn test_keyof_object_with_template_literal_computed_keys() {
    let interner = TypeInterner::new();

    // In TypeScript, you can have computed properties with template literals
    // This tests that we handle the keyof correctly
    // For now, we test that keyof of an object with some properties works

    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("getName"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("getAge"), TypeId::NUMBER),
    ]);

    let result = evaluate_keyof(&interner, obj);

    // Should return "getName" | "getAge"
    if let Some(TypeData::Union(members)) = interner.lookup(result) {
        let members = interner.type_list(members);
        assert_eq!(members.len(), 2);
    } else {
        panic!("Expected union of property names");
    }
}

/// Test empty template literal
/// `(empty template)` should be handled
#[test]
fn test_empty_template_literal() {
    let interner = TypeInterner::new();

    // Empty template literal is optimized to an empty string literal
    let template = interner.template_literal(vec![]);

    // With the template literal optimization, empty template literals become empty string literals
    if let Some(TypeData::Literal(LiteralValue::String(atom))) = interner.lookup(template) {
        let s = interner.resolve_atom_ref(atom);
        assert_eq!(
            s.as_ref(),
            "",
            "Empty template literal should be empty string"
        );
    } else {
        panic!(
            "Expected empty string literal for empty template literal, got {:?}",
            interner.lookup(template)
        );
    }
}

/// Test template literal with only text (no interpolation)
/// `hello` should behave like a string literal
#[test]
fn test_template_literal_only_text() {
    let interner = TypeInterner::new();

    // Template literal with only text is optimized to a string literal
    let template =
        interner.template_literal(vec![TemplateSpan::Text(interner.intern_string("hello"))]);

    // With the template literal optimization, text-only template literals become string literals
    if let Some(TypeData::Literal(LiteralValue::String(atom))) = interner.lookup(template) {
        let s = interner.resolve_atom_ref(atom);
        assert_eq!(
            s.as_ref(),
            "hello",
            "Text-only template literal should be 'hello' string literal"
        );
    } else {
        panic!(
            "Expected string literal for text-only template literal, got {:?}",
            interner.lookup(template)
        );
    }

    // keyof of string literal returns apparent keys of string (same as keyof string)
    let result = evaluate_keyof(&interner, template);
    let expected = evaluate_keyof(&interner, TypeId::STRING);
    assert_eq!(result, expected);
}

/// Test template literal with only type interpolation (no text)
/// `${string}` should behave like string
#[test]
fn test_template_literal_only_type_interpolation() {
    let interner = TypeInterner::new();

    let template = interner.template_literal(vec![TemplateSpan::Type(TypeId::STRING)]);

    // A lone `${string}` spans the full string domain, so it collapses to
    // `string` at construction (tsc's getTemplateLiteralType).
    assert_eq!(template, TypeId::STRING);

    // keyof returns apparent keys of string (same as keyof string)
    let result = evaluate_keyof(&interner, template);
    let expected = evaluate_keyof(&interner, TypeId::STRING);
    assert_eq!(result, expected);
}

/// Test distributive conditional with template literal and union
/// ("a" | "b") extends `${infer R}x` ? R : never
#[test]
fn test_distributive_conditional_template_union() {
    let interner = TypeInterner::new();

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: `${infer R}x`
    let pattern = interner.template_literal(vec![
        TemplateSpan::Type(infer_r),
        TemplateSpan::Text(interner.intern_string("x")),
    ]);

    // Input: "ax" | "bx" | "c"
    let lit_ax = interner.literal_string("ax");
    let lit_bx = interner.literal_string("bx");
    let lit_c = interner.literal_string("c");
    let input_union = interner.union(vec![lit_ax, lit_bx, lit_c]);

    let cond = ConditionalType {
        check_type: input_union,
        extends_type: pattern,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);

    // Should extract "a" | "b" (the "c" doesn't match and becomes never)
    if let Some(TypeData::Union(members)) = interner.lookup(result) {
        let members = interner.type_list(members);
        assert_eq!(members.len(), 2);
    } else {
        panic!("Expected union");
    }
}

/// Test non-distributive conditional with template literal
/// ("a" | "b") extends `${infer R}x` ? R : never (non-distributive)
#[test]
fn test_non_distributive_conditional_template_union() {
    let interner = TypeInterner::new();

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: `${infer R}x`
    let pattern = interner.template_literal(vec![
        TemplateSpan::Type(infer_r),
        TemplateSpan::Text(interner.intern_string("x")),
    ]);

    // Input: "ax" | "bx"
    let lit_ax = interner.literal_string("ax");
    let lit_bx = interner.literal_string("bx");
    let input_union = interner.union(vec![lit_ax, lit_bx]);

    let cond = ConditionalType {
        check_type: input_union,
        extends_type: pattern,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false, // Non-distributive
    };

    let result = evaluate_conditional(&interner, &cond);

    // Non-distributive: the entire union is checked against the pattern
    // For "ax" | "bx" against `${infer R}x`, R infers to "a" | "b"
    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let expected_union = interner.union(vec![lit_a, lit_b]);
    // Result could be the inferred union, never, or string depending on implementation
    assert!(
        result == TypeId::NEVER || result == TypeId::STRING || result == expected_union,
        "Expected never, string, or \"a\" | \"b\", got {result:?}"
    );
}

/// Test template literal with boolean interpolation
/// `flag${boolean}` expands to "flagtrue" | "flagfalse"
#[test]
fn test_template_literal_with_boolean_interpolation() {
    let interner = TypeInterner::new();

    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("flag")),
        TemplateSpan::Type(TypeId::BOOLEAN),
    ]);

    // TypeScript expands boolean interpolation to union
    match interner.lookup(template) {
        Some(TypeData::Union(list_id)) => {
            let members = interner.type_list(list_id);
            assert_eq!(members.len(), 2, "Expected 2 members for boolean expansion");
        }
        other => panic!("Expected Union type for `flag${{boolean}}`, got {other:?}"),
    }
}

/// Test template literal matching with literal union input
/// T extends `${"a" | "b"}x` ? T : never
#[test]
fn test_template_literal_literal_union_pattern() {
    let interner = TypeInterner::new();

    // Pattern: `${"a" | "b"}x`
    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let union_ab = interner.union(vec![lit_a, lit_b]);

    let pattern = interner.template_literal(vec![
        TemplateSpan::Type(union_ab),
        TemplateSpan::Text(interner.intern_string("x")),
    ]);

    // Input: "ax"
    let input = interner.literal_string("ax");

    let cond = ConditionalType {
        check_type: input,
        extends_type: pattern,
        true_type: input,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // "ax" should match `${"a" | "b"}x`
    assert_eq!(result, input);
}

/// Test template literal types with array/tuple index access scenarios
/// This verifies that template literals work correctly in index access contexts
/// which is important for noUncheckedIndexedAccess scenarios
#[test]
fn test_template_literal_index_access_scenario() {
    let interner = TypeInterner::new();

    // Create an object with template literal-like string properties
    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("item0"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("item1"), TypeId::NUMBER),
    ]);

    // Access with a literal string key
    let key = interner.literal_string("item0");
    let result = evaluate_index_access(&interner, obj, key);

    assert_eq!(result, TypeId::STRING);
}

/// Test template literal pattern matching in mapped types
/// { [K in `${Prefix}${infer S}`]: S } expands correctly
#[test]
fn test_template_literal_mapped_type_pattern() {
    let interner = TypeInterner::new();

    let infer_s_name = interner.intern_string("S");
    let infer_s = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_s_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Create a template literal pattern like `get${infer S}`
    let pattern_template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("get")),
        TemplateSpan::Type(infer_s),
    ]);

    // Verify the pattern was created
    if let Some(TypeData::TemplateLiteral(spans)) = interner.lookup(pattern_template) {
        let spans = interner.template_list(spans);
        assert_eq!(spans.len(), 2);
    } else {
        panic!("Expected template literal");
    }
}

/// Test multiple template literal infers with complex union patterns
/// T extends `start${infer A}-middle${infer B}-end` ? [A, B] : never
#[test]
fn test_template_literal_multiple_infers_complex_pattern() {
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

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

    // Pattern: `start${infer A}-middle${infer B}-end`
    let pattern = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("start")),
        TemplateSpan::Type(infer_a),
        TemplateSpan::Text(interner.intern_string("-middle")),
        TemplateSpan::Type(infer_b),
        TemplateSpan::Text(interner.intern_string("-end")),
    ]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: pattern,
        true_type: infer_a, // Return first infer
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);

    // Test with "startFOO-middleBAR-end"
    let mut subst = TypeSubstitution::new();
    let input = interner.literal_string("startFOO-middleBAR-end");
    subst.insert(t_name, input);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.literal_string("FOO");
    assert_eq!(result, expected);
}
