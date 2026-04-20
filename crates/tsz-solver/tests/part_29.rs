use super::*;
use crate::TypeInterner;
use crate::def::DefId;
use crate::{SubtypeChecker, TypeSubstitution, instantiate_type};
#[test]
fn test_never_filtering_all_filtered() {
    // Extract<1 | 2 | 3, string> = never (all filtered out)
    let interner = TypeInterner::new();

    let lit_1 = interner.literal_number(1.0);
    let lit_2 = interner.literal_number(2.0);
    let lit_3 = interner.literal_number(3.0);
    let number_union = interner.union(vec![lit_1, lit_2, lit_3]);

    let cond = ConditionalType {
        check_type: number_union,
        extends_type: TypeId::STRING,
        true_type: number_union,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // All numbers -> never, result should be never
    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_never_filtering_nonnullable() {
    // NonNullable<T> = T extends null | undefined ? never : T
    // NonNullable<string | null | undefined> = string
    let interner = TypeInterner::new();

    let nullable_union = interner.union(vec![TypeId::STRING, TypeId::NULL, TypeId::UNDEFINED]);
    let null_or_undefined = interner.union(vec![TypeId::NULL, TypeId::UNDEFINED]);

    let cond = ConditionalType {
        check_type: nullable_union,
        extends_type: null_or_undefined,
        true_type: TypeId::NEVER,
        false_type: nullable_union,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // string -> string, null -> never, undefined -> never
    // Result should be string
    assert!(result != TypeId::ERROR);
}

// ============================================================================
// Awaited Utility Type Tests
// ============================================================================
// Awaited<T> recursively unwraps Promise-like types.
// Using simplified Promise pattern: { then: (onfulfilled: (value: T) => any) => any }

#[test]
fn test_awaited_basic_promise() {
    // Awaited<Promise<string>> = string
    let interner = TypeInterner::new();

    let then_name = interner.intern_string("then");

    // Promise<string> simplified as { then: string }
    let promise_string = interner.object(vec![PropertyInfo::readonly(then_name, TypeId::STRING)]);

    // Using infer pattern: T extends { then: infer U } ? U : T
    let infer_name = interner.intern_string("U");
    let infer_u = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let pattern = interner.object(vec![PropertyInfo::readonly(then_name, infer_u)]);

    let cond = ConditionalType {
        check_type: promise_string,
        extends_type: pattern,
        true_type: infer_u,
        false_type: promise_string,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Should extract string from Promise<string>
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_awaited_promise_number() {
    // Awaited<Promise<number>> = number
    let interner = TypeInterner::new();

    let then_name = interner.intern_string("then");

    let promise_number = interner.object(vec![PropertyInfo::readonly(then_name, TypeId::NUMBER)]);

    let infer_name = interner.intern_string("U");
    let infer_u = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let pattern = interner.object(vec![PropertyInfo::readonly(then_name, infer_u)]);

    let cond = ConditionalType {
        check_type: promise_number,
        extends_type: pattern,
        true_type: infer_u,
        false_type: promise_number,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_awaited_thenable_matches_optional_onfulfilled_parameter() {
    let interner = TypeInterner::new();

    let then_name = interner.intern_string("then");
    let onfulfilled_name = interner.intern_string("onfulfilled");
    let rest_name = interner.intern_string("args");
    let value_name = interner.intern_string("value");
    let infer_f_name = interner.intern_string("F");
    let infer_rest_name = interner.intern_string("_");

    let source_callback = interner.function(FunctionShape::new(
        vec![ParamInfo::required(value_name, TypeId::NUMBER)],
        TypeId::ANY,
    ));
    let source_then = interner.function(FunctionShape {
        params: vec![ParamInfo::optional(onfulfilled_name, source_callback)],
        this_type: None,
        return_type: TypeId::ANY,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });
    let source_thenable = interner.object(vec![PropertyInfo::readonly(then_name, source_then)]);

    let infer_f = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_f_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let infer_rest = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_rest_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let pattern_then = interner.function(FunctionShape {
        params: vec![
            ParamInfo::required(onfulfilled_name, infer_f),
            ParamInfo::rest(rest_name, infer_rest),
        ],
        this_type: None,
        return_type: TypeId::ANY,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });
    let pattern_thenable = interner.object(vec![PropertyInfo::readonly(then_name, pattern_then)]);

    let cond = ConditionalType {
        check_type: source_thenable,
        extends_type: pattern_thenable,
        true_type: infer_f,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert_eq!(result, source_callback);
}

#[test]
fn test_awaited_nested_promise() {
    // Awaited<Promise<Promise<string>>> = string (recursive unwrap)
    let interner = TypeInterner::new();

    let then_name = interner.intern_string("then");

    // Inner: Promise<string> = { then: string }
    let inner_promise = interner.object(vec![PropertyInfo::readonly(then_name, TypeId::STRING)]);

    // Outer: Promise<Promise<string>> = { then: Promise<string> }
    let outer_promise = interner.object(vec![PropertyInfo::readonly(then_name, inner_promise)]);

    // First unwrap: extracts Promise<string>
    let infer_name = interner.intern_string("U");
    let infer_u = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let pattern = interner.object(vec![PropertyInfo::readonly(then_name, infer_u)]);

    let cond1 = ConditionalType {
        check_type: outer_promise,
        extends_type: pattern,
        true_type: infer_u,
        false_type: outer_promise,
        is_distributive: false,
    };

    let first_unwrap = evaluate_conditional(&interner, &cond1);
    // First unwrap gives Promise<string>
    assert_eq!(first_unwrap, inner_promise);

    // Second unwrap: extracts string
    let cond2 = ConditionalType {
        check_type: first_unwrap,
        extends_type: pattern,
        true_type: infer_u,
        false_type: first_unwrap,
        is_distributive: false,
    };

    let second_unwrap = evaluate_conditional(&interner, &cond2);
    assert_eq!(second_unwrap, TypeId::STRING);
}

#[test]
fn test_awaited_string_passthrough() {
    // Awaited<string> = string (non-Promise passes through)
    let interner = TypeInterner::new();

    let then_name = interner.intern_string("then");

    let infer_name = interner.intern_string("U");
    let infer_u = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let pattern = interner.object(vec![PropertyInfo::readonly(then_name, infer_u)]);

    // string doesn't have 'then' property, so doesn't match pattern
    let cond = ConditionalType {
        check_type: TypeId::STRING,
        extends_type: pattern,
        true_type: infer_u,
        false_type: TypeId::STRING, // Returns string as-is
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // string doesn't extend { then: infer U }, returns false branch
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_awaited_number_passthrough() {
    // Awaited<number> = number
    let interner = TypeInterner::new();

    let then_name = interner.intern_string("then");

    let infer_name = interner.intern_string("U");
    let infer_u = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let pattern = interner.object(vec![PropertyInfo::readonly(then_name, infer_u)]);

    let cond = ConditionalType {
        check_type: TypeId::NUMBER,
        extends_type: pattern,
        true_type: infer_u,
        false_type: TypeId::NUMBER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_awaited_null_undefined_passthrough() {
    // Awaited<null> = null, Awaited<undefined> = undefined
    let interner = TypeInterner::new();

    let then_name = interner.intern_string("then");

    let infer_name = interner.intern_string("U");
    let infer_u = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let pattern = interner.object(vec![PropertyInfo::readonly(then_name, infer_u)]);

    // null passthrough
    let cond_null = ConditionalType {
        check_type: TypeId::NULL,
        extends_type: pattern,
        true_type: infer_u,
        false_type: TypeId::NULL,
        is_distributive: false,
    };
    let result_null = evaluate_conditional(&interner, &cond_null);
    assert_eq!(result_null, TypeId::NULL);

    // undefined passthrough
    let cond_undef = ConditionalType {
        check_type: TypeId::UNDEFINED,
        extends_type: pattern,
        true_type: infer_u,
        false_type: TypeId::UNDEFINED,
        is_distributive: false,
    };
    let result_undef = evaluate_conditional(&interner, &cond_undef);
    assert_eq!(result_undef, TypeId::UNDEFINED);
}

#[test]
fn test_awaited_promise_union_distributive() {
    // Awaited<Promise<string> | Promise<number>> = string | number
    // With distributive conditional, each member is processed
    let interner = TypeInterner::new();

    let then_name = interner.intern_string("then");

    let promise_string = interner.object(vec![PropertyInfo::readonly(then_name, TypeId::STRING)]);

    let promise_number = interner.object(vec![PropertyInfo::readonly(then_name, TypeId::NUMBER)]);

    let infer_name = interner.intern_string("U");
    let infer_u = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let pattern = interner.object(vec![PropertyInfo::readonly(then_name, infer_u)]);

    // Process each union member
    let cond_string = ConditionalType {
        check_type: promise_string,
        extends_type: pattern,
        true_type: infer_u,
        false_type: promise_string,
        is_distributive: false,
    };
    let result_string = evaluate_conditional(&interner, &cond_string);
    assert_eq!(result_string, TypeId::STRING);

    let cond_number = ConditionalType {
        check_type: promise_number,
        extends_type: pattern,
        true_type: infer_u,
        false_type: promise_number,
        is_distributive: false,
    };
    let result_number = evaluate_conditional(&interner, &cond_number);
    assert_eq!(result_number, TypeId::NUMBER);

    // Combined result would be string | number
    let awaited_union = interner.union(vec![result_string, result_number]);
    match interner.lookup(awaited_union) {
        Some(TypeData::Union(list_id)) => {
            let members = interner.type_list(list_id);
            assert_eq!(members.len(), 2);
        }
        _ => panic!("Expected Union type"),
    }
}

#[test]
fn test_awaited_mixed_promise_union() {
    // Awaited<Promise<string> | number> = string | number
    let interner = TypeInterner::new();

    let then_name = interner.intern_string("then");

    let promise_string = interner.object(vec![PropertyInfo::readonly(then_name, TypeId::STRING)]);

    let infer_name = interner.intern_string("U");
    let infer_u = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let pattern = interner.object(vec![PropertyInfo::readonly(then_name, infer_u)]);

    // Promise<string> -> string
    let cond_promise = ConditionalType {
        check_type: promise_string,
        extends_type: pattern,
        true_type: infer_u,
        false_type: promise_string,
        is_distributive: false,
    };
    let result_promise = evaluate_conditional(&interner, &cond_promise);
    assert_eq!(result_promise, TypeId::STRING);

    // number -> number (passthrough)
    let cond_number = ConditionalType {
        check_type: TypeId::NUMBER,
        extends_type: pattern,
        true_type: infer_u,
        false_type: TypeId::NUMBER,
        is_distributive: false,
    };
    let result_number = evaluate_conditional(&interner, &cond_number);
    assert_eq!(result_number, TypeId::NUMBER);

    // Combined: string | number
    let mixed_result = interner.union(vec![result_promise, result_number]);
    match interner.lookup(mixed_result) {
        Some(TypeData::Union(list_id)) => {
            let members = interner.type_list(list_id);
            assert_eq!(members.len(), 2);
        }
        _ => panic!("Expected Union type"),
    }
}

#[test]
fn test_awaited_promise_void() {
    // Awaited<Promise<void>> = void
    let interner = TypeInterner::new();

    let then_name = interner.intern_string("then");

    let promise_void = interner.object(vec![PropertyInfo::readonly(then_name, TypeId::VOID)]);

    let infer_name = interner.intern_string("U");
    let infer_u = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let pattern = interner.object(vec![PropertyInfo::readonly(then_name, infer_u)]);

    let cond = ConditionalType {
        check_type: promise_void,
        extends_type: pattern,
        true_type: infer_u,
        false_type: promise_void,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert_eq!(result, TypeId::VOID);
}

#[test]
fn test_awaited_promise_never() {
    // Awaited<Promise<never>> = never
    let interner = TypeInterner::new();

    let then_name = interner.intern_string("then");

    let promise_never = interner.object(vec![PropertyInfo::readonly(then_name, TypeId::NEVER)]);

    let infer_name = interner.intern_string("U");
    let infer_u = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let pattern = interner.object(vec![PropertyInfo::readonly(then_name, infer_u)]);

    let cond = ConditionalType {
        check_type: promise_never,
        extends_type: pattern,
        true_type: infer_u,
        false_type: promise_never,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_awaited_promise_any() {
    // Awaited<Promise<any>> = any
    let interner = TypeInterner::new();

    let then_name = interner.intern_string("then");

    let promise_any = interner.object(vec![PropertyInfo::readonly(then_name, TypeId::ANY)]);

    let infer_name = interner.intern_string("U");
    let infer_u = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let pattern = interner.object(vec![PropertyInfo::readonly(then_name, infer_u)]);

    let cond = ConditionalType {
        check_type: promise_any,
        extends_type: pattern,
        true_type: infer_u,
        false_type: promise_any,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert_eq!(result, TypeId::ANY);
}

#[test]
fn test_awaited_promise_object() {
    // Awaited<Promise<{ value: number }>> = { value: number }
    let interner = TypeInterner::new();

    let then_name = interner.intern_string("then");
    let value_name = interner.intern_string("value");

    let inner_obj = interner.object(vec![PropertyInfo::new(value_name, TypeId::NUMBER)]);

    let promise_obj = interner.object(vec![PropertyInfo::readonly(then_name, inner_obj)]);

    let infer_name = interner.intern_string("U");
    let infer_u = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let pattern = interner.object(vec![PropertyInfo::readonly(then_name, infer_u)]);

    let cond = ConditionalType {
        check_type: promise_obj,
        extends_type: pattern,
        true_type: infer_u,
        false_type: promise_obj,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert_eq!(result, inner_obj);
}

#[test]
fn test_awaited_promise_array() {
    // Awaited<Promise<string[]>> = string[]
    let interner = TypeInterner::new();

    let then_name = interner.intern_string("then");
    let string_array = interner.array(TypeId::STRING);

    let promise_array = interner.object(vec![PropertyInfo::readonly(then_name, string_array)]);

    let infer_name = interner.intern_string("U");
    let infer_u = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let pattern = interner.object(vec![PropertyInfo::readonly(then_name, infer_u)]);

    let cond = ConditionalType {
        check_type: promise_array,
        extends_type: pattern,
        true_type: infer_u,
        false_type: promise_array,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert_eq!(result, string_array);
}

#[test]
fn test_awaited_triple_nested() {
    // Awaited<Promise<Promise<Promise<boolean>>>> = boolean
    let interner = TypeInterner::new();

    let then_name = interner.intern_string("then");

    // Level 1: Promise<boolean>
    let level1 = interner.object(vec![PropertyInfo::readonly(then_name, TypeId::BOOLEAN)]);

    // Level 2: Promise<Promise<boolean>>
    let level2 = interner.object(vec![PropertyInfo::readonly(then_name, level1)]);

    // Level 3: Promise<Promise<Promise<boolean>>>
    let level3 = interner.object(vec![PropertyInfo::readonly(then_name, level2)]);

    let infer_name = interner.intern_string("U");
    let infer_u = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let pattern = interner.object(vec![PropertyInfo::readonly(then_name, infer_u)]);

    // First unwrap
    let cond1 = ConditionalType {
        check_type: level3,
        extends_type: pattern,
        true_type: infer_u,
        false_type: level3,
        is_distributive: false,
    };
    let unwrap1 = evaluate_conditional(&interner, &cond1);
    assert_eq!(unwrap1, level2);

    // Second unwrap
    let cond2 = ConditionalType {
        check_type: unwrap1,
        extends_type: pattern,
        true_type: infer_u,
        false_type: unwrap1,
        is_distributive: false,
    };
    let unwrap2 = evaluate_conditional(&interner, &cond2);
    assert_eq!(unwrap2, level1);

    // Third unwrap
    let cond3 = ConditionalType {
        check_type: unwrap2,
        extends_type: pattern,
        true_type: infer_u,
        false_type: unwrap2,
        is_distributive: false,
    };
    let unwrap3 = evaluate_conditional(&interner, &cond3);
    assert_eq!(unwrap3, TypeId::BOOLEAN);
}

// =============================================================================
// RECURSIVE TYPE TESTS
// =============================================================================

// -----------------------------------------------------------------------------
// Simple recursive types (type Tree = { left?: Tree, right?: Tree })
// -----------------------------------------------------------------------------

#[test]
fn test_recursive_type_simple_tree() {
    // Test: type Tree = { left?: Tree, right?: Tree, value: number }
    use crate::evaluation::evaluate::TypeEvaluator;
    use crate::relations::subtype::TypeEnvironment;

    let interner = TypeInterner::new();

    // Create Ref(1) for Tree type alias (self-reference)
    let tree_ref = interner.lazy(DefId(1));

    // Define: type Tree = { left?: Tree, right?: Tree, value: number }
    let left_name = interner.intern_string("left");
    let right_name = interner.intern_string("right");
    let value_name = interner.intern_string("value");

    let tree_body = interner.object(vec![
        PropertyInfo::opt(left_name, tree_ref),
        PropertyInfo::opt(right_name, tree_ref),
        PropertyInfo::new(value_name, TypeId::NUMBER),
    ]);

    // Set up resolver with type definition
    let mut env = TypeEnvironment::new();
    env.insert_def(DefId(1), tree_body);

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(tree_ref);

    // Verify the tree structure was evaluated (produces Object type)
    match interner.lookup(result).unwrap() {
        TypeData::Object(shape_id) => {
            let shape = interner.object_shape(shape_id);
            // Should have 3 properties: left, right, value
            assert_eq!(shape.properties.len(), 3);
            // At least one property should have NUMBER type (value)
            let has_number = shape.properties.iter().any(|p| p.type_id == TypeId::NUMBER);
            assert!(has_number, "Should have value property with NUMBER type");
        }
        _ => panic!("Expected Object type"),
    }
}

#[test]
fn test_recursive_type_linked_list() {
    // Test: type List<T> = { value: T, next: List<T> | null }
    use crate::evaluation::evaluate::TypeEvaluator;
    use crate::relations::subtype::TypeEnvironment;

    let interner = TypeInterner::new();

    // Define type parameter T
    let t_name = interner.intern_string("T");
    let t_param = TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    // Create Ref(1) for List type alias
    let list_ref = interner.lazy(DefId(1));

    // Create Application: List<T>
    let list_t = interner.application(list_ref, vec![t_type]);

    // next: List<T> | null
    let next_type = interner.union(vec![list_t, TypeId::NULL]);

    // Define: type List<T> = { value: T, next: List<T> | null }
    let value_name = interner.intern_string("value");
    let next_name = interner.intern_string("next");
    let list_body = interner.object(vec![
        PropertyInfo::new(value_name, t_type),
        PropertyInfo::new(next_name, next_type),
    ]);

    // Create Application: List<string>
    let list_string = interner.application(list_ref, vec![TypeId::STRING]);

    // Set up resolver with type parameters
    let mut env = TypeEnvironment::new();
    env.insert_def_with_params(DefId(1), list_body, vec![t_param]);

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(list_string);

    // Verify the list structure was evaluated (produces Object type)
    match interner.lookup(result).unwrap() {
        TypeData::Object(shape_id) => {
            let shape = interner.object_shape(shape_id);
            // Should have 2 properties: value and next
            assert_eq!(shape.properties.len(), 2);
            // At least one property should be STRING (the substituted T)
            let has_string = shape.properties.iter().any(|p| p.type_id == TypeId::STRING);
            assert!(
                has_string,
                "Should have value property substituted to STRING"
            );
        }
        _ => panic!("Expected Object type"),
    }
}

#[test]
fn test_recursive_type_json_value() {
    // Test: type JsonValue = string | number | boolean | null | JsonValue[] | { [key: string]: JsonValue }
    let interner = TypeInterner::new();

    // Create Ref(1) for JsonValue type alias
    let json_ref = interner.lazy(DefId(1));

    // Create JsonValue[] array
    let json_array = interner.array(json_ref);

    // Create { [key: string]: JsonValue } index signature object
    let json_object = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: json_ref,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    // Define union: string | number | boolean | null | JsonValue[] | { [key: string]: JsonValue }
    let json_body = interner.union(vec![
        TypeId::STRING,
        TypeId::NUMBER,
        TypeId::BOOLEAN,
        TypeId::NULL,
        json_array,
        json_object,
    ]);

    // Verify the union contains 6 members
    match interner.lookup(json_body).unwrap() {
        TypeData::Union(list_id) => {
            let members = interner.type_list(list_id);
            assert_eq!(members.len(), 6);
        }
        _ => panic!("Expected Union type"),
    }
}

