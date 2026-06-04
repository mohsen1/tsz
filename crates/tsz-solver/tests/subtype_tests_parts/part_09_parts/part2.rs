#[test]
fn test_recursive_promise_then_actual_lib_shape_assignable_to_promise_like() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let promise_like_def = DefId(3010);
    let promise_def = DefId(3011);

    let outer_t = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let result1 = TypeParamInfo {
        name: interner.intern_string("TResult1"),
        constraint: None,
        default: Some(interner.type_param(outer_t)),
        is_const: false,
    };
    let result2 = TypeParamInfo {
        name: interner.intern_string("TResult2"),
        constraint: None,
        default: Some(TypeId::NEVER),
        is_const: false,
    };

    let outer_t_ty = interner.type_param(outer_t);
    let result1_ty = interner.type_param(result1);
    let result2_ty = interner.type_param(result2);
    let result_union = interner.union(vec![result1_ty, result2_ty]);
    let promise_like_result =
        interner.application(interner.lazy(promise_like_def), vec![result_union]);
    let promise_result = interner.application(interner.lazy(promise_def), vec![result_union]);
    let promise_like_result1 =
        interner.application(interner.lazy(promise_like_def), vec![result1_ty]);
    let promise_like_result2 =
        interner.application(interner.lazy(promise_like_def), vec![result2_ty]);

    let onfulfilled = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
            type_id: outer_t_ty,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: interner.union(vec![result1_ty, promise_like_result1]),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let onrejected = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("reason")),
            type_id: TypeId::ANY,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: interner.union(vec![result2_ty, promise_like_result2]),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let nullable_onfulfilled = interner.union(vec![onfulfilled, TypeId::UNDEFINED, TypeId::NULL]);
    let nullable_onrejected = interner.union(vec![onrejected, TypeId::UNDEFINED, TypeId::NULL]);

    let then_promise_like = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            type_params: vec![result1, result2],
            params: vec![
                ParamInfo {
                    name: Some(interner.intern_string("onfulfilled")),
                    type_id: nullable_onfulfilled,
                    optional: true,
                    rest: false,
                },
                ParamInfo {
                    name: Some(interner.intern_string("onrejected")),
                    type_id: nullable_onrejected,
                    optional: true,
                    rest: false,
                },
            ],
            this_type: None,
            return_type: promise_like_result,
            type_predicate: None,
            is_method: true,
        }],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    let then_promise = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            type_params: vec![result1, result2],
            params: vec![
                ParamInfo {
                    name: Some(interner.intern_string("onfulfilled")),
                    type_id: nullable_onfulfilled,
                    optional: true,
                    rest: false,
                },
                ParamInfo {
                    name: Some(interner.intern_string("onrejected")),
                    type_id: nullable_onrejected,
                    optional: true,
                    rest: false,
                },
            ],
            this_type: None,
            return_type: promise_result,
            type_predicate: None,
            is_method: true,
        }],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    let promise_like_body = interner.object(vec![PropertyInfo::method(
        interner.intern_string("then"),
        then_promise_like,
    )]);
    let promise_body = interner.object(vec![PropertyInfo::method(
        interner.intern_string("then"),
        then_promise,
    )]);

    env.insert_def_with_params(promise_like_def, promise_like_body, vec![outer_t]);
    env.insert_def_kind(promise_like_def, crate::def::DefKind::Interface);
    env.insert_def_with_params(promise_def, promise_body, vec![outer_t]);
    env.insert_def_kind(promise_def, crate::def::DefKind::Interface);

    let mut checker = SubtypeChecker::with_resolver(&interner, &env);
    let promise_number = interner.application(interner.lazy(promise_def), vec![TypeId::NUMBER]);
    let promise_like_number =
        interner.application(interner.lazy(promise_like_def), vec![TypeId::NUMBER]);

    assert!(
        checker.is_subtype_of(promise_number, promise_like_number),
        "Promise<T> should be assignable to PromiseLike<T> for the real lib then shape"
    );
}

#[test]
fn test_variance_triple_nested_contravariance() {
    // Three levels of contravariance: ((f: (g: (x: T) => void) => void) => void)
    // Three contravariants = contravariant overall
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let wide_type = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // Innermost: (x: T) => void
    let inner_narrow = interner.function(FunctionShape {
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

    let inner_wide = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: wide_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Middle: (g: innermost) => void
    let middle_narrow = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("g")),
            type_id: inner_narrow,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let middle_wide = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("g")),
            type_id: inner_wide,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Outermost: (f: middle) => void
    let outer_narrow = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("f")),
            type_id: middle_narrow,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let outer_wide = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("f")),
            type_id: middle_wide,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Three levels of contravariance = contravariant (in strict mode)
    // outer_narrow <: outer_wide (narrow at innermost becomes wide at triple-contravariant)
    // Current behavior: bivariant for callback parameters - only one direction works
    assert!(!checker.is_subtype_of(outer_narrow, outer_wide));
    assert!(checker.is_subtype_of(outer_wide, outer_narrow));
}

#[test]
fn test_variance_constructor_param_bivariant() {
    // Construct signatures use bivariant parameter checking (like methods).
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let wide_type = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // Instance type
    let instance = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        TypeId::STRING,
    )]);

    let ctor_narrow = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![],
        construct_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: instance,
            type_predicate: None,
            is_method: false,
        }],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    let ctor_wide = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![],
        construct_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: wide_type,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: instance,
            type_predicate: None,
            is_method: false,
        }],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    // Both directions work (bivariant for construct signatures)
    assert!(checker.is_subtype_of(ctor_wide, ctor_narrow));
    assert!(checker.is_subtype_of(ctor_narrow, ctor_wide));
}

#[test]
fn test_variance_rest_param_contravariant() {
    // (...args: (string | number)[]) => void  <:  (...args: string[]) => void
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let wide_type = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let narrow_array = interner.array(TypeId::STRING);
    let wide_array = interner.array(wide_type);

    let fn_narrow_rest = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: narrow_array,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_wide_rest = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: wide_array,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Wide rest param <: narrow rest param (contravariant)
    assert!(checker.is_subtype_of(fn_wide_rest, fn_narrow_rest));
}
