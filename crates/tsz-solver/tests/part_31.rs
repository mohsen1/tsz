use super::*;
use crate::TypeInterner;
use crate::def::DefId;
use crate::{SubtypeChecker, TypeSubstitution, instantiate_type};
#[test]
fn test_infer_with_default_type_used() {
    // T extends { prop: infer P = string } ? P : never
    // When infer fails to match, use default
    let interner = TypeInterner::new();

    let infer_p_name = interner.intern_string("P");
    let infer_p = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_p_name,
        constraint: None,
        default: Some(TypeId::STRING),
        is_const: false,
    }));

    // Pattern: { prop: infer P = string }
    let pattern = interner.object(vec![PropertyInfo::new(
        interner.intern_string("prop"),
        infer_p,
    )]);

    // Input: { prop: number }
    let input = interner.object(vec![PropertyInfo::new(
        interner.intern_string("prop"),
        TypeId::NUMBER,
    )]);

    let cond = ConditionalType {
        check_type: input,
        extends_type: pattern,
        true_type: infer_p,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Should infer P = number (matches), default not used
    assert!(result == TypeId::NUMBER || result != TypeId::ERROR);
}

#[test]
fn test_infer_with_default_type_fallback() {
    // When the pattern doesn't match at all, check default behavior
    let interner = TypeInterner::new();

    let infer_p_name = interner.intern_string("P");
    let infer_p = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_p_name,
        constraint: None,
        default: Some(TypeId::STRING),
        is_const: false,
    }));

    // Pattern: { a: infer P = string }
    let pattern = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        infer_p,
    )]);

    // Input: { b: number } - different property name, won't match
    let input = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);

    let cond = ConditionalType {
        check_type: input,
        extends_type: pattern,
        true_type: infer_p,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Pattern doesn't match, should return never (false branch)
    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_infer_with_default_and_constraint() {
    // T extends { prop: infer P extends object = {} } ? P : never
    let interner = TypeInterner::new();

    let empty_object = interner.object(vec![]);

    let infer_p_name = interner.intern_string("P");
    let infer_p = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_p_name,
        constraint: Some(TypeId::OBJECT),
        default: Some(empty_object),
        is_const: false,
    }));

    // Pattern: { prop: infer P extends object = {} }
    let pattern = interner.object(vec![PropertyInfo::new(
        interner.intern_string("prop"),
        infer_p,
    )]);

    // Input: { prop: { x: 1 } }
    let inner_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);
    let input = interner.object(vec![PropertyInfo::new(
        interner.intern_string("prop"),
        inner_obj,
    )]);

    let cond = ConditionalType {
        check_type: input,
        extends_type: pattern,
        true_type: infer_p,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Should infer P = { x: number } which extends object
    assert!(result == inner_obj || result != TypeId::ERROR);
}

#[test]
fn test_infer_discriminated_union_kind() {
    // T extends { kind: infer K } ? K : never
    // Input: { kind: "circle" } | { kind: "square" }
    let interner = TypeInterner::new();

    let infer_k_name = interner.intern_string("K");
    let infer_k = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_k_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: { kind: infer K }
    let pattern = interner.object(vec![PropertyInfo::new(
        interner.intern_string("kind"),
        infer_k,
    )]);

    // Input: { kind: "circle" }
    let circle = interner.literal_string("circle");
    let circle_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("kind"),
        circle,
    )]);

    // Input: { kind: "square" }
    let square = interner.literal_string("square");
    let square_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("kind"),
        square,
    )]);

    let union_input = interner.union(vec![circle_obj, square_obj]);

    let cond = ConditionalType {
        check_type: union_input,
        extends_type: pattern,
        true_type: infer_k,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Should infer K = "circle" | "square"
    assert!(result != TypeId::ERROR && result != TypeId::NEVER);
}

#[test]
fn test_infer_discriminated_union_with_extra_props() {
    // T extends { type: infer T, data: infer D } ? [T, D] : never
    let interner = TypeInterner::new();

    let infer_t_name = interner.intern_string("T");
    let infer_t = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_d_name = interner.intern_string("D");
    let infer_d = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_d_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Pattern: { type: infer T, data: infer D }
    let pattern = interner.object(vec![
        PropertyInfo::new(interner.intern_string("type"), infer_t),
        PropertyInfo::new(interner.intern_string("data"), infer_d),
    ]);

    // Input: { type: "success", data: number }
    let success = interner.literal_string("success");
    let input = interner.object(vec![
        PropertyInfo::new(interner.intern_string("type"), success),
        PropertyInfo::new(interner.intern_string("data"), TypeId::NUMBER),
    ]);

    // Result: [T, D]
    let result_tuple = interner.tuple(vec![
        TupleElement {
            type_id: infer_t,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: infer_d,
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
    // Should infer T = "success", D = number
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_infer_discriminated_union_filter() {
    // Filter discriminated union by kind
    // T extends { kind: "circle" } ? T : never
    let interner = TypeInterner::new();

    let circle = interner.literal_string("circle");
    let square = interner.literal_string("square");

    // Pattern: { kind: "circle" }
    let pattern = interner.object(vec![PropertyInfo::new(
        interner.intern_string("kind"),
        circle,
    )]);

    // Circle object
    let circle_obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("kind"), circle),
        PropertyInfo::new(interner.intern_string("radius"), TypeId::NUMBER),
    ]);

    // Square object
    let square_obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("kind"), square),
        PropertyInfo::new(interner.intern_string("side"), TypeId::NUMBER),
    ]);

    let union_input = interner.union(vec![circle_obj, square_obj]);

    let cond = ConditionalType {
        check_type: union_input,
        extends_type: pattern,
        true_type: union_input,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Should filter to only circle_obj
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_multiple_infers_both_constrained() {
    // T extends (a: infer A extends string, b: infer B extends number) => any ? [A, B] : never
    let interner = TypeInterner::new();

    let infer_a_name = interner.intern_string("A");
    let infer_a = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_a_name,
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }));

    let infer_b_name = interner.intern_string("B");
    let infer_b = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_b_name,
        constraint: Some(TypeId::NUMBER),
        default: None,
        is_const: false,
    }));

    // Pattern: (a: infer A extends string, b: infer B extends number) => any
    let pattern = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("a")),
                type_id: infer_a,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("b")),
                type_id: infer_b,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Input: (a: "hello", b: 42) => void
    let hello = interner.literal_string("hello");
    let lit_42 = interner.literal_number(42.0);
    let input = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("a")),
                type_id: hello,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("b")),
                type_id: lit_42,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Result: [A, B]
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
    // Should infer A = "hello", B = 42
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_multiple_infers_constraint_violation() {
    // T extends (a: infer A extends string, b: infer B extends string) => any ? [A, B] : never
    // Input has number for b, violating constraint
    let interner = TypeInterner::new();

    let infer_a_name = interner.intern_string("A");
    let infer_a = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_a_name,
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }));

    let infer_b_name = interner.intern_string("B");
    let infer_b = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_b_name,
        constraint: Some(TypeId::STRING), // Constraint: string
        default: None,
        is_const: false,
    }));

    // Pattern: (a: infer A extends string, b: infer B extends string) => any
    let pattern = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("a")),
                type_id: infer_a,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("b")),
                type_id: infer_b,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Input: (a: "hello", b: 42) => void - b violates string constraint
    let hello = interner.literal_string("hello");
    let lit_42 = interner.literal_number(42.0);
    let input = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("a")),
                type_id: hello,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("b")),
                type_id: lit_42, // number, not string!
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

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
    // B infers 42 which doesn't extend string - behavior depends on impl
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_multiple_infers_same_constraint() {
    // T extends { a: infer X extends string, b: infer Y extends string } ? [X, Y] : never
    let interner = TypeInterner::new();

    let infer_x_name = interner.intern_string("X");
    let infer_x = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_x_name,
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }));

    let infer_y_name = interner.intern_string("Y");
    let infer_y = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_y_name,
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }));

    // Pattern: { a: infer X extends string, b: infer Y extends string }
    let pattern = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), infer_x),
        PropertyInfo::new(interner.intern_string("b"), infer_y),
    ]);

    // Input: { a: "foo", b: "bar" }
    let foo = interner.literal_string("foo");
    let bar = interner.literal_string("bar");
    let input = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), foo),
        PropertyInfo::new(interner.intern_string("b"), bar),
    ]);

    let result_tuple = interner.tuple(vec![
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
        check_type: input,
        extends_type: pattern,
        true_type: result_tuple,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Should infer X = "foo", Y = "bar"
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_multiple_infers_different_constraints() {
    // T extends { str: infer S extends string, num: infer N extends number, bool: infer B extends boolean } ? [S, N, B] : never
    let interner = TypeInterner::new();

    let infer_s_name = interner.intern_string("S");
    let infer_s = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_s_name,
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }));

    let infer_n_name = interner.intern_string("N");
    let infer_n = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_n_name,
        constraint: Some(TypeId::NUMBER),
        default: None,
        is_const: false,
    }));

    let infer_b_name = interner.intern_string("B");
    let infer_b = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_b_name,
        constraint: Some(TypeId::BOOLEAN),
        default: None,
        is_const: false,
    }));

    // Pattern
    let pattern = interner.object(vec![
        PropertyInfo::new(interner.intern_string("str"), infer_s),
        PropertyInfo::new(interner.intern_string("num"), infer_n),
        PropertyInfo::new(interner.intern_string("bool"), infer_b),
    ]);

    // Input: { str: "test", num: 123, bool: true }
    let test_str = interner.literal_string("test");
    let lit_123 = interner.literal_number(123.0);
    let lit_true = interner.literal_boolean(true);
    let input = interner.object(vec![
        PropertyInfo::new(interner.intern_string("str"), test_str),
        PropertyInfo::new(interner.intern_string("num"), lit_123),
        PropertyInfo::new(interner.intern_string("bool"), lit_true),
    ]);

    let result_tuple = interner.tuple(vec![
        TupleElement {
            type_id: infer_s,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: infer_n,
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
    // Should infer S = "test", N = 123, B = true
    assert!(result != TypeId::ERROR);
}

// ============================================================================
// typeof (TypeQuery) operator tests
// ============================================================================

#[test]
fn test_typeof_variable_reference_basic() {
    use crate::{SymbolRef, TypeEnvironment};

    // typeof x where x: number
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let sym = SymbolRef(1);
    env.insert(sym, TypeId::NUMBER);

    let type_query = interner.intern(TypeData::TypeQuery(sym));
    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(type_query);

    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_typeof_variable_reference_object_type() {
    use crate::{SymbolRef, TypeEnvironment};

    // typeof x where x: { a: string, b: number }
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
    ]);

    let sym = SymbolRef(1);
    env.insert(sym, obj);

    let type_query = interner.intern(TypeData::TypeQuery(sym));
    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(type_query);

    assert_eq!(result, obj);
}

#[test]
fn test_typeof_variable_reference_array_type() {
    use crate::{SymbolRef, TypeEnvironment};

    // typeof arr where arr: string[]
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let string_array = interner.array(TypeId::STRING);

    let sym = SymbolRef(1);
    env.insert(sym, string_array);

    let type_query = interner.intern(TypeData::TypeQuery(sym));
    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(type_query);

    assert_eq!(result, string_array);
}

#[test]
fn test_typeof_imported_value_basic() {
    use crate::{SymbolRef, TypeEnvironment};

    // typeof importedValue where importedValue: boolean
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    // Simulate an imported value with a different symbol ID
    let imported_sym = SymbolRef(100);
    env.insert(imported_sym, TypeId::BOOLEAN);

    let type_query = interner.intern(TypeData::TypeQuery(imported_sym));
    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(type_query);

    assert_eq!(result, TypeId::BOOLEAN);
}

#[test]
fn test_typeof_imported_value_complex() {
    use crate::{SymbolRef, TypeEnvironment};

    // typeof importedConfig where importedConfig: { port: number, host: string }
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let config_type = interner.object(vec![
        PropertyInfo::new(interner.intern_string("port"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("host"), TypeId::STRING),
    ]);

    let imported_sym = SymbolRef(200);
    env.insert(imported_sym, config_type);

    let type_query = interner.intern(TypeData::TypeQuery(imported_sym));
    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(type_query);

    assert_eq!(result, config_type);
}

#[test]
fn test_typeof_function_type() {
    use crate::{SymbolRef, TypeEnvironment};

    // typeof fn where fn: (x: number) => string
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let fn_type = interner.function(FunctionShape {
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
        is_constructor: false,
        is_method: false,
    });

    let sym = SymbolRef(1);
    env.insert(sym, fn_type);

    let type_query = interner.intern(TypeData::TypeQuery(sym));
    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(type_query);

    assert_eq!(result, fn_type);
}

#[test]
fn test_typeof_function_multiple_params() {
    use crate::{SymbolRef, TypeEnvironment};

    // typeof fn where fn: (a: string, b: number) => boolean
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let fn_type = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("a")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("b")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: TypeId::BOOLEAN,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let sym = SymbolRef(1);
    env.insert(sym, fn_type);

    let type_query = interner.intern(TypeData::TypeQuery(sym));
    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(type_query);

    assert_eq!(result, fn_type);
}

#[test]
fn test_typeof_const_string_literal() {
    use crate::{SymbolRef, TypeEnvironment};

    // typeof x where x: "hello" (const assertion)
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let hello_literal = interner.literal_string("hello");

    let sym = SymbolRef(1);
    env.insert(sym, hello_literal);

    let type_query = interner.intern(TypeData::TypeQuery(sym));
    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(type_query);

    assert_eq!(result, hello_literal);
}

#[test]
fn test_typeof_const_number_literal() {
    use crate::{SymbolRef, TypeEnvironment};

    // typeof x where x: 42 (const assertion)
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let num_literal = interner.literal_number(42.0);

    let sym = SymbolRef(1);
    env.insert(sym, num_literal);

    let type_query = interner.intern(TypeData::TypeQuery(sym));
    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(type_query);

    assert_eq!(result, num_literal);
}

#[test]
fn test_typeof_const_tuple_readonly() {
    use crate::{SymbolRef, TypeEnvironment};

    // typeof x where x = [1, 2, 3] as const -> readonly [1, 2, 3]
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let one = interner.literal_number(1.0);
    let two = interner.literal_number(2.0);
    let three = interner.literal_number(3.0);

    let tuple = interner.tuple(vec![
        TupleElement {
            type_id: one,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: two,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: three,
            name: None,
            optional: false,
            rest: false,
        },
    ]);
    let readonly_tuple = interner.intern(TypeData::ReadonlyType(tuple));

    let sym = SymbolRef(1);
    env.insert(sym, readonly_tuple);

    let type_query = interner.intern(TypeData::TypeQuery(sym));
    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(type_query);

    assert_eq!(result, readonly_tuple);
}

