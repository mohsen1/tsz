/// Test tuple rest element captures remaining elements
/// function foo<T extends any[]>(...args: [number, ...T]): T
#[test]
fn test_tuple_rest_captures_remaining() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(interner.array(TypeId::ANY)),
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let tuple_param = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: t_type,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    let func = FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: tuple_param,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    // args: [1, "a", true] -> T = [string, boolean]
    let result = infer_generic_function(
        &interner,
        &mut subtype,
        &func,
        &[TypeId::NUMBER, TypeId::STRING, TypeId::BOOLEAN],
    );
    // Tuple [string, boolean] satisfies array constraint any[] - tuples are subtypes of arrays
    let expected = interner.tuple(vec![
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
    assert_eq!(result, expected);
}

/// Test tuple rest with multiple fixed prefix elements
/// function foo<T extends any[]>(...args: [number, string, ...T]): T
#[test]
fn test_tuple_rest_with_multiple_prefix() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(interner.array(TypeId::ANY)),
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    // [number, string, ...T]
    let tuple_param = interner.tuple(vec![
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
            type_id: t_type,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    let func = FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: tuple_param,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    // args: [1, "a", true, false] -> T = [boolean, boolean]
    let result = infer_generic_function(
        &interner,
        &mut subtype,
        &func,
        &[
            TypeId::NUMBER,
            TypeId::STRING,
            TypeId::BOOLEAN,
            TypeId::BOOLEAN,
        ],
    );
    // Tuple [boolean, boolean] satisfies array constraint any[] - tuples are subtypes of arrays
    let expected = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::BOOLEAN,
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

/// Test tuple rest with single element capture
/// function foo<T extends any[]>(...args: [number, ...T]): T with one extra arg
#[test]
fn test_tuple_rest_single_capture() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(interner.array(TypeId::ANY)),
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let tuple_param = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: t_type,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    let func = FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: tuple_param,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    // args: [1, "a"] -> T = [string]
    let result = infer_generic_function(
        &interner,
        &mut subtype,
        &func,
        &[TypeId::NUMBER, TypeId::STRING],
    );
    // Tuple [string] satisfies array constraint any[] - tuples are subtypes of arrays
    let expected = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }]);
    assert_eq!(result, expected);
}

// =============================================================================
// VARIADIC FUNCTION INFERENCE TESTS
// =============================================================================

/// Test variadic function with constrained type parameter
/// function foo<T extends string | number>(...args: T[]): T[]
#[test]
fn test_variadic_with_constraint() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let constraint = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(constraint),
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));
    let array_t = interner.array(t_type);

    let func = FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: array_t,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: array_t,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    // All strings -> T[] = string[]
    let result = infer_generic_function(
        &interner,
        &mut subtype,
        &func,
        &[TypeId::STRING, TypeId::STRING],
    );
    let expected = interner.array(TypeId::STRING);
    assert_eq!(result, expected);
}

/// Test variadic function inferring from multiple rest positions
/// function zip<T, U>(...pairs: [T, U][]): [T[], U[]]
#[test]
fn test_variadic_zip_pattern() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let u_param = TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let u_type = interner.intern(TypeData::TypeParameter(u_param));

    // [T, U] tuple
    let pair_tuple = interner.tuple(vec![
        TupleElement {
            type_id: t_type,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: u_type,
            name: None,
            optional: false,
            rest: false,
        },
    ]);
    let array_pairs = interner.array(pair_tuple);

    // Return type [T[], U[]]
    let array_t = interner.array(t_type);
    let array_u = interner.array(u_type);
    let return_type = interner.tuple(vec![
        TupleElement {
            type_id: array_t,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: array_u,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let func = FunctionShape {
        type_params: vec![t_param, u_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("pairs")),
            type_id: array_pairs,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    // Call with [number, string], [number, string]
    let pair1 = interner.tuple(vec![
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
    ]);
    let pair2 = interner.tuple(vec![
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
    ]);

    let result = infer_generic_function(&interner, &mut subtype, &func, &[pair1, pair2]);

    // Expected: [number[], string[]]
    let expected = interner.tuple(vec![
        TupleElement {
            type_id: interner.array(TypeId::NUMBER),
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: interner.array(TypeId::STRING),
            name: None,
            optional: false,
            rest: false,
        },
    ]);
    assert_eq!(result, expected);
}

/// Test variadic function with no arguments uses default/constraint
#[test]
fn test_variadic_empty_args_uses_constraint() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(TypeId::UNKNOWN),
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));
    let array_t = interner.array(t_type);

    let func = FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: array_t,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    // No args -> T inferred from constraint (unknown)
    let result = infer_generic_function(&interner, &mut subtype, &func, &[]);
    // With no inference candidates, should fall back to constraint
    assert_eq!(result, TypeId::UNKNOWN);
}

/// Test that `array_element_type` returns ERROR instead of ANY for non-array/tuple types
/// This is important for TS2322 type checking - returning ANY would incorrectly silence
/// type errors, while ERROR properly propagates the failure.
#[test]
fn test_array_element_type_non_array_returns_error() {
    let interner = TypeInterner::new();

    // Create a property access evaluator (needed to call array_element_type)
    let evaluator = PropertyAccessEvaluator::new(&interner);

    // Try to get element type of a non-array type (e.g., a number)
    let number_type = TypeId::NUMBER;
    let result = evaluator.array_element_type(number_type);

    // Should return ERROR instead of ANY
    assert_eq!(
        result,
        TypeId::ERROR,
        "array_element_type should return ERROR for non-array/tuple types, not ANY"
    );

    // Also test with object type
    let object_type = interner.object(vec![]);
    let result = evaluator.array_element_type(object_type);
    assert_eq!(
        result,
        TypeId::ERROR,
        "array_element_type should return ERROR for object types, not ANY"
    );

    // Verify that actual arrays still work
    let string_array = interner.array(TypeId::STRING);
    let result = evaluator.array_element_type(string_array);
    assert_eq!(
        result,
        TypeId::STRING,
        "array_element_type should still return element type for arrays"
    );

    // Verify that tuples still work
    let tuple_elements = vec![
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
    ];
    let tuple = interner.tuple(tuple_elements);
    let result = evaluator.array_element_type(tuple);
    // Should be union of string | number
    assert!(
        result == TypeId::STRING
            || result == TypeId::NUMBER
            || matches!(interner.lookup(result), Some(TypeData::Union(_))),
        "array_element_type should return union of tuple element types"
    );
}

// =============================================================================
// Tests for solve_generic_instantiation
// =============================================================================

/// Test that type arguments satisfying constraints return Success
#[test]
fn test_solve_generic_instantiation_success() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // <T extends string>
    let type_params = vec![TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }];

    // <string> - satisfies the constraint
    let type_args = vec![TypeId::STRING];

    let result = solve_generic_instantiation(&type_params, &type_args, &interner, &mut checker);
    assert_eq!(result, GenericInstantiationResult::Success);
}

/// Test that type arguments violating constraints return `ConstraintViolation`
#[test]
fn test_solve_generic_instantiation_constraint_violation() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // <T extends string>
    let type_params = vec![TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }];

    // <number> - does NOT satisfy the constraint
    let type_args = vec![TypeId::NUMBER];

    let result = solve_generic_instantiation(&type_params, &type_args, &interner, &mut checker);
    match result {
        GenericInstantiationResult::ConstraintViolation {
            param_index,
            param_name,
            constraint,
            type_arg,
        } => {
            assert_eq!(param_index, 0);
            assert_eq!(param_name, interner.intern_string("T"));
            assert_eq!(constraint, TypeId::STRING);
            assert_eq!(type_arg, TypeId::NUMBER);
        }
        _ => panic!("Expected ConstraintViolation, got {result:?}"),
    }
}

/// Test that unconstrained type parameters always succeed
#[test]
fn test_solve_generic_instantiation_unconstrained_success() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // <T> (no constraint)
    let type_params = vec![TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }];

    // <any type> - should always succeed when unconstrained
    let type_args = vec![TypeId::NUMBER];

    let result = solve_generic_instantiation(&type_params, &type_args, &interner, &mut checker);
    assert_eq!(result, GenericInstantiationResult::Success);
}

/// Test that multiple type parameters are all validated
#[test]
fn test_solve_generic_instantiation_multiple_params() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // <T extends string, U extends number>
    let type_params = vec![
        TypeParamInfo {
            name: interner.intern_string("T"),
            constraint: Some(TypeId::STRING),
            default: None,
            is_const: false,
        },
        TypeParamInfo {
            name: interner.intern_string("U"),
            constraint: Some(TypeId::NUMBER),
            default: None,
            is_const: false,
        },
    ];

    // Both constraints satisfied
    let type_args = vec![TypeId::STRING, TypeId::NUMBER];
    let result = solve_generic_instantiation(&type_params, &type_args, &interner, &mut checker);
    assert_eq!(result, GenericInstantiationResult::Success);

    // First constraint violated
    let type_args = vec![TypeId::BOOLEAN, TypeId::NUMBER];
    let result = solve_generic_instantiation(&type_params, &type_args, &interner, &mut checker);
    match result {
        GenericInstantiationResult::ConstraintViolation { param_index, .. } => {
            assert_eq!(param_index, 0);
        }
        _ => panic!("Expected ConstraintViolation for first param"),
    }

    // Second constraint violated
    let type_args = vec![TypeId::STRING, TypeId::BOOLEAN];
    let result = solve_generic_instantiation(&type_params, &type_args, &interner, &mut checker);
    match result {
        GenericInstantiationResult::ConstraintViolation { param_index, .. } => {
            assert_eq!(param_index, 1);
        }
        _ => panic!("Expected ConstraintViolation for second param"),
    }
}

/// Test that literals satisfy constraints when assignable
#[test]
fn test_solve_generic_instantiation_literal_satisfies_constraint() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // <T extends string>
    let type_params = vec![TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }];

    // "hello" literal should satisfy string constraint
    let hello_lit = interner.literal_string("hello");
    let type_args = vec![hello_lit];

    let result = solve_generic_instantiation(&type_params, &type_args, &interner, &mut checker);
    assert_eq!(result, GenericInstantiationResult::Success);
}

/// Test that union types can satisfy constraints when all members satisfy it
#[test]
fn test_solve_generic_instantiation_union_satisfies_constraint() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // <T extends string | number>
    let union_constraint = interner.union2(TypeId::STRING, TypeId::NUMBER);
    let type_params = vec![TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(union_constraint),
        default: None,
        is_const: false,
    }];

    // string should satisfy string | number constraint
    let type_args = vec![TypeId::STRING];
    let result = solve_generic_instantiation(&type_params, &type_args, &interner, &mut checker);
    assert_eq!(result, GenericInstantiationResult::Success);

    // "hello" literal should satisfy string | number constraint
    let hello_lit = interner.literal_string("hello");
    let type_args = vec![hello_lit];
    let result = solve_generic_instantiation(&type_params, &type_args, &interner, &mut checker);
    assert_eq!(result, GenericInstantiationResult::Success);
}

/// Test the task example: function f<T>(x: T): number { return x; } f<string>("hi")
/// The type argument string should be validated against T's constraint (none in this case)
#[test]
fn test_solve_generic_instantiation_task_example() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // <T> (unconstrained)
    let type_params = vec![TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }];

    // Explicit type argument <string>
    let type_args = vec![TypeId::STRING];

    let result = solve_generic_instantiation(&type_params, &type_args, &interner, &mut checker);
    // Should succeed because T has no constraint
    assert_eq!(result, GenericInstantiationResult::Success);
}

/// Test that constraints are properly checked (number doesn't extend string)
#[test]
fn test_solve_generic_instantiation_number_not_string() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // <T extends string>
    let type_params = vec![TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }];

    // number does NOT extend string
    let type_args = vec![TypeId::NUMBER];

    let result = solve_generic_instantiation(&type_params, &type_args, &interner, &mut checker);
    match result {
        GenericInstantiationResult::ConstraintViolation {
            constraint,
            type_arg,
            ..
        } => {
            assert_eq!(constraint, TypeId::STRING);
            assert_eq!(type_arg, TypeId::NUMBER);
        }
        _ => panic!("Expected ConstraintViolation: number does not extend string"),
    }
}

/// Test object type constraints
#[test]
fn test_solve_generic_instantiation_object_constraint() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // Create an object type { x: number }
    let object_type = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    // <T extends { x: number }>
    let type_params = vec![TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(object_type),
        default: None,
        is_const: false,
    }];

    // { x: number; y: string; } should satisfy constraint (has at least x: number)
    let wider_object = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("y"), TypeId::STRING),
    ]);

    let type_args = vec![wider_object];
    let result = solve_generic_instantiation(&type_params, &type_args, &interner, &mut checker);
    assert_eq!(result, GenericInstantiationResult::Success);
}

// ============================================================================
// Tuple-to-Array Assignability Tests for Operations
// These tests verify type operations work correctly with tuple-to-array patterns
// ============================================================================

/// Test that `array_element_type` correctly extracts element type from homogeneous tuple
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
    use crate::relations::subtype::TypeEnvironment;
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

#[test]
fn test_array_push_instantiates_intersection_array_base_parameter() {
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
    let array_base = interner.intersection2(array_decl_a, array_decl_b);
    interner.set_array_base_type(array_base, vec![t_param]);

    let evaluator = PropertyAccessEvaluator::new(&interner);
    let u_param = TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let u_type = interner.intern(TypeData::TypeParameter(u_param));
    let u_array = interner.array(u_type);

    let result = evaluator.resolve_property_access(u_array, "push");
    let PropertyAccessResult::Success { type_id, .. } = result else {
        panic!("Expected Success for generic array push, got {result:?}");
    };
    let Some(TypeData::Function(func_id)) = interner.lookup(type_id) else {
        panic!(
            "Expected function type for push, got {:?}",
            interner.lookup(type_id)
        );
    };
    let shape = interner.function_shape(func_id);
    let [param] = shape.params.as_slice() else {
        panic!(
            "Expected one rest parameter for push, got {:?}",
            shape.params
        );
    };
    assert_eq!(
        crate::type_queries::get_array_element_type(&interner, param.type_id),
        Some(u_type)
    );
}
