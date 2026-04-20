#[test]
fn test_variance_function_returning_function() {
    // () => (x: string) => void  vs  () => (x: string | number) => void
    // Outer return is covariant, inner callback param is contravariant
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let wide_param = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // Inner function with narrow param
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

    // Inner function with wide param
    let inner_wide = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: wide_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Factory returning narrow-param function
    let factory_narrow = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: inner_narrow,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Factory returning wide-param function
    let factory_wide = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: inner_wide,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Factory returning wide-param <: factory returning narrow-param
    // Return is covariant, and wide-param callback <: narrow-param callback
    assert!(checker.is_subtype_of(factory_wide, factory_narrow));
    assert!(!checker.is_subtype_of(factory_narrow, factory_wide));
}
#[test]
fn test_variance_union_in_contravariant_position() {
    // (x: A | B) => void  <:  (x: A) => void  (contravariance)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union_ab = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let fn_union_param = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: union_ab,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_single_param = interner.function(FunctionShape {
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

    // Union param <: single param (contravariance)
    assert!(checker.is_subtype_of(fn_union_param, fn_single_param));
    // Single param should NOT be subtype of union param
    assert!(!checker.is_subtype_of(fn_single_param, fn_union_param));
}
#[test]
fn test_variance_intersection_in_covariant_position() {
    // () => A & B  <:  () => A  (covariance)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let obj_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);

    let intersection_ab = interner.intersection(vec![obj_a, obj_b]);

    let fn_returns_intersection = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: intersection_ab,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_returns_a = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: obj_a,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Returns A & B <: returns A (covariance, intersection subtype of member)
    assert!(checker.is_subtype_of(fn_returns_intersection, fn_returns_a));
}
#[test]
fn test_variance_array_element_unsound_covariance() {
    // string[] <: (string | number)[] - TypeScript's unsound covariance
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let wide_element = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let narrow_array = interner.array(TypeId::STRING);
    let wide_array = interner.array(wide_element);

    // TypeScript allows this (unsound)
    assert!(checker.is_subtype_of(narrow_array, wide_array));
}
#[test]
fn test_variance_method_bivariant_params() {
    // Methods are bivariant in their parameters (TypeScript unsoundness)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let wide_type = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // Object with method taking narrow param
    let narrow_method_obj = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![],
        construct_signatures: vec![],
        properties: vec![PropertyInfo {
            name: interner.intern_string("handle"),
            type_id: interner.function(FunctionShape {
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
            }),
            write_type: TypeId::VOID,
            optional: false,
            readonly: false,
            is_method: true,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        }],
        string_index: None,
        number_index: None,
    });

    // Object with method taking wide param
    let wide_method_obj = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![],
        construct_signatures: vec![],
        properties: vec![PropertyInfo {
            name: interner.intern_string("handle"),
            type_id: interner.function(FunctionShape {
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
            }),
            write_type: TypeId::VOID,
            optional: false,
            readonly: false,
            is_method: true,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        }],
        string_index: None,
        number_index: None,
    });

    // Methods are bivariant - both directions should work
    assert!(checker.is_subtype_of(narrow_method_obj, wide_method_obj));
    assert!(checker.is_subtype_of(wide_method_obj, narrow_method_obj));
}
#[test]
fn test_variance_function_property_contravariant() {
    // Function properties are strictly contravariant (not bivariant like methods)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let wide_type = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // Object with function property taking narrow param
    let narrow_fn_obj = interner.object(vec![PropertyInfo {
        name: interner.intern_string("handle"),
        type_id: interner.function(FunctionShape {
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
        }),
        write_type: TypeId::VOID,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    // Object with function property taking wide param
    let wide_fn_obj = interner.object(vec![PropertyInfo {
        name: interner.intern_string("handle"),
        type_id: interner.function(FunctionShape {
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
        }),
        write_type: TypeId::VOID,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    // Wide param function <: narrow param function (contravariant)
    assert!(checker.is_subtype_of(wide_fn_obj, narrow_fn_obj));
}
#[test]
fn test_variance_promise_covariant() {
    // Promise<string> <: Promise<string | number> (covariant)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let wide_type = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // Simulate Promise<string> as { then: (cb: (value: string) => void) => void }
    let then_narrow = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("cb")),
            type_id: interner.function(FunctionShape {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("value")),
                    type_id: TypeId::STRING,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: TypeId::VOID,
                type_predicate: None,
                is_constructor: false,
                is_method: false,
            }),
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let then_wide = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("cb")),
            type_id: interner.function(FunctionShape {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("value")),
                    type_id: wide_type,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: TypeId::VOID,
                type_predicate: None,
                is_constructor: false,
                is_method: false,
            }),
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let promise_narrow = interner.object(vec![PropertyInfo::method(
        interner.intern_string("then"),
        then_narrow,
    )]);

    let promise_wide = interner.object(vec![PropertyInfo::method(
        interner.intern_string("then"),
        then_wide,
    )]);

    // Promise<string> <: Promise<string | number> (covariant in T)
    // then callback param is contravariant, then is contravariant in object = covariant overall
    assert!(checker.is_subtype_of(promise_narrow, promise_wide));
}
#[test]
fn test_recursive_promise_then_assignable_to_promise_like() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let promise_like_def = DefId(3000);
    let promise_def = DefId(3001);

    let outer_t = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };

    let inner_u = TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: None,
        default: None,
        is_const: false,
    };

    let outer_t_ty = interner.type_param(outer_t);
    let inner_u_ty = interner.type_param(inner_u);

    let promise_like_u = interner.application(interner.lazy(promise_like_def), vec![inner_u_ty]);
    let promise_u = interner.application(interner.lazy(promise_def), vec![inner_u_ty]);

    let onfulfilled_promise_like = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
            type_id: outer_t_ty,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: interner.union(vec![inner_u_ty, promise_like_u]),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let then_promise_like = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            type_params: vec![inner_u],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("onfulfilled")),
                type_id: interner.union(vec![onfulfilled_promise_like, TypeId::UNDEFINED]),
                optional: true,
                rest: false,
            }],
            this_type: None,
            return_type: promise_like_u,
            type_predicate: None,
            is_method: true,
        }],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    let onfulfilled_promise = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
            type_id: outer_t_ty,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: interner.union(vec![inner_u_ty, promise_like_u]),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let then_promise = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            type_params: vec![inner_u],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("onfulfilled")),
                type_id: interner.union(vec![onfulfilled_promise, TypeId::UNDEFINED]),
                optional: true,
                rest: false,
            }],
            this_type: None,
            return_type: promise_u,
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
        "Promise<T> should be assignable to PromiseLike<T> in recursive then comparison"
    );
}
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
#[test]
fn test_variance_optional_param_covariant_optionality() {
    // (x?: string) => void  <:  (x: string) => void
    // Optional is more permissive, can be called with fewer args
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let fn_optional = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: true,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
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
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Optional param function <: required param function
    // If you can call with no args, you can certainly call with one
    assert!(checker.is_subtype_of(fn_optional, fn_required));
}
// =============================================================================
// FUNCTION TYPE TESTS - OVERLOADS
// =============================================================================
#[test]
fn test_overload_single_signature_subtype() {
    // Function with one signature should be subtype of callable with same signature
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let fn_type = interner.function(FunctionShape {
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

    let callable_type = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
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
        }],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    // Function <: callable with same signature
    assert!(checker.is_subtype_of(fn_type, callable_type));
}
#[test]
fn test_overload_multiple_to_single() {
    // Callable with multiple overloads <: callable with one matching overload
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let multi_overload = interner.callable(CallableShape {
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
                return_type: TypeId::STRING,
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
                return_type: TypeId::NUMBER,
                type_predicate: None,
                is_method: false,
            },
        ],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    let single_overload = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
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
            is_method: false,
        }],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    // Multi-overload <: single overload (has matching signature)
    assert!(checker.is_subtype_of(multi_overload, single_overload));
}
#[test]
fn test_overload_order_independent_matching() {
    // Overload matching should find the best match regardless of order
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let overloads_ab = interner.callable(CallableShape {
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
                return_type: TypeId::STRING,
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
                return_type: TypeId::NUMBER,
                type_predicate: None,
                is_method: false,
            },
        ],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    let overloads_ba = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![
            CallSignature {
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
                is_method: false,
            },
            CallSignature {
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
                is_method: false,
            },
        ],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    // Order shouldn't matter for subtype relationship
    assert!(checker.is_subtype_of(overloads_ab, overloads_ba));
    assert!(checker.is_subtype_of(overloads_ba, overloads_ab));
}
#[test]
fn test_overload_missing_signature_not_subtype() {
    // Callable missing a required overload is not a subtype
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let single_overload = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
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
            is_method: false,
        }],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    let two_overloads = interner.callable(CallableShape {
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
                return_type: TypeId::STRING,
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
                return_type: TypeId::NUMBER,
                type_predicate: None,
                is_method: false,
            },
        ],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    // Single overload should not be subtype of callable requiring two overloads
    assert!(!checker.is_subtype_of(single_overload, two_overloads));
}
#[test]
fn test_overload_wider_param_satisfies_target() {
    // Overload with wider param type can satisfy narrower target overload
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let wide_param = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let wide_overload = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: wide_param,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    let narrow_overload = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
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
        }],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    // Wide param <: narrow param (contravariance)
    assert!(checker.is_subtype_of(wide_overload, narrow_overload));
}
#[test]
fn test_overload_constructor_subtype() {
    // Constructor overloads should follow same rules as call overloads
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let instance = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        TypeId::STRING,
    )]);

    let multi_ctor = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![],
        construct_signatures: vec![
            CallSignature {
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
            },
            CallSignature {
                type_params: vec![],
                params: vec![],
                this_type: None,
                return_type: instance,
                type_predicate: None,
                is_method: false,
            },
        ],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    let single_ctor = interner.callable(CallableShape {
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

    // Multi-constructor <: single constructor (has matching)
    assert!(checker.is_subtype_of(multi_ctor, single_ctor));
}
#[test]
fn test_overload_with_different_arity() {
    // Overloads with different arities
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let multi_arity = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![
            CallSignature {
                type_params: vec![],
                params: vec![],
                this_type: None,
                return_type: TypeId::STRING,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
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
                is_method: false,
            },
            CallSignature {
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
                is_method: false,
            },
        ],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    let no_args = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: TypeId::STRING,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    // Multi-arity should satisfy no-args target
    assert!(checker.is_subtype_of(multi_arity, no_args));
}

// =============================================================================
// FUNCTION TYPE TESTS - THIS PARAMETER
// =============================================================================
#[test]
fn test_this_parameter_explicit_type() {
    // function(this: Foo, x: string): void
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let foo_type = interner.object(vec![PropertyInfo::new(
        interner.intern_string("name"),
        TypeId::STRING,
    )]);

    let fn_with_this = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: Some(foo_type),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_without_this = interner.function(FunctionShape {
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

    // Function without this requirement <: function with this requirement
    // (less restrictive is subtype)
    assert!(checker.is_subtype_of(fn_without_this, fn_with_this));
}
#[test]
fn test_this_parameter_covariant_in_method() {
    // For methods, this is covariant (subclass method can be assigned to superclass)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let base_type = interner.object(vec![PropertyInfo::new(
        interner.intern_string("name"),
        TypeId::STRING,
    )]);

    let derived_type = interner.object(vec![
        PropertyInfo::new(interner.intern_string("name"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("age"), TypeId::NUMBER),
    ]);

    // Method on derived type
    let derived_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: Some(derived_type),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Method on base type
    let base_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: Some(base_type),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Base method <: derived method (covariant this)
    assert!(checker.is_subtype_of(base_method, derived_method));
}
#[test]
fn test_this_parameter_void_this() {
    // this: void means the function doesn't use this
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let fn_void_this = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: Some(TypeId::VOID),
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_any_this = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: Some(TypeId::ANY),
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_no_this = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // void this and no this should be compatible
    assert!(checker.is_subtype_of(fn_void_this, fn_no_this));
    assert!(checker.is_subtype_of(fn_no_this, fn_void_this));

    // any this is more permissive
    assert!(checker.is_subtype_of(fn_any_this, fn_no_this));
}
#[test]
fn test_this_parameter_in_callable_method() {
    // Callable with method that has this parameter
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj_type = interner.object(vec![PropertyInfo::new(
        interner.intern_string("data"),
        TypeId::STRING,
    )]);

    // Method with this type
    let method_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: Some(obj_type),
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let callable_with_method = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![],
        construct_signatures: vec![],
        properties: vec![PropertyInfo::method(
            interner.intern_string("getData"),
            method_fn,
        )],
        string_index: None,
        number_index: None,
    });

    // Plain method without this
    let plain_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let callable_plain = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![],
        construct_signatures: vec![],
        properties: vec![PropertyInfo::method(
            interner.intern_string("getData"),
            plain_method,
        )],
        string_index: None,
        number_index: None,
    });

    // Both should be compatible (methods are bivariant)
    assert!(checker.is_subtype_of(callable_with_method, callable_plain));
}
#[test]
fn test_this_parameter_fluent_api_pattern() {
    // Fluent API: method returns this type
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // Builder type with set method returning this
    let builder_type = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        TypeId::STRING,
    )]);

    // Method returning the builder (this type)
    let set_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: Some(builder_type),
        return_type: builder_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Different builder that also returns self
    let other_builder = interner.object(vec![
        PropertyInfo::new(interner.intern_string("value"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("extra"), TypeId::NUMBER),
    ]);

    let other_set_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: Some(other_builder),
        return_type: other_builder,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Methods with different this/return types are not subtypes
    // (unless there's a structural relationship)
    assert!(!checker.is_subtype_of(set_method, other_set_method));
}
#[test]
fn test_this_parameter_unknown_this() {
    // this: unknown is maximally restrictive
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let fn_unknown_this = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: Some(TypeId::UNKNOWN),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_string_this = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: Some(TypeId::STRING),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // unknown this should work with any this type
    assert!(checker.is_subtype_of(fn_unknown_this, fn_string_this));
}
#[test]
fn test_overload_with_call_and_construct() {
    // Callable that can be both called and constructed
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let instance = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        TypeId::STRING,
    )]);

    let dual_callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
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
            is_method: false,
        }],
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

    let call_only = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
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
            is_method: false,
        }],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    // Dual callable <: call-only (has matching call signature)
    assert!(checker.is_subtype_of(dual_callable, call_only));
}
#[test]
fn test_overload_rest_vs_multiple_params() {
    // (...args: string[]) should be compatible with (a: string, b: string)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);

    let rest_fn = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("args")),
                type_id: string_array,
                optional: false,
                rest: true,
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    let two_params = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
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
                    type_id: TypeId::STRING,
                    optional: false,
                    rest: false,
                },
            ],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    // Rest params can satisfy fixed params
    assert!(checker.is_subtype_of(rest_fn, two_params));
}
#[test]
fn test_this_in_overload_signature() {
    // Overload with this parameter
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj_type = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    let overload_with_this = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![],
            this_type: Some(obj_type),
            return_type: TypeId::NUMBER,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    let overload_no_this = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: TypeId::NUMBER,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    // No-this is compatible with with-this (no-this is more general)
    assert!(checker.is_subtype_of(overload_no_this, overload_with_this));
}

// =============================================================================
// SYMBOL TYPE TESTS - Unique Symbols, Well-Known Symbols, Symbol.iterator
// =============================================================================
#[test]
fn test_unique_symbol_self_subtype() {
    // A unique symbol is subtype of itself
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let sym = interner.intern(TypeData::UniqueSymbol(SymbolRef(42)));

    assert!(checker.is_subtype_of(sym, sym));
}
#[test]
fn test_unique_symbol_not_subtype_of_different() {
    // Different unique symbols are not subtypes of each other
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let sym_a = interner.intern(TypeData::UniqueSymbol(SymbolRef(1)));
    let sym_b = interner.intern(TypeData::UniqueSymbol(SymbolRef(2)));

    assert!(!checker.is_subtype_of(sym_a, sym_b));
    assert!(!checker.is_subtype_of(sym_b, sym_a));
}
#[test]
fn test_unique_symbol_subtype_of_symbol() {
    // Every unique symbol is subtype of symbol primitive
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let unique_sym = interner.intern(TypeData::UniqueSymbol(SymbolRef(100)));

    assert!(checker.is_subtype_of(unique_sym, TypeId::SYMBOL));
}
#[test]
fn test_symbol_not_subtype_of_unique_symbol() {
    // symbol primitive is not subtype of any unique symbol
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let unique_sym = interner.intern(TypeData::UniqueSymbol(SymbolRef(100)));

    assert!(!checker.is_subtype_of(TypeId::SYMBOL, unique_sym));
}
#[test]
fn test_unique_symbol_in_union() {
    // unique symbol | string
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let unique_sym = interner.intern(TypeData::UniqueSymbol(SymbolRef(1)));
    let sym_or_string = interner.union(vec![unique_sym, TypeId::STRING]);

    // unique symbol is subtype of the union
    assert!(checker.is_subtype_of(unique_sym, sym_or_string));

    // string is subtype of the union
    assert!(checker.is_subtype_of(TypeId::STRING, sym_or_string));

    // union is subtype of symbol | string
    let symbol_or_string = interner.union(vec![TypeId::SYMBOL, TypeId::STRING]);
    assert!(checker.is_subtype_of(sym_or_string, symbol_or_string));
}
#[test]
fn test_well_known_symbol_iterator() {
    // Symbol.iterator is a unique symbol
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // Using conventional SymbolRef for well-known symbols
    let sym_iterator = interner.intern(TypeData::UniqueSymbol(SymbolRef(1000)));

    // It's a subtype of symbol
    assert!(checker.is_subtype_of(sym_iterator, TypeId::SYMBOL));

    // But not equal to another unique symbol
    let sym_async_iterator = interner.intern(TypeData::UniqueSymbol(SymbolRef(1001)));
    assert!(!checker.is_subtype_of(sym_iterator, sym_async_iterator));
}
#[test]
fn test_well_known_symbol_async_iterator() {
    // Symbol.asyncIterator is a unique symbol
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let sym_async_iterator = interner.intern(TypeData::UniqueSymbol(SymbolRef(1001)));

    assert!(checker.is_subtype_of(sym_async_iterator, TypeId::SYMBOL));
}
#[test]
fn test_well_known_symbol_to_string_tag() {
    // Symbol.toStringTag is a unique symbol
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let sym_to_string_tag = interner.intern(TypeData::UniqueSymbol(SymbolRef(1002)));

    assert!(checker.is_subtype_of(sym_to_string_tag, TypeId::SYMBOL));
}
#[test]
fn test_well_known_symbol_has_instance() {
    // Symbol.hasInstance is a unique symbol
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let sym_has_instance = interner.intern(TypeData::UniqueSymbol(SymbolRef(1003)));

    assert!(checker.is_subtype_of(sym_has_instance, TypeId::SYMBOL));
}
#[test]
fn test_symbol_keyed_object_property() {
    // Object with symbol-keyed property
    // { [Symbol.iterator]: () => Iterator }
    let interner = TypeInterner::new();
    let _checker = SubtypeChecker::new(&interner);

    let _sym_iterator = interner.intern(TypeData::UniqueSymbol(SymbolRef(1000)));

    // Iterator-like return type
    let iterator_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Object with symbol-keyed method (using string name as proxy)
    let iterable_obj = interner.object(vec![PropertyInfo {
        name: interner.intern_string("[Symbol.iterator]"),
        type_id: iterator_fn,
        write_type: iterator_fn,
        optional: false,
        readonly: true,
        is_method: true,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    assert!(iterable_obj != TypeId::ERROR);
}
#[test]
fn test_symbol_union_with_multiple_unique() {
    // Union of multiple unique symbols
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let sym_a = interner.intern(TypeData::UniqueSymbol(SymbolRef(1)));
    let sym_b = interner.intern(TypeData::UniqueSymbol(SymbolRef(2)));
    let sym_c = interner.intern(TypeData::UniqueSymbol(SymbolRef(3)));

    let sym_union = interner.union(vec![sym_a, sym_b, sym_c]);

    // Each unique symbol is subtype of the union
    assert!(checker.is_subtype_of(sym_a, sym_union));
    assert!(checker.is_subtype_of(sym_b, sym_union));
    assert!(checker.is_subtype_of(sym_c, sym_union));

    // Union is subtype of symbol
    assert!(checker.is_subtype_of(sym_union, TypeId::SYMBOL));
}
#[test]
fn test_symbol_not_subtype_of_string() {
    // symbol is not subtype of string
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    assert!(!checker.is_subtype_of(TypeId::SYMBOL, TypeId::STRING));
    assert!(!checker.is_subtype_of(TypeId::STRING, TypeId::SYMBOL));
}
#[test]
fn test_symbol_not_subtype_of_number() {
    // symbol is not subtype of number
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    assert!(!checker.is_subtype_of(TypeId::SYMBOL, TypeId::NUMBER));
    assert!(!checker.is_subtype_of(TypeId::NUMBER, TypeId::SYMBOL));
}
#[test]
fn test_unique_symbol_intersection() {
    // Intersection of unique symbol with other type
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let unique_sym = interner.intern(TypeData::UniqueSymbol(SymbolRef(42)));

    // unique symbol & symbol = unique symbol (more specific)
    let intersection = interner.intersection(vec![unique_sym, TypeId::SYMBOL]);

    // The intersection is subtype of symbol
    assert!(checker.is_subtype_of(intersection, TypeId::SYMBOL));
}
#[test]
fn test_symbol_as_property_key() {
    // Symbols can be used as property keys: PropertyKey = string | number | symbol
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let property_key = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::SYMBOL]);

    // symbol is subtype of PropertyKey
    assert!(checker.is_subtype_of(TypeId::SYMBOL, property_key));

    // unique symbol is also subtype of PropertyKey
    let unique_sym = interner.intern(TypeData::UniqueSymbol(SymbolRef(1)));
    assert!(checker.is_subtype_of(unique_sym, property_key));
}
#[test]
fn test_const_unique_symbol_type() {
    // const sym = Symbol("description") has type unique symbol
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let const_sym = interner.intern(TypeData::UniqueSymbol(SymbolRef(999)));

    // Type is unique symbol, not just symbol
    assert!(checker.is_subtype_of(const_sym, TypeId::SYMBOL));
    assert!(!checker.is_subtype_of(TypeId::SYMBOL, const_sym));
}
#[test]
fn test_let_symbol_type() {
    // let sym = Symbol("description") has type symbol (widened)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // let binding gets widened to symbol
    let let_sym = TypeId::SYMBOL;

    // It's just symbol, not unique
    assert!(checker.is_subtype_of(let_sym, TypeId::SYMBOL));
}
#[test]
fn test_symbol_for_shared() {
    // Symbol.for("key") returns shared symbol (not unique)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // Symbol.for returns symbol type (not unique symbol)
    let shared_sym = TypeId::SYMBOL;

    assert!(checker.is_subtype_of(shared_sym, TypeId::SYMBOL));
}
#[test]
fn test_iterable_protocol_types() {
    // Iterable<T> has [Symbol.iterator](): Iterator<T>
    let interner = TypeInterner::new();
    let _checker = SubtypeChecker::new(&interner);

    // IteratorResult<number> = { value: number, done: boolean }
    let value_name = interner.intern_string("value");
    let done_name = interner.intern_string("done");

    let iter_result = interner.object(vec![
        PropertyInfo::readonly(value_name, TypeId::NUMBER),
        PropertyInfo::readonly(done_name, TypeId::BOOLEAN),
    ]);

    // Iterator<number> = { next(): IteratorResult<number> }
    let next_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: iter_result,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let iterator = interner.object(vec![PropertyInfo {
        name: interner.intern_string("next"),
        type_id: next_fn,
        write_type: next_fn,
        optional: false,
        readonly: true,
        is_method: true,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    // Iterator is valid object type
    assert!(iterator != TypeId::ERROR);
}
#[test]
fn test_async_iterable_protocol_types() {
    // AsyncIterable<T> has [Symbol.asyncIterator](): AsyncIterator<T>
    let interner = TypeInterner::new();
    let _checker = SubtypeChecker::new(&interner);

    // AsyncIteratorResult<number> = { value: number, done: boolean }
    let value_name = interner.intern_string("value");
    let done_name = interner.intern_string("done");

    let async_iter_result = interner.object(vec![
        PropertyInfo::readonly(value_name, TypeId::NUMBER),
        PropertyInfo::readonly(done_name, TypeId::BOOLEAN),
    ]);

    // Promise<AsyncIteratorResult<number>>
    let promise = interner.object(vec![PropertyInfo {
        name: interner.intern_string("then"),
        type_id: async_iter_result,
        write_type: async_iter_result,
        optional: false,
        readonly: true,
        is_method: true,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    // AsyncIterator<number> = { next(): Promise<AsyncIteratorResult<number>> }
    let next_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: promise,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let async_iterator = interner.object(vec![PropertyInfo {
        name: interner.intern_string("next"),
        type_id: next_fn,
        write_type: next_fn,
        optional: false,
        readonly: true,
        is_method: true,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    // AsyncIterator is valid object type
    assert!(async_iterator != TypeId::ERROR);
}
#[test]
fn test_symbol_keyof_type() {
    // keyof { [sym]: value } includes the symbol
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let unique_sym = interner.intern(TypeData::UniqueSymbol(SymbolRef(1)));

    // keyof type includes the symbol
    let keyof_result = interner.union(vec![unique_sym, interner.literal_string("name")]);

    // symbol is in the keyof result
    assert!(checker.is_subtype_of(unique_sym, keyof_result));
}
#[test]
fn test_symbol_in_discriminated_union() {
    // Symbol can be used as discriminant
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let sym_a = interner.intern(TypeData::UniqueSymbol(SymbolRef(1)));
    let sym_b = interner.intern(TypeData::UniqueSymbol(SymbolRef(2)));

    // Two variants discriminated by symbol
    let variant_a = interner.object(vec![PropertyInfo::readonly(
        interner.intern_string("kind"),
        sym_a,
    )]);

    let variant_b = interner.object(vec![PropertyInfo::readonly(
        interner.intern_string("kind"),
        sym_b,
    )]);

    let discriminated_union = interner.union(vec![variant_a, variant_b]);

    // Each variant is subtype of union
    assert!(checker.is_subtype_of(variant_a, discriminated_union));
    assert!(checker.is_subtype_of(variant_b, discriminated_union));

    // But not interchangeable
    assert!(!checker.is_subtype_of(variant_a, variant_b));
}

// =============================================================================
// NULL TYPE TESTS - Strict Null Checks, Nullable Unions
// =============================================================================
#[test]
fn test_null_not_subtype_of_string_strict() {
    // With strictNullChecks, null is not assignable to string
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    assert!(!checker.is_subtype_of(TypeId::NULL, TypeId::STRING));
}
#[test]
fn test_undefined_not_subtype_of_string_strict() {
    // With strictNullChecks, undefined is not assignable to string
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    assert!(!checker.is_subtype_of(TypeId::UNDEFINED, TypeId::STRING));
}
#[test]
fn test_null_subtype_of_string_legacy() {
    // Without strictNullChecks, null is assignable to string
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = false;

    assert!(checker.is_subtype_of(TypeId::NULL, TypeId::STRING));
}
#[test]
fn test_undefined_subtype_of_string_legacy() {
    // Without strictNullChecks, undefined is assignable to string
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = false;

    assert!(checker.is_subtype_of(TypeId::UNDEFINED, TypeId::STRING));
}
#[test]
fn test_nullable_union_string() {
    // string | null
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    let nullable_string = interner.union(vec![TypeId::STRING, TypeId::NULL]);

    // null is subtype of string | null
    assert!(checker.is_subtype_of(TypeId::NULL, nullable_string));

    // string is subtype of string | null
    assert!(checker.is_subtype_of(TypeId::STRING, nullable_string));

    // string | null is not subtype of string
    assert!(!checker.is_subtype_of(nullable_string, TypeId::STRING));
}
#[test]
fn test_nullable_union_number() {
    // number | null
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    let nullable_number = interner.union(vec![TypeId::NUMBER, TypeId::NULL]);

    assert!(checker.is_subtype_of(TypeId::NULL, nullable_number));
    assert!(checker.is_subtype_of(TypeId::NUMBER, nullable_number));
    assert!(!checker.is_subtype_of(nullable_number, TypeId::NUMBER));
}
#[test]
fn test_optional_union_undefined() {
    // string | undefined (optional parameter type)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    let optional_string = interner.union(vec![TypeId::STRING, TypeId::UNDEFINED]);

    // undefined is subtype of string | undefined
    assert!(checker.is_subtype_of(TypeId::UNDEFINED, optional_string));

    // string is subtype of string | undefined
    assert!(checker.is_subtype_of(TypeId::STRING, optional_string));

    // string | undefined is not subtype of string
    assert!(!checker.is_subtype_of(optional_string, TypeId::STRING));
}
#[test]
fn test_nullable_and_optional_union() {
    // string | null | undefined
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    let nullable_optional = interner.union(vec![TypeId::STRING, TypeId::NULL, TypeId::UNDEFINED]);

    // All three are subtypes
    assert!(checker.is_subtype_of(TypeId::STRING, nullable_optional));
    assert!(checker.is_subtype_of(TypeId::NULL, nullable_optional));
    assert!(checker.is_subtype_of(TypeId::UNDEFINED, nullable_optional));

    // Not subtype of any individual
    assert!(!checker.is_subtype_of(nullable_optional, TypeId::STRING));
    assert!(!checker.is_subtype_of(nullable_optional, TypeId::NULL));
    assert!(!checker.is_subtype_of(nullable_optional, TypeId::UNDEFINED));
}
#[test]
fn test_null_distinct_from_undefined() {
    // null and undefined are distinct types
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    assert!(!checker.is_subtype_of(TypeId::NULL, TypeId::UNDEFINED));
    assert!(!checker.is_subtype_of(TypeId::UNDEFINED, TypeId::NULL));
}
#[test]
fn test_null_subtype_of_self() {
    // null is subtype of null
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    assert!(checker.is_subtype_of(TypeId::NULL, TypeId::NULL));
}
#[test]
fn test_undefined_subtype_of_self() {
    // undefined is subtype of undefined
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    assert!(checker.is_subtype_of(TypeId::UNDEFINED, TypeId::UNDEFINED));
}
#[test]
fn test_null_subtype_of_any() {
    // null is subtype of any
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    assert!(checker.is_subtype_of(TypeId::NULL, TypeId::ANY));
}
#[test]
fn test_undefined_subtype_of_any() {
    // undefined is subtype of any
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    assert!(checker.is_subtype_of(TypeId::UNDEFINED, TypeId::ANY));
}
#[test]
fn test_null_subtype_of_unknown() {
    // null is subtype of unknown
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    assert!(checker.is_subtype_of(TypeId::NULL, TypeId::UNKNOWN));
}
#[test]
fn test_undefined_subtype_of_unknown() {
    // undefined is subtype of unknown
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    assert!(checker.is_subtype_of(TypeId::UNDEFINED, TypeId::UNKNOWN));
}
#[test]
fn test_null_not_subtype_of_object() {
    // null is not subtype of object
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    assert!(!checker.is_subtype_of(TypeId::NULL, TypeId::OBJECT));
}
#[test]
fn test_undefined_not_subtype_of_object() {
    // undefined is not subtype of object
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    assert!(!checker.is_subtype_of(TypeId::UNDEFINED, TypeId::OBJECT));
}
#[test]
fn test_null_not_subtype_of_never() {
    // null is not subtype of never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    assert!(!checker.is_subtype_of(TypeId::NULL, TypeId::NEVER));
}
#[test]
fn test_never_subtype_of_null() {
    // never is subtype of null (never is bottom type)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    assert!(checker.is_subtype_of(TypeId::NEVER, TypeId::NULL));
}
#[test]
fn test_nullable_object_type() {
    // { x: string } | null
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::STRING,
    )]);

    let nullable_obj = interner.union(vec![obj, TypeId::NULL]);

    // Object is subtype of nullable object
    assert!(checker.is_subtype_of(obj, nullable_obj));

    // null is subtype of nullable object
    assert!(checker.is_subtype_of(TypeId::NULL, nullable_obj));

    // Nullable object is not subtype of object
    assert!(!checker.is_subtype_of(nullable_obj, obj));
}
#[test]
fn test_nullable_function_type() {
    // (() => void) | null
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    let fn_type = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let nullable_fn = interner.union(vec![fn_type, TypeId::NULL]);

    // Function is subtype of nullable function
    assert!(checker.is_subtype_of(fn_type, nullable_fn));

    // null is subtype of nullable function
    assert!(checker.is_subtype_of(TypeId::NULL, nullable_fn));
}
#[test]
fn test_nullable_array_type() {
    // string[] | null
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    let string_array = interner.array(TypeId::STRING);
    let nullable_array = interner.union(vec![string_array, TypeId::NULL]);

    // Array is subtype of nullable array
    assert!(checker.is_subtype_of(string_array, nullable_array));

    // null is subtype of nullable array
    assert!(checker.is_subtype_of(TypeId::NULL, nullable_array));
}
#[test]
fn test_void_distinct_from_undefined() {
    // void is not the same as undefined
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    // undefined is subtype of void
    assert!(checker.is_subtype_of(TypeId::UNDEFINED, TypeId::VOID));

    // void is not subtype of undefined (void is wider)
    // Note: In TypeScript, void can accept undefined
    // but void is not assignable to undefined
}
#[test]
fn test_nullable_literal_type() {
    // "hello" | null
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    let hello = interner.literal_string("hello");
    let nullable_hello = interner.union(vec![hello, TypeId::NULL]);

    // Literal is subtype of nullable literal
    assert!(checker.is_subtype_of(hello, nullable_hello));

    // null is subtype of nullable literal
    assert!(checker.is_subtype_of(TypeId::NULL, nullable_hello));

    // string is not subtype of nullable literal
    assert!(!checker.is_subtype_of(TypeId::STRING, nullable_hello));
}
#[test]
fn test_non_null_assertion_type() {
    // NonNullable<string | null | undefined> = string
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    // After non-null assertion, only string remains
    let non_null_result = TypeId::STRING;

    // string is subtype of the original union
    let nullable_optional = interner.union(vec![TypeId::STRING, TypeId::NULL, TypeId::UNDEFINED]);
    assert!(checker.is_subtype_of(non_null_result, nullable_optional));
}
#[test]
fn test_nullable_union_widening() {
    // string | null | undefined is wider than string | null
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    let nullable = interner.union(vec![TypeId::STRING, TypeId::NULL]);
    let nullable_optional = interner.union(vec![TypeId::STRING, TypeId::NULL, TypeId::UNDEFINED]);

    // string | null is subtype of string | null | undefined
    assert!(checker.is_subtype_of(nullable, nullable_optional));

    // string | null | undefined is not subtype of string | null
    assert!(!checker.is_subtype_of(nullable_optional, nullable));
}
#[test]
fn test_null_in_intersection() {
    // string & null = never (incompatible)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    let intersection = interner.intersection(vec![TypeId::STRING, TypeId::NULL]);

    // Intersection of incompatible types reduces to never-like
    // The intersection is subtype of string (vacuously)
    assert!(checker.is_subtype_of(intersection, TypeId::STRING));
}
#[test]
fn test_optional_property_accepts_undefined() {
    // { x?: string } - x can be string | undefined
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    // Optional property type
    let optional_value = interner.union(vec![TypeId::STRING, TypeId::UNDEFINED]);

    // undefined is valid
    assert!(checker.is_subtype_of(TypeId::UNDEFINED, optional_value));

    // string is valid
    assert!(checker.is_subtype_of(TypeId::STRING, optional_value));

    // null is not valid for optional property (unless explicitly added)
    assert!(!checker.is_subtype_of(TypeId::NULL, optional_value));
}
#[test]
fn test_nullish_coalescing_result_type() {
    // (string | null) ?? "default" -> string
    // The result excludes null
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    // After ?? operation, null is excluded
    let result = TypeId::STRING;

    // Result is subtype of original nullable
    let nullable = interner.union(vec![TypeId::STRING, TypeId::NULL]);
    assert!(checker.is_subtype_of(result, nullable));
}
#[test]
fn test_null_union_with_literal_numbers() {
    // 1 | 2 | 3 | null
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    let lit_1 = interner.literal_number(1.0);
    let lit_2 = interner.literal_number(2.0);
    let lit_3 = interner.literal_number(3.0);

    let nullable_nums = interner.union(vec![lit_1, lit_2, lit_3, TypeId::NULL]);

    // Each literal is subtype
    assert!(checker.is_subtype_of(lit_1, nullable_nums));
    assert!(checker.is_subtype_of(lit_2, nullable_nums));
    assert!(checker.is_subtype_of(lit_3, nullable_nums));
    assert!(checker.is_subtype_of(TypeId::NULL, nullable_nums));

    // Number itself is not subtype
    assert!(!checker.is_subtype_of(TypeId::NUMBER, nullable_nums));
}
#[test]
fn test_undefined_union_with_boolean() {
    // boolean | undefined
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    let optional_bool = interner.union(vec![TypeId::BOOLEAN, TypeId::UNDEFINED]);

    assert!(checker.is_subtype_of(TypeId::BOOLEAN, optional_bool));
    assert!(checker.is_subtype_of(TypeId::UNDEFINED, optional_bool));

    // true/false literals are subtypes too
    let lit_true = interner.literal_boolean(true);
    let lit_false = interner.literal_boolean(false);
    assert!(checker.is_subtype_of(lit_true, optional_bool));
    assert!(checker.is_subtype_of(lit_false, optional_bool));
}

// =============================================================================
// Intersection Type Tests - Object and Primitive Intersections
// =============================================================================
// Additional tests for intersection type behavior
#[test]
fn test_primitive_intersection_string_number_is_never() {
    // string & number should reduce to never (disjoint primitives)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_and_number = interner.intersection(vec![TypeId::STRING, TypeId::NUMBER]);

    // Should be never (or equivalent to never)
    assert!(checker.is_subtype_of(string_and_number, TypeId::NEVER));
}
#[test]
fn test_primitive_intersection_boolean_string_is_never() {
    // boolean & string should reduce to never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let bool_and_string = interner.intersection(vec![TypeId::BOOLEAN, TypeId::STRING]);

    assert!(checker.is_subtype_of(bool_and_string, TypeId::NEVER));
}
#[test]
fn test_primitive_intersection_number_bigint_is_never() {
    // number & bigint should reduce to never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let num_and_bigint = interner.intersection(vec![TypeId::NUMBER, TypeId::BIGINT]);

    assert!(checker.is_subtype_of(num_and_bigint, TypeId::NEVER));
}
#[test]
fn test_literal_intersection_same_type() {
    // "hello" & string should be "hello"
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let hello = interner.literal_string("hello");
    let hello_and_string = interner.intersection(vec![hello, TypeId::STRING]);

    // "hello" & string is just "hello"
    assert!(checker.is_subtype_of(hello_and_string, hello));
    assert!(checker.is_subtype_of(hello, hello_and_string));
}
#[test]
fn test_literal_intersection_different_literals_is_never() {
    // "hello" & "world" should be never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let hello = interner.literal_string("hello");
    let world = interner.literal_string("world");
    let hello_and_world = interner.intersection(vec![hello, world]);

    assert!(checker.is_subtype_of(hello_and_world, TypeId::NEVER));
}
#[test]
fn test_number_literal_intersection_different_values() {
    // 1 & 2 should be never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let one = interner.literal_number(1.0);
    let two = interner.literal_number(2.0);
    let one_and_two = interner.intersection(vec![one, two]);

    assert!(checker.is_subtype_of(one_and_two, TypeId::NEVER));
}
#[test]
fn test_boolean_literal_intersection() {
    // true & false should be never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let lit_true = interner.literal_boolean(true);
    let lit_false = interner.literal_boolean(false);
    let true_and_false = interner.intersection(vec![lit_true, lit_false]);

    assert!(checker.is_subtype_of(true_and_false, TypeId::NEVER));
}
#[test]
fn test_object_intersection_disjoint_properties() {
    // { a: string } & { b: number } = { a: string, b: number }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");

    let obj_a = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    let obj_b = interner.object(vec![PropertyInfo::new(b_name, TypeId::NUMBER)]);

    let intersection = interner.intersection(vec![obj_a, obj_b]);

    // Should be subtype of both components
    assert!(checker.is_subtype_of(intersection, obj_a));
    assert!(checker.is_subtype_of(intersection, obj_b));
}
#[test]
fn test_object_intersection_same_property_compatible() {
    // { x: string } & { x: string } = { x: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");

    let obj1 = interner.object(vec![PropertyInfo::new(x_name, TypeId::STRING)]);

    let obj2 = interner.object(vec![PropertyInfo::new(x_name, TypeId::STRING)]);

    let intersection = interner.intersection(vec![obj1, obj2]);

    // Should be equivalent to the original
    assert!(checker.is_subtype_of(intersection, obj1));
    assert!(checker.is_subtype_of(obj1, intersection));
}
#[test]
fn test_object_intersection_property_narrowing() {
    // { x: string | number } & { x: string } = { x: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");
    let string_or_number = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let obj_wide = interner.object(vec![PropertyInfo::new(x_name, string_or_number)]);

    let obj_narrow = interner.object(vec![PropertyInfo::new(x_name, TypeId::STRING)]);

    let intersection = interner.intersection(vec![obj_wide, obj_narrow]);

    // Intersection should be subtype of the narrow version
    assert!(checker.is_subtype_of(intersection, obj_narrow));
}
#[test]
fn test_intersection_with_any() {
    // T & any = any (any absorbs in intersection for assignability)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo::new(x_name, TypeId::STRING)]);

    let obj_and_any = interner.intersection(vec![obj, TypeId::ANY]);

    // any is assignable to/from most things
    assert!(checker.is_subtype_of(TypeId::ANY, obj_and_any));
}
#[test]
fn test_intersection_with_unknown() {
    // T & unknown = T (unknown is identity for intersection)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo::new(x_name, TypeId::STRING)]);

    let obj_and_unknown = interner.intersection(vec![obj, TypeId::UNKNOWN]);

    // Should be equivalent to obj
    assert!(checker.is_subtype_of(obj_and_unknown, obj));
    assert!(checker.is_subtype_of(obj, obj_and_unknown));
}
#[test]
fn test_function_intersection_creates_overload() {
    // ((x: string) => number) & ((x: number) => string)
    // Creates an overloaded function type
    let interner = TypeInterner::new();

    let x_name = interner.intern_string("x");

    let fn_str_to_num = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo::required(x_name, TypeId::STRING)],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_num_to_str = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo::required(x_name, TypeId::NUMBER)],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let intersection = interner.intersection(vec![fn_str_to_num, fn_num_to_str]);

    // Intersection should be valid (creates overloaded type)
    assert!(intersection != TypeId::ERROR);
    assert!(intersection != TypeId::NEVER);
}
#[test]
fn test_intersection_brand_pattern() {
    // Branded type: string & { __brand: "UserId" }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let brand_name = interner.intern_string("__brand");
    let user_id_lit = interner.literal_string("UserId");

    let brand_obj = interner.object(vec![PropertyInfo::new(brand_name, user_id_lit)]);

    let branded_string = interner.intersection(vec![TypeId::STRING, brand_obj]);

    // Branded string should NOT be assignable to plain string
    // (intersection is more specific)
    assert!(!checker.is_subtype_of(TypeId::STRING, branded_string));

    // Branded string IS a subtype of string
    assert!(checker.is_subtype_of(branded_string, TypeId::STRING));
}
#[test]
fn test_intersection_different_brands_is_never() {
    // (string & {__brand: "A"}) & (string & {__brand: "B"}) = never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let brand_name = interner.intern_string("__brand");
    let lit_a = interner.literal_string("A");
    let lit_b = interner.literal_string("B");

    let brand_a = interner.object(vec![PropertyInfo::new(brand_name, lit_a)]);

    let brand_b = interner.object(vec![PropertyInfo::new(brand_name, lit_b)]);

    let branded_a = interner.intersection(vec![TypeId::STRING, brand_a]);
    let branded_b = interner.intersection(vec![TypeId::STRING, brand_b]);
    let both = interner.intersection(vec![branded_a, branded_b]);

    // Two different brands intersected should be never
    assert!(checker.is_subtype_of(both, TypeId::NEVER));
}
#[test]
fn test_intersection_readonly_property() {
    // { readonly x: string } & { x: string }
    let interner = TypeInterner::new();
    let _checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");

    let readonly_obj = interner.object(vec![PropertyInfo::readonly(x_name, TypeId::STRING)]);

    let mutable_obj = interner.object(vec![PropertyInfo::new(x_name, TypeId::STRING)]);

    let intersection = interner.intersection(vec![readonly_obj, mutable_obj]);

    // Should be a valid intersection
    assert!(intersection != TypeId::ERROR);
}
#[test]
fn test_intersection_optional_and_required() {
    // { x?: string } & { x: string } = { x: string } (required wins)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");

    let optional_obj = interner.object(vec![PropertyInfo::opt(x_name, TypeId::STRING)]);

    let required_obj = interner.object(vec![PropertyInfo::new(x_name, TypeId::STRING)]);

    let intersection = interner.intersection(vec![optional_obj, required_obj]);

    // Intersection should be subtype of required
    assert!(checker.is_subtype_of(intersection, required_obj));
}
#[test]
fn test_intersection_index_signature_with_properties() {
    // { [key: string]: number } & { x: number }
    let interner = TypeInterner::new();

    let x_name = interner.intern_string("x");

    let index_sig = interner.object_with_index(ObjectShape {
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

    let prop_obj = interner.object(vec![PropertyInfo::new(x_name, TypeId::NUMBER)]);

    let intersection = interner.intersection(vec![index_sig, prop_obj]);

    // Should be valid
    assert!(intersection != TypeId::ERROR);
}
#[test]
fn test_intersection_two_index_signatures() {
    // { [key: string]: number } & { [key: string]: 1 | 2 }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let one = interner.literal_number(1.0);
    let two = interner.literal_number(2.0);
    let one_or_two = interner.union(vec![one, two]);

    let index_number = interner.object_with_index(ObjectShape {
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

    let index_literal = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: one_or_two,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let intersection = interner.intersection(vec![index_number, index_literal]);

    // Intersection should be subtype of the more specific one
    assert!(checker.is_subtype_of(intersection, index_literal));
}
#[test]
fn test_array_intersection() {
    // string[] & number[] — tsc does NOT eagerly reduce this to never.
    // The intersection remains a valid (albeit uninhabitable) type.
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let number_array = interner.array(TypeId::NUMBER);

    let intersection = interner.intersection(vec![string_array, number_array]);

    // tsc does not reduce array intersections with incompatible elements to never
    assert!(!checker.is_subtype_of(intersection, TypeId::NEVER));
}
#[test]
fn test_tuple_intersection_compatible() {
    // [string, number] & [string, number] = [string, number]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

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

    let intersection = interner.intersection(vec![tuple, tuple]);

    // Should be equivalent to the tuple itself
    assert!(checker.is_subtype_of(intersection, tuple));
    assert!(checker.is_subtype_of(tuple, intersection));
}
#[test]
fn test_tuple_intersection_incompatible() {
    // [string, number] & [number, string] — tsc does NOT reduce to never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let tuple1 = interner.tuple(vec![
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

    let tuple2 = interner.tuple(vec![
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

    let intersection = interner.intersection(vec![tuple1, tuple2]);

    // tsc does not eagerly reduce tuple intersections with incompatible elements to never
    assert!(!checker.is_subtype_of(intersection, TypeId::NEVER));
}
#[test]
fn test_intersection_union_distribution() {
    // (A | B) & C = (A & C) | (B & C) in terms of assignability
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");
    let c_name = interner.intern_string("c");

    let obj_a = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    let obj_b = interner.object(vec![PropertyInfo::new(b_name, TypeId::NUMBER)]);

    let obj_c = interner.object(vec![PropertyInfo::new(c_name, TypeId::BOOLEAN)]);

    let a_or_b = interner.union(vec![obj_a, obj_b]);
    let union_and_c = interner.intersection(vec![a_or_b, obj_c]);

    let a_and_c = interner.intersection(vec![obj_a, obj_c]);
    let b_and_c = interner.intersection(vec![obj_b, obj_c]);
    let distributed = interner.union(vec![a_and_c, b_and_c]);

    // Both should be mutually subtype (equivalent)
    assert!(checker.is_subtype_of(union_and_c, distributed));
    assert!(checker.is_subtype_of(distributed, union_and_c));
}
#[test]
fn test_intersection_null_with_object_is_never() {
    // null & { x: string } = never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo::new(x_name, TypeId::STRING)]);

    let null_and_obj = interner.intersection(vec![TypeId::NULL, obj]);

    assert!(checker.is_subtype_of(null_and_obj, TypeId::NEVER));
}
#[test]
fn test_intersection_undefined_with_object_is_never() {
    // undefined & { x: string } = never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo::new(x_name, TypeId::STRING)]);

    let undefined_and_obj = interner.intersection(vec![TypeId::UNDEFINED, obj]);

    assert!(checker.is_subtype_of(undefined_and_obj, TypeId::NEVER));
}
#[test]
fn test_intersection_method_signatures() {
    // { foo(): void } & { bar(): void } = { foo(): void, bar(): void }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let foo_name = interner.intern_string("foo");
    let bar_name = interner.intern_string("bar");

    let fn_void = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let obj_foo = interner.object(vec![PropertyInfo::method(foo_name, fn_void)]);

    let obj_bar = interner.object(vec![PropertyInfo::method(bar_name, fn_void)]);

    let intersection = interner.intersection(vec![obj_foo, obj_bar]);

    // Should be subtype of both
    assert!(checker.is_subtype_of(intersection, obj_foo));
    assert!(checker.is_subtype_of(intersection, obj_bar));
}
#[test]
fn test_intersection_same_method_different_returns() {
    // { foo(): string } & { foo(): number } - conflicting method returns
    let interner = TypeInterner::new();

    let foo_name = interner.intern_string("foo");

    let fn_string = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_number = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let obj_foo_string = interner.object(vec![PropertyInfo::method(foo_name, fn_string)]);

    let obj_foo_number = interner.object(vec![PropertyInfo::method(foo_name, fn_number)]);

    let intersection = interner.intersection(vec![obj_foo_string, obj_foo_number]);

    // Should produce valid intersection (methods become overloaded or intersection)
    assert!(intersection != TypeId::ERROR);
}
#[test]
fn test_intersection_three_objects() {
    // { a: string } & { b: number } & { c: boolean }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");
    let c_name = interner.intern_string("c");

    let obj_a = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    let obj_b = interner.object(vec![PropertyInfo::new(b_name, TypeId::NUMBER)]);

    let obj_c = interner.object(vec![PropertyInfo::new(c_name, TypeId::BOOLEAN)]);

    let intersection = interner.intersection(vec![obj_a, obj_b, obj_c]);

    // Should be subtype of all three
    assert!(checker.is_subtype_of(intersection, obj_a));
    assert!(checker.is_subtype_of(intersection, obj_b));
    assert!(checker.is_subtype_of(intersection, obj_c));
}
#[test]
fn test_intersection_symbol_with_primitive_is_never() {
    // symbol & string = never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let symbol_and_string = interner.intersection(vec![TypeId::SYMBOL, TypeId::STRING]);

    assert!(checker.is_subtype_of(symbol_and_string, TypeId::NEVER));
}
#[test]
fn test_intersection_object_intrinsic_with_object() {
    // object & { x: string } - object intrinsic with concrete object
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo::new(x_name, TypeId::STRING)]);

    let object_and_obj = interner.intersection(vec![TypeId::OBJECT, obj]);

    // { x: string } is an object, so intersection should be equivalent to { x: string }
    assert!(checker.is_subtype_of(object_and_obj, obj));
}
#[test]
fn test_intersection_never_identity() {
    // never & T = never (never absorbs everything)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo::new(x_name, TypeId::STRING)]);

    let never_and_obj = interner.intersection(vec![TypeId::NEVER, obj]);
    let obj_and_never = interner.intersection(vec![obj, TypeId::NEVER]);

    assert!(checker.is_subtype_of(never_and_obj, TypeId::NEVER));
    assert!(checker.is_subtype_of(obj_and_never, TypeId::NEVER));
}

// =============================================================================
// KeyOf Type Operator Tests
// =============================================================================
// Tests for keyof type operator and property key relationships
#[test]
fn test_keyof_single_property_is_literal() {
    // keyof { x: number } = "x"
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo::new(x_name, TypeId::NUMBER)]);

    let keyof_obj = interner.intern(TypeData::KeyOf(obj));
    let lit_x = interner.literal_string("x");

    // keyof { x } should be subtype of "x" (they're equivalent)
    assert!(checker.is_subtype_of(keyof_obj, lit_x));
}
#[test]
fn test_keyof_multiple_properties_is_union() {
    // keyof { a, b, c } = "a" | "b" | "c"
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("b"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("c"), TypeId::BOOLEAN),
    ]);

    let keyof_obj = interner.intern(TypeData::KeyOf(obj));
    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let lit_c = interner.literal_string("c");
    let expected = interner.union(vec![lit_a, lit_b, lit_c]);

    // Each literal key should be subtype of keyof
    assert!(checker.is_subtype_of(lit_a, keyof_obj));
    assert!(checker.is_subtype_of(lit_b, keyof_obj));
    assert!(checker.is_subtype_of(lit_c, keyof_obj));

    // keyof should be subtype of the union of keys
    assert!(checker.is_subtype_of(keyof_obj, expected));
}
#[test]
fn test_keyof_empty_object_is_never() {
    // keyof {} = never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let empty_obj = interner.object(vec![]);
    let keyof_empty = interner.intern(TypeData::KeyOf(empty_obj));

    // keyof {} should be subtype of never (they're equivalent)
    assert!(checker.is_subtype_of(keyof_empty, TypeId::NEVER));
}
#[test]
fn test_keyof_with_optional_property() {
    // keyof { x?: number } = "x"
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo::opt(x_name, TypeId::NUMBER)]);

    let keyof_obj = interner.intern(TypeData::KeyOf(obj));
    let lit_x = interner.literal_string("x");

    // Optional property still contributes to keyof
    assert!(checker.is_subtype_of(lit_x, keyof_obj));
}
#[test]
fn test_keyof_with_readonly_property() {
    // keyof { readonly x: number } = "x"
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo::readonly(x_name, TypeId::NUMBER)]);

    let keyof_obj = interner.intern(TypeData::KeyOf(obj));
    let lit_x = interner.literal_string("x");

    // Readonly property still contributes to keyof
    assert!(checker.is_subtype_of(lit_x, keyof_obj));
}
#[test]
fn test_keyof_with_method() {
    // keyof { foo(): void } = "foo"
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let foo_name = interner.intern_string("foo");
    let fn_void = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let obj = interner.object(vec![PropertyInfo::method(foo_name, fn_void)]);

    let keyof_obj = interner.intern(TypeData::KeyOf(obj));
    let lit_foo = interner.literal_string("foo");

    assert!(checker.is_subtype_of(lit_foo, keyof_obj));
}
#[test]
fn test_keyof_subtype_of_string() {
    // keyof { x: number } <: string
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo::new(x_name, TypeId::NUMBER)]);

    let keyof_obj = interner.intern(TypeData::KeyOf(obj));

    // keyof object with string keys is subtype of string
    assert!(checker.is_subtype_of(keyof_obj, TypeId::STRING));
}
#[test]
fn test_keyof_not_equal_to_string() {
    // string is NOT a subtype of keyof { x: number }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo::new(x_name, TypeId::NUMBER)]);

    let keyof_obj = interner.intern(TypeData::KeyOf(obj));

    // string is wider than keyof { x }
    assert!(!checker.is_subtype_of(TypeId::STRING, keyof_obj));
}
#[test]
fn test_keyof_wider_object_has_more_keys() {
    // keyof { a, b } has more keys than keyof { a }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::NUMBER,
    )]);

    let obj_ab = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("b"), TypeId::STRING),
    ]);

    let keyof_a = interner.intern(TypeData::KeyOf(obj_a));
    let keyof_ab = interner.intern(TypeData::KeyOf(obj_ab));

    // keyof { a } <: keyof { a, b } (fewer keys is narrower)
    assert!(checker.is_subtype_of(keyof_a, keyof_ab));
    // keyof { a, b } is NOT subtype of keyof { a }
    assert!(!checker.is_subtype_of(keyof_ab, keyof_a));
}
#[test]
fn test_keyof_union_is_intersection_of_keys() {
    // keyof (A | B) = (keyof A) & (keyof B) - only common keys
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj_ab = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("b"), TypeId::STRING),
    ]);

    let obj_bc = interner.object(vec![
        PropertyInfo::new(interner.intern_string("b"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("c"), TypeId::BOOLEAN),
    ]);

    let union = interner.union(vec![obj_ab, obj_bc]);
    let keyof_union = interner.intern(TypeData::KeyOf(union));
    let lit_b = interner.literal_string("b");

    // Only "b" is common to both - should be subtype of keyof union
    assert!(checker.is_subtype_of(lit_b, keyof_union));
}
#[test]
fn test_keyof_intersection_is_union_of_keys() {
    // keyof (A & B) = (keyof A) | (keyof B) - all keys from both
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::NUMBER,
    )]);

    let obj_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::STRING,
    )]);

    let intersection = interner.intersection(vec![obj_a, obj_b]);
    let keyof_intersection = interner.intern(TypeData::KeyOf(intersection));

    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");

    // Both "a" and "b" should be subtypes of keyof intersection
    assert!(checker.is_subtype_of(lit_a, keyof_intersection));
    assert!(checker.is_subtype_of(lit_b, keyof_intersection));
}
#[test]
fn test_keyof_any_is_string_number_symbol() {
    // keyof any = string | number | symbol
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let keyof_any = interner.intern(TypeData::KeyOf(TypeId::ANY));
    let property_key = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::SYMBOL]);

    // keyof any should be equivalent to PropertyKey
    assert!(checker.is_subtype_of(keyof_any, property_key));
}
#[test]
fn test_keyof_unknown_is_never() {
    // keyof unknown = never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let keyof_unknown = interner.intern(TypeData::KeyOf(TypeId::UNKNOWN));

    assert!(checker.is_subtype_of(keyof_unknown, TypeId::NEVER));
}
#[test]
fn test_keyof_never_is_string_number_symbol() {
    // keyof never = string | number | symbol (vacuously true)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let keyof_never = interner.intern(TypeData::KeyOf(TypeId::NEVER));
    let property_key = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::SYMBOL]);

    assert!(checker.is_subtype_of(keyof_never, property_key));
}
#[test]
fn test_keyof_string_has_string_methods() {
    // keyof string includes string method names
    let interner = TypeInterner::new();

    let keyof_string = interner.intern(TypeData::KeyOf(TypeId::STRING));

    // Should be valid type
    assert!(keyof_string != TypeId::ERROR);
}
#[test]
fn test_keyof_number_has_number_methods() {
    // keyof number includes number method names
    let interner = TypeInterner::new();

    let keyof_number = interner.intern(TypeData::KeyOf(TypeId::NUMBER));

    // Should be valid type
    assert!(keyof_number != TypeId::ERROR);
}
#[test]
fn test_keyof_array_type() {
    // keyof string[] includes array methods and number
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let keyof_array = interner.intern(TypeData::KeyOf(string_array));

    // number should be subtype of keyof array (for index access)
    assert!(checker.is_subtype_of(TypeId::NUMBER, keyof_array));
}
#[test]
fn test_keyof_tuple_type() {
    // keyof [string, number] includes "0" | "1" | array methods
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

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

    let keyof_tuple = interner.intern(TypeData::KeyOf(tuple));
    let lit_0 = interner.literal_string("0");
    let lit_1 = interner.literal_string("1");

    // "0" and "1" should be subtypes of keyof tuple
    assert!(checker.is_subtype_of(lit_0, keyof_tuple));
    assert!(checker.is_subtype_of(lit_1, keyof_tuple));
}
#[test]
fn test_keyof_with_index_signature_includes_string() {
    // keyof { [key: string]: number } includes string
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let indexed_obj = interner.object_with_index(ObjectShape {
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

    let keyof_indexed = interner.intern(TypeData::KeyOf(indexed_obj));

    // string should be subtype of keyof { [key: string]: number }
    assert!(checker.is_subtype_of(TypeId::STRING, keyof_indexed));
}
#[test]
fn test_keyof_with_number_index_signature() {
    // keyof { [key: number]: string } includes number
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let indexed_obj = interner.object_with_index(ObjectShape {
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

    let keyof_indexed = interner.intern(TypeData::KeyOf(indexed_obj));

    // number should be subtype of keyof { [key: number]: string }
    assert!(checker.is_subtype_of(TypeId::NUMBER, keyof_indexed));
}
#[test]
fn test_keyof_nested_object() {
    // keyof { x: { y: number } } = "x" (not "x" | "y")
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let y_name = interner.intern_string("y");
    let inner_obj = interner.object(vec![PropertyInfo::new(y_name, TypeId::NUMBER)]);

    let x_name = interner.intern_string("x");
    let outer_obj = interner.object(vec![PropertyInfo::new(x_name, inner_obj)]);

    let keyof_outer = interner.intern(TypeData::KeyOf(outer_obj));
    let lit_x = interner.literal_string("x");
    let lit_y = interner.literal_string("y");

    // "x" is a key of outer
    assert!(checker.is_subtype_of(lit_x, keyof_outer));
    // "y" is NOT a key of outer (it's a key of the nested object)
    assert!(!checker.is_subtype_of(lit_y, keyof_outer));
}
#[test]
fn test_keyof_generic_constraint() {
    // <K extends keyof T> constraint pattern
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("name"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("age"), TypeId::NUMBER),
    ]);

    let keyof_obj = interner.intern(TypeData::KeyOf(obj));
    let lit_name = interner.literal_string("name");
    let lit_age = interner.literal_string("age");
    let lit_invalid = interner.literal_string("invalid");

    // Valid keys satisfy the constraint
    assert!(checker.is_subtype_of(lit_name, keyof_obj));
    assert!(checker.is_subtype_of(lit_age, keyof_obj));
    // Invalid key doesn't satisfy
    assert!(!checker.is_subtype_of(lit_invalid, keyof_obj));
}
#[test]
fn test_keyof_mapped_type_source() {
    // keyof used as constraint in mapped type: { [K in keyof T]: ... }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("y"), TypeId::NUMBER),
    ]);

    let keyof_obj = interner.intern(TypeData::KeyOf(obj));

    // keyof should produce valid keys for iteration
    assert!(keyof_obj != TypeId::ERROR);
    assert!(keyof_obj != TypeId::NEVER);

    // Should be subtype of string (for string-keyed objects)
    assert!(checker.is_subtype_of(keyof_obj, TypeId::STRING));
}
#[test]
fn test_keyof_reflexive() {
    // keyof T <: keyof T
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    let keyof_obj = interner.intern(TypeData::KeyOf(obj));

    assert!(checker.is_subtype_of(keyof_obj, keyof_obj));
}
#[test]
fn test_keyof_null_is_never() {
    // keyof null = never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let keyof_null = interner.intern(TypeData::KeyOf(TypeId::NULL));

    assert!(checker.is_subtype_of(keyof_null, TypeId::NEVER));
}
#[test]
fn test_keyof_undefined_is_never() {
    // keyof undefined = never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let keyof_undefined = interner.intern(TypeData::KeyOf(TypeId::UNDEFINED));

    assert!(checker.is_subtype_of(keyof_undefined, TypeId::NEVER));
}
#[test]
fn test_keyof_void_is_never() {
    // keyof void = never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let keyof_void = interner.intern(TypeData::KeyOf(TypeId::VOID));

    assert!(checker.is_subtype_of(keyof_void, TypeId::NEVER));
}
#[test]
fn test_keyof_object_intrinsic() {
    // keyof object includes all possible property keys
    let interner = TypeInterner::new();

    let keyof_object = interner.intern(TypeData::KeyOf(TypeId::OBJECT));

    // Should be valid
    assert!(keyof_object != TypeId::ERROR);
}
#[test]
fn test_keyof_symbol_keyed_object() {
    // Objects with symbol keys in keyof result
    let interner = TypeInterner::new();
    let _checker = SubtypeChecker::new(&interner);

    // Simulated: { [Symbol.iterator]: () => Iterator }
    let sym_iterator = interner.intern_string("Symbol.iterator");
    let fn_iterator = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::OBJECT,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let obj = interner.object(vec![PropertyInfo::method(sym_iterator, fn_iterator)]);

    let keyof_obj = interner.intern(TypeData::KeyOf(obj));

    // Should include the symbol key
    assert!(keyof_obj != TypeId::NEVER);
}

// =============================================================================
// Constructor Type Tests
// =============================================================================
// Tests for new signatures, abstract constructors, and constructor types
#[test]
fn test_constructor_basic_new_signature() {
    // new () => T
    let interner = TypeInterner::new();
    let _checker = SubtypeChecker::new(&interner);

    let instance = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    let constructor = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: instance,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    // Constructor type should be valid
    assert!(constructor != TypeId::ERROR);
    assert!(constructor != TypeId::NEVER);
}
#[test]
fn test_constructor_with_parameters() {
    // new (x: string, y: number) => T
    let interner = TypeInterner::new();

    let instance = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("y"), TypeId::NUMBER),
    ]);

    let constructor = interner.function(FunctionShape {
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
        return_type: instance,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    assert!(constructor != TypeId::ERROR);
}
#[test]
fn test_constructor_vs_regular_function() {
    // Constructor and regular function are different types
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let instance = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    let constructor = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: instance,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    let regular_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: instance,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Constructor and function with same signature are not assignable
    assert!(!checker.is_subtype_of(constructor, regular_fn));
    assert!(!checker.is_subtype_of(regular_fn, constructor));
}
#[test]
fn test_constructor_callable_with_construct_signature() {
    // interface C { new (): T }
    let interner = TypeInterner::new();
    let _checker = SubtypeChecker::new(&interner);

    let instance = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        TypeId::STRING,
    )]);

    let callable_with_new = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![],
        construct_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: instance,
            type_predicate: None,
            is_method: false,
        }],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    assert!(callable_with_new != TypeId::ERROR);
}
#[test]
fn test_constructor_with_call_and_construct() {
    // interface F { (): string; new (): T }
    let interner = TypeInterner::new();

    let instance = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    let callable_both = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: TypeId::STRING,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: instance,
            type_predicate: None,
            is_method: false,
        }],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    assert!(callable_both != TypeId::ERROR);
}
#[test]
fn test_constructor_subtype_by_return_type() {
    // new () => Derived <: new () => Base
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let base = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    let derived = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("y"), TypeId::STRING),
    ]);

    let ctor_base = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: base,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    let ctor_derived = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: derived,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    // Constructor returning derived is subtype of constructor returning base
    assert!(checker.is_subtype_of(ctor_derived, ctor_base));
    // Reverse is not true
    assert!(!checker.is_subtype_of(ctor_base, ctor_derived));
}
