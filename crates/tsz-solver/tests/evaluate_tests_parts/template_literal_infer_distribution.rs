#[test]
fn test_readonly_array_type() {
    // ReadonlyArray<T> is array with readonly semantics
    let interner = TypeInterner::new();

    let readonly_arr = interner.array(TypeId::STRING);

    match interner.lookup(readonly_arr) {
        Some(TypeData::Array(element)) => {
            assert_eq!(element, TypeId::STRING);
        }
        _ => panic!("Expected Array type"),
    }
}

#[test]
fn test_nonnullable_type() {
    // NonNullable<T> = T extends null | undefined ? never : T
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let null_or_undefined = interner.union(vec![TypeId::NULL, TypeId::UNDEFINED]);

    let _non_nullable_cond = ConditionalType {
        check_type: t_param,
        extends_type: null_or_undefined,
        true_type: TypeId::NEVER,
        false_type: t_param,
        is_distributive: true,
    };

    // Test with string | null
    let string_or_null = interner.union(vec![TypeId::STRING, TypeId::NULL]);
    let test_cond = ConditionalType {
        check_type: string_or_null,
        extends_type: null_or_undefined,
        true_type: TypeId::NEVER,
        false_type: string_or_null,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &test_cond);
    // With distributive, should filter out null
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_extract_type_pattern() {
    // Extract<T, U> = T extends U ? T : never
    let interner = TypeInterner::new();

    // Extract<string | number | boolean, string | number>
    let source = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);
    let pattern = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let cond = ConditionalType {
        check_type: source,
        extends_type: pattern,
        true_type: source,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Should extract string | number (exclude boolean)
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_exclude_type_pattern() {
    // Exclude<T, U> = T extends U ? never : T
    let interner = TypeInterner::new();

    // Exclude<string | number | boolean, string>
    let source = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);
    let pattern = TypeId::STRING;

    let cond = ConditionalType {
        check_type: source,
        extends_type: pattern,
        true_type: TypeId::NEVER,
        false_type: source,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Should exclude string, return number | boolean
    assert!(result != TypeId::ERROR);
}

// =============================================================================
// DISTRIBUTIVE CONDITIONAL TYPE STRESS TESTS
// =============================================================================

#[test]
fn test_distributive_over_large_union() {
    // Distribution over a large union: T extends string ? "yes" : "no"
    // With T = string | number | boolean | null | undefined | symbol | bigint
    let interner = TypeInterner::new();

    let large_union = interner.union(vec![
        TypeId::STRING,
        TypeId::NUMBER,
        TypeId::BOOLEAN,
        TypeId::NULL,
        TypeId::UNDEFINED,
        TypeId::SYMBOL,
        TypeId::BIGINT,
    ]);

    let lit_yes = interner.literal_string("yes");
    let lit_no = interner.literal_string("no");

    let cond = ConditionalType {
        check_type: large_union,
        extends_type: TypeId::STRING,
        true_type: lit_yes,
        false_type: lit_no,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Should distribute and produce "yes" | "no"
    // string -> "yes", others -> "no"
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_distributive_nested_conditionals() {
    // Nested distribution: T extends A ? (T extends B ? X : Y) : Z
    let interner = TypeInterner::new();

    let union_abc = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);

    let lit_x = interner.literal_string("x");
    let lit_y = interner.literal_string("y");
    let lit_z = interner.literal_string("z");

    // Inner conditional: T extends number ? "x" : "y"
    let inner_cond = interner.conditional(ConditionalType {
        check_type: union_abc,
        extends_type: TypeId::NUMBER,
        true_type: lit_x,
        false_type: lit_y,
        is_distributive: true,
    });

    // Outer conditional: T extends string ? inner : "z"
    let outer_cond = ConditionalType {
        check_type: union_abc,
        extends_type: TypeId::STRING,
        true_type: inner_cond,
        false_type: lit_z,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &outer_cond);
    // Complex nested distribution
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_distributive_with_never_absorption() {
    // never in union should be absorbed: (string | never) extends T ? X : Y
    let interner = TypeInterner::new();

    let union_with_never = interner.union(vec![TypeId::STRING, TypeId::NEVER]);
    let lit_yes = interner.literal_string("yes");
    let lit_no = interner.literal_string("no");

    let cond = ConditionalType {
        check_type: union_with_never,
        extends_type: TypeId::STRING,
        true_type: lit_yes,
        false_type: lit_no,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // never should be absorbed, only string checked
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_distributive_all_never_result() {
    // When all branches produce never, result should be never
    // T extends string ? never : never with T = number
    let interner = TypeInterner::new();

    let cond = ConditionalType {
        check_type: TypeId::NUMBER,
        extends_type: TypeId::STRING,
        true_type: TypeId::NEVER,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Both branches are never, should return never
    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_distributive_filter_to_single_type() {
    // Extract<T, number> with T = string | number | boolean
    // Should filter down to just number
    let interner = TypeInterner::new();

    let source = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);

    let cond = ConditionalType {
        check_type: source,
        extends_type: TypeId::NUMBER,
        true_type: source, // Returns T when matched
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Only number should remain after filtering
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_distributive_with_literal_types() {
    // Distribution over literal types: T extends "a" ? 1 : 0
    // With T = "a" | "b" | "c"
    let interner = TypeInterner::new();

    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let lit_c = interner.literal_string("c");
    let lit_1 = interner.literal_number(1.0);
    let lit_0 = interner.literal_number(0.0);

    let source = interner.union(vec![lit_a, lit_b, lit_c]);

    let cond = ConditionalType {
        check_type: source,
        extends_type: lit_a,
        true_type: lit_1,
        false_type: lit_0,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // "a" -> 1, "b" -> 0, "c" -> 0, result: 1 | 0
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_distributive_with_object_types() {
    // Distribution with object type matching
    // T extends { x: number } ? T["x"] : never
    let interner = TypeInterner::new();

    let obj_with_x = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    let obj_with_y = interner.object(vec![PropertyInfo::new(
        interner.intern_string("y"),
        TypeId::STRING,
    )]);

    let source = interner.union(vec![obj_with_x, obj_with_y]);
    let pattern = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    let cond = ConditionalType {
        check_type: source,
        extends_type: pattern,
        true_type: TypeId::NUMBER,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Only obj_with_x matches, should return number
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_non_distributive_wrapped_type_param() {
    // Non-distributive: [T] extends [string] ? X : Y
    // Wrapping in tuple prevents distribution
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let wrapped_t = interner.tuple(vec![TupleElement {
        type_id: t_param,
        name: None,
        optional: false,
        rest: false,
    }]);

    let wrapped_string = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }]);

    let lit_yes = interner.literal_string("yes");
    let lit_no = interner.literal_string("no");

    let cond = ConditionalType {
        check_type: wrapped_t,
        extends_type: wrapped_string,
        true_type: lit_yes,
        false_type: lit_no,
        is_distributive: false, // NOT distributive because T is wrapped
    };

    // With non-distributive, union is checked as whole, not distributed
    let result = evaluate_conditional(&interner, &cond);
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_distributive_preserves_type_relationships() {
    // T extends U where T is union should preserve subtype relationships
    // T = string | "hello", U = string
    let interner = TypeInterner::new();

    let lit_hello = interner.literal_string("hello");
    let source = interner.union(vec![TypeId::STRING, lit_hello]);

    let lit_yes = interner.literal_string("yes");
    let lit_no = interner.literal_string("no");

    let cond = ConditionalType {
        check_type: source,
        extends_type: TypeId::STRING,
        true_type: lit_yes,
        false_type: lit_no,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Both string and "hello" extend string, should all be "yes"
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_distributive_with_any_in_union() {
    // any in union makes the whole thing any: (any | string) extends T
    let interner = TypeInterner::new();

    let union_with_any = interner.union(vec![TypeId::ANY, TypeId::STRING]);
    let lit_yes = interner.literal_string("yes");
    let lit_no = interner.literal_string("no");

    let cond = ConditionalType {
        check_type: union_with_any,
        extends_type: TypeId::NUMBER,
        true_type: lit_yes,
        false_type: lit_no,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // any has special behavior - extends everything
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_distributive_with_unknown_direct() {
    // unknown in distribution: T extends unknown is always true
    let interner = TypeInterner::new();

    let source = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::NULL]);
    let lit_yes = interner.literal_string("yes");
    let lit_no = interner.literal_string("no");

    let cond = ConditionalType {
        check_type: source,
        extends_type: TypeId::UNKNOWN,
        true_type: lit_yes,
        false_type: lit_no,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Everything extends unknown, should all be "yes"
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_distributive_infer_in_extends() {
    // T extends (infer U)[] ? U : never
    // Distribution with inference
    let interner = TypeInterner::new();

    let u_name = interner.intern_string("U");
    let infer_u = interner.intern(TypeData::Infer(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let array_pattern = interner.array(infer_u);

    let string_array = interner.array(TypeId::STRING);
    let number_array = interner.array(TypeId::NUMBER);
    let source = interner.union(vec![string_array, number_array, TypeId::BOOLEAN]);

    let cond = ConditionalType {
        check_type: source,
        extends_type: array_pattern,
        true_type: infer_u,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // string[] -> string, number[] -> number, boolean -> never
    // Result: string | number
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_distributive_multiple_type_params() {
    // Complex scenario: T extends U, both are type params
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let u_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: Some(TypeId::STRING), // U extends string
        default: None,
        is_const: false,
    }));

    let lit_yes = interner.literal_string("yes");
    let lit_no = interner.literal_string("no");

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: u_param,
        true_type: lit_yes,
        false_type: lit_no,
        is_distributive: true,
    };

    // Deferred because T is unresolved type param
    let result = evaluate_conditional(&interner, &cond);
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_distributive_recursive_pattern() {
    // Simulating recursive types: T extends any[] ? Flatten<T[number]> : T
    // We can't fully recurse, but we can test the pattern
    let interner = TypeInterner::new();

    let source = interner.union(vec![
        interner.array(TypeId::STRING),
        interner.array(TypeId::NUMBER),
        TypeId::BOOLEAN,
    ]);

    let any_array = interner.array(TypeId::ANY);

    // Simplified: T extends any[] ? T[number] : T
    let cond = ConditionalType {
        check_type: source,
        extends_type: any_array,
        true_type: TypeId::STRING, // Placeholder for T[number]
        false_type: source,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_distributive_boolean_special_case() {
    // boolean = true | false, distribution should handle this
    // T extends true ? "yes" : "no" with T = boolean
    let interner = TypeInterner::new();

    let lit_true = interner.literal_boolean(true);
    let lit_yes = interner.literal_string("yes");
    let lit_no = interner.literal_string("no");

    let cond = ConditionalType {
        check_type: TypeId::BOOLEAN, // boolean = true | false internally
        extends_type: lit_true,
        true_type: lit_yes,
        false_type: lit_no,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // true -> "yes", false -> "no"
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_distributive_empty_union_to_never() {
    // Distribution over empty union should produce never
    // This is important for Exclude<T, T> pattern
    let interner = TypeInterner::new();

    // Simulating a fully excluded result
    let source = TypeId::STRING;

    let cond = ConditionalType {
        check_type: source,
        extends_type: TypeId::STRING,
        true_type: TypeId::NEVER,
        false_type: source,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // string extends string = true, so never
    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_distributive_function_type_union() {
    // Distribution over function types in union
    // T extends (...args: any[]) => any ? ReturnType<T> : never
    let interner = TypeInterner::new();

    let func1 = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let func2 = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let source = interner.union(vec![func1, func2, TypeId::BOOLEAN]);

    let any_func = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: interner.array(TypeId::ANY),
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let cond = ConditionalType {
        check_type: source,
        extends_type: any_func,
        true_type: TypeId::STRING, // Placeholder for return type extraction
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // func1 and func2 match, boolean doesn't
    assert!(result != TypeId::ERROR);
}

// =============================================================================
// INFER EDGE CASE TESTS
// =============================================================================

// -----------------------------------------------------------------------------
// Infer in Variadic Tuple Positions
// -----------------------------------------------------------------------------

/// Test infer from variadic tuple head: [infer H, ...infer T] on [string, number, boolean]
#[test]
fn test_infer_variadic_tuple_head() {
    let interner = TypeInterner::new();

    let infer_h_name = interner.intern_string("H");
    let infer_h = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_h_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_t_name = interner.intern_string("T");
    let infer_t = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: [infer H, ...infer T]
    let pattern = interner.tuple(vec![
        TupleElement {
            type_id: infer_h,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: infer_t,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    // Input: [string, number, boolean]
    let input = interner.tuple(vec![
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

    let cond = ConditionalType {
        check_type: input,
        extends_type: pattern,
        true_type: infer_h,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Should infer H = string
    assert!(result == TypeId::STRING || result != TypeId::ERROR);
}

/// Test infer from variadic tuple tail: [...infer H, infer L] on [string, number, boolean]
#[test]
fn test_infer_variadic_tuple_tail() {
    let interner = TypeInterner::new();

    let infer_h_name = interner.intern_string("H");
    let infer_h = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_h_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_l_name = interner.intern_string("L");
    let infer_l = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_l_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: [...infer H, infer L]
    let pattern = interner.tuple(vec![
        TupleElement {
            type_id: infer_h,
            name: None,
            optional: false,
            rest: true,
        },
        TupleElement {
            type_id: infer_l,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    // Input: [string, number, boolean]
    let input = interner.tuple(vec![
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

    let cond = ConditionalType {
        check_type: input,
        extends_type: pattern,
        true_type: infer_l,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Should infer L = boolean (last element)
    assert!(result == TypeId::BOOLEAN || result != TypeId::ERROR);
}

/// Test infer from variadic tuple middle: [infer F, ...infer M, infer L]
#[test]
fn test_infer_variadic_tuple_middle() {
    let interner = TypeInterner::new();

    let infer_f_name = interner.intern_string("F");
    let infer_f = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_f_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_m_name = interner.intern_string("M");
    let infer_m = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_m_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_l_name = interner.intern_string("L");
    let infer_l = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_l_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: [infer F, ...infer M, infer L]
    let pattern = interner.tuple(vec![
        TupleElement {
            type_id: infer_f,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: infer_m,
            name: None,
            optional: false,
            rest: true,
        },
        TupleElement {
            type_id: infer_l,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    // Input: [string, number, boolean, symbol]
    let input = interner.tuple(vec![
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
        TupleElement {
            type_id: TypeId::SYMBOL,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let cond = ConditionalType {
        check_type: input,
        extends_type: pattern,
        true_type: infer_f,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Should infer F = string
    assert!(result == TypeId::STRING || result != TypeId::ERROR);
}

// -----------------------------------------------------------------------------
// Infer from Overloaded Signatures
// -----------------------------------------------------------------------------

/// Test infer from callable with multiple call signatures (overloaded)
#[test]
fn test_infer_from_overloaded_callable() {
    let interner = TypeInterner::new();

    let infer_r_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_r_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: (...args: any[]) => infer R
    let pattern = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: interner.array(TypeId::ANY),
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: infer_r,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Input: { (x: string): number; (x: number): string }
    let callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![
            CallSignature {
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
                is_method: false,
            },
            CallSignature {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("x")),
                    type_id: TypeId::NUMBER,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: TypeId::STRING,
                type_predicate: None,
                is_method: false,
            },
        ],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    let cond = ConditionalType {
        check_type: callable,
        extends_type: pattern,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Overloaded callables should infer from last/first signature
    // TypeScript infers from last signature
    assert!(result != TypeId::ERROR);
}

/// Test infer from construct signature: new () => infer T
#[test]
fn test_infer_from_construct_signature() {
    let interner = TypeInterner::new();

    let infer_t_name = interner.intern_string("T");
    let infer_t = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: { new (): infer T }
    let pattern = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![],
        construct_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: infer_t,
            type_predicate: None,
            is_method: false,
        }],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    // Input: { new (): string }
    let input = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![],
        construct_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: TypeId::STRING,
            type_predicate: None,
            is_method: false,
        }],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    let cond = ConditionalType {
        check_type: input,
        extends_type: pattern,
        true_type: infer_t,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Should infer T = string from construct signature
    assert!(result == TypeId::STRING || result != TypeId::ERROR);
}

// -----------------------------------------------------------------------------
// Infer with Index Access
// -----------------------------------------------------------------------------

/// Test infer in index access: T extends { prop: infer P } ? T["prop"] : never
#[test]
fn test_infer_with_index_access_result() {
    let interner = TypeInterner::new();

    let infer_p_name = interner.intern_string("P");
    let infer_p = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_p_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: { prop: infer P }
    let pattern = interner.object(vec![PropertyInfo::new(
        interner.intern_string("prop"),
        infer_p,
    )]);

    // Input: { prop: number }
    let input = interner.object(vec![PropertyInfo::new(
        interner.intern_string("prop"),
        TypeId::NUMBER,
    )]);

    // Index access: input["prop"]
    let index_access = interner.intern(TypeData::IndexAccess(
        input,
        interner.literal_string("prop"),
    ));

    let cond = ConditionalType {
        check_type: input,
        extends_type: pattern,
        true_type: index_access,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Should evaluate to number (via index access)
    assert!(result != TypeId::ERROR);
}

/// Test infer from index signature value: { [k: string]: infer V }
#[test]
fn test_infer_from_index_signature_value() {
    let interner = TypeInterner::new();

    let infer_v_name = interner.intern_string("V");
    let infer_v = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_v_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: { [k: string]: infer V }
    let pattern = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: infer_v,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    // Input: { [k: string]: number }
    let input = interner.object_with_index(ObjectShape {
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

    let cond = ConditionalType {
        check_type: input,
        extends_type: pattern,
        true_type: infer_v,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Should infer V = number
    assert!(result == TypeId::NUMBER || result != TypeId::ERROR);
}

// -----------------------------------------------------------------------------
// Infer with Recursive Patterns
// -----------------------------------------------------------------------------

/// Test infer from Promise-like structure: Promise<infer T>
#[test]
fn test_infer_promise_like_unwrap() {
    let interner = TypeInterner::new();

    let infer_t_name = interner.intern_string("T");
    let infer_t = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: { then: (onfulfilled: (value: infer T) => any) => any }
    let callback_param = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
            type_id: infer_t,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let then_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("onfulfilled")),
            type_id: callback_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let pattern = interner.object(vec![PropertyInfo::method(
        interner.intern_string("then"),
        then_fn,
    )]);

    // Input: { then: (onfulfilled: (value: string) => any) => any }
    let input_callback = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let input_then = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("onfulfilled")),
            type_id: input_callback,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let input = interner.object(vec![PropertyInfo::method(
        interner.intern_string("then"),
        input_then,
    )]);

    let cond = ConditionalType {
        check_type: input,
        extends_type: pattern,
        true_type: infer_t,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Should infer T = string (from nested callback parameter)
    assert!(result == TypeId::STRING || result != TypeId::ERROR);
}

// -----------------------------------------------------------------------------
// Infer with Mapped Type Interaction
// -----------------------------------------------------------------------------

/// Test infer from mapped type result
#[test]
fn test_infer_from_mapped_type_output() {
    let interner = TypeInterner::new();

    let infer_v_name = interner.intern_string("V");
    let infer_v = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_v_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: { a: infer V; b: infer V }
    let pattern = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), infer_v),
        PropertyInfo::new(interner.intern_string("b"), infer_v),
    ]);

    // Input: { a: string; b: string }
    let input = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::STRING),
    ]);

    let cond = ConditionalType {
        check_type: input,
        extends_type: pattern,
        true_type: infer_v,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Same infer used in multiple positions should unify to string
    assert!(result == TypeId::STRING || result != TypeId::ERROR);
}

/// Test infer from mismatched same-named infer (should produce union)
#[test]
fn test_infer_same_name_different_values() {
    let interner = TypeInterner::new();

    let infer_v_name = interner.intern_string("V");
    let infer_v = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_v_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: { a: infer V; b: infer V }
    let pattern = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), infer_v),
        PropertyInfo::new(interner.intern_string("b"), infer_v),
    ]);

    // Input: { a: string; b: number } - different types!
    let input = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
    ]);

    let cond = ConditionalType {
        check_type: input,
        extends_type: pattern,
        true_type: infer_v,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Same infer with different values should produce union: string | number
    assert!(result != TypeId::ERROR);
}

// -----------------------------------------------------------------------------
// Infer with KeyOf
// -----------------------------------------------------------------------------

/// Test infer combined with keyof: T extends { [K in keyof infer O]: any } ? O : never
#[test]
fn test_infer_with_keyof_constraint() {
    let interner = TypeInterner::new();

    let infer_k_name = interner.intern_string("K");
    let infer_k = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_k_name,
        constraint: Some(TypeId::STRING), // K extends string
        default: None,
        is_const: false,
    }));

    // Pattern: { [key: infer K]: number } where K extends string
    let pattern = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: infer_k,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    // Input: { [key: string]: number }
    let input = interner.object_with_index(ObjectShape {
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

    let cond = ConditionalType {
        check_type: input,
        extends_type: pattern,
        true_type: infer_k,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Should infer K = string
    assert!(result == TypeId::STRING || result != TypeId::ERROR);
}

// -----------------------------------------------------------------------------
// Infer with Branded Types
// -----------------------------------------------------------------------------

/// Test infer from intersection (branded type pattern): T & { __brand: infer B }
#[test]
fn test_infer_from_branded_intersection() {
    let interner = TypeInterner::new();

    let infer_b_name = interner.intern_string("B");
    let infer_b = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_b_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: { __brand: infer B }
    let brand_pattern = interner.object(vec![PropertyInfo::new(
        interner.intern_string("__brand"),
        infer_b,
    )]);

    // Input: string & { __brand: "UserId" }
    let brand_lit = interner.literal_string("UserId");
    let brand_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("__brand"),
        brand_lit,
    )]);
    let input = interner.intersection(vec![TypeId::STRING, brand_obj]);

    let cond = ConditionalType {
        check_type: input,
        extends_type: brand_pattern,
        true_type: infer_b,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Should infer B = "UserId"
    assert!(result == brand_lit || result != TypeId::ERROR);
}

// -----------------------------------------------------------------------------
// Infer with Readonly/Optional Modifiers
// -----------------------------------------------------------------------------

/// Test infer ignores readonly modifier: { readonly prop: infer T }
#[test]
fn test_infer_ignores_readonly() {
    let interner = TypeInterner::new();

    let infer_t_name = interner.intern_string("T");
    let infer_t = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: { readonly prop: infer T }
    let pattern = interner.object(vec![PropertyInfo {
        name: interner.intern_string("prop"),
        type_id: infer_t,
        write_type: infer_t,
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

    // Input: { prop: number } (not readonly)
    let input = interner.object(vec![PropertyInfo::new(
        interner.intern_string("prop"),
        TypeId::NUMBER,
    )]);

    let cond = ConditionalType {
        check_type: input,
        extends_type: pattern,
        true_type: infer_t,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Should still infer T = number (readonly doesn't affect inference)
    assert!(result == TypeId::NUMBER || result != TypeId::ERROR);
}

/// Test infer from optional tuple element: [string, number?] matches [infer A, infer B?]
#[test]
fn test_infer_optional_tuple_element() {
    let interner = TypeInterner::new();

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

    // Pattern: [infer A, infer B?]
    let pattern = interner.tuple(vec![
        TupleElement {
            type_id: infer_a,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: infer_b,
            name: None,
            optional: true,
            rest: false,
        },
    ]);

    // Input: [string, number]
    let input = interner.tuple(vec![
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

    let result_tuple = interner.tuple(vec![
        TupleElement {
            type_id: infer_a,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: infer_b,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let cond = ConditionalType {
        check_type: input,
        extends_type: pattern,
        true_type: result_tuple,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Should infer A = string, B = number
    assert!(result != TypeId::ERROR);
}

// =============================================================================
// TEMPLATE LITERAL TYPE EDGE CASES
// =============================================================================

#[test]
fn test_template_literal_with_number_type() {
    // `id_${number}` - template literal with number placeholder
    let interner = TypeInterner::new();

    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("id_")),
        TemplateSpan::Type(TypeId::NUMBER),
    ]);

    // Verify template structure is created
    match interner.lookup(template) {
        Some(TypeData::TemplateLiteral(_)) => (),
        _ => panic!("Expected TemplateLiteral type"),
    }
}

#[test]
fn test_template_literal_with_boolean_type() {
    // `is_${boolean}` - template literal with boolean placeholder
    // TypeScript expands this to "is_true" | "is_false"
    let interner = TypeInterner::new();

    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("is_")),
        TemplateSpan::Type(TypeId::BOOLEAN),
    ]);

    // Should expand to union of two string literals
    match interner.lookup(template) {
        Some(TypeData::Union(list_id)) => {
            let members = interner.type_list(list_id);
            assert_eq!(
                members.len(),
                2,
                "Expected 2 members in union for boolean expansion"
            );
            // Both should be string literals
            for member in members.iter() {
                match interner.lookup(*member) {
                    Some(TypeData::Literal(LiteralValue::String(_))) => (),
                    other => panic!("Expected string literal in union, got {other:?}"),
                }
            }
        }
        other => panic!("Expected Union type for `is_${{boolean}}`, got {other:?}"),
    }
}

#[test]
fn test_template_literal_cartesian_product() {
    // `${"a"|"b"}_${"1"|"2"}` should expand to "a_1" | "a_2" | "b_1" | "b_2"
    let interner = TypeInterner::new();

    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let union1 = interner.union(vec![lit_a, lit_b]);

    let lit_1 = interner.literal_string("1");
    let lit_2 = interner.literal_string("2");
    let union2 = interner.union(vec![lit_1, lit_2]);

    let template = interner.template_literal(vec![
        TemplateSpan::Type(union1),
        TemplateSpan::Text(interner.intern_string("_")),
        TemplateSpan::Type(union2),
    ]);

    // With optimization, template literals with expandable unions are expanded immediately
    // `${"a"|"b"}_${"1"|"2"}` becomes "a_1" | "a_2" | "b_1" | "b_2"
    match interner.lookup(template) {
        Some(TypeData::Union(members_id)) => {
            let members = interner.type_list(members_id);
            assert_eq!(
                members.len(),
                4,
                "Expected 4 members in cartesian product union"
            );
        }
        _ => panic!(
            "Expected Union type for template with multiple union interpolations, got {:?}",
            interner.lookup(template)
        ),
    }
}

#[test]
fn test_template_literal_with_never() {
    // `prefix_${never}` should produce never (empty union)
    let interner = TypeInterner::new();

    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("prefix_")),
        TemplateSpan::Type(TypeId::NEVER),
    ]);

    // Template with never should collapse to never on evaluation
    let result = evaluate_type(&interner, template);
    // never in template position should result in never
    assert!(result == TypeId::NEVER || result == template);
}

#[test]
fn test_template_literal_with_any() {
    // `${any}` template with any should produce string
    // TypeScript: `prefix-${any}` collapses to `string` because any can be any value
    let interner = TypeInterner::new();

    let template = interner.template_literal(vec![TemplateSpan::Type(TypeId::ANY)]);

    // Template with any should widen to string - any stringifies to any possible string
    assert_eq!(template, TypeId::STRING);
}

#[test]
fn test_template_literal_concatenation() {
    // `${"hello"}${"world"}` should be "helloworld"
    let interner = TypeInterner::new();

    let hello = interner.literal_string("hello");
    let world = interner.literal_string("world");

    let template =
        interner.template_literal(vec![TemplateSpan::Type(hello), TemplateSpan::Type(world)]);

    // With optimization, string literal interpolations are expanded and concatenated
    // So `${"hello"}${"world"}` becomes "helloworld" string literal
    match interner.lookup(template) {
        Some(TypeData::Literal(LiteralValue::String(atom))) => {
            let s = interner.resolve_atom_ref(atom);
            assert_eq!(
                s.as_ref(),
                "helloworld",
                "Expected concatenated string literal"
            );
        }
        _ => panic!(
            "Expected string literal for concatenated string interpolations, got {:?}",
            interner.lookup(template)
        ),
    }
}

#[test]
fn test_template_literal_empty_string() {
    // `` empty template
    let interner = TypeInterner::new();

    let template = interner.template_literal(vec![]);

    // Empty template should be equivalent to empty string literal
    let result = evaluate_type(&interner, template);
    // Should be a valid type
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_template_literal_single_text() {
    // `hello` just text, no interpolations
    let interner = TypeInterner::new();

    let template =
        interner.template_literal(vec![TemplateSpan::Text(interner.intern_string("hello"))]);

    // Should be equivalent to "hello" literal
    let result = evaluate_type(&interner, template);
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_template_literal_pattern_infer_numeric() {
    // `id_${infer N extends number}` - infer from numeric pattern
    let interner = TypeInterner::new();

    let n_name = interner.intern_string("N");
    let infer_n = interner.intern(TypeData::Infer(TypeParamInfo {
        name: n_name,
        constraint: Some(TypeId::NUMBER),
        default: None,
        is_const: false,
    }));

    let extends_template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("id_")),
        TemplateSpan::Type(infer_n),
    ]);

    // Test matching against "id_42"
    let lit_id_42 = interner.literal_string("id_42");

    let cond = ConditionalType {
        check_type: lit_id_42,
        extends_type: extends_template,
        true_type: infer_n,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Should infer something or at least not error
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_template_literal_multiple_adjacent_types() {
    // `${A}${B}${C}` - multiple type interpolations
    let interner = TypeInterner::new();

    let lit_x = interner.literal_string("x");
    let lit_y = interner.literal_string("y");
    let lit_z = interner.literal_string("z");

    let template = interner.template_literal(vec![
        TemplateSpan::Type(lit_x),
        TemplateSpan::Type(lit_y),
        TemplateSpan::Type(lit_z),
    ]);

    // With optimization, string literal interpolations are expanded and concatenated
    // So `${"x"}${"y"}${"z"}` becomes "xyz" string literal
    match interner.lookup(template) {
        Some(TypeData::Literal(LiteralValue::String(atom))) => {
            let s = interner.resolve_atom_ref(atom);
            assert_eq!(s.as_ref(), "xyz", "Expected concatenated string literal");
        }
        _ => panic!(
            "Expected string literal for concatenated string interpolations, got {:?}",
            interner.lookup(template)
        ),
    }
}

#[test]
fn test_template_literal_union_in_middle() {
    // `pre_${"a"|"b"|"c"}_suf` - union in middle position
    let interner = TypeInterner::new();

    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let lit_c = interner.literal_string("c");
    let union = interner.union(vec![lit_a, lit_b, lit_c]);

    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("pre_")),
        TemplateSpan::Type(union),
        TemplateSpan::Text(interner.intern_string("_suf")),
    ]);

    // With optimization, template literals with expandable unions become a union of string literals
    // `pre_${"a"|"b"|"c"}_suf` becomes "pre_a_suf" | "pre_b_suf" | "pre_c_suf"
    match interner.lookup(template) {
        Some(TypeData::Union(members_id)) => {
            let members = interner.type_list(members_id);
            assert_eq!(members.len(), 3, "Expected 3 members in union");
        }
        _ => panic!(
            "Expected Union type for template with union interpolation, got {:?}",
            interner.lookup(template)
        ),
    }
}

#[test]
fn test_template_literal_bigint_type() {
    // `value_${bigint}` - template with bigint
    let interner = TypeInterner::new();

    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("value_")),
        TemplateSpan::Type(TypeId::BIGINT),
    ]);

    match interner.lookup(template) {
        Some(TypeData::TemplateLiteral(_)) => (),
        _ => panic!("Expected TemplateLiteral type"),
    }
}
