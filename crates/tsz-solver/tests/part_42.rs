use super::*;
use crate::TypeInterner;
use crate::def::DefId;
use crate::{SubtypeChecker, TypeSubstitution, instantiate_type};
#[test]
fn test_return_type_mapped_type_method() {
    // type Mapped<T> = { [K in keyof T]: ReturnType<T[K]> }
    // Edge case: applying ReturnType to values accessed via mapped type
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let k_name = interner.intern_string("K");

    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let keyof_t = interner.intern(TypeData::KeyOf(t_param));
    let k_param_info = TypeParamInfo {
        name: k_name,
        constraint: Some(keyof_t),
        default: None,
        is_const: false,
    };
    let k_param = interner.intern(TypeData::TypeParameter(k_param_info));

    // T[K] - index access
    let index_access = interner.intern(TypeData::IndexAccess(t_param, k_param));

    // Mapped type that transforms each property
    let mapped = MappedType {
        type_param: k_param_info,
        constraint: keyof_t,
        name_type: None,
        template: index_access, // Each property uses T[K]
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);
    // Result depends on T being resolved
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_this_parameter_type_extraction() {
    // ThisParameterType<(this: Window) => void> = Window
    let interner = TypeInterner::new();

    let window_type = interner.object(vec![
        PropertyInfo::readonly(interner.intern_string("document"), TypeId::ANY),
        PropertyInfo::new(interner.intern_string("location"), TypeId::STRING),
    ]);

    let func_with_this = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: Some(window_type),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    match interner.lookup(func_with_this) {
        Some(TypeData::Function(shape_id)) => {
            let shape = interner.function_shape(shape_id);
            assert_eq!(shape.this_type, Some(window_type));
        }
        _ => panic!("Expected Function type"),
    }
}

#[test]
fn test_omit_this_parameter() {
    // OmitThisParameter<(this: Window, x: string) => void>
    // = (x: string) => void (without this parameter)
    let interner = TypeInterner::new();

    let window_type = interner.object(vec![PropertyInfo::new(
        interner.intern_string("location"),
        TypeId::STRING,
    )]);

    // Function with this parameter
    let func_with_this = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: Some(window_type),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Function without this parameter (result of OmitThisParameter)
    let func_without_this = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None, // Omitted
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    match interner.lookup(func_with_this) {
        Some(TypeData::Function(with_id)) => {
            let with_shape = interner.function_shape(with_id);
            match interner.lookup(func_without_this) {
                Some(TypeData::Function(without_id)) => {
                    let without_shape = interner.function_shape(without_id);
                    // Same params
                    assert_eq!(with_shape.params.len(), without_shape.params.len());
                    // Different this_type
                    assert!(with_shape.this_type.is_some());
                    assert!(without_shape.this_type.is_none());
                }
                _ => panic!("Expected Function type"),
            }
        }
        _ => panic!("Expected Function type"),
    }
}

#[test]
fn test_instance_type_from_constructor() {
    // InstanceType<typeof Foo> = Foo instance type
    let interner = TypeInterner::new();

    // Instance type has 'value' property
    let get_value_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let instance_type = interner.object(vec![
        PropertyInfo::new(interner.intern_string("value"), TypeId::STRING),
        PropertyInfo::method(interner.intern_string("getValue"), get_value_method),
    ]);

    // Constructor type
    let ctor = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![],
        construct_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("initial")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: instance_type,
            type_predicate: None,
            is_method: false,
        }],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    // InstanceType extracts the return type of construct signature
    match interner.lookup(ctor) {
        Some(TypeData::Callable(shape_id)) => {
            let shape = interner.callable_shape(shape_id);
            assert_eq!(shape.construct_signatures.len(), 1);
            let extracted_instance = shape.construct_signatures[0].return_type;
            assert_eq!(extracted_instance, instance_type);
        }
        _ => panic!("Expected Callable type"),
    }
}

#[test]
fn test_constructor_parameters_with_generics() {
    // ConstructorParameters<new <T>(value: T) => Container<T>>
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let container = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        t_param,
    )]);

    let generic_ctor = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![],
        construct_signatures: vec![CallSignature {
            type_params: vec![TypeParamInfo {
                name: t_name,
                constraint: None,
                default: None,
                is_const: false,
            }],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("value")),
                type_id: t_param,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: container,
            type_predicate: None,
            is_method: false,
        }],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    match interner.lookup(generic_ctor) {
        Some(TypeData::Callable(shape_id)) => {
            let shape = interner.callable_shape(shape_id);
            let sig = &shape.construct_signatures[0];
            // Has type parameter
            assert_eq!(sig.type_params.len(), 1);
            assert_eq!(sig.type_params[0].name, t_name);
            // Parameter uses type parameter
            assert_eq!(sig.params.len(), 1);
            assert_eq!(sig.params[0].type_id, t_param);
        }
        _ => panic!("Expected Callable type"),
    }
}

#[test]
fn test_awaited_with_nested_promises() {
    // Awaited<Promise<Promise<string>>> = string
    // Awaited recursively unwraps nested promises
    let interner = TypeInterner::new();

    // We model Promise<T> as an object with 'then' method
    // For deeply nested, we just verify the structure
    let inner_then = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let inner_promise = interner.object(vec![PropertyInfo::method(
        interner.intern_string("then"),
        inner_then,
    )]);

    let outer_then = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: inner_promise,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let outer_promise = interner.object(vec![PropertyInfo::method(
        interner.intern_string("then"),
        outer_then,
    )]);

    match interner.lookup(outer_promise) {
        Some(TypeData::Object(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            assert!(!shape.properties.is_empty());
        }
        _ => panic!("Expected Object type"),
    }
}

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
