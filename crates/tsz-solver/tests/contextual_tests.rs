use super::*;
use crate::TypeInterner;
use crate::{CompatChecker, InferenceContext, infer_generic_function};
#[test]
fn test_contextual_no_context() {
    let interner = TypeInterner::new();
    let ctx = ContextualTypeContext::new(&interner);

    assert!(!ctx.has_context());
    assert!(ctx.expected().is_none());
}

#[test]
fn test_contextual_with_expected() {
    let interner = TypeInterner::new();
    let ctx = ContextualTypeContext::with_expected(&interner, TypeId::STRING);

    assert!(ctx.has_context());
    assert_eq!(ctx.expected(), Some(TypeId::STRING));
}

// =============================================================================
// Function Parameter Contextual Typing
// =============================================================================

#[test]
fn test_contextual_function_parameter() {
    let interner = TypeInterner::new();

    // type Handler = (e: string, i: number) => void
    let handler = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("e")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("i")),
                type_id: TypeId::NUMBER,
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

    let ctx = ContextualTypeContext::with_expected(&interner, handler);

    // First parameter should be string
    assert_eq!(ctx.get_parameter_type(0), Some(TypeId::STRING));
    // Second parameter should be number
    assert_eq!(ctx.get_parameter_type(1), Some(TypeId::NUMBER));
    // Third parameter doesn't exist
    assert_eq!(ctx.get_parameter_type(2), None);
}

#[test]
fn test_contextual_function_this_parameter() {
    let interner = TypeInterner::new();

    // type Handler = (this: string, x: number) => void
    let handler = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: Some(TypeId::STRING),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let ctx = ContextualTypeContext::with_expected(&interner, handler);
    assert_eq!(ctx.get_this_type(), Some(TypeId::STRING));
    assert_eq!(ctx.get_parameter_type(0), Some(TypeId::NUMBER));
}

#[test]
fn test_contextual_function_return() {
    let interner = TypeInterner::new();

    // type Fn = () => string
    let fn_type = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let ctx = ContextualTypeContext::with_expected(&interner, fn_type);

    assert_eq!(ctx.get_return_type(), Some(TypeId::STRING));
}

#[test]
fn test_contextual_callable_signature() {
    let interner = TypeInterner::new();

    let call_sig = CallSignature {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: Some(TypeId::BOOLEAN),
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_method: false,
    };

    let callable = interner.callable(CallableShape {
        symbol: None,
        call_signatures: vec![call_sig],
        construct_signatures: vec![],
        properties: vec![],
        ..Default::default()
    });

    let ctx = ContextualTypeContext::with_expected(&interner, callable);

    assert_eq!(ctx.get_parameter_type(0), Some(TypeId::STRING));
    assert_eq!(ctx.get_return_type(), Some(TypeId::NUMBER));
    assert_eq!(ctx.get_this_type(), Some(TypeId::BOOLEAN));
}

#[test]
fn test_contextual_callable_overload_union() {
    let interner = TypeInterner::new();

    let call_sig_a = CallSignature {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: Some(TypeId::STRING),
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_method: false,
    };

    let call_sig_b = CallSignature {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("y")),
                type_id: TypeId::BOOLEAN,
                optional: false,
                rest: false,
            },
        ],
        this_type: Some(TypeId::BOOLEAN),
        return_type: TypeId::STRING,
        type_predicate: None,
        is_method: false,
    };

    let callable = interner.callable(CallableShape {
        symbol: None,
        call_signatures: vec![call_sig_a, call_sig_b],
        construct_signatures: vec![],
        properties: vec![],
        ..Default::default()
    });

    // Use no_implicit_any: true to get union behavior for multiple signatures
    let ctx = ContextualTypeContext::with_expected_and_options(&interner, callable, true);

    let expected_param0 = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let expected_return = interner.union(vec![TypeId::NUMBER, TypeId::STRING]);
    let expected_this = interner.union(vec![TypeId::STRING, TypeId::BOOLEAN]);

    assert_eq!(ctx.get_parameter_type(0), Some(expected_param0));
    assert_eq!(ctx.get_parameter_type(1), Some(TypeId::BOOLEAN));
    assert_eq!(ctx.get_return_type(), Some(expected_return));
    assert_eq!(ctx.get_this_type(), Some(expected_this));
}

#[test]
fn test_contextual_callable_overload_by_arity() {
    let interner = TypeInterner::new();

    let call_sig_a = CallSignature {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_method: false,
    };

    let call_sig_b = CallSignature {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("y")),
                type_id: TypeId::BOOLEAN,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_method: false,
    };

    let callable = interner.callable(CallableShape {
        symbol: None,
        call_signatures: vec![call_sig_a, call_sig_b],
        construct_signatures: vec![],
        properties: vec![],
        ..Default::default()
    });

    let ctx = ContextualTypeContext::with_expected(&interner, callable);

    assert_eq!(ctx.get_parameter_type_for_call(0, 1), Some(TypeId::STRING));
    assert_eq!(ctx.get_parameter_type_for_call(1, 1), None);
    assert_eq!(ctx.get_parameter_type_for_call(0, 2), Some(TypeId::NUMBER));
    assert_eq!(ctx.get_parameter_type_for_call(1, 2), Some(TypeId::BOOLEAN));
}

#[test]
fn test_contextual_function_rest_parameter() {
    let interner = TypeInterner::new();

    // type Fn = (...args: number[]) => void
    let number_array = interner.array(TypeId::NUMBER);
    let fn_type = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: number_array,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let ctx = ContextualTypeContext::with_expected(&interner, fn_type);

    // Any index should get number (from rest parameter)
    assert_eq!(ctx.get_parameter_type(0), Some(TypeId::NUMBER));
    assert_eq!(ctx.get_parameter_type(5), Some(TypeId::NUMBER));
}

// =============================================================================
// Array Contextual Typing
// =============================================================================

#[test]
fn test_contextual_array_element() {
    let interner = TypeInterner::new();

    // number[]
    let number_array = interner.array(TypeId::NUMBER);
    let ctx = ContextualTypeContext::with_expected(&interner, number_array);

    assert_eq!(ctx.get_array_element_type(), Some(TypeId::NUMBER));
}

#[test]
fn test_contextual_tuple_element() {
    let interner = TypeInterner::new();

    // [string, number, boolean]
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

    let ctx = ContextualTypeContext::with_expected(&interner, tuple);

    assert_eq!(ctx.get_tuple_element_type(0), Some(TypeId::STRING));
    assert_eq!(ctx.get_tuple_element_type(1), Some(TypeId::NUMBER));
    assert_eq!(ctx.get_tuple_element_type(2), Some(TypeId::BOOLEAN));
    assert_eq!(ctx.get_tuple_element_type(3), None);
}

// =============================================================================
// Object Contextual Typing
// =============================================================================

#[test]
fn test_contextual_property() {
    let interner = TypeInterner::new();

    // { x: number, y: string }
    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("y"), TypeId::STRING),
    ]);

    let ctx = ContextualTypeContext::with_expected(&interner, obj);

    assert_eq!(ctx.get_property_type("x"), Some(TypeId::NUMBER));
    assert_eq!(ctx.get_property_type("y"), Some(TypeId::STRING));
    assert_eq!(ctx.get_property_type("z"), None);
}

// =============================================================================
// Nested Context
// =============================================================================

#[test]
fn test_contextual_nested_property() {
    let interner = TypeInterner::new();

    // { nested: { value: number } }
    let inner = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        TypeId::NUMBER,
    )]);
    let outer = interner.object(vec![PropertyInfo::new(
        interner.intern_string("nested"),
        inner,
    )]);

    let ctx = ContextualTypeContext::with_expected(&interner, outer);

    // Get child context for "nested"
    let nested_ctx = ctx.for_property("nested");
    assert!(nested_ctx.has_context());
    assert_eq!(nested_ctx.get_property_type("value"), Some(TypeId::NUMBER));
}

#[test]
fn test_contextual_for_array_element() {
    let interner = TypeInterner::new();

    // number[]
    let number_array = interner.array(TypeId::NUMBER);
    let ctx = ContextualTypeContext::with_expected(&interner, number_array);

    let elem_ctx = ctx.for_array_element();
    assert!(elem_ctx.has_context());
    assert_eq!(elem_ctx.expected(), Some(TypeId::NUMBER));
}

#[test]
fn test_contextual_for_parameter() {
    let interner = TypeInterner::new();

    // (x: string) => void
    let fn_type = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let ctx = ContextualTypeContext::with_expected(&interner, fn_type);

    let param_ctx = ctx.for_parameter(0);
    assert!(param_ctx.has_context());
    assert_eq!(param_ctx.expected(), Some(TypeId::STRING));
}

// =============================================================================
// Apply Contextual Type
// =============================================================================

#[test]
fn test_apply_contextual_no_context() {
    let interner = TypeInterner::new();

    // No context - returns expression type
    let result = apply_contextual_type(&interner, TypeId::STRING, None);
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_apply_contextual_any_uses_context() {
    let interner = TypeInterner::new();

    // Expression type is any - use contextual type
    let result = apply_contextual_type(&interner, TypeId::ANY, Some(TypeId::STRING));
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_apply_contextual_any_uses_literal_context() {
    let interner = TypeInterner::new();
    let literal = interner.literal_string("ready");

    // Expression type is any - use contextual literal type
    let result = apply_contextual_type(&interner, TypeId::ANY, Some(literal));
    assert_eq!(result, literal);
}

#[test]
fn test_apply_contextual_unknown_uses_context() {
    let interner = TypeInterner::new();

    // Expression type is unknown - use contextual type
    let result = apply_contextual_type(&interner, TypeId::UNKNOWN, Some(TypeId::NUMBER));
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_apply_contextual_union_preserves_literal() {
    let interner = TypeInterner::new();
    let literal = interner.literal_string("ready");
    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // Union context should not widen a literal expression.
    let result = apply_contextual_type(&interner, literal, Some(union));
    assert_eq!(result, literal);
}

#[test]
fn test_contextual_generic_call_union_preserves_literal() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param.clone()));
    let func = FunctionShape {
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
    };

    let literal = interner.literal_string("ready");
    let inferred = infer_generic_function(&interner, &mut checker, &func, &[literal]);
    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let result = apply_contextual_type(&interner, inferred, Some(union));
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_contextual_generic_return_union_preserves_literal() {
    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");
    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // Expected: () => string | number
    let expected_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: union,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let ctx = ContextualTypeContext::with_expected(&interner, expected_fn);
    let return_ctx = ctx.for_return();

    // Simulate generic return inference: T inferred from returning a literal.
    let mut infer_ctx = InferenceContext::new(&interner);
    let var_t = infer_ctx.fresh_type_param(t_name, false);
    let literal = interner.literal_string("ready");
    infer_ctx.add_lower_bound(var_t, literal);
    let inferred = infer_ctx.resolve_with_constraints(var_t).unwrap();

    let result = apply_contextual_type(&interner, inferred, return_ctx.expected());
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_contextual_union_function_return_preserves_literal() {
    let interner = TypeInterner::new();

    let fn_string = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let fn_number = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let union = interner.union(vec![fn_string, fn_number]);

    let ctx = ContextualTypeContext::with_expected(&interner, union);
    let return_ctx = ctx.for_return();
    let literal = interner.literal_string("ready");
    let result = apply_contextual_type(&interner, literal, return_ctx.expected());
    assert_eq!(result, literal);
}

#[test]
fn test_contextual_generic_return_union_any_uses_context() {
    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");

    let fn_string = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let fn_number = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let union = interner.union(vec![fn_string, fn_number]);

    let ctx = ContextualTypeContext::with_expected(&interner, union);
    let return_ctx = ctx.for_return();

    let mut infer_ctx = InferenceContext::new(&interner);
    let var_t = infer_ctx.fresh_type_param(t_name, false);
    infer_ctx.add_lower_bound(var_t, TypeId::ANY);
    let inferred = infer_ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(inferred, TypeId::ANY);

    let expected_return = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let result = apply_contextual_type(&interner, inferred, return_ctx.expected());
    assert_eq!(result, expected_return);
}

#[test]
fn test_apply_contextual_same_type() {
    let interner = TypeInterner::new();

    // Same type - returns it
    let result = apply_contextual_type(&interner, TypeId::STRING, Some(TypeId::STRING));
    assert_eq!(result, TypeId::STRING);
}

// =============================================================================
// Union Contextual Types
// =============================================================================

#[test]
fn test_contextual_union_function() {
    let interner = TypeInterner::new();

    // ((x: string) => void) | ((x: number) => void)
    let fn1 = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let fn2 = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let union = interner.union(vec![fn1, fn2]);

    let ctx = ContextualTypeContext::with_expected(&interner, union);

    // Parameter type should be string | number
    let param_type = ctx.get_parameter_type(0).unwrap();
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(param_type, expected);
}

#[test]
fn test_contextual_union_arity_param_preserves_literal() {
    let interner = TypeInterner::new();

    // ((x: string) => string) | ((x: string, y: number) => string)
    let fn_one = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let fn_two = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("y")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let union = interner.union(vec![fn_one, fn_two]);
    let ctx = ContextualTypeContext::with_expected(&interner, union);

    let param_type = ctx.get_parameter_type(0).unwrap();
    assert_eq!(param_type, TypeId::STRING);

    let literal = interner.literal_string("ready");
    let param_ctx = ctx.for_parameter(0);
    let result = apply_contextual_type(&interner, literal, param_ctx.expected());
    assert_eq!(result, literal);

    let second_param = ctx.get_parameter_type(1).unwrap();
    assert_eq!(second_param, TypeId::NUMBER);
}

#[test]
fn test_contextual_union_rest_param_preserves_literal() {
    let interner = TypeInterner::new();

    // ((x: string) => string) | ((...args: string[]) => string)
    let fn_one = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let rest_array = interner.array(TypeId::STRING);
    let fn_rest = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: rest_array,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let union = interner.union(vec![fn_one, fn_rest]);

    let ctx = ContextualTypeContext::with_expected(&interner, union);
    let param_type = ctx.get_parameter_type(0).unwrap();
    assert_eq!(param_type, TypeId::STRING);

    let literal = interner.literal_string("ready");
    let param_ctx = ctx.for_parameter(0);
    let result = apply_contextual_type(&interner, literal, param_ctx.expected());
    assert_eq!(result, literal);
}

#[test]
fn test_contextual_union_empty_param_preserves_literal() {
    let interner = TypeInterner::new();

    // (() => string) | ((x: string) => string)
    let fn_empty = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let fn_one = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let union = interner.union(vec![fn_empty, fn_one]);

    let ctx = ContextualTypeContext::with_expected(&interner, union);
    let param_type = ctx.get_parameter_type(0).unwrap();
    assert_eq!(param_type, TypeId::STRING);

    let literal = interner.literal_string("ready");
    let param_ctx = ctx.for_parameter(0);
    let result = apply_contextual_type(&interner, literal, param_ctx.expected());
    assert_eq!(result, literal);
}

#[test]
fn test_contextual_union_optional_param_preserves_literal() {
    let interner = TypeInterner::new();

    // ((x?: string) => string) | ((x: string) => string)
    let fn_optional = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: true,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let fn_required = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let union = interner.union(vec![fn_optional, fn_required]);

    let ctx = ContextualTypeContext::with_expected(&interner, union);
    let param_type = ctx.get_parameter_type(0).unwrap();
    assert_eq!(param_type, TypeId::STRING);

    let literal = interner.literal_string("ready");
    let param_ctx = ctx.for_parameter(0);
    let result = apply_contextual_type(&interner, literal, param_ctx.expected());
    assert_eq!(result, literal);
}

#[test]
fn test_contextual_union_function_param_return_preserves_literal() {
    let interner = TypeInterner::new();

    // ((x: string) => string) | ((x: number) => number)
    let fn_string = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let fn_number = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let union = interner.union(vec![fn_string, fn_number]);

    let ctx = ContextualTypeContext::with_expected(&interner, union);
    let literal = interner.literal_string("ready");

    let param_ctx = ctx.for_parameter(0);
    let param_result = apply_contextual_type(&interner, literal, param_ctx.expected());
    assert_eq!(param_result, literal);

    let return_ctx = ctx.for_return();
    let return_result = apply_contextual_type(&interner, literal, return_ctx.expected());
    assert_eq!(return_result, literal);
}

#[test]
fn test_contextual_union_param_preserves_literal() {
    let interner = TypeInterner::new();

    let union_param = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let fn_type = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: union_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let ctx = ContextualTypeContext::with_expected(&interner, fn_type);
    let param_ctx = ctx.for_parameter(0);
    let literal = interner.literal_string("ready");

    let param_result = apply_contextual_type(&interner, literal, param_ctx.expected());
    assert_eq!(param_result, literal);
}

/// Test that contextual typing works for generic function types like <T>(x: T) => void
/// The parameter should get the type parameter T as its contextual type.
#[test]
fn test_contextual_generic_function_parameter() {
    let interner = TypeInterner::new();

    // Create type parameter T
    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param.clone()));

    // <T>(x: T) => void
    let generic_fn = interner.function(FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: t_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let ctx = ContextualTypeContext::with_expected(&interner, generic_fn);

    // First parameter should be T (the type parameter)
    let param_type = ctx.get_parameter_type(0);
    assert!(
        param_type.is_some(),
        "Generic function parameter should have a contextual type"
    );
    assert_ne!(
        param_type.unwrap(),
        TypeId::UNKNOWN,
        "Generic function parameter type should not be UNKNOWN"
    );
    // The parameter type should be the type parameter T
    assert_eq!(
        param_type.unwrap(),
        t_type,
        "Generic function parameter should be contextually typed as T"
    );
}

#[test]
fn test_contextual_callable_overload_no_implicit_any_false() {
    // When noImplicitAny is false and signatures have different parameter types,
    // contextual typing returns None (which becomes `any` in the checker)
    let interner = TypeInterner::new();

    let call_sig_a = CallSignature {
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
    };

    let call_sig_b = CallSignature {
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
    };

    let callable = interner.callable(CallableShape {
        symbol: None,
        call_signatures: vec![call_sig_a, call_sig_b],
        construct_signatures: vec![],
        properties: vec![],
        ..Default::default()
    });

    // Use no_implicit_any: false - should return None for parameter type
    let ctx = ContextualTypeContext::with_expected_and_options(&interner, callable, false);

    // With noImplicitAny: false, different parameter types result in None (falls back to `any`)
    assert_eq!(ctx.get_parameter_type(0), None);
}

// =============================================================================
// Union Contextual Types - Property Extraction
// =============================================================================

#[test]
fn test_contextual_property_union_with_undefined() {
    let interner = TypeInterner::new();

    // { fn: (x: number) => void } | undefined
    let fn_type = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("fn"),
        fn_type,
    )]);
    let union = interner.union(vec![obj, TypeId::UNDEFINED]);

    let ctx = ContextualTypeContext::with_expected(&interner, union);

    // Property "fn" should be found from the object member of the union
    let prop_type = ctx.get_property_type("fn");
    assert!(
        prop_type.is_some(),
        "Should find property 'fn' in union with undefined"
    );
    assert_eq!(prop_type.unwrap(), fn_type);
}

#[test]
fn test_contextual_property_union_with_null() {
    let interner = TypeInterner::new();

    // { x: number } | null
    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);
    let union = interner.union(vec![obj, TypeId::NULL]);

    let ctx = ContextualTypeContext::with_expected(&interner, union);

    assert_eq!(
        ctx.get_property_type("x"),
        Some(TypeId::NUMBER),
        "Should find property in union with null"
    );
}

#[test]
fn test_contextual_property_union_of_two_objects() {
    let interner = TypeInterner::new();

    // { x: number } | { x: string }
    let obj1 = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);
    let obj2 = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::STRING,
    )]);
    let union = interner.union(vec![obj1, obj2]);

    let ctx = ContextualTypeContext::with_expected(&interner, union);

    // Property type should be number | string
    let prop_type = ctx.get_property_type("x");
    assert!(prop_type.is_some());
    let expected = interner.union_preserve_members(vec![TypeId::NUMBER, TypeId::STRING]);
    assert_eq!(prop_type.unwrap(), expected);
}
