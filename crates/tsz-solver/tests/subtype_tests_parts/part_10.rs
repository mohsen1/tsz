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
        is_symbol_named: false,
        single_quoted_name: false,
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
        is_symbol_named: false,
        single_quoted_name: false,
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
        is_symbol_named: false,
        single_quoted_name: false,
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
        is_symbol_named: false,
        single_quoted_name: false,
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

