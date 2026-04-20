#[test]
fn test_function_source_bivariant_against_method_property() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    let name = interner.intern_string("m");

    let wide_param = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let narrow_param = TypeId::STRING;

    let narrow_func = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: narrow_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let wide_method = interner.function(FunctionShape {
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

    let source = interner.object(vec![PropertyInfo {
        name,
        type_id: narrow_func,
        write_type: narrow_func,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    let target = interner.object(vec![PropertyInfo {
        name,
        type_id: wide_method,
        write_type: wide_method,
        optional: false,
        readonly: false,
        is_method: true,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    assert!(checker.is_subtype_of(source, target));
}
#[test]
fn test_variance_optional_rest_method_optional_bivariant() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    let method_name = interner.intern_string("m");

    let wide_param = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let narrow_param = TypeId::STRING;

    let wide_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: wide_param,
            optional: true,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let narrow_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: narrow_param,
            optional: true,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let wide_obj = interner.object(vec![PropertyInfo::method(method_name, wide_method)]);
    let narrow_obj = interner.object(vec![PropertyInfo::method(method_name, narrow_method)]);

    assert!(checker.is_subtype_of(wide_obj, narrow_obj));
    assert!(checker.is_subtype_of(narrow_obj, wide_obj));
}
#[test]
fn test_variance_optional_rest_method_rest_bivariant() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    let method_name = interner.intern_string("m");

    let wide_elem = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let narrow_elem = TypeId::STRING;
    let wide_rest = interner.array(wide_elem);
    let narrow_rest = interner.array(narrow_elem);

    let wide_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: wide_rest,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let narrow_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: narrow_rest,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let wide_obj = interner.object(vec![PropertyInfo::method(method_name, wide_method)]);
    let narrow_obj = interner.object(vec![PropertyInfo::method(method_name, narrow_method)]);

    assert!(checker.is_subtype_of(wide_obj, narrow_obj));
    assert!(checker.is_subtype_of(narrow_obj, wide_obj));
}
#[test]
fn test_variance_optional_rest_method_optional_with_this_bivariant() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    let method_name = interner.intern_string("m");

    let wide_this = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let wide_param = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let narrow_param = TypeId::STRING;

    let wide_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: wide_param,
            optional: true,
            rest: false,
        }],
        this_type: Some(wide_this),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let narrow_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: narrow_param,
            optional: true,
            rest: false,
        }],
        this_type: Some(TypeId::STRING),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let wide_obj = interner.object(vec![PropertyInfo::method(method_name, wide_method)]);
    let narrow_obj = interner.object(vec![PropertyInfo::method(method_name, narrow_method)]);

    assert!(checker.is_subtype_of(wide_obj, narrow_obj));
    assert!(checker.is_subtype_of(narrow_obj, wide_obj));
}
#[test]
fn test_variance_optional_rest_method_rest_with_this_bivariant() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    let method_name = interner.intern_string("m");

    let wide_this = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let wide_elem = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let narrow_elem = TypeId::STRING;
    let wide_rest = interner.array(wide_elem);
    let narrow_rest = interner.array(narrow_elem);

    let wide_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: wide_rest,
            optional: false,
            rest: true,
        }],
        this_type: Some(wide_this),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let narrow_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: narrow_rest,
            optional: false,
            rest: true,
        }],
        this_type: Some(TypeId::STRING),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let wide_obj = interner.object(vec![PropertyInfo::method(method_name, wide_method)]);
    let narrow_obj = interner.object(vec![PropertyInfo::method(method_name, narrow_method)]);

    assert!(checker.is_subtype_of(wide_obj, narrow_obj));
    assert!(checker.is_subtype_of(narrow_obj, wide_obj));
}
#[test]
fn test_variance_optional_rest_function_optional_with_this_contravariant() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    let func_name = interner.intern_string("f");

    let wide_this = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let wide_param = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let narrow_param = TypeId::STRING;

    let wide_func = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: wide_param,
            optional: true,
            rest: false,
        }],
        this_type: Some(wide_this),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let narrow_func = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: narrow_param,
            optional: true,
            rest: false,
        }],
        this_type: Some(TypeId::STRING),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let wide_obj = interner.object(vec![PropertyInfo::new(func_name, wide_func)]);
    let narrow_obj = interner.object(vec![PropertyInfo::new(func_name, narrow_func)]);

    assert!(checker.is_subtype_of(wide_obj, narrow_obj));
    assert!(!checker.is_subtype_of(narrow_obj, wide_obj));
}
#[test]
fn test_variance_optional_rest_function_rest_with_this_contravariant() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    let func_name = interner.intern_string("f");

    let wide_this = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let wide_elem = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let narrow_elem = TypeId::STRING;
    let wide_rest = interner.array(wide_elem);
    let narrow_rest = interner.array(narrow_elem);

    let wide_func = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: wide_rest,
            optional: false,
            rest: true,
        }],
        this_type: Some(wide_this),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let narrow_func = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: narrow_rest,
            optional: false,
            rest: true,
        }],
        this_type: Some(TypeId::STRING),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let wide_obj = interner.object(vec![PropertyInfo::new(func_name, wide_func)]);
    let narrow_obj = interner.object(vec![PropertyInfo::new(func_name, narrow_func)]);

    assert!(checker.is_subtype_of(wide_obj, narrow_obj));
    assert!(!checker.is_subtype_of(narrow_obj, wide_obj));
}
#[test]
fn test_variance_optional_rest_constructor_optional_bivariant() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let wide_param = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let narrow_param = TypeId::STRING;

    let wide_ctor = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: wide_param,
            optional: true,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    let narrow_ctor = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: narrow_param,
            optional: true,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    assert!(checker.is_subtype_of(wide_ctor, narrow_ctor));
    // Constructor signatures are bivariant (like methods), not contravariant
    assert!(checker.is_subtype_of(narrow_ctor, wide_ctor));
}
#[test]
fn test_variance_optional_rest_constructor_rest_bivariant() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let wide_elem = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let narrow_elem = TypeId::STRING;
    let wide_rest = interner.array(wide_elem);
    let narrow_rest = interner.array(narrow_elem);

    let wide_ctor = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: wide_rest,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    let narrow_ctor = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: narrow_rest,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    assert!(checker.is_subtype_of(wide_ctor, narrow_ctor));
    // Constructor signatures are bivariant (like methods), not contravariant
    assert!(checker.is_subtype_of(narrow_ctor, wide_ctor));
}
#[test]
fn test_function_required_count_allows_optional_source_extra() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let source = interner.function(FunctionShape {
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
                optional: true,
                rest: false,
            },
        ],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let target = interner.function(FunctionShape {
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

    assert!(checker.is_subtype_of(source, target));
}
#[test]
fn test_function_required_count_rejects_required_source_extra() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let source = interner.function(FunctionShape {
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
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let target = interner.function(FunctionShape {
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
                optional: true,
                rest: false,
            },
        ],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // In tsc, (x: string, y: number) => void IS assignable to (x: string, y?: number) => void.
    // Optional parameters are compared by declared type (number), not number | undefined.
    assert!(checker.is_subtype_of(source, target));
}
#[test]
fn test_function_variance_param_contravariance() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let wide_param = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let narrow_param = TypeId::STRING;

    let source = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: wide_param,
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
        is_constructor: false,
        is_method: false,
    });

    let target = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: narrow_param,
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
        is_constructor: false,
        is_method: false,
    });

    assert!(checker.is_subtype_of(source, target));
    assert!(!checker.is_subtype_of(target, source));
}
#[test]
fn test_function_variance_return_covariance() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let narrow_return = TypeId::STRING;
    let wide_return = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let source = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::BOOLEAN,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: narrow_return,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let target = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::BOOLEAN,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: wide_return,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    assert!(checker.is_subtype_of(source, target));
    assert!(!checker.is_subtype_of(target, source));
}
#[test]
fn test_function_return_covariance() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let returns_string = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let returns_string_or_number = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: interner.union(vec![TypeId::STRING, TypeId::NUMBER]),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    assert!(checker.is_subtype_of(returns_string, returns_string_or_number));
    assert!(!checker.is_subtype_of(returns_string_or_number, returns_string));
}
#[test]
fn test_void_return_exception_subtype() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let returns_number = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let returns_void = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    assert!(!checker.is_subtype_of(returns_number, returns_void));

    checker.allow_void_return = true;
    assert!(checker.is_subtype_of(returns_number, returns_void));
    assert!(!checker.is_subtype_of(returns_void, returns_number));
}
#[test]
fn test_void_return_exception_method_property() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    let method_name = interner.intern_string("m");

    let returns_number = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let returns_void = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let source = interner.object(vec![PropertyInfo::method(method_name, returns_number)]);
    let target = interner.object(vec![PropertyInfo::method(method_name, returns_void)]);

    assert!(!checker.is_subtype_of(source, target));

    checker.allow_void_return = true;
    assert!(checker.is_subtype_of(source, target));
    assert!(!checker.is_subtype_of(target, source));
}
#[test]
fn test_constructor_void_exception_subtype() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let instance = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    let returns_instance = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: instance,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    let returns_void = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    assert!(!checker.is_subtype_of(returns_instance, returns_void));

    checker.allow_void_return = true;
    assert!(checker.is_subtype_of(returns_instance, returns_void));
    assert!(!checker.is_subtype_of(returns_void, returns_instance));
}
#[test]
fn test_function_top_assignability() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let function_top = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: Vec::new(),
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        ..Default::default()
    });

    let specific_fn = interner.function(FunctionShape {
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

    assert!(checker.is_subtype_of(specific_fn, function_top));
    assert!(!checker.is_subtype_of(function_top, specific_fn));
}
#[test]
fn test_this_parameter_variance() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union_this = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let union_this_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: Some(union_this),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let string_this_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: Some(TypeId::STRING),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // this parameter is contravariant like regular parameters
    assert!(checker.is_subtype_of(union_this_fn, string_this_fn));
    assert!(!checker.is_subtype_of(string_this_fn, union_this_fn));
}
#[test]
fn test_this_parameter_method_property_bivariant() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    let method_name = interner.intern_string("m");

    let wide_this = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let wide_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: Some(wide_this),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let narrow_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: Some(TypeId::STRING),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let wide_obj = interner.object(vec![PropertyInfo::method(method_name, wide_method)]);
    let narrow_obj = interner.object(vec![PropertyInfo::method(method_name, narrow_method)]);

    assert!(checker.is_subtype_of(wide_obj, narrow_obj));
    assert!(checker.is_subtype_of(narrow_obj, wide_obj));
}
#[test]
fn test_this_parameter_function_property_contravariant() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    let func_name = interner.intern_string("f");

    let wide_this = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let wide_func = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: Some(wide_this),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let narrow_func = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: Some(TypeId::STRING),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let wide_obj = interner.object(vec![PropertyInfo::new(func_name, wide_func)]);
    let narrow_obj = interner.object(vec![PropertyInfo::new(func_name, narrow_func)]);

    assert!(checker.is_subtype_of(wide_obj, narrow_obj));
    assert!(!checker.is_subtype_of(narrow_obj, wide_obj));
}
#[test]
fn test_this_parameter_method_source_bivariant_against_function_property() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    let name = interner.intern_string("m");

    let wide_this = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let narrow_this = TypeId::STRING;

    let narrow_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: Some(narrow_this),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let wide_func = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: Some(wide_this),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let source = interner.object(vec![PropertyInfo {
        name,
        type_id: narrow_method,
        write_type: narrow_method,
        optional: false,
        readonly: false,
        is_method: true,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    let target = interner.object(vec![PropertyInfo {
        name,
        type_id: wide_func,
        write_type: wide_func,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    assert!(checker.is_subtype_of(source, target));
}
#[test]
fn test_this_parameter_function_source_bivariant_against_method_property() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    let name = interner.intern_string("m");

    let wide_this = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let narrow_this = TypeId::STRING;

    let narrow_func = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: Some(narrow_this),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let wide_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: Some(wide_this),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let source = interner.object(vec![PropertyInfo {
        name,
        type_id: narrow_func,
        write_type: narrow_func,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    let target = interner.object(vec![PropertyInfo {
        name,
        type_id: wide_method,
        write_type: wide_method,
        optional: false,
        readonly: false,
        is_method: true,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    assert!(checker.is_subtype_of(source, target));
}
#[test]
fn test_this_type_in_param_contravariant() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    let func_name = interner.intern_string("compare");

    let this_type = interner.intern(TypeData::ThisType);
    let this_or_number = interner.union(vec![this_type, TypeId::NUMBER]);

    let narrow_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("other")),
            type_id: this_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let wide_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("other")),
            type_id: this_or_number,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let narrow_obj = interner.object(vec![PropertyInfo::new(func_name, narrow_fn)]);
    let wide_obj = interner.object(vec![PropertyInfo::new(func_name, wide_fn)]);

    // Under strict function types (non-method), parameter checking is contravariant.
    // narrow_fn param `this` vs wide_fn param `this | number`:
    //   narrow ≤ wide: contravariant check `this | number ≤ this`? NO → FALSE
    //   wide ≤ narrow: contravariant check `this ≤ this | number`? YES → TRUE
    assert!(!checker.is_subtype_of(narrow_obj, wide_obj));
    assert!(checker.is_subtype_of(wide_obj, narrow_obj));
}
#[test]
fn test_class_like_subtyping_this_param_contravariant() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let compare = interner.intern_string("compare");
    let id = interner.intern_string("id");
    let extra = interner.intern_string("extra");

    let this_type = interner.intern(TypeData::ThisType);
    let this_or_number = interner.union(vec![this_type, TypeId::NUMBER]);

    let base_compare = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("other")),
            type_id: this_or_number,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let derived_compare = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("other")),
            type_id: this_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let base = interner.object(vec![
        PropertyInfo::new(id, TypeId::STRING),
        PropertyInfo::new(compare, base_compare),
    ]);

    let derived = interner.object(vec![
        PropertyInfo::new(id, TypeId::STRING),
        PropertyInfo::new(extra, TypeId::NUMBER),
        PropertyInfo::new(compare, derived_compare),
    ]);

    // Under strict function types (non-method), parameter checking is contravariant.
    // derived.compare param `this` vs base.compare param `this | number`:
    //   derived ≤ base: contravariant check `this | number ≤ this`? NO → FALSE
    //   (derived has extra props, but the compare method fails contravariance)
    assert!(!checker.is_subtype_of(derived, base));
    assert!(!checker.is_subtype_of(base, derived));
}

/// Regression test: function parameter contravariance must not be broken by
/// `this` type presence. When a class has a method returning `this`, function
/// parameters typed with that class should still use correct contravariant
/// checking under strict function types.
///
/// Reproduces: `(x: A | B) => void` should be assignable to `(x: B) => void`
/// because B (target param) ≤ A | B (source param) via contravariance.
#[test]
fn test_this_type_does_not_break_union_function_contravariance() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let this_type = interner.intern(TypeData::ThisType);

    // Class A: { a: number, m(): this }
    let a_m = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: this_type,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });
    let a_type = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("m"), a_m),
    ]);

    // Class B: { b: number, m(): this }
    let b_m = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: this_type,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });
    let b_type = interner.object(vec![
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("m"), b_m),
    ]);

    let a_or_b = interner.union(vec![a_type, b_type]);

    // (x: A | B) => void
    let fn_wide = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: a_or_b,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // (x: B) => void
    let fn_narrow = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: b_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // (x: A | B) => void should be assignable to (x: B) => void
    // because B ≤ A | B (B is a member of the union), and
    // function parameters are contravariant under strict function types.
    assert!(
        checker.is_subtype_of(fn_wide, fn_narrow),
        "(x: A | B) => void should be assignable to (x: B) => void via contravariance"
    );

    // (x: B) => void should NOT be assignable to (x: A | B) => void
    // because A | B is NOT a subtype of B.
    assert!(
        !checker.is_subtype_of(fn_narrow, fn_wide),
        "(x: B) => void should NOT be assignable to (x: A | B) => void"
    );
}
#[test]
fn test_function_fixed_to_rest_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // Source: (name: string, mixed: any, arg: any) => any
    let source = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("name")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("mixed")),
                type_id: TypeId::ANY,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("arg")),
                type_id: TypeId::ANY,
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

    // Target: (name: string, mixed: any, ...args: any[]) => any
    let any_array = interner.array(TypeId::ANY);
    let target = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("name")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("mixed")),
                type_id: TypeId::ANY,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("args")),
                type_id: any_array,
                optional: false,
                rest: true,
            },
        ],
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Function with fixed params should be subtype of function with rest params
    // This matches TypeScript behavior
    assert!(
        checker.is_subtype_of(source, target),
        "Function with 3 fixed params should be subtype of function with 2 fixed + rest params"
    );
}
#[test]
fn test_function_fixed_to_rest_extra_param_accepts_undefined() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let num_or_undef = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);

    let source = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("name")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("value")),
                type_id: num_or_undef,
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

    let number_array = interner.array(TypeId::NUMBER);
    let target = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("name")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("args")),
                type_id: number_array,
                optional: false,
                rest: true,
            },
        ],
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    assert!(checker.is_subtype_of(source, target));
}
#[test]
fn test_rest_any_three_fixed_to_two_fixed_plus_rest() {
    // This matches the failing conformance test: aliasOfGenericFunctionWithRestBehavedSameAsUnaliased.ts
    // type a3 = (name: string, mixed: any, args_0: any) => any
    // type b3 = (name: string, mixed: any, ...args: any[]) => any
    // type test3 = a3 extends b3 ? "y" : "n"  // tsc: "y", tsz should be: "y"
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.allow_bivariant_rest = true;

    let rest_any = interner.array(TypeId::ANY);

    // Source: (name: string, mixed: any, args_0: any) => any
    let source = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("name")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("mixed")),
                type_id: TypeId::ANY,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("args_0")),
                type_id: TypeId::ANY,
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

    // Target: (name: string, mixed: any, ...args: any[]) => any
    let target = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("name")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("mixed")),
                type_id: TypeId::ANY,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("args")),
                type_id: rest_any,
                optional: false,
                rest: true,
            },
        ],
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    assert!(checker.is_subtype_of(source, target));
}
#[test]
fn test_function_fixed_to_rest_extra_param_compatible() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let source = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("name")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("value")),
                type_id: TypeId::NUMBER,
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

    let number_array = interner.array(TypeId::NUMBER);
    let target = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("name")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("args")),
                type_id: number_array,
                optional: false,
                rest: true,
            },
        ],
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Fixed params with matching types ARE subtype of rest with same element type.
    // TypeScript allows (name: string, value: number) → (name: string, ...args: number[]).
    assert!(checker.is_subtype_of(source, target));
}
#[test]
fn test_function_rest_tuple_to_rest_array_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // Source: (name: string, mixed: any, ...args: [any]) => any
    let tuple_one_any = interner.tuple(vec![TupleElement {
        type_id: TypeId::ANY,
        name: None,
        optional: false,
        rest: false,
    }]);
    let source = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("name")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("mixed")),
                type_id: TypeId::ANY,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("args")),
                type_id: tuple_one_any,
                optional: false,
                rest: true,
            },
        ],
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Target: (name: string, mixed: any, ...args: any[]) => any
    let any_array = interner.array(TypeId::ANY);
    let target = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("name")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("mixed")),
                type_id: TypeId::ANY,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("args")),
                type_id: any_array,
                optional: false,
                rest: true,
            },
        ],
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Function with rest tuple should be subtype of function with rest array
    // (name, mixed, ...args: [any]) should be assignable to (name, mixed, ...args: any[])
    assert!(
        checker.is_subtype_of(source, target),
        "Function with rest tuple [any] should be subtype of function with rest array any[]"
    );
}
#[test]
fn test_keyof_intersection_contravariant() {
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
    let keyof_a = interner.intern(TypeData::KeyOf(obj_a));
    let keyof_intersection = interner.intern(TypeData::KeyOf(intersection));

    assert!(checker.is_subtype_of(keyof_a, keyof_intersection));
    assert!(!checker.is_subtype_of(keyof_intersection, keyof_a));
}
#[test]
fn test_keyof_contravariant_object_subtyping() {
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

    assert!(checker.is_subtype_of(obj_ab, obj_a));

    let keyof_a = interner.intern(TypeData::KeyOf(obj_a));
    let keyof_ab = interner.intern(TypeData::KeyOf(obj_ab));

    assert!(checker.is_subtype_of(keyof_a, keyof_ab));
    assert!(!checker.is_subtype_of(keyof_ab, keyof_a));
}
#[test]
fn test_keyof_intersection_union_of_keys() {
    use crate::evaluate_keyof;

    let interner = TypeInterner::new();

    let obj_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::NUMBER,
    )]);
    let obj_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::STRING,
    )]);

    let intersection = interner.intersection(vec![obj_a, obj_b]);
    let result = evaluate_keyof(&interner, intersection);
    let expected = interner.union(vec![
        interner.literal_string("a"),
        interner.literal_string("b"),
    ]);

    assert_eq!(result, expected);
}
#[test]
fn test_keyof_union_disjoint_object_keys_is_never() {
    use crate::evaluate_keyof;

    let interner = TypeInterner::new();

    let obj_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::NUMBER,
    )]);
    let obj_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::STRING,
    )]);

    let union = interner.union(vec![obj_a, obj_b]);
    let result = evaluate_keyof(&interner, union);

    assert_eq!(result, TypeId::NEVER);
}
#[test]
fn test_keyof_union_index_signature_contravariant() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_index = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });
    let number_index = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
    });

    let union = interner.union(vec![string_index, number_index]);
    let keyof_union = interner.intern(TypeData::KeyOf(union));

    assert!(checker.is_subtype_of(keyof_union, TypeId::NUMBER));
    assert!(!checker.is_subtype_of(keyof_union, TypeId::STRING));
}
#[test]
fn test_keyof_union_string_index_and_literal_narrows() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_index = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });
    let key_a = interner.intern_string("a");
    let obj_a = interner.object(vec![PropertyInfo::new(key_a, TypeId::NUMBER)]);

    let union = interner.union(vec![string_index, obj_a]);
    let keyof_union = interner.intern(TypeData::KeyOf(union));
    let key_a_literal = interner.literal_string("a");

    assert!(checker.is_subtype_of(keyof_union, key_a_literal));
    assert!(checker.is_subtype_of(keyof_union, TypeId::STRING));
    assert!(!checker.is_subtype_of(keyof_union, TypeId::NUMBER));
    assert!(checker.is_subtype_of(key_a_literal, keyof_union));
    assert!(!checker.is_subtype_of(TypeId::STRING, keyof_union));
}
#[test]
fn test_keyof_union_overlapping_keys_is_common() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let key_a = interner.intern_string("a");
    let key_b = interner.intern_string("b");
    let key_c = interner.intern_string("c");

    let obj_ab = interner.object(vec![
        PropertyInfo::new(key_a, TypeId::NUMBER),
        PropertyInfo::new(key_b, TypeId::STRING),
    ]);
    let obj_ac = interner.object(vec![
        PropertyInfo::new(key_a, TypeId::NUMBER),
        PropertyInfo::new(key_c, TypeId::BOOLEAN),
    ]);

    let union = interner.union(vec![obj_ab, obj_ac]);
    let keyof_union = interner.intern(TypeData::KeyOf(union));
    let key_a_literal = interner.literal_string("a");
    let key_b_literal = interner.literal_string("b");
    let key_c_literal = interner.literal_string("c");

    assert!(checker.is_subtype_of(keyof_union, key_a_literal));
    assert!(!checker.is_subtype_of(keyof_union, key_b_literal));
    assert!(!checker.is_subtype_of(keyof_union, key_c_literal));
    assert!(checker.is_subtype_of(key_a_literal, keyof_union));
}
#[test]
fn test_keyof_union_optional_key_is_common() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let key_a = interner.intern_string("a");
    let key_b = interner.intern_string("b");

    let obj_optional_a = interner.object(vec![PropertyInfo::opt(key_a, TypeId::NUMBER)]);
    let obj_ab = interner.object(vec![
        PropertyInfo::new(key_a, TypeId::NUMBER),
        PropertyInfo::new(key_b, TypeId::STRING),
    ]);

    let union = interner.union(vec![obj_optional_a, obj_ab]);
    let keyof_union = interner.intern(TypeData::KeyOf(union));
    let key_a_literal = interner.literal_string("a");
    let key_b_literal = interner.literal_string("b");

    assert!(checker.is_subtype_of(keyof_union, key_a_literal));
    assert!(!checker.is_subtype_of(keyof_union, key_b_literal));
    assert!(checker.is_subtype_of(key_a_literal, keyof_union));
}
#[test]
fn test_keyof_deferred_not_subtype_of_string() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let type_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }));
    let keyof_param = interner.intern(TypeData::KeyOf(type_param));

    assert!(!checker.is_subtype_of(keyof_param, TypeId::STRING));
}
#[test]
fn test_keyof_deferred_subtype_of_string_number_symbol_union() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let type_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }));
    let keyof_param = interner.intern(TypeData::KeyOf(type_param));

    let key_union = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::SYMBOL]);
    assert!(checker.is_subtype_of(keyof_param, key_union));
}
#[test]
fn test_keyof_deferred_not_subtype_of_string_number_union() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let type_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }));
    let keyof_param = interner.intern(TypeData::KeyOf(type_param));

    let key_union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert!(!checker.is_subtype_of(keyof_param, key_union));
}
#[test]
fn test_keyof_any_subtyping_union() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let keyof_any = interner.intern(TypeData::KeyOf(TypeId::ANY));
    let key_union = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::SYMBOL]);
    let string_number_union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    assert!(checker.is_subtype_of(keyof_any, key_union));
    assert!(!checker.is_subtype_of(keyof_any, string_number_union));
}
#[test]
fn test_intersection_reduction_disjoint_discriminant_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let kind = interner.intern_string("kind");
    let obj_a = interner.object(vec![PropertyInfo::new(kind, interner.literal_string("a"))]);
    let obj_b = interner.object(vec![PropertyInfo::new(kind, interner.literal_string("b"))]);

    let intersection = interner.intersection(vec![obj_a, obj_b]);

    assert!(checker.is_subtype_of(intersection, TypeId::NEVER));
    assert!(checker.is_subtype_of(intersection, TypeId::STRING));
}
#[test]
fn test_intersection_reduction_disjoint_intrinsics() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let intersection = interner.intersection(vec![TypeId::STRING, TypeId::NUMBER]);

    assert!(checker.is_subtype_of(intersection, TypeId::NEVER));
}
#[test]
fn test_mapped_type_over_number_keys_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let constraint = interner.intern(TypeData::KeyOf(TypeId::NUMBER));
    let mapped = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint,
        name_type: None,
        template: TypeId::BOOLEAN,
        readonly_modifier: None,
        optional_modifier: None,
    });

    let to_fixed = interner.intern_string("toFixed");
    let expected = interner.object(vec![PropertyInfo::new(to_fixed, TypeId::BOOLEAN)]);
    let mismatch = interner.object(vec![PropertyInfo::new(to_fixed, TypeId::NUMBER)]);
    let to_upper = interner.intern_string("toUpperCase");
    let wrong_key = interner.object(vec![PropertyInfo::new(to_upper, TypeId::BOOLEAN)]);

    assert!(checker.is_subtype_of(mapped, expected));
    assert!(!checker.is_subtype_of(mapped, mismatch));
    assert!(!checker.is_subtype_of(mapped, wrong_key));
    assert!(!checker.is_subtype_of(expected, mapped));
}
#[test]
fn test_mapped_type_over_number_keys_optional_readonly_add_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let constraint = interner.intern(TypeData::KeyOf(TypeId::NUMBER));
    let mapped = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint,
        name_type: None,
        template: TypeId::BOOLEAN,
        readonly_modifier: Some(MappedModifier::Add),
        optional_modifier: Some(MappedModifier::Add),
    });

    let to_fixed = interner.intern_string("toFixed");
    let optional_readonly = interner.object(vec![PropertyInfo {
        name: to_fixed,
        type_id: TypeId::BOOLEAN,
        write_type: TypeId::BOOLEAN,
        optional: true,
        readonly: true,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);
    let required_readonly =
        interner.object(vec![PropertyInfo::readonly(to_fixed, TypeId::BOOLEAN)]);
    let optional_mutable = interner.object(vec![PropertyInfo::opt(to_fixed, TypeId::BOOLEAN)]);

    assert!(checker.is_subtype_of(mapped, optional_readonly));
    assert!(!checker.is_subtype_of(mapped, required_readonly));
    // TypeScript allows readonly → mutable property assignment
    assert!(checker.is_subtype_of(mapped, optional_mutable));
}
#[test]
fn test_mapped_type_over_string_keys_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let constraint = interner.intern(TypeData::KeyOf(TypeId::STRING));
    let mapped = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint,
        name_type: None,
        template: TypeId::BOOLEAN,
        readonly_modifier: None,
        optional_modifier: None,
    });

    let to_upper = interner.intern_string("toUpperCase");
    let expected = interner.object(vec![PropertyInfo::new(to_upper, TypeId::BOOLEAN)]);
    let mismatch = interner.object(vec![PropertyInfo::new(to_upper, TypeId::STRING)]);

    assert!(checker.is_subtype_of(mapped, expected));
    assert!(!checker.is_subtype_of(mapped, mismatch));
    assert!(!checker.is_subtype_of(expected, mapped));
}
#[test]
fn test_mapped_type_over_string_keys_number_index_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let constraint = interner.intern(TypeData::KeyOf(TypeId::STRING));
    let mapped = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint,
        name_type: None,
        template: TypeId::BOOLEAN,
        readonly_modifier: None,
        optional_modifier: None,
    });

    let number_index = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::BOOLEAN,
            readonly: false,
            param_name: None,
        }),
    });
    let mismatch = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
    });

    assert!(checker.is_subtype_of(mapped, number_index));
    assert!(!checker.is_subtype_of(mapped, mismatch));
}
#[test]
fn test_mapped_type_over_string_keys_key_remap_omit_length() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let constraint = interner.intern(TypeData::KeyOf(TypeId::STRING));
    let key_param = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let key_param_id = interner.intern(TypeData::TypeParameter(key_param));
    let length_key = interner.literal_string("length");
    let name_type = interner.conditional(ConditionalType {
        check_type: key_param_id,
        extends_type: length_key,
        true_type: TypeId::NEVER,
        false_type: key_param_id,
        is_distributive: true,
    });
    let mapped = interner.mapped(MappedType {
        type_param: key_param,
        constraint,
        name_type: Some(name_type),
        template: TypeId::BOOLEAN,
        readonly_modifier: None,
        optional_modifier: None,
    });

    let to_upper = interner.intern_string("toUpperCase");
    let expected = interner.object(vec![PropertyInfo::new(to_upper, TypeId::BOOLEAN)]);
    let length = interner.intern_string("length");
    let requires_length = interner.object(vec![PropertyInfo::new(length, TypeId::BOOLEAN)]);

    assert!(checker.is_subtype_of(mapped, expected));
    assert!(!checker.is_subtype_of(mapped, requires_length));
}
#[test]
fn test_mapped_type_over_boolean_keys_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let constraint = interner.intern(TypeData::KeyOf(TypeId::BOOLEAN));
    let mapped = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint,
        name_type: None,
        template: TypeId::NUMBER,
        readonly_modifier: None,
        optional_modifier: None,
    });

    let value_of = interner.intern_string("valueOf");
    let expected = interner.object(vec![PropertyInfo::new(value_of, TypeId::NUMBER)]);
    let mismatch = interner.object(vec![PropertyInfo::new(value_of, TypeId::STRING)]);

    assert!(checker.is_subtype_of(mapped, expected));
    assert!(!checker.is_subtype_of(mapped, mismatch));
    assert!(!checker.is_subtype_of(expected, mapped));
}
#[test]
fn test_mapped_type_over_symbol_keys_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let constraint = interner.intern(TypeData::KeyOf(TypeId::SYMBOL));
    let mapped = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint,
        name_type: None,
        template: TypeId::NUMBER,
        readonly_modifier: None,
        optional_modifier: None,
    });

    let description = interner.intern_string("description");
    let expected = interner.object(vec![PropertyInfo::new(description, TypeId::NUMBER)]);
    let mismatch = interner.object(vec![PropertyInfo::new(description, TypeId::STRING)]);

    assert!(checker.is_subtype_of(mapped, expected));
    assert!(!checker.is_subtype_of(mapped, mismatch));
    assert!(!checker.is_subtype_of(expected, mapped));
}
#[test]
fn test_mapped_type_over_bigint_keys_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let constraint = interner.intern(TypeData::KeyOf(TypeId::BIGINT));
    let mapped = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint,
        name_type: None,
        template: TypeId::NUMBER,
        readonly_modifier: None,
        optional_modifier: None,
    });

    let to_string = interner.intern_string("toString");
    let expected = interner.object(vec![PropertyInfo::new(to_string, TypeId::NUMBER)]);
    let mismatch = interner.object(vec![PropertyInfo::new(to_string, TypeId::STRING)]);
    let to_upper = interner.intern_string("toUpperCase");
    let wrong_key = interner.object(vec![PropertyInfo::new(to_upper, TypeId::NUMBER)]);

    assert!(checker.is_subtype_of(mapped, expected));
    assert!(!checker.is_subtype_of(mapped, mismatch));
    assert!(!checker.is_subtype_of(mapped, wrong_key));
    assert!(!checker.is_subtype_of(expected, mapped));
}
#[test]
fn test_mapped_type_optional_modifier_add_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let keys = interner.union(vec![key_a, key_b]);

    let mapped = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keys,
        name_type: None,
        template: TypeId::NUMBER,
        readonly_modifier: None,
        optional_modifier: Some(MappedModifier::Add),
    });

    let name_a = interner.intern_string("a");
    let name_b = interner.intern_string("b");
    let optional_target = interner.object(vec![
        PropertyInfo::opt(name_a, TypeId::NUMBER),
        PropertyInfo::opt(name_b, TypeId::NUMBER),
    ]);
    let required_target = interner.object(vec![
        PropertyInfo::new(name_a, TypeId::NUMBER),
        PropertyInfo::new(name_b, TypeId::NUMBER),
    ]);

    assert!(checker.is_subtype_of(mapped, optional_target));
    assert!(!checker.is_subtype_of(mapped, required_target));
}
#[test]
fn test_mapped_type_readonly_modifier_add_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let keys = interner.union(vec![key_a, key_b]);

    let mapped = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keys,
        name_type: None,
        template: TypeId::NUMBER,
        readonly_modifier: Some(MappedModifier::Add),
        optional_modifier: None,
    });

    let name_a = interner.intern_string("a");
    let name_b = interner.intern_string("b");
    let readonly_target = interner.object(vec![
        PropertyInfo::readonly(name_a, TypeId::NUMBER),
        PropertyInfo::readonly(name_b, TypeId::NUMBER),
    ]);
    let mutable_target = interner.object(vec![
        PropertyInfo::new(name_a, TypeId::NUMBER),
        PropertyInfo::new(name_b, TypeId::NUMBER),
    ]);

    assert!(checker.is_subtype_of(mapped, readonly_target));
    // TypeScript allows readonly → mutable property assignment
    assert!(checker.is_subtype_of(mapped, mutable_target));
}
#[test]
fn test_mapped_type_optional_readonly_add_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let keys = interner.union(vec![key_a, key_b]);

    let mapped = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keys,
        name_type: None,
        template: TypeId::NUMBER,
        readonly_modifier: Some(MappedModifier::Add),
        optional_modifier: Some(MappedModifier::Add),
    });

    let name_a = interner.intern_string("a");
    let name_b = interner.intern_string("b");
    let optional_readonly_target = interner.object(vec![
        PropertyInfo {
            name: name_a,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: true,
            readonly: true,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        },
        PropertyInfo {
            name: name_b,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: true,
            readonly: true,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        },
    ]);
    let mutable_required_target = interner.object(vec![
        PropertyInfo::new(name_a, TypeId::NUMBER),
        PropertyInfo::new(name_b, TypeId::NUMBER),
    ]);

    assert!(checker.is_subtype_of(mapped, optional_readonly_target));
    assert!(!checker.is_subtype_of(mapped, mutable_required_target));
}
#[test]
fn test_mapped_type_optional_readonly_remove_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let key_a = interner.literal_string("a");
    let keys = interner.union(vec![key_a]);

    let mapped = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keys,
        name_type: None,
        template: TypeId::NUMBER,
        readonly_modifier: Some(MappedModifier::Remove),
        optional_modifier: Some(MappedModifier::Remove),
    });

    let name_a = interner.intern_string("a");
    let mutable_required_target = interner.object(vec![PropertyInfo::new(name_a, TypeId::NUMBER)]);
    let readonly_optional_target = interner.object(vec![PropertyInfo {
        name: name_a,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: true,
        readonly: true,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    assert!(checker.is_subtype_of(mapped, mutable_required_target));
    assert!(checker.is_subtype_of(mapped, readonly_optional_target));
    assert!(!checker.is_subtype_of(readonly_optional_target, mapped));
}
#[test]
fn test_mapped_type_optional_modifier_remove_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let key_a = interner.literal_string("a");
    let keys = interner.union(vec![key_a]);

    let mapped = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keys,
        name_type: None,
        template: TypeId::NUMBER,
        readonly_modifier: None,
        optional_modifier: Some(MappedModifier::Remove),
    });

    let name_a = interner.intern_string("a");
    let required_target = interner.object(vec![PropertyInfo::new(name_a, TypeId::NUMBER)]);
    let optional_target = interner.object(vec![PropertyInfo::opt(name_a, TypeId::NUMBER)]);

    assert!(checker.is_subtype_of(mapped, required_target));
    assert!(checker.is_subtype_of(mapped, optional_target));
    assert!(!checker.is_subtype_of(optional_target, mapped));
}
#[test]
fn test_mapped_type_optional_remove_from_optional_keyof() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let key_a = interner.intern_string("a");
    let source_obj = interner.object(vec![PropertyInfo::opt(key_a, TypeId::NUMBER)]);
    let keys = interner.intern(TypeData::KeyOf(source_obj));

    let mapped = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keys,
        name_type: None,
        template: TypeId::STRING,
        readonly_modifier: None,
        optional_modifier: Some(MappedModifier::Remove),
    });

    let required_target = interner.object(vec![PropertyInfo::new(key_a, TypeId::STRING)]);
    let optional_target = interner.object(vec![PropertyInfo::opt(key_a, TypeId::STRING)]);

    assert!(checker.is_subtype_of(mapped, required_target));
    assert!(checker.is_subtype_of(mapped, optional_target));
    assert!(!checker.is_subtype_of(optional_target, mapped));
}
#[test]
fn test_mapped_type_readonly_remove_from_readonly_keyof() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let key_a = interner.intern_string("a");
    let source_obj = interner.object(vec![PropertyInfo::readonly(key_a, TypeId::STRING)]);
    let keys = interner.intern(TypeData::KeyOf(source_obj));

    let mapped = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keys,
        name_type: None,
        template: TypeId::NUMBER,
        readonly_modifier: Some(MappedModifier::Remove),
        optional_modifier: None,
    });

    let mutable_target = interner.object(vec![PropertyInfo::new(key_a, TypeId::NUMBER)]);
    let readonly_target = interner.object(vec![PropertyInfo::readonly(key_a, TypeId::NUMBER)]);

    assert!(checker.is_subtype_of(mapped, mutable_target));
    assert!(checker.is_subtype_of(mapped, readonly_target));
    // TypeScript allows readonly → mutable property assignment
    assert!(checker.is_subtype_of(readonly_target, mapped));
}
#[test]
fn test_mapped_type_readonly_modifier_remove_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let key_a = interner.literal_string("a");
    let keys = interner.union(vec![key_a]);

    let mapped = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keys,
        name_type: None,
        template: TypeId::NUMBER,
        readonly_modifier: Some(MappedModifier::Remove),
        optional_modifier: None,
    });

    let name_a = interner.intern_string("a");
    let mutable_target = interner.object(vec![PropertyInfo::new(name_a, TypeId::NUMBER)]);
    let readonly_target = interner.object(vec![PropertyInfo::readonly(name_a, TypeId::NUMBER)]);

    assert!(checker.is_subtype_of(mapped, mutable_target));
    assert!(checker.is_subtype_of(mapped, readonly_target));
    // TypeScript allows readonly → mutable property assignment
    assert!(checker.is_subtype_of(readonly_target, mapped));
}
#[test]
fn test_mapped_type_key_remap_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let prop_a = PropertyInfo::new(interner.intern_string("a"), TypeId::STRING);
    let prop_b = PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER);
    let obj = interner.object(vec![prop_a.clone(), prop_b.clone()]);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let keys = interner.union(vec![key_a, key_b]);

    let key_param = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(keys),
        default: None,
        is_const: false,
    };
    let key_param_id = interner.intern(TypeData::TypeParameter(key_param));

    let name_type = interner.conditional(ConditionalType {
        check_type: key_param_id,
        extends_type: key_a,
        true_type: TypeId::NEVER,
        false_type: key_param_id,
        is_distributive: true,
    });
    let template = interner.intern(TypeData::IndexAccess(obj, key_param_id));

    let mapped = interner.mapped(MappedType {
        type_param: key_param,
        constraint: keys,
        name_type: Some(name_type),
        template,
        readonly_modifier: None,
        optional_modifier: None,
    });

    let expected = interner.object(vec![PropertyInfo::new(prop_b.name, TypeId::NUMBER)]);
    let requires_a = interner.object(vec![PropertyInfo::new(prop_a.name, TypeId::STRING)]);

    assert!(checker.is_subtype_of(mapped, expected));
    assert!(!checker.is_subtype_of(mapped, requires_a));
}
#[test]
fn test_mapped_type_key_remap_optional_add_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let prop_a = PropertyInfo::new(interner.intern_string("a"), TypeId::STRING);
    let prop_b = PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER);
    let obj = interner.object(vec![prop_a, prop_b.clone()]);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let keys = interner.union(vec![key_a, key_b]);

    let key_param = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(keys),
        default: None,
        is_const: false,
    };
    let key_param_id = interner.intern(TypeData::TypeParameter(key_param));

    let name_type = interner.conditional(ConditionalType {
        check_type: key_param_id,
        extends_type: key_a,
        true_type: TypeId::NEVER,
        false_type: key_param_id,
        is_distributive: true,
    });
    let template = interner.intern(TypeData::IndexAccess(obj, key_param_id));

    let mapped = interner.mapped(MappedType {
        type_param: key_param,
        constraint: keys,
        name_type: Some(name_type),
        template,
        readonly_modifier: None,
        optional_modifier: Some(MappedModifier::Add),
    });

    let optional_b = interner.object(vec![PropertyInfo::opt(prop_b.name, TypeId::NUMBER)]);
    let required_b = interner.object(vec![PropertyInfo::new(prop_b.name, TypeId::NUMBER)]);

    assert!(checker.is_subtype_of(mapped, optional_b));
    assert!(!checker.is_subtype_of(mapped, required_b));
}
#[test]
fn test_mapped_type_key_remap_optional_remove_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let prop_a = PropertyInfo::new(interner.intern_string("a"), TypeId::STRING);
    let prop_b = PropertyInfo::opt(interner.intern_string("b"), TypeId::NUMBER);
    let obj = interner.object(vec![prop_a, prop_b.clone()]);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let keys = interner.union(vec![key_a, key_b]);

    let key_param = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(keys),
        default: None,
        is_const: false,
    };
    let key_param_id = interner.intern(TypeData::TypeParameter(key_param));

    let name_type = interner.conditional(ConditionalType {
        check_type: key_param_id,
        extends_type: key_a,
        true_type: TypeId::NEVER,
        false_type: key_param_id,
        is_distributive: true,
    });
    let template = interner.intern(TypeData::IndexAccess(obj, key_param_id));

    let mapped = interner.mapped(MappedType {
        type_param: key_param,
        constraint: keys,
        name_type: Some(name_type),
        template,
        readonly_modifier: None,
        optional_modifier: Some(MappedModifier::Remove),
    });

    let required_b = interner.object(vec![PropertyInfo::new(prop_b.name, TypeId::NUMBER)]);
    let optional_b = interner.object(vec![PropertyInfo::opt(prop_b.name, TypeId::NUMBER)]);

    assert!(!checker.is_subtype_of(mapped, required_b));
    assert!(checker.is_subtype_of(mapped, optional_b));
}
#[test]
fn test_mapped_type_key_remap_optional_readonly_add_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let prop_a = PropertyInfo::new(interner.intern_string("a"), TypeId::STRING);
    let prop_b = PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER);
    let obj = interner.object(vec![prop_a, prop_b.clone()]);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let keys = interner.union(vec![key_a, key_b]);

    let key_param = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(keys),
        default: None,
        is_const: false,
    };
    let key_param_id = interner.intern(TypeData::TypeParameter(key_param));

    let name_type = interner.conditional(ConditionalType {
        check_type: key_param_id,
        extends_type: key_a,
        true_type: TypeId::NEVER,
        false_type: key_param_id,
        is_distributive: true,
    });
    let template = interner.intern(TypeData::IndexAccess(obj, key_param_id));

    let mapped = interner.mapped(MappedType {
        type_param: key_param,
        constraint: keys,
        name_type: Some(name_type),
        template,
        readonly_modifier: Some(MappedModifier::Add),
        optional_modifier: Some(MappedModifier::Add),
    });

    let optional_readonly_b = interner.object(vec![PropertyInfo {
        name: prop_b.name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: true,
        readonly: true,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);
    let required_readonly_b =
        interner.object(vec![PropertyInfo::readonly(prop_b.name, TypeId::NUMBER)]);
    let optional_mutable_b = interner.object(vec![PropertyInfo::opt(prop_b.name, TypeId::NUMBER)]);

    assert!(checker.is_subtype_of(mapped, optional_readonly_b));
    assert!(!checker.is_subtype_of(mapped, required_readonly_b));
    // TypeScript allows readonly → mutable property assignment
    assert!(checker.is_subtype_of(mapped, optional_mutable_b));
}
#[test]
fn test_mapped_type_key_remap_optional_readonly_remove_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let prop_a = PropertyInfo::new(interner.intern_string("a"), TypeId::STRING);
    let prop_b = PropertyInfo {
        name: interner.intern_string("b"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: true,
        readonly: true,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    };
    let obj = interner.object(vec![prop_a, prop_b.clone()]);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let keys = interner.union(vec![key_a, key_b]);

    let key_param = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(keys),
        default: None,
        is_const: false,
    };
    let key_param_id = interner.intern(TypeData::TypeParameter(key_param));

    let name_type = interner.conditional(ConditionalType {
        check_type: key_param_id,
        extends_type: key_a,
        true_type: TypeId::NEVER,
        false_type: key_param_id,
        is_distributive: true,
    });
    let template = interner.intern(TypeData::IndexAccess(obj, key_param_id));

    let mapped = interner.mapped(MappedType {
        type_param: key_param,
        constraint: keys,
        name_type: Some(name_type),
        template,
        readonly_modifier: Some(MappedModifier::Remove),
        optional_modifier: Some(MappedModifier::Remove),
    });

    let required_mutable_b = interner.object(vec![PropertyInfo::new(prop_b.name, TypeId::NUMBER)]);
    let number_or_undefined = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
    let required_mutable_b_with_undef =
        interner.object(vec![PropertyInfo::new(prop_b.name, number_or_undefined)]);
    let optional_mutable_b = interner.object(vec![PropertyInfo::opt(prop_b.name, TypeId::NUMBER)]);

    assert!(!checker.is_subtype_of(mapped, required_mutable_b));
    assert!(checker.is_subtype_of(mapped, required_mutable_b_with_undef));
    assert!(checker.is_subtype_of(mapped, optional_mutable_b));
    assert!(!checker.is_subtype_of(optional_mutable_b, mapped));
}
#[test]
fn test_mapped_type_key_remap_readonly_add_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let prop_a = PropertyInfo::new(interner.intern_string("a"), TypeId::STRING);
    let prop_b = PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER);
    let obj = interner.object(vec![prop_a, prop_b.clone()]);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let keys = interner.union(vec![key_a, key_b]);

    let key_param = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(keys),
        default: None,
        is_const: false,
    };
    let key_param_id = interner.intern(TypeData::TypeParameter(key_param));

    let name_type = interner.conditional(ConditionalType {
        check_type: key_param_id,
        extends_type: key_a,
        true_type: TypeId::NEVER,
        false_type: key_param_id,
        is_distributive: true,
    });
    let template = interner.intern(TypeData::IndexAccess(obj, key_param_id));

    let mapped = interner.mapped(MappedType {
        type_param: key_param,
        constraint: keys,
        name_type: Some(name_type),
        template,
        readonly_modifier: Some(MappedModifier::Add),
        optional_modifier: None,
    });

    let readonly_b = interner.object(vec![PropertyInfo::readonly(prop_b.name, TypeId::NUMBER)]);
    let mutable_b = interner.object(vec![PropertyInfo::new(prop_b.name, TypeId::NUMBER)]);

    assert!(checker.is_subtype_of(mapped, readonly_b));
    // TypeScript allows readonly → mutable property assignment
    assert!(checker.is_subtype_of(mapped, mutable_b));
}
#[test]
fn test_mapped_type_key_remap_readonly_remove_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let prop_a = PropertyInfo::new(interner.intern_string("a"), TypeId::STRING);
    let prop_b = PropertyInfo::readonly(interner.intern_string("b"), TypeId::NUMBER);
    let obj = interner.object(vec![prop_a, prop_b.clone()]);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let keys = interner.union(vec![key_a, key_b]);

    let key_param = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(keys),
        default: None,
        is_const: false,
    };
    let key_param_id = interner.intern(TypeData::TypeParameter(key_param));

    let name_type = interner.conditional(ConditionalType {
        check_type: key_param_id,
        extends_type: key_a,
        true_type: TypeId::NEVER,
        false_type: key_param_id,
        is_distributive: true,
    });
    let template = interner.intern(TypeData::IndexAccess(obj, key_param_id));

    let mapped = interner.mapped(MappedType {
        type_param: key_param,
        constraint: keys,
        name_type: Some(name_type),
        template,
        readonly_modifier: Some(MappedModifier::Remove),
        optional_modifier: None,
    });

    let mutable_b = interner.object(vec![PropertyInfo::new(prop_b.name, TypeId::NUMBER)]);
    let readonly_b = interner.object(vec![PropertyInfo::readonly(prop_b.name, TypeId::NUMBER)]);

    assert!(checker.is_subtype_of(mapped, mutable_b));
    assert!(checker.is_subtype_of(mapped, readonly_b));
    // TypeScript allows readonly → mutable property assignment
    assert!(checker.is_subtype_of(readonly_b, mapped));
}
#[test]
fn test_mapped_type_key_remap_all_never_empty_object() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let keys = interner.union(vec![key_a, key_b]);

    let key_param = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(keys),
        default: None,
        is_const: false,
    };
    let key_param_id = interner.intern(TypeData::TypeParameter(key_param));

    let name_type = interner.conditional(ConditionalType {
        check_type: key_param_id,
        extends_type: TypeId::STRING,
        true_type: TypeId::NEVER,
        false_type: key_param_id,
        is_distributive: true,
    });

    let mapped = interner.mapped(MappedType {
        type_param: key_param,
        constraint: keys,
        name_type: Some(name_type),
        template: TypeId::BOOLEAN,
        readonly_modifier: None,
        optional_modifier: None,
    });

    let empty_object = interner.object(Vec::new());

    assert!(checker.is_subtype_of(mapped, empty_object));
    assert!(checker.is_subtype_of(empty_object, mapped));
}

// =============================================================================
// Variance in Generic Positions
// =============================================================================
#[test]
fn test_generic_function_constraint_directionality() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_function_types = true;

    let t = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(TypeId::OBJECT),
        default: None,
        is_const: false,
    };
    let t_id = interner.intern(TypeData::TypeParameter(t));

    let t1 = TypeParamInfo {
        name: interner.intern_string("T1"),
        constraint: Some(t_id),
        default: None,
        is_const: false,
    };
    let t1_id = interner.intern(TypeData::TypeParameter(t1));

    let u = TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: Some(t_id),
        default: None,
        is_const: false,
    };
    let u_id = interner.intern(TypeData::TypeParameter(u));

    let v = TypeParamInfo {
        name: interner.intern_string("V"),
        constraint: Some(t1_id),
        default: None,
        is_const: false,
    };
    let v_id = interner.intern(TypeData::TypeParameter(v));

    let fn_t = interner.function(FunctionShape {
        type_params: vec![u],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: u_id,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: u_id,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_t1 = interner.function(FunctionShape {
        type_params: vec![v],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: v_id,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: v_id,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // fn_t: <U extends T>(x: U) => U   (broader constraint: U extends T)
    // fn_t1: <V extends T1>(x: V) => V (narrower constraint: V extends T1, T1 extends T)
    //
    // Alpha-rename check uses target_to_source: targetConstraint ≤ sourceConstraint.
    // fn_t ≤ fn_t1: target=fn_t1, source=fn_t → targetConstraint(T1) ≤ sourceConstraint(T)
    //   T1 ≤ T → true (T1 extends T) → alpha-rename succeeds → subtype ✓
    assert!(checker.is_subtype_of(fn_t, fn_t1));
    // fn_t1 ≤ fn_t: target=fn_t, source=fn_t1 → targetConstraint(T) ≤ sourceConstraint(T1)
    //   T ≤ T1 → false (T doesn't extend T1) → alpha-rename fails
    //   → falls through to erasure/inference which may or may not succeed
    // This direction is NOT guaranteed to succeed with alpha-rename.
    // (The fallback erasure path handles it via constraint erasure + inference.)
}
#[test]
fn test_generic_covariant_return_position() {
    // Producer<T> = { get(): T } - T is in covariant position
    // Producer<string> <: Producer<string | number> (covariant)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let get_name = interner.intern_string("get");

    let get_string = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let get_union = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: interner.union(vec![TypeId::STRING, TypeId::NUMBER]),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let producer_string = interner.object(vec![PropertyInfo {
        name: get_name,
        type_id: get_string,
        write_type: get_string,
        optional: false,
        readonly: true,
        is_method: true,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    let producer_union = interner.object(vec![PropertyInfo {
        name: get_name,
        type_id: get_union,
        write_type: get_union,
        optional: false,
        readonly: true,
        is_method: true,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    // Covariant: Producer<string> <: Producer<string | number>
    assert!(checker.is_subtype_of(producer_string, producer_union));
    // Not the reverse
    assert!(!checker.is_subtype_of(producer_union, producer_string));
}
#[test]
fn test_generic_contravariant_param_position() {
    // Consumer<T> = { accept(x: T): void } - T is in contravariant position
    // Consumer<string | number> <: Consumer<string> (contravariant)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let accept_name = interner.intern_string("accept");

    let accept_string = interner.function(FunctionShape {
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

    let accept_union = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: interner.union(vec![TypeId::STRING, TypeId::NUMBER]),
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let consumer_string = interner.object(vec![PropertyInfo::readonly(accept_name, accept_string)]);

    let consumer_union = interner.object(vec![PropertyInfo::readonly(accept_name, accept_union)]);

    // Contravariant: Consumer<string | number> <: Consumer<string>
    assert!(checker.is_subtype_of(consumer_union, consumer_string));
    // Not the reverse
    assert!(!checker.is_subtype_of(consumer_string, consumer_union));
}
#[test]
fn test_generic_mixed_variance_positions() {
    // Transform<T, U> = { process(input: T): U }
    // T is contravariant (param), U is covariant (return)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let process_name = interner.intern_string("process");
    let wide_type = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // process(input: string | number): string
    let process_wide_in_narrow_out = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("input")),
            type_id: wide_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // process(input: string): string | number
    let process_narrow_in_wide_out = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("input")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: wide_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let transform_a = interner.object(vec![PropertyInfo {
        name: process_name,
        type_id: process_wide_in_narrow_out,
        write_type: process_wide_in_narrow_out,
        optional: false,
        readonly: true,
        is_method: true,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    let transform_b = interner.object(vec![PropertyInfo {
        name: process_name,
        type_id: process_narrow_in_wide_out,
        write_type: process_narrow_in_wide_out,
        optional: false,
        readonly: true,
        is_method: true,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    // Transform with wider input and narrower output is subtype
    // (contravariant input, covariant output)
    assert!(checker.is_subtype_of(transform_a, transform_b));
    assert!(!checker.is_subtype_of(transform_b, transform_a));
}

// =============================================================================
// Bivariant Method Parameters
// =============================================================================
#[test]
fn test_method_bivariant_wider_param() {
    // Methods are bivariant in their parameters (TypeScript legacy behavior)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let method_name = interner.intern_string("handler");
    let wide_param = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let method_narrow = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("e")),
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

    let method_wide = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("e")),
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

    let obj_narrow_method = interner.object(vec![PropertyInfo::method(method_name, method_narrow)]);

    let obj_wide_method = interner.object(vec![PropertyInfo::method(method_name, method_wide)]);

    // Methods are bivariant - both directions should work
    assert!(checker.is_subtype_of(obj_narrow_method, obj_wide_method));
    assert!(checker.is_subtype_of(obj_wide_method, obj_narrow_method));
}
#[test]
fn test_method_bivariant_callback_param() {
    // Method with callback parameter - bivariant behavior
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let method_name = interner.intern_string("on");

    let callback_narrow = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("data")),
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

    let callback_wide = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("data")),
            type_id: interner.union(vec![TypeId::STRING, TypeId::NUMBER]),
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let method_with_narrow_cb = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("cb")),
            type_id: callback_narrow,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let method_with_wide_cb = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("cb")),
            type_id: callback_wide,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let obj_narrow_cb = interner.object(vec![PropertyInfo::method(
        method_name,
        method_with_narrow_cb,
    )]);

    let obj_wide_cb = interner.object(vec![PropertyInfo::method(method_name, method_with_wide_cb)]);

    // Bivariant methods allow both directions
    assert!(checker.is_subtype_of(obj_narrow_cb, obj_wide_cb));
    assert!(checker.is_subtype_of(obj_wide_cb, obj_narrow_cb));
}
#[test]
fn test_function_property_contravariant_not_bivariant() {
    // Function properties (not methods) should be contravariant in strict mode
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let prop_name = interner.intern_string("handler");
    let wide_param = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let fn_narrow = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("e")),
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

    let fn_wide = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("e")),
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

    // is_method: false - these are function properties, not methods
    let obj_narrow_fn = interner.object(vec![PropertyInfo::new(prop_name, fn_narrow)]);

    let obj_wide_fn = interner.object(vec![PropertyInfo::new(prop_name, fn_wide)]);

    // Function properties are contravariant in strict mode
    // wide param <: narrow param target (can accept string when expecting string|number)
    assert!(checker.is_subtype_of(obj_wide_fn, obj_narrow_fn));
    // Not bivariant - narrow param !<: wide param target
    assert!(!checker.is_subtype_of(obj_narrow_fn, obj_wide_fn));
}

// =============================================================================
// Invariant Mutable Property Types
// =============================================================================
#[test]
fn test_mutable_property_invariant_same_type() {
    // Mutable properties with same type should be compatible
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let prop_name = interner.intern_string("value");

    let obj_string = interner.object(vec![PropertyInfo::new(prop_name, TypeId::STRING)]);

    let obj_string_2 = interner.object(vec![PropertyInfo::new(prop_name, TypeId::STRING)]);

    // Same mutable property types are compatible
    assert!(checker.is_subtype_of(obj_string, obj_string_2));
    assert!(checker.is_subtype_of(obj_string_2, obj_string));
}
#[test]
fn test_mutable_property_invariant_different_types() {
    // tsc uses covariant (not invariant) checking for mutable properties.
    // {value: string} IS assignable to {value: string | number} (covariant)
    // {value: string | number} is NOT assignable to {value: string} (narrowing)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let prop_name = interner.intern_string("value");
    let wide_type = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let obj_narrow = interner.object(vec![PropertyInfo::new(prop_name, TypeId::STRING)]);

    let obj_wide = interner.object(vec![PropertyInfo::new(prop_name, wide_type)]);

    // Narrow -> wide: OK (covariant property checking)
    assert!(checker.is_subtype_of(obj_narrow, obj_wide));
    // Wide -> narrow: NOT OK (string|number is not assignable to string)
    assert!(!checker.is_subtype_of(obj_wide, obj_narrow));
}
#[test]
fn test_mutable_property_split_accessor_wider_write() {
    // Property with split accessor: read narrow, write wide
    // This is safe and should be covariant-like
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let prop_name = interner.intern_string("value");
    let wide_type = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let obj_split = interner.object(vec![PropertyInfo {
        name: prop_name,
        type_id: TypeId::STRING, // read type
        write_type: wide_type,   // write type (wider)
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    let obj_normal = interner.object(vec![PropertyInfo::new(prop_name, TypeId::STRING)]);

    // Split accessor with wider write is a subtype (can write more, reads same)
    assert!(checker.is_subtype_of(obj_split, obj_normal));
    // Normal cannot substitute for split (narrower write type)
    assert!(!checker.is_subtype_of(obj_normal, obj_split));
}
#[test]
fn test_readonly_property_covariant() {
    // Readonly properties should be covariant (no writes)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let prop_name = interner.intern_string("value");
    let wide_type = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let obj_narrow_readonly =
        interner.object(vec![PropertyInfo::readonly(prop_name, TypeId::STRING)]);

    let obj_wide_readonly = interner.object(vec![PropertyInfo::readonly(prop_name, wide_type)]);

    // Readonly is covariant - narrow <: wide
    assert!(checker.is_subtype_of(obj_narrow_readonly, obj_wide_readonly));
    // Not the reverse
    assert!(!checker.is_subtype_of(obj_wide_readonly, obj_narrow_readonly));
}
#[test]
fn test_mutable_array_element_invariant() {
    // Arrays are covariant in TypeScript (unsound but intentional)
    // This test documents that behavior
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let wide_array = interner.array(interner.union(vec![TypeId::STRING, TypeId::NUMBER]));

    // TypeScript arrays are covariant (allows unsound mutations)
    assert!(checker.is_subtype_of(string_array, wide_array));
    // Not the reverse
    assert!(!checker.is_subtype_of(wide_array, string_array));
}

// =============================================================================
// Intersection Type Tests
// =============================================================================
#[test]
fn test_intersection_flattening_nested() {
    // (A & B) & C should be equivalent to A & B & C
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");
    let c_name = interner.intern_string("c");

    let obj_a = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    let obj_b = interner.object(vec![PropertyInfo::new(b_name, TypeId::NUMBER)]);

    let obj_c = interner.object(vec![PropertyInfo::new(c_name, TypeId::BOOLEAN)]);

    // Nested: (A & B) & C
    let ab = interner.intersection(vec![obj_a, obj_b]);
    let nested = interner.intersection(vec![ab, obj_c]);

    // Flat: A & B & C
    let flat = interner.intersection(vec![obj_a, obj_b, obj_c]);

    // Both should be subtypes of each other (equivalent)
    assert!(checker.is_subtype_of(nested, flat));
    assert!(checker.is_subtype_of(flat, nested));
}
#[test]
fn test_intersection_flattening_single_element() {
    // A & (single element) should be equivalent to just A
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let obj_a = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    // Single element intersection
    let single = interner.intersection(vec![obj_a]);

    // Should be equivalent to the element itself
    assert!(checker.is_subtype_of(single, obj_a));
    assert!(checker.is_subtype_of(obj_a, single));
}
#[test]
fn test_intersection_flattening_duplicates() {
    // A & A should be equivalent to A
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let obj_a = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    let duplicated = interner.intersection(vec![obj_a, obj_a]);

    // Should be equivalent to original
    assert!(checker.is_subtype_of(duplicated, obj_a));
    assert!(checker.is_subtype_of(obj_a, duplicated));
}
#[test]
fn test_intersection_with_never_is_never() {
    // A & never = never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let obj_a = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    let with_never = interner.intersection(vec![obj_a, TypeId::NEVER]);

    // A & never should be subtype of never (i.e., is never)
    assert!(checker.is_subtype_of(with_never, TypeId::NEVER));
    // never is subtype of everything
    assert!(checker.is_subtype_of(TypeId::NEVER, with_never));
}
#[test]
fn test_intersection_never_absorbs_all() {
    // string & number & boolean & never = never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let multi_with_never = interner.intersection(vec![
        TypeId::STRING,
        TypeId::NUMBER,
        TypeId::BOOLEAN,
        TypeId::NEVER,
    ]);

    assert!(checker.is_subtype_of(multi_with_never, TypeId::NEVER));
}
#[test]
fn test_intersection_never_at_any_position() {
    // never at beginning, middle, end should all reduce to never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let at_start = interner.intersection(vec![TypeId::NEVER, TypeId::STRING, TypeId::NUMBER]);
    let at_middle = interner.intersection(vec![TypeId::STRING, TypeId::NEVER, TypeId::NUMBER]);
    let at_end = interner.intersection(vec![TypeId::STRING, TypeId::NUMBER, TypeId::NEVER]);

    assert!(checker.is_subtype_of(at_start, TypeId::NEVER));
    assert!(checker.is_subtype_of(at_middle, TypeId::NEVER));
    assert!(checker.is_subtype_of(at_end, TypeId::NEVER));
}
#[test]
fn test_object_intersection_merges_properties() {
    // { a: string } & { b: number } <: { a: string, b: number }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");

    let obj_a = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    let obj_b = interner.object(vec![PropertyInfo::new(b_name, TypeId::NUMBER)]);

    let intersection = interner.intersection(vec![obj_a, obj_b]);

    let merged = interner.object(vec![
        PropertyInfo::new(a_name, TypeId::STRING),
        PropertyInfo::new(b_name, TypeId::NUMBER),
    ]);

    // Intersection should be subtype of merged object
    assert!(checker.is_subtype_of(intersection, merged));
    // Merged object should also be subtype of intersection
    assert!(checker.is_subtype_of(merged, intersection));
}
#[test]
fn test_object_intersection_same_property_narrowing() {
    // { x: string | number } & { x: string } = { x: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");
    let wide_type = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let obj_wide = interner.object(vec![PropertyInfo::new(x_name, wide_type)]);

    let obj_narrow = interner.object(vec![PropertyInfo::new(x_name, TypeId::STRING)]);

    let intersection = interner.intersection(vec![obj_wide, obj_narrow]);

    // Intersection should be subtype of narrow (narrowed to string)
    assert!(checker.is_subtype_of(intersection, obj_narrow));
}
#[test]
fn test_object_intersection_three_objects() {
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

    // Should be subtype of each individual object
    assert!(checker.is_subtype_of(intersection, obj_a));
    assert!(checker.is_subtype_of(intersection, obj_b));
    assert!(checker.is_subtype_of(intersection, obj_c));
}
#[test]
fn test_object_intersection_with_optional_property() {
    // { a: string } & { b?: number } should have required a and optional b
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");

    let obj_a = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    let obj_b_optional = interner.object(vec![PropertyInfo::opt(b_name, TypeId::NUMBER)]);

    let intersection = interner.intersection(vec![obj_a, obj_b_optional]);

    // Should be subtype of required a
    assert!(checker.is_subtype_of(intersection, obj_a));
    // Should be subtype of optional b
    assert!(checker.is_subtype_of(intersection, obj_b_optional));
}
#[test]
fn test_intersection_subtype_of_each_member() {
    // A & B should be subtype of A and subtype of B
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");

    let obj_a = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    let obj_b = interner.object(vec![PropertyInfo::new(b_name, TypeId::NUMBER)]);

    let intersection = interner.intersection(vec![obj_a, obj_b]);

    // A & B <: A
    assert!(checker.is_subtype_of(intersection, obj_a));
    // A & B <: B
    assert!(checker.is_subtype_of(intersection, obj_b));
    // A !<: A & B (missing b property)
    assert!(!checker.is_subtype_of(obj_a, intersection));
    // B !<: A & B (missing a property)
    assert!(!checker.is_subtype_of(obj_b, intersection));
}

// =============================================================================
// Literal Type Tests
// =============================================================================
#[test]
fn test_string_literal_narrows_to_union() {
    // "a" <: "a" | "b" | "c"
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a = interner.literal_string("a");
    let b = interner.literal_string("b");
    let c = interner.literal_string("c");

    let union = interner.union(vec![a, b, c]);

    // Each literal is subtype of the union
    assert!(checker.is_subtype_of(a, union));
    assert!(checker.is_subtype_of(b, union));
    assert!(checker.is_subtype_of(c, union));

    // Union is not subtype of individual literal
    assert!(!checker.is_subtype_of(union, a));
    assert!(!checker.is_subtype_of(union, b));
    assert!(!checker.is_subtype_of(union, c));
}
#[test]
fn test_string_literal_not_subtype_of_different_literal() {
    // "hello" is not subtype of "world"
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let hello = interner.literal_string("hello");
    let world = interner.literal_string("world");

    assert!(!checker.is_subtype_of(hello, world));
    assert!(!checker.is_subtype_of(world, hello));
}
#[test]
fn test_string_literal_subtype_of_string() {
    // Any string literal is subtype of string
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let hello = interner.literal_string("hello");
    let empty = interner.literal_string("");
    let special = interner.literal_string("!@#$%^&*()");

    assert!(checker.is_subtype_of(hello, TypeId::STRING));
    assert!(checker.is_subtype_of(empty, TypeId::STRING));
    assert!(checker.is_subtype_of(special, TypeId::STRING));

    // string is not subtype of literal
    assert!(!checker.is_subtype_of(TypeId::STRING, hello));
}
#[test]
fn test_string_literal_union_subtype_of_string() {
    // "a" | "b" <: string
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a = interner.literal_string("a");
    let b = interner.literal_string("b");
    let union = interner.union(vec![a, b]);

    assert!(checker.is_subtype_of(union, TypeId::STRING));
    assert!(!checker.is_subtype_of(TypeId::STRING, union));
}
#[test]
fn test_numeric_literal_types() {
    // 1 <: number, 1 === 1, 1 !== 2
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let one = interner.literal_number(1.0);
    let two = interner.literal_number(2.0);
    let zero = interner.literal_number(0.0);
    let negative = interner.literal_number(-42.0);
    const APPROX_FLOAT: f64 = 3.15;
    let float = interner.literal_number(APPROX_FLOAT);

    // Same literal is subtype of itself
    assert!(checker.is_subtype_of(one, one));
    assert!(checker.is_subtype_of(two, two));

    // Different literals are not subtypes of each other
    assert!(!checker.is_subtype_of(one, two));
    assert!(!checker.is_subtype_of(two, one));

    // All numeric literals are subtypes of number
    assert!(checker.is_subtype_of(one, TypeId::NUMBER));
    assert!(checker.is_subtype_of(zero, TypeId::NUMBER));
    assert!(checker.is_subtype_of(negative, TypeId::NUMBER));
    assert!(checker.is_subtype_of(float, TypeId::NUMBER));

    // number is not subtype of numeric literal
    assert!(!checker.is_subtype_of(TypeId::NUMBER, one));
}
#[test]
fn test_numeric_literal_union() {
    // 1 | 2 | 3 <: number
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let one = interner.literal_number(1.0);
    let two = interner.literal_number(2.0);
    let three = interner.literal_number(3.0);

    let union = interner.union(vec![one, two, three]);

    // Union of numeric literals is subtype of number
    assert!(checker.is_subtype_of(union, TypeId::NUMBER));

    // Each literal is subtype of the union
    assert!(checker.is_subtype_of(one, union));
    assert!(checker.is_subtype_of(two, union));
    assert!(checker.is_subtype_of(three, union));

    // number is not subtype of the union
    assert!(!checker.is_subtype_of(TypeId::NUMBER, union));
}
#[test]
fn test_numeric_literal_special_values() {
    // Test special numeric values
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let zero = interner.literal_number(0.0);
    let neg_zero = interner.literal_number(-0.0);

    // Both are subtypes of number
    assert!(checker.is_subtype_of(zero, TypeId::NUMBER));
    assert!(checker.is_subtype_of(neg_zero, TypeId::NUMBER));
}
#[test]
fn test_template_literal_pattern_prefix() {
    // `prefix${string}` matches "prefix-anything"
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // Template: `prefix-${string}`
    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("prefix-")),
        TemplateSpan::Type(TypeId::STRING),
    ]);

    // Template is subtype of string
    assert!(checker.is_subtype_of(template, TypeId::STRING));

    // String literal matching the pattern
    let matching = interner.literal_string("prefix-hello");
    assert!(checker.is_subtype_of(matching, TypeId::STRING));

    // Literal "prefix-hello" should be subtype of the template pattern
    assert!(checker.is_subtype_of(matching, template));
}
#[test]
fn test_template_literal_pattern_suffix() {
    // `${string}-suffix` pattern
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // Template: `${string}-suffix`
    let template = interner.template_literal(vec![
        TemplateSpan::Type(TypeId::STRING),
        TemplateSpan::Text(interner.intern_string("-suffix")),
    ]);

    // Template is subtype of string
    assert!(checker.is_subtype_of(template, TypeId::STRING));

    // Matching literal
    let matching = interner.literal_string("hello-suffix");
    assert!(checker.is_subtype_of(matching, template));

    // Non-matching literal should NOT be subtype
    let not_matching = interner.literal_string("hello-other");
    assert!(!checker.is_subtype_of(not_matching, template));
}
#[test]
fn test_template_literal_pattern_with_union() {
    // `color-${"red" | "blue"}` = "color-red" | "color-blue"
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let red = interner.literal_string("red");
    let blue = interner.literal_string("blue");
    let colors = interner.union(vec![red, blue]);

    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("color-")),
        TemplateSpan::Type(colors),
    ]);

    // Template is subtype of string
    assert!(checker.is_subtype_of(template, TypeId::STRING));

    // Matching literals
    let color_red = interner.literal_string("color-red");
    let color_blue = interner.literal_string("color-blue");

    assert!(checker.is_subtype_of(color_red, template));
    assert!(checker.is_subtype_of(color_blue, template));

    // Non-matching literal
    let color_green = interner.literal_string("color-green");
    assert!(!checker.is_subtype_of(color_green, template));
}
#[test]
fn test_template_literal_pattern_multiple_parts() {
    // `${string}-${number}` pattern
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let template = interner.template_literal(vec![
        TemplateSpan::Type(TypeId::STRING),
        TemplateSpan::Text(interner.intern_string("-")),
        TemplateSpan::Type(TypeId::NUMBER),
    ]);

    // Template is subtype of string
    assert!(checker.is_subtype_of(template, TypeId::STRING));

    // Matching literal
    let matching = interner.literal_string("hello-42");
    assert!(checker.is_subtype_of(matching, template));
}
#[test]
fn test_template_literal_empty_parts() {
    // Template with just string interpolation `${string}`
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let template = interner.template_literal(vec![TemplateSpan::Type(TypeId::STRING)]);

    // Should be equivalent to string
    assert!(checker.is_subtype_of(template, TypeId::STRING));

    // Any string literal should match
    let hello = interner.literal_string("hello");
    assert!(checker.is_subtype_of(hello, template));
}
#[test]
fn test_boolean_literal_types() {
    // true and false literal types
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // Use literal_boolean to create true/false literal types
    let type_true = interner.literal_boolean(true);
    let type_false = interner.literal_boolean(false);

    // true and false literal types are subtypes of boolean
    assert!(checker.is_subtype_of(type_true, TypeId::BOOLEAN));
    assert!(checker.is_subtype_of(type_false, TypeId::BOOLEAN));

    // true and false are not subtypes of each other
    assert!(!checker.is_subtype_of(type_true, type_false));
    assert!(!checker.is_subtype_of(type_false, type_true));

    // boolean is not subtype of true or false
    assert!(!checker.is_subtype_of(TypeId::BOOLEAN, type_true));
    assert!(!checker.is_subtype_of(TypeId::BOOLEAN, type_false));
}

// =============================================================================
// Variance Tests - Covariant, Contravariant, Invariant, Bivariant
// =============================================================================

// -----------------------------------------------------------------------------
// Covariant Position (Return Types)
// -----------------------------------------------------------------------------
#[test]
fn test_covariant_return_type_subtype() {
    // () => string <: () => string | number
    // Return type is covariant: narrower return assignable to wider
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let fn_return_string = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let fn_return_union = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: union,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Covariant: () => string <: () => string | number
    assert!(checker.is_subtype_of(fn_return_string, fn_return_union));
    // Not the reverse
    assert!(!checker.is_subtype_of(fn_return_union, fn_return_string));
}
#[test]
fn test_covariant_return_type_literal() {
    // () => "hello" <: () => string
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let hello = interner.literal_string("hello");
    let fn_return_literal = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: hello,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_return_string = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Covariant: () => "hello" <: () => string
    assert!(checker.is_subtype_of(fn_return_literal, fn_return_string));
    // Not the reverse
    assert!(!checker.is_subtype_of(fn_return_string, fn_return_literal));
}
#[test]
fn test_covariant_return_type_object() {
    // () => { a: string, b: number } <: () => { a: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_prop = interner.intern_string("a");
    let b_prop = interner.intern_string("b");

    let obj_ab = interner.object(vec![
        PropertyInfo::new(a_prop, TypeId::STRING),
        PropertyInfo::new(b_prop, TypeId::NUMBER),
    ]);

    let obj_a = interner.object(vec![PropertyInfo::new(a_prop, TypeId::STRING)]);

    let fn_return_ab = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: obj_ab,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_return_a = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: obj_a,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Covariant: more properties in return is subtype of fewer
    assert!(checker.is_subtype_of(fn_return_ab, fn_return_a));
    assert!(!checker.is_subtype_of(fn_return_a, fn_return_ab));
}
#[test]
fn test_covariant_return_type_array() {
    // () => string[] <: () => (string | number)[]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let union_array = interner.array(union);

    let fn_return_string_arr = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: string_array,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_return_union_arr = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: union_array,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Covariant: narrower array type in return
    assert!(checker.is_subtype_of(fn_return_string_arr, fn_return_union_arr));
    assert!(!checker.is_subtype_of(fn_return_union_arr, fn_return_string_arr));
}
