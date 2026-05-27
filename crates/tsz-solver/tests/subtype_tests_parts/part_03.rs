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
        is_symbol_named: false,
        single_quoted_name: false,
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
        is_symbol_named: false,
        single_quoted_name: false,
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
        is_symbol_named: false,
        single_quoted_name: false,
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
        is_symbol_named: false,
        single_quoted_name: false,
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
        is_symbol_named: false,
        single_quoted_name: false,
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
            is_symbol_named: false,
            single_quoted_name: false,
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
            is_symbol_named: false,
            single_quoted_name: false,
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
        is_symbol_named: false,
        single_quoted_name: false,
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

    assert!(checker.is_subtype_of(mapped, required_b));
    assert!(checker.is_subtype_of(mapped, optional_b));
}

