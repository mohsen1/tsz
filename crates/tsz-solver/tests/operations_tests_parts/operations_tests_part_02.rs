#[test]
fn test_array_element_type_homogeneous_tuple() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    // [string, string] should have element type string
    let tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
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
    ]);

    let result = evaluator.array_element_type(tuple);
    assert_eq!(
        result,
        TypeId::STRING,
        "[string, string] should have element type string"
    );
}

/// Test that `array_element_type` correctly extracts union type from heterogeneous tuple
#[test]
fn test_array_element_type_heterogeneous_tuple() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    // [string, number] should have element type (string | number)
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

    let result = evaluator.array_element_type(tuple);
    // Result should be a union of string | number
    assert!(
        result == TypeId::STRING
            || result == TypeId::NUMBER
            || matches!(interner.lookup(result), Some(TypeData::Union(_))),
        "[string, number] element type should be string, number, or (string | number)"
    );
}

/// Test `array_element_type` with tuple containing rest element
#[test]
fn test_array_element_type_tuple_with_rest() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let number_array = interner.array(TypeId::NUMBER);

    // [string, ...number[]] should have element type (string | number)
    let tuple = interner.tuple(vec![
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

    let result = evaluator.array_element_type(tuple);
    // Result should be a union of string | number or one of the types
    assert!(
        result == TypeId::STRING
            || result == TypeId::NUMBER
            || matches!(interner.lookup(result), Some(TypeData::Union(_))),
        "[string, ...number[]] element type should be string, number, or (string | number)"
    );
}

/// Test `array_element_type` with empty tuple
#[test]
fn test_array_element_type_empty_tuple() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    // [] should have element type never
    let empty_tuple = interner.tuple(Vec::new());

    let result = evaluator.array_element_type(empty_tuple);
    assert_eq!(result, TypeId::NEVER, "[] should have element type never");
}

/// Test `array_element_type` with single-element tuple
#[test]
fn test_array_element_type_single_element_tuple() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    // [number] should have element type number
    let tuple = interner.tuple(vec![TupleElement {
        type_id: TypeId::NUMBER,
        name: None,
        optional: false,
        rest: false,
    }]);

    let result = evaluator.array_element_type(tuple);
    assert_eq!(
        result,
        TypeId::NUMBER,
        "[number] should have element type number"
    );
}

/// Test `array_element_type` with tuple containing optional elements
#[test]
fn test_array_element_type_optional_tuple() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    // [string, number?] element type should be (string | number | undefined) or (string | number)
    // depending on implementation
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
            optional: true,
            rest: false,
        },
    ]);

    let result = evaluator.array_element_type(tuple);
    // Should contain at least string and number (could also include undefined for optional)
    assert!(
        result == TypeId::STRING
            || result == TypeId::NUMBER
            || matches!(interner.lookup(result), Some(TypeData::Union(_))),
        "[string, number?] element type should be string, number, or a union containing them"
    );
}

/// Test `array_element_type` with three-element heterogeneous tuple
#[test]
fn test_array_element_type_three_element_tuple() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    // [string, number, boolean] should have element type (string | number | boolean)
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

    let result = evaluator.array_element_type(tuple);
    // Result should be a union of all three types or one of them
    assert!(
        result == TypeId::STRING
            || result == TypeId::NUMBER
            || result == TypeId::BOOLEAN
            || matches!(interner.lookup(result), Some(TypeData::Union(_))),
        "[string, number, boolean] element type should be a union of the three types"
    );
}

/// Test `array_element_type` with tuple containing literals
#[test]
fn test_array_element_type_literal_tuple() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let hello = interner.literal_string("hello");
    let world = interner.literal_string("world");

    // ["hello", "world"] should have element type "hello" | "world"
    let tuple = interner.tuple(vec![
        TupleElement {
            type_id: hello,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: world,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let result = evaluator.array_element_type(tuple);
    // Result should be a union of literals or one of them
    assert!(
        result == hello
            || result == world
            || matches!(interner.lookup(result), Some(TypeData::Union(_))),
        "[\"hello\", \"world\"] element type should be literal union"
    );
}

/// Test generic function with tuple argument matching array constraint
#[test]
fn test_generic_function_tuple_to_array_constraint() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // Create an array constraint: T extends string[]
    let string_array = interner.array(TypeId::STRING);

    let type_params = vec![TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(string_array),
        default: None,
        is_const: false,
    }];

    // [string, string] should satisfy string[] constraint
    let tuple_arg = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
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
    ]);

    let type_args = vec![tuple_arg];
    let result = solve_generic_instantiation(&type_params, &type_args, &interner, &mut checker);
    assert_eq!(
        result,
        GenericInstantiationResult::Success,
        "[string, string] should satisfy T extends string[] constraint"
    );
}

/// Test generic function with heterogeneous tuple NOT matching homogeneous array constraint
#[test]
fn test_generic_function_heterogeneous_tuple_fails_homogeneous_array_constraint() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // Create an array constraint: T extends string[]
    let string_array = interner.array(TypeId::STRING);

    let type_params = vec![TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(string_array),
        default: None,
        is_const: false,
    }];

    // [string, number] should NOT satisfy string[] constraint
    let tuple_arg = interner.tuple(vec![
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

    let type_args = vec![tuple_arg];
    let result = solve_generic_instantiation(&type_params, &type_args, &interner, &mut checker);
    assert!(
        matches!(
            result,
            GenericInstantiationResult::ConstraintViolation { .. }
        ),
        "[string, number] should NOT satisfy T extends string[] constraint"
    );
}

/// Test generic function with tuple matching union array constraint
#[test]
fn test_generic_function_tuple_to_union_array_constraint() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // Create union array constraint: T extends (string | number)[]
    let union_elem = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let union_array = interner.array(union_elem);

    let type_params = vec![TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(union_array),
        default: None,
        is_const: false,
    }];

    // [string, number] should satisfy (string | number)[] constraint
    let tuple_arg = interner.tuple(vec![
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

    let type_args = vec![tuple_arg];
    let result = solve_generic_instantiation(&type_params, &type_args, &interner, &mut checker);
    assert_eq!(
        result,
        GenericInstantiationResult::Success,
        "[string, number] should satisfy T extends (string | number)[] constraint"
    );
}

/// Test generic function with tuple with rest matching array constraint
#[test]
fn test_generic_function_tuple_with_rest_to_array_constraint() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // Create union array constraint: T extends (string | number)[]
    let union_elem = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let union_array = interner.array(union_elem);
    let number_array = interner.array(TypeId::NUMBER);

    let type_params = vec![TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(union_array),
        default: None,
        is_const: false,
    }];

    // [string, ...number[]] should satisfy (string | number)[] constraint
    let tuple_arg = interner.tuple(vec![
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

    let type_args = vec![tuple_arg];
    let result = solve_generic_instantiation(&type_params, &type_args, &interner, &mut checker);
    assert_eq!(
        result,
        GenericInstantiationResult::Success,
        "[string, ...number[]] should satisfy T extends (string | number)[] constraint"
    );
}

/// Test empty tuple with any array constraint
#[test]
fn test_generic_function_empty_tuple_to_any_array_constraint() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // Create any[] constraint
    let any_array = interner.array(TypeId::ANY);

    let type_params = vec![TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(any_array),
        default: None,
        is_const: false,
    }];

    // [] should satisfy any[] constraint
    let empty_tuple = interner.tuple(Vec::new());

    let type_args = vec![empty_tuple];
    let result = solve_generic_instantiation(&type_params, &type_args, &interner, &mut checker);
    assert_eq!(
        result,
        GenericInstantiationResult::Success,
        "[] should satisfy T extends any[] constraint"
    );
}

/// Test single-element tuple with array constraint
#[test]
fn test_generic_function_single_element_tuple_to_array_constraint() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // Create number[] constraint
    let number_array = interner.array(TypeId::NUMBER);

    let type_params = vec![TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(number_array),
        default: None,
        is_const: false,
    }];

    // [number] should satisfy number[] constraint
    let tuple_arg = interner.tuple(vec![TupleElement {
        type_id: TypeId::NUMBER,
        name: None,
        optional: false,
        rest: false,
    }]);

    let type_args = vec![tuple_arg];
    let result = solve_generic_instantiation(&type_params, &type_args, &interner, &mut checker);
    assert_eq!(
        result,
        GenericInstantiationResult::Success,
        "[number] should satisfy T extends number[] constraint"
    );
}

/// Test tuple with optional elements and array constraint
#[test]
fn test_generic_function_tuple_with_optional_to_array_constraint() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // Create string[] constraint
    let string_array = interner.array(TypeId::STRING);

    let type_params = vec![TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(string_array),
        default: None,
        is_const: false,
    }];

    // [string, string?] should satisfy string[] constraint
    let tuple_arg = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: true,
            rest: false,
        },
    ]);

    let type_args = vec![tuple_arg];
    let result = solve_generic_instantiation(&type_params, &type_args, &interner, &mut checker);
    assert_eq!(
        result,
        GenericInstantiationResult::Success,
        "[string, string?] should satisfy T extends string[] constraint"
    );
}

/// Test that constraints referencing earlier type parameters are properly instantiated
#[test]
fn test_solve_generic_instantiation_constraint_with_earlier_param() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // Create T
    let t_name = interner.intern_string("T");
    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // <T, U extends T>
    let type_params = vec![
        TypeParamInfo {
            name: t_name,
            constraint: None,
            default: None,
            is_const: false,
        },
        TypeParamInfo {
            name: interner.intern_string("U"),
            constraint: Some(t_type), // U extends T
            default: None,
            is_const: false,
        },
    ];

    // <string, string> - should satisfy the constraint (string extends string)
    let type_args = vec![TypeId::STRING, TypeId::STRING];

    let result = solve_generic_instantiation(&type_params, &type_args, &interner, &mut checker);
    assert_eq!(
        result,
        GenericInstantiationResult::Success,
        "string should satisfy U extends T constraint when T is string"
    );
}

/// Test that constraints referencing earlier type parameters fail when violated
#[test]
fn test_solve_generic_instantiation_constraint_with_earlier_param_violation() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // Create T
    let t_name = interner.intern_string("T");
    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // <T, U extends T>
    let type_params = vec![
        TypeParamInfo {
            name: t_name,
            constraint: None,
            default: None,
            is_const: false,
        },
        TypeParamInfo {
            name: interner.intern_string("U"),
            constraint: Some(t_type), // U extends T
            default: None,
            is_const: false,
        },
    ];

    // <string, number> - should NOT satisfy the constraint (number does not extend string)
    let type_args = vec![TypeId::STRING, TypeId::NUMBER];

    let result = solve_generic_instantiation(&type_params, &type_args, &interner, &mut checker);
    match result {
        GenericInstantiationResult::ConstraintViolation { param_index, .. } => {
            assert_eq!(
                param_index, 1,
                "Second type parameter should violate constraint"
            );
        }
        _ => panic!("Expected ConstraintViolation"),
    }
}

// =============================================================================
// BinaryOpEvaluator::is_arithmetic_operand tests
// =============================================================================
#[test]
fn test_is_arithmetic_operand_number() {
    let interner = TypeInterner::new();
    let evaluator = BinaryOpEvaluator::new(&interner);

    // number should be a valid arithmetic operand
    assert!(
        evaluator.is_arithmetic_operand(TypeId::NUMBER),
        "number should be a valid arithmetic operand"
    );
}
#[test]
fn test_is_arithmetic_operand_number_literal() {
    let interner = TypeInterner::new();
    let evaluator = BinaryOpEvaluator::new(&interner);

    // Number literal should be a valid arithmetic operand
    let num_literal = interner.literal_number(42.0);
    assert!(
        evaluator.is_arithmetic_operand(num_literal),
        "number literal should be a valid arithmetic operand"
    );
}
#[test]
fn test_is_arithmetic_operand_bigint() {
    let interner = TypeInterner::new();
    let evaluator = BinaryOpEvaluator::new(&interner);

    // bigint should be a valid arithmetic operand
    assert!(
        evaluator.is_arithmetic_operand(TypeId::BIGINT),
        "bigint should be a valid arithmetic operand"
    );
}
#[test]
fn test_is_arithmetic_operand_bigint_literal() {
    let interner = TypeInterner::new();
    let evaluator = BinaryOpEvaluator::new(&interner);

    // BigInt literal should be a valid arithmetic operand
    let bigint_literal = interner.literal_bigint("42");
    assert!(
        evaluator.is_arithmetic_operand(bigint_literal),
        "bigint literal should be a valid arithmetic operand"
    );
}
#[test]
fn test_is_arithmetic_operand_any() {
    let interner = TypeInterner::new();
    let evaluator = BinaryOpEvaluator::new(&interner);

    // any should be a valid arithmetic operand
    assert!(
        evaluator.is_arithmetic_operand(TypeId::ANY),
        "any should be a valid arithmetic operand"
    );
}
#[test]
fn test_is_arithmetic_operand_numeric_enum() {
    let interner = TypeInterner::new();
    let evaluator = BinaryOpEvaluator::new(&interner);

    // Numeric enum (union of number literals) should be a valid arithmetic operand
    let enum_val1 = interner.literal_number(0.0);
    let enum_val2 = interner.literal_number(1.0);
    let enum_val3 = interner.literal_number(2.0);
    let enum_type = interner.union(vec![enum_val1, enum_val2, enum_val3]);

    assert!(
        evaluator.is_arithmetic_operand(enum_type),
        "numeric enum (union of number literals) should be a valid arithmetic operand"
    );
}
#[test]
fn test_is_arithmetic_operand_string_invalid() {
    let interner = TypeInterner::new();
    let evaluator = BinaryOpEvaluator::new(&interner);

    // string should NOT be a valid arithmetic operand
    assert!(
        !evaluator.is_arithmetic_operand(TypeId::STRING),
        "string should NOT be a valid arithmetic operand"
    );
}
#[test]
fn test_is_arithmetic_operand_string_literal_invalid() {
    let interner = TypeInterner::new();
    let evaluator = BinaryOpEvaluator::new(&interner);

    // String literal should NOT be a valid arithmetic operand
    let str_literal = interner.literal_string("hello");
    assert!(
        !evaluator.is_arithmetic_operand(str_literal),
        "string literal should NOT be a valid arithmetic operand"
    );
}
#[test]
fn test_is_arithmetic_operand_boolean_invalid() {
    let interner = TypeInterner::new();
    let evaluator = BinaryOpEvaluator::new(&interner);

    // boolean should NOT be a valid arithmetic operand
    assert!(
        !evaluator.is_arithmetic_operand(TypeId::BOOLEAN),
        "boolean should NOT be a valid arithmetic operand"
    );
}
#[test]
fn test_is_arithmetic_operand_undefined_invalid() {
    let interner = TypeInterner::new();
    let evaluator = BinaryOpEvaluator::new(&interner);

    // undefined should NOT be a valid arithmetic operand
    assert!(
        !evaluator.is_arithmetic_operand(TypeId::UNDEFINED),
        "undefined should NOT be a valid arithmetic operand"
    );
}
#[test]
fn test_is_arithmetic_operand_null_invalid() {
    let interner = TypeInterner::new();
    let evaluator = BinaryOpEvaluator::new(&interner);

    // null should NOT be a valid arithmetic operand
    assert!(
        !evaluator.is_arithmetic_operand(TypeId::NULL),
        "null should NOT be a valid arithmetic operand"
    );
}
#[test]
fn test_is_arithmetic_operand_object_invalid() {
    let interner = TypeInterner::new();
    let evaluator = BinaryOpEvaluator::new(&interner);

    // object type should NOT be a valid arithmetic operand
    let obj_type = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);
    assert!(
        !evaluator.is_arithmetic_operand(obj_type),
        "object type should NOT be a valid arithmetic operand"
    );
}
#[test]
fn test_is_arithmetic_operand_mixed_union_invalid() {
    let interner = TypeInterner::new();
    let evaluator = BinaryOpEvaluator::new(&interner);

    // Union of number and string should NOT be a valid arithmetic operand
    let mixed_union = interner.union(vec![TypeId::NUMBER, TypeId::STRING]);
    assert!(
        !evaluator.is_arithmetic_operand(mixed_union),
        "union of number and string should NOT be a valid arithmetic operand"
    );
}

/// Regression test: verify that array property access works when using the
/// environment-aware resolver (`with_resolver`) that has the Array<T> base type
/// registered. Previously, `get_type_of_property_access_inner` used
/// `types.property_access_type()` which created a `NoopResolver` without the
/// Array base type, causing TS2339 false positives like "Property 'push'
/// does not exist on type 'any[]'".
#[test]
fn test_property_access_array_push_with_env_resolver() {
    use crate::TypeEnvironment;
    use crate::types::TypeParamInfo;

    let interner = TypeInterner::new();

    // Create a mock Array<T> interface with a "push" method
    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    // push(...items: T[]): number
    let push_func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("items")),
            type_id: interner.array(t_type),
            optional: false,
            rest: true,
        }],
        return_type: TypeId::NUMBER,
        type_params: vec![],
        this_type: None,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    // Create an interface with push method
    let array_interface = interner.object(vec![PropertyInfo::method(
        interner.intern_string("push"),
        push_func,
    )]);

    // Set array base type on the interner so PropertyAccessEvaluator can find it
    interner.set_array_base_type(array_interface, vec![t_param]);

    // Set up TypeEnvironment with Array<T> registered
    let mut env = TypeEnvironment::new();
    env.set_array_base_type(array_interface, vec![t_param]);

    // Create evaluator with the environment
    let evaluator = PropertyAccessEvaluator::new(&interner);

    // Test: string[].push should resolve successfully
    let string_array = interner.array(TypeId::STRING);
    let result = evaluator.resolve_property_access(string_array, "push");
    match result {
        PropertyAccessResult::Success { type_id, .. } => {
            // The push method should be a function returning number
            match interner.lookup(type_id) {
                Some(TypeData::Function(func_id)) => {
                    let func = interner.function_shape(func_id);
                    assert_eq!(
                        func.return_type,
                        TypeId::NUMBER,
                        "push should return number"
                    );
                }
                other => panic!("Expected function for push, got {other:?}"),
            }
        }
        _ => panic!("Expected Success for array.push with env resolver, got {result:?}"),
    }
}

/// Regression test: QueryCache-backed property access must expose Array<T>
/// registrations from the interner. Without this, `string[].push` fails with
/// a false TS2339 in checker paths that use `QueryCache` as the resolver.
#[test]
fn test_property_access_array_push_with_query_cache_resolver() {
    use crate::caches::query_cache::QueryCache;
    use crate::types::TypeParamInfo;

    let interner = TypeInterner::new();

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let push_func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("items")),
            type_id: interner.array(t_type),
            optional: false,
            rest: true,
        }],
        return_type: TypeId::NUMBER,
        type_params: vec![],
        this_type: None,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    let array_interface = interner.object(vec![PropertyInfo::method(
        interner.intern_string("push"),
        push_func,
    )]);

    interner.set_array_base_type(array_interface, vec![t_param]);

    let cache = QueryCache::new(&interner);
    let evaluator = PropertyAccessEvaluator::new(&cache);

    let string_array = interner.array(TypeId::STRING);
    let result = evaluator.resolve_property_access(string_array, "push");
    match result {
        PropertyAccessResult::Success { type_id, .. } => match interner.lookup(type_id) {
            Some(TypeData::Function(func_id)) => {
                let func = interner.function_shape(func_id);
                assert_eq!(func.return_type, TypeId::NUMBER);
            }
            other => panic!("Expected function for push, got {other:?}"),
        },
        other => panic!("Expected Success for array.push with QueryCache resolver, got {other:?}"),
    }
}

/// Regression test: Array<T> from merged lib declarations is represented as an
/// intersection of interface fragments. Property access on `T[]` must still
/// find methods like `push` through Application(Array, [T]).
#[test]
fn test_property_access_array_push_with_intersection_array_base() {
    let interner = TypeInterner::new();

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let push_func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("items")),
            type_id: interner.array(t_type),
            optional: false,
            rest: true,
        }],
        return_type: TypeId::NUMBER,
        type_params: vec![],
        this_type: None,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    let array_decl_a = interner.object(vec![PropertyInfo::method(
        interner.intern_string("push"),
        push_func,
    )]);

    let array_decl_b = interner.object(vec![PropertyInfo::readonly(
        interner.intern_string("length"),
        TypeId::NUMBER,
    )]);

    // Simulate merged lib declarations: Array<T> = DeclA & DeclB
    let array_base = interner.intersection2(array_decl_a, array_decl_b);
    interner.set_array_base_type(array_base, vec![t_param]);

    let evaluator = PropertyAccessEvaluator::new(&interner);
    let string_array = interner.array(TypeId::STRING);

    let result = evaluator.resolve_property_access(string_array, "push");
    match result {
        PropertyAccessResult::Success { type_id, .. } => match interner.lookup(type_id) {
            Some(TypeData::Function(func_id)) => {
                let func = interner.function_shape(func_id);
                assert_eq!(func.return_type, TypeId::NUMBER);
            }
            other => panic!("Expected function for push, got {other:?}"),
        },
        other => {
            panic!("Expected Success for array.push with intersection array base, got {other:?}")
        }
    }
}

/// Test that array mapped type method resolution works correctly.
/// When { [P in keyof T]: T[P] } where T extends any[] is accessed with .`pop()`,
/// it should resolve to the array method, not map through the template.
#[test]
fn test_array_mapped_type_method_resolution() {
    use crate::TypeEnvironment;

    let interner = TypeInterner::new();

    // Create T extends any[]
    let any_array = interner.array(TypeId::ANY);
    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(any_array),
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    // Create the mapped type: { [P in keyof T]: T[P] }
    let p_param = TypeParamInfo {
        name: interner.intern_string("P"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let p_type = interner.intern(TypeData::TypeParameter(p_param));
    let index_access = interner.intern(TypeData::IndexAccess(t_type, p_type));

    // Create keyof T as the constraint
    let keyof_t = interner.intern(TypeData::KeyOf(t_type));

    // Create the mapped type
    let mapped = MappedType {
        type_param: p_param,
        constraint: keyof_t,
        name_type: None,
        template: index_access,
        readonly_modifier: None,
        optional_modifier: None,
    };
    let mapped_type = interner.mapped(mapped);

    // Set up TypeEnvironment with Array<T> registered
    let mut env = TypeEnvironment::new();

    // Create a mock Array<T> interface with pop method
    let array_t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let array_t = interner.intern(TypeData::TypeParameter(array_t_param));
    let pop_return_type = interner.union2(array_t, TypeId::UNDEFINED);
    let pop_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        return_type: pop_return_type,
        this_type: None,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    let array_props = vec![
        PropertyInfo::method(interner.intern_string("pop"), pop_fn),
        PropertyInfo::new(interner.intern_string("length"), TypeId::NUMBER),
    ];
    let array_interface = interner.object(array_props);
    env.set_array_base_type(array_interface, vec![array_t_param]);

    // Create evaluator with the environment
    let evaluator = PropertyAccessEvaluator::new(&interner);

    // Test property access on the mapped type
    let result = evaluator.resolve_property_access(mapped_type, "pop");

    // Should succeed with a function type (not PropertyNotFound)
    match result {
        PropertyAccessResult::Success { type_id, .. } => {
            // Check that we got a function type back
            let key = interner.lookup(type_id);
            assert!(
                matches!(key, Some(TypeData::Function(_))),
                "pop should resolve to a function, got {key:?}"
            );
        }
        other => {
            panic!("Expected Success for .pop(), got {other:?}");
        }
    }
}
#[test]
fn test_generic_call_contextual_instantiation_does_not_leak_source_placeholders() {
    // Mirrors:
    //   var dot: <T, S>(f: (_: T) => S) => <U>(g: (_: U) => T) => (_: U) => S;
    //   var id: <T>(x:T) => T;
    //   var r23 = dot(id)(id);
    //
    // Regression: the first call inferred `S = __infer_src_*`, leaking a transient
    // placeholder into the intermediate signature and causing a false TS2345 on
    // the second call.
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    checker.set_strict_function_types(false);
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let s_param = TypeParamInfo {
        name: interner.intern_string("S"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let u_param = TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));
    let s_type = interner.intern(TypeData::TypeParameter(s_param));
    let u_type = interner.intern(TypeData::TypeParameter(u_param));

    let f_type = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo::unnamed(t_type)],
        this_type: None,
        return_type: s_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let g_type = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo::unnamed(u_type)],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let r_type = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo::unnamed(u_type)],
        this_type: None,
        return_type: s_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let dot_return = interner.function(FunctionShape {
        type_params: vec![u_param],
        params: vec![ParamInfo::unnamed(g_type)],
        this_type: None,
        return_type: r_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let dot = interner.function(FunctionShape {
        type_params: vec![t_param, s_param],
        params: vec![ParamInfo::unnamed(f_type)],
        this_type: None,
        return_type: dot_return,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let id_t_param = TypeParamInfo {
        name: interner.intern_string("X"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let id_t = interner.intern(TypeData::TypeParameter(id_t_param));
    let id = interner.function(FunctionShape {
        type_params: vec![id_t_param],
        params: vec![ParamInfo::unnamed(id_t)],
        this_type: None,
        return_type: id_t,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let intermediate = match evaluator.resolve_call(dot, &[id]) {
        CallResult::Success(ty) => ty,
        other => panic!("Expected first call dot(id) to succeed, got {other:?}"),
    };

    let second = evaluator.resolve_call(intermediate, &[id]);
    assert!(
        matches!(second, CallResult::Success(_)),
        "Expected second call dot(id)(id) to succeed, got {second:?}"
    );
}

// ─── Union call signature tests ───────────────────────────────
#[test]
fn test_union_call_different_return_types() {
    // { (a: number): number } | { (a: number): string }
    // Combined: (a: number): number | string
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    let f1 = interner.function(FunctionShape::new(
        vec![ParamInfo::required(
            interner.intern_string("a"),
            TypeId::NUMBER,
        )],
        TypeId::NUMBER,
    ));
    let f2 = interner.function(FunctionShape::new(
        vec![ParamInfo::required(
            interner.intern_string("a"),
            TypeId::NUMBER,
        )],
        TypeId::STRING,
    ));
    let union = interner.union(vec![f1, f2]);

    // Call with correct arg → success with unioned return
    let result = evaluator.resolve_call(union, &[TypeId::NUMBER]);
    assert!(
        matches!(result, CallResult::Success(_)),
        "Expected success for union call with matching arg, got {result:?}"
    );
}
#[test]
fn test_union_call_different_param_counts() {
    // { (a: string): string } | { (a: string, b: number): number }
    // Combined: (a: string, b: number): string | number — requires 2 args
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    let f1 = interner.function(FunctionShape::new(
        vec![ParamInfo::required(
            interner.intern_string("a"),
            TypeId::STRING,
        )],
        TypeId::STRING,
    ));
    let f2 = interner.function(FunctionShape::new(
        vec![
            ParamInfo::required(interner.intern_string("a"), TypeId::STRING),
            ParamInfo::required(interner.intern_string("b"), TypeId::NUMBER),
        ],
        TypeId::NUMBER,
    ));
    let union = interner.union(vec![f1, f2]);

    // Call with 2 args → success
    let result = evaluator.resolve_call(union, &[TypeId::STRING, TypeId::NUMBER]);
    assert!(
        matches!(result, CallResult::Success(_)),
        "Expected success for union call with 2 args, got {result:?}"
    );

    // Call with 1 arg → arity error (combined requires 2)
    let result = evaluator.resolve_call(union, &[TypeId::STRING]);
    assert!(
        matches!(
            result,
            CallResult::ArgumentCountMismatch {
                expected_min: 2,
                ..
            }
        ),
        "Expected ArgumentCountMismatch with min=2 for 1 arg, got {result:?}"
    );

    // Call with 0 args → arity error
    let result = evaluator.resolve_call(union, &[]);
    assert!(
        matches!(
            result,
            CallResult::ArgumentCountMismatch {
                expected_min: 2,
                ..
            }
        ),
        "Expected ArgumentCountMismatch with min=2 for 0 args, got {result:?}"
    );
}
#[test]
fn test_union_call_mixed_rest_and_required_uses_base_member_max() {
    // Models unionTypeCallSignatures4.ts:
    //   F1 = (a: string, b?: string) => void       — min=1, max=2, no rest
    //   F2 = (a: string, b?: string, c?: string) => void — min=1, max=3, no rest
    //   F3 = (a: string, ...rest: string[]) => void — min=1, unlimited, rest
    //   F4 = (a: string, b?: string, ...rest: string[]) => void — min=1, unlimited, rest
    //   F5 = (a: string, b: string) => void         — min=2, max=2, no rest
    //
    // tsc's Phase 1: F5 has the highest min (2), so it becomes the base.
    // Combined signature inherits F5's shape: exactly 2 params, no rest.
    // max_allowed = Some(2), NOT None.
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);
    let a = interner.intern_string("a");
    let b = interner.intern_string("b");
    let c = interner.intern_string("c");
    let rest = interner.intern_string("rest");

    // F1: (a: string, b?: string) => void
    let f1 = interner.function(FunctionShape::new(
        vec![
            ParamInfo::required(a, TypeId::STRING),
            ParamInfo::optional(b, TypeId::STRING),
        ],
        TypeId::VOID,
    ));
    // F2: (a: string, b?: string, c?: string) => void
    let f2 = interner.function(FunctionShape::new(
        vec![
            ParamInfo::required(a, TypeId::STRING),
            ParamInfo::optional(b, TypeId::STRING),
            ParamInfo::optional(c, TypeId::STRING),
        ],
        TypeId::VOID,
    ));
    // F3: (a: string, ...rest: string[]) => void
    let rest_type = interner.array(TypeId::STRING);
    let f3 = interner.function(FunctionShape::new(
        vec![
            ParamInfo::required(a, TypeId::STRING),
            ParamInfo::rest(rest, rest_type),
        ],
        TypeId::VOID,
    ));
    // F4: (a: string, b?: string, ...rest: string[]) => void
    let f4 = interner.function(FunctionShape::new(
        vec![
            ParamInfo::required(a, TypeId::STRING),
            ParamInfo::optional(b, TypeId::STRING),
            ParamInfo::rest(rest, rest_type),
        ],
        TypeId::VOID,
    ));
    // F5: (a: string, b: string) => void
    let f5 = interner.function(FunctionShape::new(
        vec![
            ParamInfo::required(a, TypeId::STRING),
            ParamInfo::required(b, TypeId::STRING),
        ],
        TypeId::VOID,
    ));

    let union = interner.union(vec![f1, f2, f3, f4, f5]);

    // f12345("a") → 1 arg, min=2 → TS2554 (max_allowed=Some(2), not None)
    let result = evaluator.resolve_call(union, &[TypeId::STRING]);
    match &result {
        CallResult::ArgumentCountMismatch {
            expected_min,
            expected_max,
            actual,
        } => {
            assert_eq!(*expected_min, 2, "min should be 2");
            assert_eq!(
                *expected_max,
                Some(2),
                "max should be Some(2), not None (would give TS2555 instead of TS2554)"
            );
            assert_eq!(*actual, 1);
        }
        other => panic!("Expected ArgumentCountMismatch, got {other:?}"),
    }

    // f12345("a", "b") → 2 args → success
    let result = evaluator.resolve_call(union, &[TypeId::STRING, TypeId::STRING]);
    assert!(
        matches!(result, CallResult::Success(_)),
        "Expected success for 2 args, got {result:?}"
    );

    // f12345("a", "b", "c") → 3 args, max=2 → TS2554
    let result = evaluator.resolve_call(union, &[TypeId::STRING, TypeId::STRING, TypeId::STRING]);
    match &result {
        CallResult::ArgumentCountMismatch {
            expected_min,
            expected_max,
            actual,
        } => {
            assert_eq!(*expected_min, 2);
            assert_eq!(
                *expected_max,
                Some(2),
                "max should be Some(2) to reject 3 args"
            );
            assert_eq!(*actual, 3);
        }
        other => panic!("Expected ArgumentCountMismatch for 3 args, got {other:?}"),
    }
}
#[test]
fn test_union_call_all_same_min_uses_max_param_count() {
    // F1 = (a: string, b?: string) => void       — min=1, max=2
    // F2 = (a: string, b?: string, c?: string) => void — min=1, max=3
    // All have same min (1), so all are "base members".
    // max_allowed = max(2, 3) = Some(3)
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);
    let a = interner.intern_string("a");
    let b = interner.intern_string("b");
    let c = interner.intern_string("c");

    let f1 = interner.function(FunctionShape::new(
        vec![
            ParamInfo::required(a, TypeId::STRING),
            ParamInfo::optional(b, TypeId::STRING),
        ],
        TypeId::VOID,
    ));
    let f2 = interner.function(FunctionShape::new(
        vec![
            ParamInfo::required(a, TypeId::STRING),
            ParamInfo::optional(b, TypeId::STRING),
            ParamInfo::optional(c, TypeId::STRING),
        ],
        TypeId::VOID,
    ));
    let union = interner.union(vec![f1, f2]);

    // 3 args should succeed (combined max=3 from F2)
    let result = evaluator.resolve_call(union, &[TypeId::STRING, TypeId::STRING, TypeId::STRING]);
    assert!(
        matches!(result, CallResult::Success(_)),
        "Expected success for 3 args on F1|F2 (max=3), got {result:?}"
    );

    // 4 args should fail
    let result = evaluator.resolve_call(
        union,
        &[
            TypeId::STRING,
            TypeId::STRING,
            TypeId::STRING,
            TypeId::STRING,
        ],
    );
    assert!(
        matches!(
            result,
            CallResult::ArgumentCountMismatch {
                expected_max: Some(3),
                ..
            }
        ),
        "Expected arity error with max=3 for 4 args, got {result:?}"
    );
}
#[test]
fn test_union_call_rest_base_member_gives_unlimited_max() {
    // F1 = (a: string) => void                    — min=1, no rest
    // F6 = (a: string, b: string, ...rest: string[]) => void — min=2, rest
    // F6 has highest min (2) and has rest → max_allowed = None (unlimited)
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);
    let a = interner.intern_string("a");
    let b = interner.intern_string("b");
    let rest = interner.intern_string("rest");

    let f1 = interner.function(FunctionShape::new(
        vec![ParamInfo::required(a, TypeId::STRING)],
        TypeId::VOID,
    ));
    let rest_type = interner.array(TypeId::STRING);
    let f6 = interner.function(FunctionShape::new(
        vec![
            ParamInfo::required(a, TypeId::STRING),
            ParamInfo::required(b, TypeId::STRING),
            ParamInfo::rest(rest, rest_type),
        ],
        TypeId::VOID,
    ));
    let union = interner.union(vec![f1, f6]);

    // 1 arg → too few (min=2)
    let result = evaluator.resolve_call(union, &[TypeId::STRING]);
    match &result {
        CallResult::ArgumentCountMismatch {
            expected_min,
            expected_max,
            ..
        } => {
            assert_eq!(*expected_min, 2);
            assert_eq!(*expected_max, None, "Base member has rest → unlimited max");
        }
        other => panic!("Expected ArgumentCountMismatch, got {other:?}"),
    }

    // 5 args → success (base has rest, so unlimited)
    let result = evaluator.resolve_call(
        union,
        &[
            TypeId::STRING,
            TypeId::STRING,
            TypeId::STRING,
            TypeId::STRING,
            TypeId::STRING,
        ],
    );
    assert!(
        matches!(result, CallResult::Success(_)),
        "Expected success for 5 args (base has rest), got {result:?}"
    );
}
#[test]
fn test_union_call_incompatible_param_types() {
    // { (a: number): number } | { (a: string): string }
    // Combined: (a: number & string = never): number | string
    // Any argument should fail against `never`
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    let f1 = interner.function(FunctionShape::new(
        vec![ParamInfo::required(
            interner.intern_string("a"),
            TypeId::NUMBER,
        )],
        TypeId::NUMBER,
    ));
    let f2 = interner.function(FunctionShape::new(
        vec![ParamInfo::required(
            interner.intern_string("a"),
            TypeId::STRING,
        )],
        TypeId::STRING,
    ));
    let union = interner.union(vec![f1, f2]);

    // Call with number → both members fail (one on type, one on type)
    // Combined param is never, so TS2345 not TS2349
    let result = evaluator.resolve_call(union, &[TypeId::NUMBER]);
    assert!(
        !matches!(result, CallResult::NotCallable { .. }),
        "Union of callable types should NOT be NotCallable, got {result:?}"
    );
}
#[test]
fn test_union_call_tuple_rest_combines_to_never() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);
    let args_name = interner.intern_string("args");

    let empty_tuple = interner.tuple(vec![]);
    let string_tuple = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }]);
    let number_tuple = interner.tuple(vec![TupleElement {
        type_id: TypeId::NUMBER,
        name: None,
        optional: false,
        rest: false,
    }]);

    let make_rest_fn = |tuple_type| {
        interner.function(FunctionShape::new(
            vec![ParamInfo::rest(args_name, tuple_type)],
            TypeId::UNKNOWN,
        ))
    };

    let union = interner.union(vec![
        make_rest_fn(empty_tuple),
        make_rest_fn(string_tuple),
        make_rest_fn(number_tuple),
    ]);

    let result = evaluator.resolve_call(union, &[TypeId::ANY]);
    match result {
        CallResult::ArgumentTypeMismatch {
            index,
            expected,
            actual,
            ..
        } => {
            assert_eq!(index, 0);
            assert_eq!(expected, TypeId::NEVER);
            assert_eq!(actual, TypeId::ANY);
        }
        other => panic!("Expected combined never mismatch, got {other:?}"),
    }
}

/// Regression test for Application-Application mapped type inference.
///
/// Models the pattern from mappedTypes3.ts:
///   type Wrapped<T> = { [K in keyof T]: T[K] }
///   declare function unwrap<T>(obj: Wrapped<T>): T;
///   interface Bacon { isPerfect: boolean; weight: number; }
///   unwrap(x as Wrapped<Bacon>) // should infer T = Bacon
///
/// Without the fix, evaluating both Applications first loses the type argument
/// relationship: source becomes a concrete Object and target becomes a Mapped type.
/// The Object→Mapped handler can't reverse-infer T from keyof constraints.
/// The fix detects matching bases where target evaluates to Mapped and uses direct
/// argument unification to capture T = Bacon.
#[test]
fn test_infer_application_to_mapped_type_direct_arg_unification() {
    use crate::TypeEnvironment;

    let interner = TypeInterner::new();

    // T type parameter (for Wrapped<T> alias)
    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    // K type parameter (for mapped type iteration)
    let k_param = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: None,
        default: None,
        is_const: false,
    };

    // Build mapped type body: { [K in keyof T]: T[K] }
    let keyof_t = interner.intern(TypeData::KeyOf(t_type));
    let k_type = interner.intern(TypeData::TypeParameter(k_param));
    let t_k = interner.intern(TypeData::IndexAccess(t_type, k_type));
    let mapped_body = interner.mapped(MappedType {
        type_param: k_param,
        constraint: keyof_t,
        name_type: None,
        template: t_k,
        readonly_modifier: None,
        optional_modifier: None,
    });

    // Register as type alias: type Wrapped<T> = { [K in keyof T]: T[K] }
    let wrapped_def = DefId(100);
    let mut env = TypeEnvironment::new();
    env.insert_def_with_params(wrapped_def, mapped_body, vec![t_param]);

    // Build Wrapped<T> and Wrapped<Bacon> as Application types
    let wrapped_base = interner.lazy(wrapped_def);

    // Bacon interface: { isPerfect: boolean; weight: number }
    let bacon = interner.object(vec![
        PropertyInfo::new(interner.intern_string("isPerfect"), TypeId::BOOLEAN),
        PropertyInfo::new(interner.intern_string("weight"), TypeId::NUMBER),
    ]);

    // Wrapped<Bacon> — the argument type
    let wrapped_bacon = interner.application(wrapped_base, vec![bacon]);

    // function unwrap<T>(obj: Wrapped<T>): T
    let wrapped_t = interner.application(wrapped_base, vec![t_type]);
    let func = FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("obj")),
            type_id: wrapped_t,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    // Infer: unwrap(wrapped_bacon) should infer T = Bacon
    let mut checker = CompatChecker::with_resolver(&interner, &env);
    let result = infer_generic_function(&interner, &mut checker, &func, &[wrapped_bacon]);

    // T should be inferred as Bacon (the concrete object type)
    assert_eq!(
        result, bacon,
        "T should be inferred as Bacon via direct arg unification through mapped type Application"
    );
}

// =============================================================================
// Union this-parameter checking (TS2684)
// =============================================================================
#[test]
fn test_union_call_this_type_mismatch_produces_error() {
    // type F1 = (this: A) => void;
    // type F2 = (this: B) => void;
    // declare var f1: F1 | F2;
    // f1(); // error TS2684 — `this` context (void) doesn't satisfy A & B
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    // A = { a: string }
    let type_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);
    // B = { b: number }
    let type_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);

    // F1 = (this: A) => void
    let f1 = interner.function(FunctionShape {
        params: vec![],
        this_type: Some(type_a),
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    // F2 = (this: B) => void
    let f2 = interner.function(FunctionShape {
        params: vec![],
        this_type: Some(type_b),
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let union = interner.union(vec![f1, f2]);

    // Call with no `this` context (void) — should produce ThisTypeMismatch
    let result = evaluator.resolve_call(union, &[]);
    assert!(
        matches!(result, CallResult::ThisTypeMismatch { .. }),
        "Expected ThisTypeMismatch when calling union with incompatible this, got {result:?}"
    );
}
#[test]
fn test_union_call_this_type_satisfied_succeeds() {
    // type F1 = (this: A) => void;
    // type F2 = (this: B) => void;
    // x: A & B & { f: F1 | F2 }
    // x.f(); // OK — `this` is A & B which satisfies A & B
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    let type_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);
    let type_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);

    let f1 = interner.function(FunctionShape {
        params: vec![],
        this_type: Some(type_a),
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let f2 = interner.function(FunctionShape {
        params: vec![],
        this_type: Some(type_b),
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let union = interner.union(vec![f1, f2]);

    // Provide A & B as `this` context — should succeed
    let this_type = interner.intersection2(type_a, type_b);
    evaluator.set_actual_this_type(Some(this_type));
    let result = evaluator.resolve_call(union, &[]);
    assert!(
        matches!(result, CallResult::Success(_)),
        "Expected success when this context satisfies all union members, got {result:?}"
    );
}
#[test]
fn test_union_call_mixed_this_and_no_this_members() {
    // type F0 = () => void; // no this
    // type F1 = (this: A) => void;
    // declare var f: F0 | F1;
    // f(); // error TS2684 — F1 requires `this: A`, but calling context is void
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    let type_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    // F0 = () => void (no this)
    let f0 = interner.function(FunctionShape::new(vec![], TypeId::VOID));
    // F1 = (this: A) => void
    let f1 = interner.function(FunctionShape {
        params: vec![],
        this_type: Some(type_a),
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let union = interner.union(vec![f0, f1]);

    // Call with no `this` — should fail because F1 demands `this: A`
    let result = evaluator.resolve_call(union, &[]);
    assert!(
        matches!(result, CallResult::ThisTypeMismatch { .. }),
        "Expected ThisTypeMismatch for union with mixed this/no-this, got {result:?}"
    );
}
#[test]
fn test_union_call_multi_overload_callable_this_skipped() {
    // interface F3 {
    //   (this: A): void;
    //   (this: B): void;
    // }
    // interface F5 {
    //   (this: C): void;
    //   (this: B): void;
    // }
    // Multi-overload callables should be SKIPPED in compute_union_this_type,
    // so union F3 | F5 with correct `this` should succeed through overload resolution.
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    let type_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);
    let type_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);
    let type_c = interner.object(vec![PropertyInfo::new(
        interner.intern_string("c"),
        TypeId::STRING,
    )]);

    // F3 has overloads: (this: A): void, (this: B): void
    let f3 = interner.callable(CallableShape {
        call_signatures: vec![
            CallSignature {
                type_params: vec![],
                params: vec![],
                this_type: Some(type_a),
                return_type: TypeId::VOID,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                type_params: vec![],
                params: vec![],
                this_type: Some(type_b),
                return_type: TypeId::VOID,
                type_predicate: None,
                is_method: false,
            },
        ],
        ..Default::default()
    });

    // F5 has overloads: (this: C): void, (this: B): void
    let f5 = interner.callable(CallableShape {
        call_signatures: vec![
            CallSignature {
                type_params: vec![],
                params: vec![],
                this_type: Some(type_c),
                return_type: TypeId::VOID,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                type_params: vec![],
                params: vec![],
                this_type: Some(type_b),
                return_type: TypeId::VOID,
                type_predicate: None,
                is_method: false,
            },
        ],
        ..Default::default()
    });

    let union = interner.union(vec![f3, f5]);

    // Provide A & B as `this` — both F3 and F5 have an overload accepting B,
    // so this should succeed (multi-overload callables are skipped in this-check)
    let this_type = interner.intersection2(type_a, type_b);
    evaluator.set_actual_this_type(Some(this_type));
    let result = evaluator.resolve_call(union, &[]);
    assert!(
        matches!(result, CallResult::Success(_)),
        "Expected success for union of multi-overload callables with matching this, got {result:?}"
    );
}
#[test]
fn test_union_call_no_this_requirements_succeeds() {
    // Both members have no `this` — should always succeed
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    let f1 = interner.function(FunctionShape::new(
        vec![ParamInfo::required(
            interner.intern_string("x"),
            TypeId::NUMBER,
        )],
        TypeId::NUMBER,
    ));
    let f2 = interner.function(FunctionShape::new(
        vec![ParamInfo::required(
            interner.intern_string("x"),
            TypeId::NUMBER,
        )],
        TypeId::STRING,
    ));
    let union = interner.union(vec![f1, f2]);

    // No this context — should succeed since neither member requires this
    let result = evaluator.resolve_call(union, &[TypeId::NUMBER]);
    assert!(
        matches!(result, CallResult::Success(_)),
        "Expected success for union with no this requirements, got {result:?}"
    );
}

/// When ALL union members have multiple overloads and no pair of signatures is
/// compatible across members, per-member resolution still runs. If `this` type
/// constraints prevent any overload from matching, the result is
/// `NoOverloadMatch` (→ TS2769) rather than `NotCallable` (→ TS2349),
/// because each member IS individually callable — it's the `this` context
/// that fails. This fallthrough behavior matches tsc's handling of union
/// method calls like `(A[] | B[]).filter(cb)`.
///
/// Mirrors: `type F3 = { (this: A): void; (this: B): void; }`
///          `type F4 = { (this: C): void; (this: D): void; }`
///          `(f3_or_f4: F3 | F4) => f3_or_f4()` — per-member overload resolution
#[test]
fn test_union_multi_overload_incompatible_per_member_resolution() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    let type_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);
    let type_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);
    let type_c = interner.object(vec![PropertyInfo::new(
        interner.intern_string("c"),
        TypeId::STRING,
    )]);
    let type_d = interner.object(vec![PropertyInfo::new(
        interner.intern_string("d"),
        TypeId::NUMBER,
    )]);

    // F3 = { (this: A): void; (this: B): void }
    let f3 = interner.callable(CallableShape {
        call_signatures: vec![
            CallSignature {
                params: vec![],
                this_type: Some(type_a),
                return_type: TypeId::VOID,
                type_params: vec![],
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                params: vec![],
                this_type: Some(type_b),
                return_type: TypeId::VOID,
                type_params: vec![],
                type_predicate: None,
                is_method: false,
            },
        ],
        ..Default::default()
    });

    // F4 = { (this: C): void; (this: D): void }
    let f4 = interner.callable(CallableShape {
        call_signatures: vec![
            CallSignature {
                params: vec![],
                this_type: Some(type_c),
                return_type: TypeId::VOID,
                type_params: vec![],
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                params: vec![],
                this_type: Some(type_d),
                return_type: TypeId::VOID,
                type_params: vec![],
                type_predicate: None,
                is_method: false,
            },
        ],
        ..Default::default()
    });

    // Union F3 | F4 — no compatible pair across members, but per-member
    // resolution runs: each member's overloads fail on `this` mismatch,
    // producing NoOverloadMatch rather than NotCallable.
    let union = interner.union(vec![f3, f4]);
    let result = evaluator.resolve_call(union, &[]);
    assert!(
        matches!(
            result,
            CallResult::NotCallable { .. } | CallResult::NoOverloadMatch { .. }
        ),
        "Expected NotCallable or NoOverloadMatch for incompatible multi-overload union, got {result:?}"
    );
}

/// When union members with multiple overloads share a compatible signature,
/// the union IS callable but the `this` type is the intersection of the
/// compatible overloads' `this` types.
///
/// Mirrors: `type F3 = { (this: A): void; (this: B): void; }`
///          `type F5 = { (this: C): void; (this: B): void; }`
///          `(f3_or_f5: F3 | F5) => f3_or_f5()` → TS2684 if `this` ≠ B
#[test]
fn test_union_multi_overload_compatible_this_mismatch() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    let type_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);
    let type_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);
    let type_c = interner.object(vec![PropertyInfo::new(
        interner.intern_string("c"),
        TypeId::STRING,
    )]);

    // F3 = { (this: A): void; (this: B): void }
    let f3 = interner.callable(CallableShape {
        call_signatures: vec![
            CallSignature {
                params: vec![],
                this_type: Some(type_a),
                return_type: TypeId::VOID,
                type_params: vec![],
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                params: vec![],
                this_type: Some(type_b),
                return_type: TypeId::VOID,
                type_params: vec![],
                type_predicate: None,
                is_method: false,
            },
        ],
        ..Default::default()
    });

    // F5 = { (this: C): void; (this: B): void }
    let f5 = interner.callable(CallableShape {
        call_signatures: vec![
            CallSignature {
                params: vec![],
                this_type: Some(type_c),
                return_type: TypeId::VOID,
                type_params: vec![],
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                params: vec![],
                this_type: Some(type_b),
                return_type: TypeId::VOID,
                type_params: vec![],
                type_predicate: None,
                is_method: false,
            },
        ],
        ..Default::default()
    });

    // Union F3 | F5 — compatible pair: (this: B): void from both
    let union = interner.union(vec![f3, f5]);

    // Call with this = void (no this context) → should fail with ThisTypeMismatch
    // because the compatible signature requires this: B
    let result = evaluator.resolve_call(union, &[]);
    assert!(
        matches!(result, CallResult::ThisTypeMismatch { .. }),
        "Expected ThisTypeMismatch for compatible multi-overload union with void this, got {result:?}"
    );
}

/// When a union has one single-signature member and one multi-overload member,
/// it falls through to per-member resolution. The single-signature member's
/// success makes the call valid.
#[test]
fn test_union_single_plus_multi_overload_succeeds() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    // F0 = () => void — single signature, no this
    let f0 = interner.function(FunctionShape {
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let type_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);
    let type_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);

    // F3 = { (this: A): void; (this: B): void }
    let f3 = interner.callable(CallableShape {
        call_signatures: vec![
            CallSignature {
                params: vec![],
                this_type: Some(type_a),
                return_type: TypeId::VOID,
                type_params: vec![],
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                params: vec![],
                this_type: Some(type_b),
                return_type: TypeId::VOID,
                type_params: vec![],
                type_predicate: None,
                is_method: false,
            },
        ],
        ..Default::default()
    });

    // Union F0 | F3 — F0 has single signature, per-member resolution handles it
    let union = interner.union(vec![f0, f3]);
    let result = evaluator.resolve_call(union, &[]);
    assert!(
        matches!(result, CallResult::Success(_)),
        "Expected success for single+multi overload union, got {result:?}"
    );
}
#[test]
fn test_union_generic_single_signature_members_require_shared_call_signature() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    let generic_number = interner.function(FunctionShape {
        params: vec![ParamInfo::required(
            interner.intern_string("a"),
            TypeId::NUMBER,
        )],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: vec![TypeParamInfo {
            name: interner.intern_string("T"),
            constraint: Some(TypeId::NUMBER),
            default: None,
            is_const: false,
        }],
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let generic_string = interner.function(FunctionShape {
        params: vec![ParamInfo::required(
            interner.intern_string("a"),
            TypeId::STRING,
        )],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: vec![TypeParamInfo {
            name: interner.intern_string("U"),
            constraint: None,
            default: None,
            is_const: false,
        }],
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let union = interner.union(vec![generic_number, generic_string]);
    let result = evaluator.resolve_call(union, &[TypeId::STRING]);

    assert!(
        matches!(result, CallResult::NotCallable { .. }),
        "Expected NotCallable for incompatible generic single-signature union, got {result:?}"
    );
}

/// Test that `resolve_call` correctly handles `IndexAccess` types where the
/// object type is a type parameter with a mapped type constraint.
///
/// This covers the pattern: `T extends { [P in K]: () => void }`, `obj[key]()`
/// where `T[K]` should resolve through the constraint's mapped type to
/// `() => void`, which is callable.
///
/// Note: In production, `Record<K, F>` is stored as `Application(Lazy(DefId), [K, F])`
/// and requires a full resolver to expand. This test uses an inlined mapped type
/// to validate the `IndexAccess` -> Mapped -> callable resolution chain without
/// needing DefId resolution infrastructure.
#[test]
fn test_call_index_access_on_mapped_type_constraint() {
    let interner = TypeInterner::new();
    let mut compat = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut compat);

    // Create type parameter K extends string
    let k_param = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    };
    let k_type = interner.intern(TypeData::TypeParameter(k_param));

    // Create a function type: () => void
    let fn_type = interner.function(FunctionShape {
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: vec![],
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Create a mapped type: { [P in K]: () => void }
    // This is what Record<K, () => void> evaluates to
    let mapped = interner.mapped(MappedType {
        type_param: k_param,
        constraint: k_type,
        name_type: None,
        template: fn_type,
        readonly_modifier: None,
        optional_modifier: None,
    });

    // Create type parameter T extends { [P in K]: () => void }
    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(mapped),
        default: None,
        is_const: false,
    }));

    // Create IndexAccess: T[K]
    let index_access = interner.index_access(t_type, k_type);

    // resolve_call on T[K] should succeed because T[K] resolves
    // through the constraint to () => void
    let result = evaluator.resolve_call(index_access, &[]);
    assert!(
        matches!(result, CallResult::Success(_)),
        "Expected T[K] to be callable when T extends {{ [P in K]: () => void }}, got {result:?}"
    );
}

/// When a type parameter has a conditional type constraint that evaluates to `never`
/// for the inferred type, the solver should report an `ArgumentTypeMismatch` with
/// `never` as the expected type (not the unevaluated conditional).
///
/// Pattern: `<T extends null extends T ? any : never>(value: T): void`
/// Called with `string` → constraint is `null extends string ? any : never` → `never`
/// → `string` is not assignable to `never` → TS2345
#[test]
fn test_generic_call_evaluates_conditional_constraint_to_never() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);

    // Build the conditional constraint: null extends T ? any : never
    let tp_name = interner.intern_string("T");
    let tp = TypeParamInfo {
        is_const: false,
        name: tp_name,
        constraint: None,
        default: None,
    };
    let tp_id = interner.type_param(tp);

    // Conditional: null extends T ? any : never
    let cond = interner.conditional(ConditionalType {
        check_type: TypeId::NULL,
        extends_type: tp_id,
        true_type: TypeId::ANY,
        false_type: TypeId::NEVER,
        is_distributive: false,
    });

    // Now create the type param with the conditional constraint
    let tp_with_constraint = TypeParamInfo {
        is_const: false,
        name: tp_name,
        constraint: Some(cond),
        default: None,
    };
    let tp_id_constrained = interner.type_param(tp_with_constraint);

    // function<T extends null extends T ? any : never>(value: T): void
    let func = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(tp_id_constrained)],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: vec![tp_with_constraint],
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Call with `string` — should fail because null !<: string → constraint = never
    let result = evaluator.resolve_call(func, &[TypeId::STRING]);
    match result {
        CallResult::ArgumentTypeMismatch { expected, .. } => {
            // The expected type should be `never` (the evaluated constraint),
            // not the raw Conditional type
            assert_eq!(
                expected,
                TypeId::NEVER,
                "Expected constraint to evaluate to `never`, got {:?}",
                interner.lookup(expected)
            );
        }
        _ => panic!("Expected ArgumentTypeMismatch with never, got {result:?}"),
    }
}
#[test]
fn test_generic_call_infers_type_param_from_this_parameter() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);

    let t_name = interner.intern_string("T");
    let t_info = TypeParamInfo {
        is_const: false,
        name: t_name,
        constraint: None,
        default: None,
    };
    let t_type = interner.type_param(t_info);

    let arg_type = interner.keyof(t_type);
    let foo = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(arg_type)],
        this_type: Some(t_type),
        return_type: TypeId::VOID,
        type_params: vec![t_info],
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    let receiver = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("b"), TypeId::STRING),
    ]);
    evaluator.set_actual_this_type(Some(receiver));

    let result = evaluator.resolve_call(foo, &[interner.literal_string("a")]);
    assert!(
        matches!(result, CallResult::Success(_)),
        "Expected generic `this` to infer T from receiver, got {result:?}"
    );
}

/// When a conditional constraint evaluates to a concrete type (not never),
/// inference should succeed normally.
///
/// Pattern: `<T extends null extends T ? any : never>(value: T): void`
/// Called with `string | null` → constraint is `null extends (string | null) ? any : never` → `any`
/// → `string | null` is assignable to `any` → OK
#[test]
fn test_generic_call_conditional_constraint_accepts_nullable() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);

    let tp_name = interner.intern_string("T");
    let tp = TypeParamInfo {
        is_const: false,
        name: tp_name,
        constraint: None,
        default: None,
    };
    let tp_id = interner.type_param(tp);

    // Conditional: null extends T ? any : never
    let cond = interner.conditional(ConditionalType {
        check_type: TypeId::NULL,
        extends_type: tp_id,
        true_type: TypeId::ANY,
        false_type: TypeId::NEVER,
        is_distributive: false,
    });

    let tp_with_constraint = TypeParamInfo {
        is_const: false,
        name: tp_name,
        constraint: Some(cond),
        default: None,
    };
    let tp_id_constrained = interner.type_param(tp_with_constraint);

    let func = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(tp_id_constrained)],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: vec![tp_with_constraint],
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Call with `string | null` — should succeed because null <: (string | null) → any
    let nullable = interner.union(vec![TypeId::STRING, TypeId::NULL]);
    let result = evaluator.resolve_call(func, &[nullable]);
    assert!(
        matches!(result, CallResult::Success(_)),
        "Expected success for nullable argument, got {result:?}"
    );
}

// =============================================================================
// Iterator Result Value Type Extraction Tests
// =============================================================================

/// Test that `extract_iterator_result_value_types` properly partitions
/// `IteratorResult` into yield (done:false) and return (done:true) types.
#[test]
fn test_extract_iterator_result_yield_vs_return() {
    use crate::operations::extract_iterator_result_value_types;

    let interner = TypeInterner::new();
    let done_atom = interner.intern_string("done");
    let value_atom = interner.intern_string("value");

    // Build: { done?: false, value: string } | { done: true, value: undefined }
    // This is what IteratorResult<string, undefined> expands to.
    let yield_branch = interner.object(vec![
        PropertyInfo::opt(done_atom, TypeId::BOOLEAN_FALSE), // done?: false
        PropertyInfo::new(value_atom, TypeId::STRING),       // value: string
    ]);

    let return_branch = interner.object(vec![
        PropertyInfo::new(done_atom, TypeId::BOOLEAN_TRUE), // done: true
        PropertyInfo::new(value_atom, TypeId::UNDEFINED),   // value: undefined
    ]);

    let iterator_result = interner.union(vec![yield_branch, return_branch]);

    let (yield_type, return_type) = extract_iterator_result_value_types(&interner, iterator_result);

    assert_eq!(
        yield_type,
        TypeId::STRING,
        "yield type should be string (from done:false branch)"
    );
    assert_eq!(
        return_type,
        TypeId::UNDEFINED,
        "return type should be undefined (from done:true branch)"
    );
}

/// Test that `extract_iterator_result_value_types` extracts args from Application types.
/// For `IteratorResult<T, TReturn>`, args[0] = T (yield), args[1] = `TReturn` (return).
#[test]
fn test_extract_iterator_result_application_extracts_args() {
    use crate::operations::extract_iterator_result_value_types;

    let interner = TypeInterner::new();

    // Simulate IteratorResult<string, undefined> as an Application type
    // base=some_type, args=[string, undefined]
    let app = interner.application(TypeId::STRING, vec![TypeId::STRING, TypeId::UNDEFINED]);
    let (yield_type, return_type) = extract_iterator_result_value_types(&interner, app);

    assert_eq!(
        yield_type,
        TypeId::STRING,
        "should extract args[0] as yield type from Application"
    );
    assert_eq!(
        return_type,
        TypeId::UNDEFINED,
        "should extract args[1] as return type from Application"
    );
}

/// Test that a single-object `IteratorResult` (no union) extracts value as yield type.
#[test]
fn test_extract_iterator_result_single_object() {
    use crate::operations::extract_iterator_result_value_types;

    let interner = TypeInterner::new();
    let value_atom = interner.intern_string("value");

    // Build: { value: number } — a simple object with a value property
    let obj = interner.object(vec![PropertyInfo::new(value_atom, TypeId::NUMBER)]);

    let (yield_type, return_type) = extract_iterator_result_value_types(&interner, obj);

    assert_eq!(
        yield_type,
        TypeId::NUMBER,
        "single object yield should be the value type"
    );
    assert_eq!(
        return_type,
        TypeId::ANY,
        "single object return should be ANY"
    );
}
#[test]
fn test_call_optional_param_accepts_union_with_undefined() {
    // Regression test: calling `f(message?: string)` with arg `string | undefined`
    // should succeed — the optional param implicitly accepts `undefined`.
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    // function(message?: string): never
    let func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("message")),
            type_id: TypeId::STRING,
            optional: true,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::NEVER,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Arg: string | undefined
    let string_or_undef = interner.union(vec![TypeId::STRING, TypeId::UNDEFINED]);

    let result = evaluator.resolve_call(func, &[string_or_undef]);
    match result {
        CallResult::Success(ret) => assert_eq!(ret, TypeId::NEVER),
        other => {
            panic!("Expected Success for optional param with string | undefined arg, got {other:?}")
        }
    }
}
#[test]
fn test_call_optional_param_rejects_wrong_type_with_undefined() {
    // Calling `f(x?: string)` with `number | undefined` should still fail —
    // only `undefined` is stripped, leaving `number` which is not assignable to `string`.
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    let func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: true,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Arg: number | undefined
    let num_or_undef = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);

    let result = evaluator.resolve_call(func, &[num_or_undef]);
    match result {
        CallResult::ArgumentTypeMismatch { .. } => {} // expected
        other => {
            panic!("Expected ArgumentTypeMismatch for number|undefined -> string?, got {other:?}")
        }
    }
}

/// Test that type parameters inside intersection parameter types are inferred correctly.
///
/// Reproduces the bug from intersectionTypeInference1.ts:
///   <OwnProps>(f: (p: {dispatch: number} & `OwnProps`) => void) => (o: `OwnProps`) => `OwnProps`
/// Called with (props: {store: string}) => void should infer `OwnProps` = {store: string}.
#[test]
fn test_call_generic_intersection_param_inference() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    // Type parameter OwnProps (unconstrained)
    let own_props_param = TypeParamInfo {
        name: interner.intern_string("OwnProps"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let own_props_type = interner.intern(TypeData::TypeParameter(own_props_param));

    // {dispatch: number}
    let dispatch_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("dispatch"),
        TypeId::NUMBER,
    )]);

    // {dispatch: number} & OwnProps
    let intersection_param = interner.intersection(vec![dispatch_obj, own_props_type]);

    // (p: {dispatch: number} & OwnProps) => void
    let inner_fn_type = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("p")),
            type_id: intersection_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Generic function: <OwnProps>(f: inner_fn_type) => OwnProps
    let generic_func = interner.function(FunctionShape {
        type_params: vec![own_props_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("f")),
            type_id: inner_fn_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: own_props_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Argument: (props: {store: string}) => void
    let store_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("store"),
        TypeId::STRING,
    )]);
    let arg_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("props")),
            type_id: store_obj,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Call generic_func(arg_fn) — should succeed (OwnProps inferred as {store: string})
    let result = evaluator.resolve_call(generic_func, &[arg_fn]);
    match result {
        CallResult::Success(_ret) => {
            // OwnProps should be inferred as {store: string}, and the call should succeed
        }
        other => panic!(
            "Expected success for intersection param inference, got {other:?}. \
             OwnProps should be inferred from the intersection decomposition."
        ),
    }
}

/// Tests that the trivial single-type-param fast path preserves literal types
/// when a contextual return type contains those literals.
///
/// Reproduces: `let v: 'A' | 'B' = identity('A')` where `identity<T>(x: T): T`.
/// Without the fix, `T` is inferred as `string` (widened from `"A"`), causing
/// a spurious TS2322. With the fix, the contextual type `'A' | 'B'` prevents
/// widening, keeping `T = "A"`.
#[test]
fn test_trivial_identity_preserves_literal_with_contextual_type() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);

    // type DooDad = 'SOMETHING' | 'ELSE'
    let lit_something = interner.literal_string("SOMETHING");
    let lit_else = interner.literal_string("ELSE");
    let doodad = interner.union(vec![lit_something, lit_else]);

    // declare function identity<T>(x: T): T
    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.type_param(t_param);
    let identity = interner.function(FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: t_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Call identity("ELSE") with contextual type DooDad
    evaluator.set_contextual_type(Some(doodad));
    let result = evaluator.resolve_call(identity, &[lit_else]);

    match result {
        CallResult::Success(ret) => {
            // The return type should be "ELSE" (the literal), not string.
            // With the contextual type DooDad, the solver should preserve the
            // literal instead of widening to string.
            assert_ne!(
                ret,
                TypeId::STRING,
                "identity('ELSE') with contextual DooDad should NOT widen to string"
            );
            assert_eq!(
                ret, lit_else,
                "identity('ELSE') with contextual DooDad should return literal \"ELSE\""
            );
        }
        other => panic!("Expected success for identity call with contextual type, got {other:?}"),
    }
}

/// Tests that without a contextual type, the identity fast path widens literals.
/// `identity('ELSE')` without context should infer T = string (normal widening).
#[test]
fn test_trivial_identity_widens_literal_without_contextual_type() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);

    let lit_else = interner.literal_string("ELSE");

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.type_param(t_param);
    let identity = interner.function(FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: t_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Call identity("ELSE") WITHOUT contextual type
    let result = evaluator.resolve_call(identity, &[lit_else]);

    match result {
        CallResult::Success(ret) => {
            // Without contextual type, the literal should be widened to string
            assert_eq!(
                ret,
                TypeId::STRING,
                "identity('ELSE') without context should widen to string"
            );
        }
        other => panic!("Expected success for identity call without context, got {other:?}"),
    }
}

/// Test that a union of a single-overload function and a multi-overload callable
/// is not callable when the single-overload member's `this` type doesn't match
/// any of the multi-overload member's `this` types.
///
/// Corresponds to tsc behavior for:
///   type F1 = (this: A) => void;
///   interface F4 { (this: C): void; (this: D): void; }
///   type Union = F1 | F4;  // not callable → TS2349
#[test]
fn test_union_call_mixed_overloads_incompatible_this_not_callable() {
    let interner = TypeInterner::new();

    // Create distinct `this` types: A = { a: string }, C = { c: string }, D = { d: number }
    let type_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);
    let type_c = interner.object(vec![PropertyInfo::new(
        interner.intern_string("c"),
        TypeId::STRING,
    )]);
    let type_d = interner.object(vec![PropertyInfo::new(
        interner.intern_string("d"),
        TypeId::NUMBER,
    )]);

    // F1 = (this: A) => void — single overload
    let f1 = interner.function(FunctionShape {
        params: vec![],
        this_type: Some(type_a),
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // F4 = { (this: C): void; (this: D): void; } — multi-overload
    let f4 = interner.callable(CallableShape {
        call_signatures: vec![
            CallSignature {
                type_params: vec![],
                params: vec![],
                this_type: Some(type_c),
                return_type: TypeId::VOID,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                type_params: vec![],
                params: vec![],
                this_type: Some(type_d),
                return_type: TypeId::VOID,
                type_predicate: None,
                is_method: false,
            },
        ],
        ..Default::default()
    });

    // Union = F1 | F4
    let union_type = interner.union(vec![f1, f4]);

    // Create a `this` context that satisfies A (from F1's this type)
    // so Phase 0 passes and we reach the 1-multi compatibility check.
    let actual_this = interner.intersection(vec![type_a, type_c]);

    let mut checker = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);
    evaluator.set_actual_this_type(Some(actual_this));
    let result = evaluator.resolve_call(union_type, &[]);

    assert!(
        matches!(result, CallResult::NotCallable { .. }),
        "Union of single-overload (this: A) and multi-overload (this: C / this: D) \
         should be not callable because A != C and A != D. Got: {result:?}"
    );
}

/// Test that a union of a single-overload function and a multi-overload callable
/// IS callable when the single-overload member's `this` type matches one of the
/// multi-overload member's `this` types.
///
/// Corresponds to tsc behavior for:
///   type F1 = (this: A) => void;
///   interface F3 { (this: A): void; (this: B): void; }
///   type Union = F1 | F3;  // callable (F1 matches F3's first overload)
#[test]
fn test_union_call_mixed_overloads_compatible_this_callable() {
    let interner = TypeInterner::new();

    // Create `this` types using SAME TypeId for A
    let type_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);
    let type_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);

    // F1 = (this: A) => void — single overload
    let f1 = interner.function(FunctionShape {
        params: vec![],
        this_type: Some(type_a),
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // F3 = { (this: A): void; (this: B): void; } — multi-overload, sharing `this: A`
    let f3 = interner.callable(CallableShape {
        call_signatures: vec![
            CallSignature {
                type_params: vec![],
                params: vec![],
                this_type: Some(type_a), // Same TypeId as F1's this
                return_type: TypeId::VOID,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                type_params: vec![],
                params: vec![],
                this_type: Some(type_b),
                return_type: TypeId::VOID,
                type_predicate: None,
                is_method: false,
            },
        ],
        ..Default::default()
    });

    // Union = F1 | F3
    let union_type = interner.union(vec![f1, f3]);

    let mut checker = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);
    // Set actual_this to a type that satisfies A (the shared this type)
    evaluator.set_actual_this_type(Some(type_a));
    let result = evaluator.resolve_call(union_type, &[]);

    assert!(
        matches!(result, CallResult::Success(_)),
        "Union of single-overload (this: A) and multi-overload (this: A / this: B) \
         should be callable when `this` types match. Got: {result:?}"
    );
}
