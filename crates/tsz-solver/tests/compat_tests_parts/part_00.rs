#[test]
fn test_any_assignability() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    assert!(checker.is_assignable(TypeId::ANY, TypeId::STRING));
    assert!(checker.is_assignable(TypeId::STRING, TypeId::ANY));
}

#[test]
fn compat_checker_cache_statistics_account_for_relation_entries() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let empty = checker.cache_statistics();
    assert_eq!(empty.relation_entries, 0);
    assert_eq!(empty.estimated_size_bytes(), 0);

    assert!(!checker.is_assignable(TypeId::STRING, TypeId::NUMBER));
    let populated = checker.cache_statistics();
    assert_eq!(populated.relation_entries, 1);
    assert!(
        populated.estimated_size_bytes() > empty.estimated_size_bytes(),
        "populated compatibility cache should report nonzero estimated residency"
    );

    assert!(!checker.is_assignable(TypeId::STRING, TypeId::NUMBER));
    let repeated = checker.cache_statistics();
    assert_eq!(repeated.relation_entries, populated.relation_entries);
    assert_eq!(
        repeated.estimated_size_bytes(),
        populated.estimated_size_bytes()
    );

    checker.set_strict_function_types(true);
    assert_eq!(checker.cache_statistics().relation_entries, 0);
}

#[test]
fn test_unknown_assignability() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    assert!(checker.is_assignable(TypeId::STRING, TypeId::UNKNOWN));
    assert!(checker.is_assignable(TypeId::UNKNOWN, TypeId::ANY));
    assert!(checker.is_assignable(TypeId::UNKNOWN, TypeId::UNKNOWN));
    assert!(!checker.is_assignable(TypeId::UNKNOWN, TypeId::STRING));
}

#[test]
fn test_error_type_permissive() {
    // ERROR types are assignable to/from everything (like `any` in tsc).
    // This prevents cascading diagnostics: when one type resolution fails,
    // tsc silences further errors involving that type.
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // ERROR is assignable to concrete types (like `any`)
    assert!(checker.is_assignable(TypeId::ERROR, TypeId::STRING));
    // Concrete types are assignable to ERROR (like `any`)
    assert!(checker.is_assignable(TypeId::STRING, TypeId::ERROR));
    // ERROR is assignable to itself (reflexive)
    assert!(checker.is_assignable(TypeId::ERROR, TypeId::ERROR));
}

#[test]
fn test_error_poisoning_union_normalization() {
    let interner = TypeInterner::new();

    let union = interner.union(vec![TypeId::STRING, TypeId::ERROR]);
    assert_eq!(union, TypeId::ERROR);
}

#[test]
fn test_recursion_depth_limit_assignable() {
    // Test that deep recursion doesn't crash and handles depth limit correctly.
    //
    // Following tsc's semantics (Ternary.Maybe on overflow):
    // - When depth limit is hit, we return DepthExceeded which is treated as true
    // - CycleDetected is used for valid coinductive recursion (true)
    // - Both match tsc's behavior where recursive depth overflow assumes types are related
    //
    // The depth_exceeded flag is set for TS2589 diagnostic emission.
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    fn nest_array(interner: &TypeInterner, base: TypeId, depth: usize) -> TypeId {
        let mut ty = base;
        for _ in 0..depth {
            ty = interner.array(ty);
        }
        ty
    }

    let deep_string = nest_array(&interner, TypeId::STRING, 120);
    let deep_number = nest_array(&interner, TypeId::NUMBER, 120);

    // When depth limit is exceeded, we return DepthExceeded which is treated as
    // true (matching tsc's Ternary.Maybe semantics). This prevents false TS2344
    // errors on recursive/circular generic constraints.
    let result = checker.is_assignable(deep_string, deep_number);
    // Result is true due to DepthExceeded returning true (tsc parity)
    assert!(result);

    // Same types at same depth should be assignable (identity check short-circuits)
    let deep_string2 = nest_array(&interner, TypeId::STRING, 120);
    assert!(checker.is_assignable(deep_string, deep_string2));
}

#[test]
fn test_base_constraint_assignability_compat() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }));
    let u_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }));
    let v_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("V"),
        constraint: Some(TypeId::NUMBER),
        default: None,
        is_const: false,
    }));

    assert!(checker.is_assignable(t_param, TypeId::STRING));
    assert!(!checker.is_assignable(t_param, TypeId::NUMBER));
    assert!(!checker.is_assignable(t_param, u_param));
    assert!(!checker.is_assignable(t_param, v_param));
}

#[test]
fn test_function_bivariance_default() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let (animal, dog) = make_animal_dog(&interner);

    let fn_dog = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(dog)],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_animal = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(animal)],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    assert!(checker.is_assignable(fn_dog, fn_animal));
}

#[test]
fn test_function_variance_strict() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    checker.set_strict_function_types(true);

    let (animal, dog) = make_animal_dog(&interner);

    let fn_dog = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(dog)],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_animal = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(animal)],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    assert!(!checker.is_assignable(fn_dog, fn_animal));
}

#[test]
fn test_array_covariance_assignability() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let (animal, dog) = make_animal_dog(&interner);
    let dog_array = interner.array(dog);
    let animal_array = interner.array(animal);

    assert!(checker.is_assignable(dog_array, animal_array));
    assert!(!checker.is_assignable(animal_array, dog_array));
}

#[test]
fn test_optional_parameter_assignability_allows_extra_optional() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let source = interner.function(FunctionShape {
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
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let target = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    assert!(checker.is_assignable(source, target));
}

#[test]
fn test_optional_parameter_assignability_rejects_required_extra() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let source = interner.function(FunctionShape {
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
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let target = interner.function(FunctionShape {
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
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Without strictFunctionTypes, bivariant parameter check allows this:
    // number <: (number | undefined) → YES → compatible
    assert!(checker.is_assignable(source, target));
}

#[test]
fn test_this_parameter_assignability_respects_strictness() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let source = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: Some(TypeId::STRING),
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let string_or_number = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let target = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: Some(string_or_number),
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    assert!(checker.is_assignable(source, target));

    checker.set_strict_function_types(true);
    assert!(!checker.is_assignable(source, target));
}

#[test]
fn test_rest_parameter_assignability_rejects_incompatible_fixed() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let source = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: interner.array(TypeId::STRING),
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let target = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    assert!(!checker.is_assignable(source, target));
}

#[test]
fn test_method_bivariance_even_strict() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    checker.set_strict_function_types(true);

    let name = interner.intern_string("fn");
    let string_or_number = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let source_fn = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(TypeId::STRING)],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let target_fn = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(string_or_number)],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let source = interner.object(vec![PropertyInfo {
        name,
        type_id: source_fn,
        write_type: source_fn,
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
        type_id: target_fn,
        write_type: target_fn,
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

    assert!(checker.is_assignable(source, target));
}

#[test]
fn test_function_property_stays_strict() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    checker.set_strict_function_types(true);

    let name = interner.intern_string("fn");
    let string_or_number = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let source_fn = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(TypeId::STRING)],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let target_fn = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(string_or_number)],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let source = interner.object(vec![PropertyInfo {
        name,
        type_id: source_fn,
        write_type: source_fn,
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
        type_id: target_fn,
        write_type: target_fn,
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

    assert!(!checker.is_assignable(source, target));
}

#[test]
fn test_function_return_covariance() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let (animal, dog) = make_animal_dog(&interner);

    let returns_dog = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: dog,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let returns_animal = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: animal,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    assert!(checker.is_assignable(returns_dog, returns_animal));
    assert!(!checker.is_assignable(returns_animal, returns_dog));
}

#[test]
fn test_void_return_assignability() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let returns_number = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::NUMBER,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let returns_void = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    assert!(checker.is_assignable(returns_number, returns_void));
    assert!(!checker.is_assignable(returns_void, returns_number));
}

#[test]
fn test_void_undefined_return_assignability() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let returns_void = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let returns_undefined = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::UNDEFINED,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    assert!(checker.is_assignable(returns_undefined, returns_void));
    assert!(!checker.is_assignable(returns_void, returns_undefined));
}

#[test]
fn test_constructor_void_return_assignability() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let instance = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        TypeId::NUMBER,
    )]);

    let returns_instance = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: instance,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    let returns_void = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    assert!(checker.is_assignable(returns_instance, returns_void));
    assert!(!checker.is_assignable(returns_void, returns_instance));
}

#[test]
fn test_construct_signature_void_return_assignability() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let instance = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        TypeId::NUMBER,
    )]);

    let returns_instance = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: Vec::new(),
        construct_signatures: vec![CallSignature {
            params: Vec::new(),
            this_type: None,
            return_type: instance,
            type_predicate: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        properties: Vec::new(),
        ..Default::default()
    });

    let returns_void = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: Vec::new(),
        construct_signatures: vec![CallSignature {
            params: Vec::new(),
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        properties: Vec::new(),
        ..Default::default()
    });

    assert!(checker.is_assignable(returns_instance, returns_void));
    assert!(!checker.is_assignable(returns_void, returns_instance));
}

#[test]
fn test_call_signature_void_return_assignability() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let returns_number = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            params: Vec::new(),
            this_type: None,
            return_type: TypeId::NUMBER,
            type_predicate: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        ..Default::default()
    });

    let returns_void = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            params: Vec::new(),
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        ..Default::default()
    });

    assert!(checker.is_assignable(returns_number, returns_void));
    assert!(!checker.is_assignable(returns_void, returns_number));
}

#[test]
fn test_call_signature_void_undefined_return_assignability() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let returns_void = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            params: Vec::new(),
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        ..Default::default()
    });

    let returns_undefined = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            params: Vec::new(),
            this_type: None,
            return_type: TypeId::UNDEFINED,
            type_predicate: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        ..Default::default()
    });

    assert!(checker.is_assignable(returns_undefined, returns_void));
    assert!(!checker.is_assignable(returns_void, returns_undefined));
}

#[test]
fn test_explain_failure_missing_property() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let (animal, dog) = make_animal_dog(&interner);
    let breed_name = interner.intern_string("breed");

    let reason = checker.explain_failure(animal, dog);
    assert!(
        matches!(reason, Some(SubtypeFailureReason::MissingProperty { property_name, .. })
            if property_name == breed_name)
    );
}

#[test]
fn test_explain_failure_parameter_mismatch_strict() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    checker.set_strict_function_types(true);

    let (animal, dog) = make_animal_dog(&interner);

    let fn_dog = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(dog)],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_animal = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(animal)],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let reason = checker.explain_failure(fn_dog, fn_animal);
    assert!(matches!(
        reason,
        Some(SubtypeFailureReason::ParameterTypeMismatch { param_index: 0, .. })
    ));
}

/// When the outer parameter mismatch is *itself* a callable, the
/// `inner_reason` should describe how the inner contravariant subtype
/// check (`target_param <: source_param`) failed. This is what lets
/// renderers distinguish an inner-callback return-type failure from an
/// inner-callback parameter-type failure (matching tsc's
/// `overrideNextErrorInfo` elision logic).
#[test]
fn test_parameter_mismatch_carries_inner_callback_return_reason() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    checker.set_strict_function_types(true);

    let (animal, dog) = make_animal_dog(&interner);

    // (x: Animal) => Animal
    let cb_animal_animal = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(animal)],
        this_type: None,
        return_type: animal,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    // (x: Dog) => Dog
    let cb_dog_dog = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(dog)],
        this_type: None,
        return_type: dog,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Outer: (f: cb_dog_dog) => void  — the source ("fc2" in tsc baseline).
    let outer_source = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(cb_dog_dog)],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    // Outer: (f: cb_animal_animal) => void  — the target ("fc1").
    let outer_target = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(cb_animal_animal)],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // `fc1 = fc2`: source=fc2, target=fc1. Inner contravariant check on
    // param `f` is `target_f <: source_f` = `cb_animal_animal <: cb_dog_dog`,
    // which fails on the *return type* (Animal not <: Dog).
    let reason = checker.explain_failure(outer_source, outer_target);
    let Some(SubtypeFailureReason::ParameterTypeMismatch { inner_reason, .. }) = reason else {
        panic!("expected ParameterTypeMismatch, got {reason:?}");
    };
    let inner = inner_reason.expect("inner_reason should be populated for failed callback param");
    assert!(
        matches!(*inner, SubtypeFailureReason::ReturnTypeMismatch { .. }),
        "expected inner ReturnTypeMismatch for fc1=fc2 case, got {inner:?}"
    );
}

/// The mirror case: `fc2 = fc1` fails on the inner callback's
/// *parameter*. The carried `inner_reason` should reflect that.
#[test]
fn test_parameter_mismatch_carries_inner_callback_param_reason() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    checker.set_strict_function_types(true);

    let (animal, dog) = make_animal_dog(&interner);

    let cb_animal_animal = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(animal)],
        this_type: None,
        return_type: animal,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let cb_dog_dog = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(dog)],
        this_type: None,
        return_type: dog,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let outer_source = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(cb_animal_animal)],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let outer_target = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(cb_dog_dog)],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // `fc2 = fc1`: source=fc1, target=fc2. Inner contravariant check on
    // param `f` is `target_f <: source_f` = `cb_dog_dog <: cb_animal_animal`,
    // which fails on the *parameter* (Animal not <: Dog).
    let reason = checker.explain_failure(outer_source, outer_target);
    let Some(SubtypeFailureReason::ParameterTypeMismatch { inner_reason, .. }) = reason else {
        panic!("expected ParameterTypeMismatch, got {reason:?}");
    };
    let inner = inner_reason.expect("inner_reason should be populated for failed callback param");
    assert!(
        !matches!(*inner, SubtypeFailureReason::ReturnTypeMismatch { .. }),
        "expected non-ReturnTypeMismatch inner for fc2=fc1 case, got {inner:?}"
    );
}

#[test]
fn test_weak_type_rejects_no_common_properties() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let a = interner.intern_string("a");
    let b = interner.intern_string("b");

    let weak_target = interner.object(vec![PropertyInfo::opt(a, TypeId::NUMBER)]);

    let source = interner.object(vec![PropertyInfo::new(b, TypeId::NUMBER)]);

    assert!(!checker.is_assignable(source, weak_target));
    assert!(matches!(
        checker.explain_failure(source, weak_target),
        Some(SubtypeFailureReason::NoCommonProperties { .. })
    ));
}

#[test]
fn test_weak_type_allows_overlap() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let a = interner.intern_string("a");

    let weak_target = interner.object(vec![PropertyInfo::opt(a, TypeId::NUMBER)]);

    let source = interner.object(vec![PropertyInfo::new(a, TypeId::NUMBER)]);

    assert!(checker.is_assignable(source, weak_target));
}

#[test]
fn test_weak_type_skips_empty_target() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let a = interner.intern_string("a");

    let empty_target = interner.object(Vec::new());
    let source = interner.object(vec![PropertyInfo::new(a, TypeId::NUMBER)]);

    assert!(checker.is_assignable(source, empty_target));
}

#[test]
fn test_weak_union_rejects_no_common_properties() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let a = interner.intern_string("a");
    let b = interner.intern_string("b");
    let c = interner.intern_string("c");

    let weak_a = interner.object(vec![PropertyInfo::opt(a, TypeId::NUMBER)]);
    let weak_b = interner.object(vec![PropertyInfo::opt(b, TypeId::NUMBER)]);
    let target = interner.union(vec![weak_a, weak_b]);

    let source = interner.object(vec![PropertyInfo::new(c, TypeId::NUMBER)]);

    assert!(!checker.is_assignable(source, target));
    assert!(matches!(
        checker.explain_failure(source, target),
        Some(SubtypeFailureReason::TypeMismatch { .. })
    ));
}

#[test]
fn test_weak_union_rejects_no_common_properties_with_refs() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let a = interner.intern_string("a");
    let b = interner.intern_string("b");
    let c = interner.intern_string("c");

    let weak_a = interner.object(vec![PropertyInfo::opt(a, TypeId::NUMBER)]);
    let weak_b = interner.object(vec![PropertyInfo::opt(b, TypeId::NUMBER)]);

    let def_id_a = DefId(1);
    let def_id_b = DefId(2);
    env.insert_def(def_id_a, weak_a);
    env.insert_def(def_id_b, weak_b);

    let target = interner.union(vec![interner.lazy(def_id_a), interner.lazy(def_id_b)]);
    let source = interner.object(vec![PropertyInfo::new(c, TypeId::NUMBER)]);

    let mut checker = CompatChecker::with_resolver(&interner, &env);
    assert!(!checker.is_assignable(source, target));
    assert!(matches!(
        checker.explain_failure(source, target),
        Some(SubtypeFailureReason::TypeMismatch { .. })
    ));
}

#[test]
fn test_weak_union_allows_overlap() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let a = interner.intern_string("a");
    let b = interner.intern_string("b");

    let weak_a = interner.object(vec![PropertyInfo::opt(a, TypeId::NUMBER)]);
    let weak_b = interner.object(vec![PropertyInfo::opt(b, TypeId::NUMBER)]);
    let target = interner.union(vec![weak_a, weak_b]);

    let source = interner.object(vec![PropertyInfo::new(a, TypeId::NUMBER)]);

    assert!(checker.is_assignable(source, target));
}

#[test]
fn test_weak_union_source_with_one_common_member_rejects() {
    // When source is a union, each source member is checked individually against the target.
    // Source: { a } | { c } where { c } has no common property with any target member.
    // Target: { a? } | { b? }
    // tsc rejects this because { c: number } has no overlap with any weak target member,
    // triggering weak type rejection for that source member.
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let a = interner.intern_string("a");
    let b = interner.intern_string("b");
    let c = interner.intern_string("c");

    // Target: union of weak types { a?: number } | { b?: number }
    let weak_a = interner.object(vec![PropertyInfo::opt(a, TypeId::NUMBER)]);
    let weak_b = interner.object(vec![PropertyInfo::opt(b, TypeId::NUMBER)]);
    let target = interner.union(vec![weak_a, weak_b]);

    // Source: union { a: number } | { c: number }
    // { a: number } has common property with target, but { c: number } does not.
    // Each source member is checked individually, so { c } fails the weak type check.
    let source_with_a = interner.object(vec![PropertyInfo::new(a, TypeId::NUMBER)]);
    let source_with_c = interner.object(vec![PropertyInfo::new(c, TypeId::NUMBER)]);
    let source = interner.union(vec![source_with_a, source_with_c]);

    // Rejected: { c: number } member lacks common property with weak union target
    assert!(
        !checker.is_assignable(source, target),
        "Union source where one member lacks common property should be rejected"
    );
}

#[test]
fn test_weak_union_source_all_members_lack_common_rejects() {
    // When source is a union, if ALL members lack common property with target, reject.
    // Source: { c } | { d } (no common properties with target)
    // Target: { a? } | { b? }
    // This should be rejected.
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let a = interner.intern_string("a");
    let b = interner.intern_string("b");
    let c = interner.intern_string("c");
    let d = interner.intern_string("d");

    // Target: union of weak types { a?: number } | { b?: number }
    let weak_a = interner.object(vec![PropertyInfo::opt(a, TypeId::NUMBER)]);
    let weak_b = interner.object(vec![PropertyInfo::opt(b, TypeId::NUMBER)]);
    let target = interner.union(vec![weak_a, weak_b]);

    // Source: union { c: number } | { d: number }
    // Neither has common property with target
    let source_with_c = interner.object(vec![PropertyInfo::new(c, TypeId::NUMBER)]);
    let source_with_d = interner.object(vec![PropertyInfo::new(d, TypeId::NUMBER)]);
    let source = interner.union(vec![source_with_c, source_with_d]);

    // Should be rejected: no member of source has common property
    assert!(
        !checker.is_assignable(source, target),
        "Union source where all members lack common property should be rejected"
    );
}

#[test]
fn test_weak_union_nested_union_source_rejects() {
    // Test with nested unions in source
    // Source: ({ a } | { c }) | { d }
    // Target: { a? } | { b? }
    // Rejected because individual source union members { c } and { d } have no
    // common property with any weak target member. Each source member is checked
    // individually against the target.
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let a = interner.intern_string("a");
    let b = interner.intern_string("b");
    let c = interner.intern_string("c");
    let d = interner.intern_string("d");

    // Target: union of weak types { a?: number } | { b?: number }
    let weak_a = interner.object(vec![PropertyInfo::opt(a, TypeId::NUMBER)]);
    let weak_b = interner.object(vec![PropertyInfo::opt(b, TypeId::NUMBER)]);
    let target = interner.union(vec![weak_a, weak_b]);

    // Source: nested union ({ a: number } | { c: number }) | { d: number }
    let source_with_a = interner.object(vec![PropertyInfo::new(a, TypeId::NUMBER)]);
    let source_with_c = interner.object(vec![PropertyInfo::new(c, TypeId::NUMBER)]);
    let source_with_d = interner.object(vec![PropertyInfo::new(d, TypeId::NUMBER)]);
    let inner_union = interner.union(vec![source_with_a, source_with_c]);
    let source = interner.union(vec![inner_union, source_with_d]);

    // Rejected: { c } and { d } members lack common property with weak union target
    assert!(
        !checker.is_assignable(source, target),
        "Nested union source where some members lack common property should be rejected"
    );
}

/// Boolean literal intrinsics (`BOOLEAN_FALSE`/`BOOLEAN_TRUE`) are reserved
/// `TypeId`s distinct from `TypeId::BOOLEAN`. The weak-type primitive check
/// must accept them as primitives so that `false`/`true` assigned to a weak
/// object type produces `NoCommonProperties` (TS2559) rather than slipping
/// through. Regression test for `is_weak_union_violation` returning false on
/// boolean literal sources against single weak object targets.
#[test]
fn test_weak_type_rejects_boolean_literal_source() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let some_prop = interner.intern_string("someProp");
    let weak_target = interner.object(vec![PropertyInfo::opt(some_prop, TypeId::STRING)]);

    for source in [TypeId::BOOLEAN_FALSE, TypeId::BOOLEAN_TRUE, TypeId::BOOLEAN] {
        assert!(
            !checker.is_assignable(source, weak_target),
            "boolean source {source:?} should not be assignable to weak object",
        );
        assert!(
            checker.is_weak_union_violation(source, weak_target),
            "is_weak_union_violation should fire for boolean source {source:?} against weak object",
        );
        assert!(
            matches!(
                checker.explain_failure(source, weak_target),
                Some(SubtypeFailureReason::NoCommonProperties { .. })
            ),
            "explain_failure should report NoCommonProperties for boolean source {source:?}",
        );
    }
}

#[test]
fn test_weak_union_with_intersection_source() {
    // Test intersection source type
    // Source: { a: number } & { c: number } (has property "a")
    // Target: { a? } | { b? }
    // Should be allowed because intersection has property "a"
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let a = interner.intern_string("a");
    let b = interner.intern_string("b");
    let c = interner.intern_string("c");

    // Target: union of weak types { a?: number } | { b?: number }
    let weak_a = interner.object(vec![PropertyInfo::opt(a, TypeId::NUMBER)]);
    let weak_b = interner.object(vec![PropertyInfo::opt(b, TypeId::NUMBER)]);
    let target = interner.union(vec![weak_a, weak_b]);

    // Source: { a: number, c: number } (as a single object with both properties)
    // This represents the intersection semantically
    let source = interner.object(vec![
        PropertyInfo::new(a, TypeId::NUMBER),
        PropertyInfo::new(c, TypeId::NUMBER),
    ]);

    // Should be assignable: source has property "a" which overlaps with target
    assert!(
        checker.is_assignable(source, target),
        "Intersection source with common property should be assignable to weak union"
    );
}

#[test]
fn test_rest_any_bivariant_even_strict() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    checker.set_strict_function_types(true);

    let rest_any = interner.array(TypeId::ANY);
    let target = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: None,
            type_id: rest_any,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let source = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(TypeId::NUMBER)],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    assert!(checker.is_assignable(source, target));
}

#[test]
fn test_rest_unknown_bivariant_even_strict() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    checker.set_strict_function_types(true);

    let rest_unknown = interner.array(TypeId::UNKNOWN);
    let target = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: None,
            type_id: rest_unknown,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let source = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(TypeId::STRING)],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    assert!(checker.is_assignable(source, target));
}

#[test]
fn test_rest_unknown_bivariant_strict_assignable() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    checker.set_strict_function_types(true);

    let rest_unknown = interner.array(TypeId::UNKNOWN);
    let target = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: None,
            type_id: rest_unknown,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let source = interner.function(FunctionShape {
        params: vec![
            ParamInfo::unnamed(TypeId::NUMBER),
            ParamInfo::unnamed(TypeId::STRING),
        ],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    assert!(checker.is_assignable_strict(source, target));
}

#[test]
fn test_rest_number_not_bivariant_even_strict() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    checker.set_strict_function_types(true);

    let rest_number = interner.array(TypeId::NUMBER);
    let target = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: None,
            type_id: rest_number,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let source = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(TypeId::STRING)],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    assert!(!checker.is_assignable(source, target));
}

#[test]
fn test_rest_unknown_vs_number_assignability_strict() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    checker.set_strict_function_types(true);

    let rest_unknown = interner.array(TypeId::UNKNOWN);
    let target_unknown = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: None,
            type_id: rest_unknown,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let rest_number = interner.array(TypeId::NUMBER);
    let target_number = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: None,
            type_id: rest_number,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let source = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(TypeId::STRING)],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    assert!(checker.is_assignable(source, target_unknown));
    assert!(!checker.is_assignable(source, target_number));
}

#[test]
fn test_rest_any_still_checks_return_type() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let rest_any = interner.array(TypeId::ANY);
    let target = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: None,
            type_id: rest_any,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let source = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(TypeId::STRING)],
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    assert!(!checker.is_assignable(source, target));
}

#[test]
fn test_explain_failure_skips_rest_unknown() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    checker.set_strict_function_types(true);

    let rest_unknown = interner.array(TypeId::UNKNOWN);
    let target = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: None,
            type_id: rest_unknown,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let source = interner.function(FunctionShape {
        params: vec![
            ParamInfo::unnamed(TypeId::NUMBER),
            ParamInfo::unnamed(TypeId::STRING),
        ],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    assert!(checker.explain_failure(source, target).is_none());
}

#[test]
fn test_explain_failure_reports_rest_mismatch() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let rest_number = interner.array(TypeId::NUMBER);
    let target = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: None,
            type_id: rest_number,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let source = interner.function(FunctionShape {
        params: vec![
            ParamInfo::unnamed(TypeId::NUMBER),
            ParamInfo::unnamed(TypeId::STRING),
        ],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let reason = checker.explain_failure(source, target);
    assert!(matches!(
        reason,
        Some(SubtypeFailureReason::ParameterTypeMismatch { .. })
    ));
    if let Some(SubtypeFailureReason::ParameterTypeMismatch {
        param_index,
        source_param,
        target_param,
        ..
    }) = reason
    {
        assert_eq!(param_index, 1);
        assert_eq!(source_param, TypeId::STRING);
        assert_eq!(target_param, TypeId::NUMBER);
    }
}

#[test]
fn test_explain_failure_reports_rest_mismatch_source_rest() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let source = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: None,
            type_id: interner.array(TypeId::STRING),
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let target = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(TypeId::NUMBER)],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let reason = checker.explain_failure(source, target);
    assert!(matches!(
        reason,
        Some(SubtypeFailureReason::ParameterTypeMismatch { .. })
    ));
    if let Some(SubtypeFailureReason::ParameterTypeMismatch {
        param_index,
        source_param,
        target_param,
        ..
    }) = reason
    {
        assert_eq!(param_index, 0);
        assert_eq!(source_param, TypeId::STRING);
        assert_eq!(target_param, TypeId::NUMBER);
    }
}

#[test]
fn test_empty_object_accepts_non_nullish() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let empty_object = interner.object(Vec::new());

    assert!(checker.is_assignable(TypeId::STRING, empty_object));
    assert!(checker.is_assignable(TypeId::NUMBER, empty_object));

    let array = interner.array(TypeId::NUMBER);
    assert!(checker.is_assignable(array, empty_object));

    let func = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    assert!(checker.is_assignable(func, empty_object));
}

#[test]
fn test_empty_object_rejects_nullish_and_unknown() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let empty_object = interner.object(Vec::new());

    assert!(!checker.is_assignable(TypeId::NULL, empty_object));
    assert!(!checker.is_assignable(TypeId::UNDEFINED, empty_object));
    assert!(!checker.is_assignable(TypeId::VOID, empty_object));
    assert!(!checker.is_assignable(TypeId::UNKNOWN, empty_object));
}

#[test]
fn test_strict_null_checks_toggle() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let empty_object = interner.object(Vec::new());
    let nullable_string = interner.union(vec![TypeId::STRING, TypeId::NULL]);

    assert!(!checker.is_assignable(TypeId::NULL, TypeId::STRING));
    assert!(!checker.is_assignable(nullable_string, TypeId::STRING));
    assert!(!checker.is_assignable(nullable_string, empty_object));

    checker.set_strict_null_checks(false);

    assert!(checker.is_assignable(TypeId::NULL, TypeId::STRING));
    assert!(checker.is_assignable(TypeId::UNDEFINED, TypeId::NUMBER));
    assert!(checker.is_assignable(nullable_string, TypeId::STRING));
    assert!(checker.is_assignable(TypeId::UNDEFINED, empty_object));
    assert!(checker.is_assignable(nullable_string, empty_object));
}

#[test]
fn test_no_unchecked_indexed_access_toggle() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let indexed = interner.object_with_index(ObjectShape {
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

    let index_access = interner.intern(TypeData::IndexAccess(indexed, TypeId::STRING));
    let number_or_undefined = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);

    assert!(checker.is_assignable(index_access, TypeId::NUMBER));

    checker.set_no_unchecked_indexed_access(true);

    assert!(!checker.is_assignable(index_access, TypeId::NUMBER));
    assert!(checker.is_assignable(index_access, number_or_undefined));
}

#[test]
fn test_no_unchecked_indexed_access_primitive_index() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let index_access = interner.intern(TypeData::IndexAccess(TypeId::STRING, TypeId::NUMBER));
    let string_or_undefined = interner.union(vec![TypeId::STRING, TypeId::UNDEFINED]);

    assert!(checker.is_assignable(index_access, TypeId::STRING));

    checker.set_no_unchecked_indexed_access(true);

    assert!(!checker.is_assignable(index_access, TypeId::STRING));
    assert!(checker.is_assignable(index_access, string_or_undefined));
}

#[test]
fn test_no_unchecked_indexed_access_array_assignable() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let index_access = interner.intern(TypeData::IndexAccess(string_array, TypeId::NUMBER));
    let string_or_undefined = interner.union(vec![TypeId::STRING, TypeId::UNDEFINED]);

    assert!(checker.is_assignable(index_access, TypeId::STRING));

    checker.set_no_unchecked_indexed_access(true);

    assert!(!checker.is_assignable(index_access, TypeId::STRING));
    assert!(checker.is_assignable(index_access, string_or_undefined));
}

#[test]
fn test_no_unchecked_object_index_signature_assignable() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let indexed = interner.object_with_index(ObjectShape {
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

    let index_access = interner.intern(TypeData::IndexAccess(indexed, TypeId::NUMBER));
    let number_or_undefined = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);

    assert!(checker.is_assignable(index_access, TypeId::NUMBER));

    checker.set_no_unchecked_indexed_access(true);

    assert!(!checker.is_assignable(index_access, TypeId::NUMBER));
    assert!(checker.is_assignable(index_access, number_or_undefined));
}

#[test]
fn test_correlated_union_index_access_assignable() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let kind = interner.intern_string("kind");
    let key_a = interner.intern_string("a");
    let key_b = interner.intern_string("b");

    let obj_a = interner.object(vec![
        PropertyInfo::new(kind, interner.literal_string("a")),
        PropertyInfo::new(key_a, TypeId::NUMBER),
    ]);
    let obj_b = interner.object(vec![
        PropertyInfo::new(kind, interner.literal_string("b")),
        PropertyInfo::new(key_b, TypeId::STRING),
    ]);

    let union_obj = interner.union(vec![obj_a, obj_b]);
    let key_union = interner.union(vec![
        interner.literal_string("a"),
        interner.literal_string("b"),
    ]);
    let index_access = interner.intern(TypeData::IndexAccess(union_obj, key_union));
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::STRING]);

    assert!(checker.is_assignable(index_access, expected));
    assert!(!checker.is_assignable(index_access, TypeId::NUMBER));
}

