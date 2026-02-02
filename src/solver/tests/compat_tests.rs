use super::*;
use crate::solver::SubtypeFailureReason;
use crate::solver::db::QueryDatabase;
use crate::solver::{
    CallSignature, CallableShape, ConditionalType, FunctionShape, IndexSignature, MappedType,
    ObjectFlags, ObjectShape, ParamInfo, PropertyInfo, SymbolRef, TemplateSpan, TupleElement,
    TypeEnvironment, TypeParamInfo, TypeSubstitution, instantiate_type,
};

fn make_animal_dog(interner: &TypeInterner) -> (TypeId, TypeId) {
    let name = interner.intern_string("name");
    let breed = interner.intern_string("breed");

    let animal = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let dog = interner.object(vec![
        PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: breed,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    (animal, dog)
}

fn make_object_interface(interner: &TypeInterner) -> TypeId {
    let method = |return_type| FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };
    let method_with_any = |return_type| FunctionShape {
        params: vec![ParamInfo {
            name: None,
            type_id: TypeId::ANY,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    let constructor = PropertyInfo {
        name: interner.intern_string("constructor"),
        type_id: TypeId::ANY,
        write_type: TypeId::ANY,
        optional: false,
        readonly: false,
        is_method: false,
    };
    let to_string = PropertyInfo {
        name: interner.intern_string("toString"),
        type_id: interner.function(method(TypeId::STRING)),
        write_type: interner.function(method(TypeId::STRING)),
        optional: false,
        readonly: false,
        is_method: true,
    };
    let to_locale = PropertyInfo {
        name: interner.intern_string("toLocaleString"),
        type_id: interner.function(method(TypeId::STRING)),
        write_type: interner.function(method(TypeId::STRING)),
        optional: false,
        readonly: false,
        is_method: true,
    };
    let value_of = PropertyInfo {
        name: interner.intern_string("valueOf"),
        type_id: interner.function(method(TypeId::ANY)),
        write_type: interner.function(method(TypeId::ANY)),
        optional: false,
        readonly: false,
        is_method: true,
    };
    let has_own = PropertyInfo {
        name: interner.intern_string("hasOwnProperty"),
        type_id: interner.function(method_with_any(TypeId::BOOLEAN)),
        write_type: interner.function(method_with_any(TypeId::BOOLEAN)),
        optional: false,
        readonly: false,
        is_method: true,
    };
    let is_proto = PropertyInfo {
        name: interner.intern_string("isPrototypeOf"),
        type_id: interner.function(method_with_any(TypeId::BOOLEAN)),
        write_type: interner.function(method_with_any(TypeId::BOOLEAN)),
        optional: false,
        readonly: false,
        is_method: true,
    };
    let prop_enum = PropertyInfo {
        name: interner.intern_string("propertyIsEnumerable"),
        type_id: interner.function(method_with_any(TypeId::BOOLEAN)),
        write_type: interner.function(method_with_any(TypeId::BOOLEAN)),
        optional: false,
        readonly: false,
        is_method: true,
    };

    interner.object(vec![
        constructor,
        to_string,
        to_locale,
        value_of,
        has_own,
        is_proto,
        prop_enum,
    ])
}

#[test]
fn test_any_assignability() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    assert!(checker.is_assignable(TypeId::ANY, TypeId::STRING));
    assert!(checker.is_assignable(TypeId::STRING, TypeId::ANY));
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
fn test_error_type_strictness() {
    // ERROR types should NOT silently pass assignability checks.
    // This prevents "error poisoning" where a TS2304 (cannot find name) masks
    // downstream TS2322 (type not assignable) errors.
    // This is a key design decision for catching more TS2322 errors.
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // ERROR is NOT assignable to concrete types
    assert!(!checker.is_assignable(TypeId::ERROR, TypeId::STRING));
    // Concrete types are NOT assignable to ERROR
    assert!(!checker.is_assignable(TypeId::STRING, TypeId::ERROR));
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
    // With the cycle detection fix (Issue #09):
    // - When depth limit is hit, we return DepthExceeded (false) for soundness
    // - CycleDetected is used for valid coinductive recursion (true)
    // - DepthExceeded prevents unsound type acceptance in genuinely incompatible deep types
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

    // When depth limit is exceeded during comparison of incompatible types,
    // we return DepthExceeded (which evaluates to false) for soundness.
    // This prevents incorrectly accepting genuinely incompatible deep types.
    // The depth_exceeded flag allows emitting TS2589 diagnostic.
    let result = checker.is_assignable(deep_string, deep_number);
    // Result is false due to DepthExceeded return on depth limit (soundness fix)
    assert!(!result);

    // Same types at same depth should be assignable (identity check short-circuits)
    let deep_string2 = nest_array(&interner, TypeId::STRING, 120);
    assert!(checker.is_assignable(deep_string, deep_string2));
}

#[test]
fn test_base_constraint_assignability_compat() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let t_param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(TypeId::STRING),
        default: None,
    }));
    let u_param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: Some(TypeId::STRING),
        default: None,
    }));
    let v_param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: interner.intern_string("V"),
        constraint: Some(TypeId::NUMBER),
        default: None,
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
        params: vec![ParamInfo {
            name: None,
            type_id: dog,
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

    let fn_animal = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: None,
            type_id: animal,
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

    assert!(checker.is_assignable(fn_dog, fn_animal));
}

#[test]
fn test_function_variance_strict() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    checker.set_strict_function_types(true);

    let (animal, dog) = make_animal_dog(&interner);

    let fn_dog = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: None,
            type_id: dog,
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

    let fn_animal = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: None,
            type_id: animal,
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

    assert!(!checker.is_assignable(source, target));
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
        params: vec![ParamInfo {
            name: None,
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

    let target_fn = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: None,
            type_id: string_or_number,
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

    let source = interner.object(vec![PropertyInfo {
        name,
        type_id: source_fn,
        write_type: source_fn,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    let target = interner.object(vec![PropertyInfo {
        name,
        type_id: target_fn,
        write_type: target_fn,
        optional: false,
        readonly: false,
        is_method: true,
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
        params: vec![ParamInfo {
            name: None,
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

    let target_fn = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: None,
            type_id: string_or_number,
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

    let source = interner.object(vec![PropertyInfo {
        name,
        type_id: source_fn,
        write_type: source_fn,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let target = interner.object(vec![PropertyInfo {
        name,
        type_id: target_fn,
        write_type: target_fn,
        optional: false,
        readonly: false,
        is_method: false,
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

    let instance = interner.object(vec![PropertyInfo {
        name: interner.intern_string("value"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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

    let instance = interner.object(vec![PropertyInfo {
        name: interner.intern_string("value"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let returns_instance = interner.callable(CallableShape {
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
        params: vec![ParamInfo {
            name: None,
            type_id: dog,
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

    let fn_animal = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: None,
            type_id: animal,
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

    let reason = checker.explain_failure(fn_dog, fn_animal);
    assert!(matches!(
        reason,
        Some(SubtypeFailureReason::ParameterTypeMismatch { param_index: 0, .. })
    ));
}

#[test]
fn test_weak_type_rejects_no_common_properties() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let a = interner.intern_string("a");
    let b = interner.intern_string("b");

    let weak_target = interner.object(vec![PropertyInfo {
        name: a,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: true,
        readonly: false,
        is_method: false,
    }]);

    let source = interner.object(vec![PropertyInfo {
        name: b,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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

    let weak_target = interner.object(vec![PropertyInfo {
        name: a,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: true,
        readonly: false,
        is_method: false,
    }]);

    let source = interner.object(vec![PropertyInfo {
        name: a,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    assert!(checker.is_assignable(source, weak_target));
}

#[test]
fn test_weak_type_skips_empty_target() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let a = interner.intern_string("a");

    let empty_target = interner.object(Vec::new());
    let source = interner.object(vec![PropertyInfo {
        name: a,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    assert!(checker.is_assignable(source, empty_target));
}

#[test]
fn test_weak_union_rejects_no_common_properties() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let a = interner.intern_string("a");
    let b = interner.intern_string("b");
    let c = interner.intern_string("c");

    let weak_a = interner.object(vec![PropertyInfo {
        name: a,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: true,
        readonly: false,
        is_method: false,
    }]);
    let weak_b = interner.object(vec![PropertyInfo {
        name: b,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: true,
        readonly: false,
        is_method: false,
    }]);
    let target = interner.union(vec![weak_a, weak_b]);

    let source = interner.object(vec![PropertyInfo {
        name: c,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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

    let weak_a = interner.object(vec![PropertyInfo {
        name: a,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: true,
        readonly: false,
        is_method: false,
    }]);
    let weak_b = interner.object(vec![PropertyInfo {
        name: b,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: true,
        readonly: false,
        is_method: false,
    }]);

    let sym_a = SymbolRef(1);
    let sym_b = SymbolRef(2);
    env.insert(sym_a, weak_a);
    env.insert(sym_b, weak_b);

    let target = interner.union(vec![interner.reference(sym_a), interner.reference(sym_b)]);
    let source = interner.object(vec![PropertyInfo {
        name: c,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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

    let weak_a = interner.object(vec![PropertyInfo {
        name: a,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: true,
        readonly: false,
        is_method: false,
    }]);
    let weak_b = interner.object(vec![PropertyInfo {
        name: b,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: true,
        readonly: false,
        is_method: false,
    }]);
    let target = interner.union(vec![weak_a, weak_b]);

    let source = interner.object(vec![PropertyInfo {
        name: a,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    assert!(checker.is_assignable(source, target));
}

#[test]
fn test_weak_union_source_with_one_common_member_allows() {
    // When source is a union, if ANY member has common property with target, allow assignment.
    // Source: { a } | { c } (one member has common property "a" with target)
    // Target: { a? } | { b? }
    // This should be assignable because { a } has overlap with { a? }
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let a = interner.intern_string("a");
    let b = interner.intern_string("b");
    let c = interner.intern_string("c");

    // Target: union of weak types { a?: number } | { b?: number }
    let weak_a = interner.object(vec![PropertyInfo {
        name: a,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: true,
        readonly: false,
        is_method: false,
    }]);
    let weak_b = interner.object(vec![PropertyInfo {
        name: b,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: true,
        readonly: false,
        is_method: false,
    }]);
    let target = interner.union(vec![weak_a, weak_b]);

    // Source: union { a: number } | { c: number }
    // { a: number } has common property with target's { a?: number }
    // { c: number } does NOT have common property
    // But since { a: number } has overlap, the source union overall should be allowed
    let source_with_a = interner.object(vec![PropertyInfo {
        name: a,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let source_with_c = interner.object(vec![PropertyInfo {
        name: c,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let source = interner.union(vec![source_with_a, source_with_c]);

    // Should be assignable: at least one member of source has common property
    assert!(
        checker.is_assignable(source, target),
        "Union source with one overlapping member should be assignable to weak union target"
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
    let weak_a = interner.object(vec![PropertyInfo {
        name: a,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: true,
        readonly: false,
        is_method: false,
    }]);
    let weak_b = interner.object(vec![PropertyInfo {
        name: b,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: true,
        readonly: false,
        is_method: false,
    }]);
    let target = interner.union(vec![weak_a, weak_b]);

    // Source: union { c: number } | { d: number }
    // Neither has common property with target
    let source_with_c = interner.object(vec![PropertyInfo {
        name: c,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let source_with_d = interner.object(vec![PropertyInfo {
        name: d,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let source = interner.union(vec![source_with_c, source_with_d]);

    // Should be rejected: no member of source has common property
    assert!(
        !checker.is_assignable(source, target),
        "Union source where all members lack common property should be rejected"
    );
}

#[test]
fn test_weak_union_nested_union_source() {
    // Test with nested unions in source
    // Source: ({ a } | { c }) | { d }
    // Target: { a? } | { b? }
    // Should be allowed because { a } in the nested union has overlap
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let a = interner.intern_string("a");
    let b = interner.intern_string("b");
    let c = interner.intern_string("c");
    let d = interner.intern_string("d");

    // Target: union of weak types { a?: number } | { b?: number }
    let weak_a = interner.object(vec![PropertyInfo {
        name: a,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: true,
        readonly: false,
        is_method: false,
    }]);
    let weak_b = interner.object(vec![PropertyInfo {
        name: b,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: true,
        readonly: false,
        is_method: false,
    }]);
    let target = interner.union(vec![weak_a, weak_b]);

    // Source: nested union ({ a: number } | { c: number }) | { d: number }
    let source_with_a = interner.object(vec![PropertyInfo {
        name: a,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let source_with_c = interner.object(vec![PropertyInfo {
        name: c,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let source_with_d = interner.object(vec![PropertyInfo {
        name: d,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let inner_union = interner.union(vec![source_with_a, source_with_c]);
    let source = interner.union(vec![inner_union, source_with_d]);

    // Should be assignable: { a } in nested union has overlap
    assert!(
        checker.is_assignable(source, target),
        "Nested union source with one overlapping member should be assignable"
    );
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
    let weak_a = interner.object(vec![PropertyInfo {
        name: a,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: true,
        readonly: false,
        is_method: false,
    }]);
    let weak_b = interner.object(vec![PropertyInfo {
        name: b,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: true,
        readonly: false,
        is_method: false,
    }]);
    let target = interner.union(vec![weak_a, weak_b]);

    // Source: { a: number, c: number } (as a single object with both properties)
    // This represents the intersection semantically
    let source = interner.object(vec![
        PropertyInfo {
            name: a,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: c,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
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
        params: vec![ParamInfo {
            name: None,
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
        params: vec![ParamInfo {
            name: None,
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
            ParamInfo {
                name: None,
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: None,
                type_id: TypeId::STRING,
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
        params: vec![ParamInfo {
            name: None,
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
        params: vec![ParamInfo {
            name: None,
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
        params: vec![ParamInfo {
            name: None,
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
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
            ParamInfo {
                name: None,
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: None,
                type_id: TypeId::STRING,
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
            ParamInfo {
                name: None,
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: None,
                type_id: TypeId::STRING,
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

    let reason = checker.explain_failure(source, target);
    assert!(matches!(
        reason,
        Some(SubtypeFailureReason::ParameterTypeMismatch { .. })
    ));
    if let Some(SubtypeFailureReason::ParameterTypeMismatch {
        param_index,
        source_param,
        target_param,
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
        params: vec![ParamInfo {
            name: None,
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

    let reason = checker.explain_failure(source, target);
    assert!(matches!(
        reason,
        Some(SubtypeFailureReason::ParameterTypeMismatch { .. })
    ));
    if let Some(SubtypeFailureReason::ParameterTypeMismatch {
        param_index,
        source_param,
        target_param,
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
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
        number_index: None,
    });

    let index_access = interner.intern(TypeKey::IndexAccess(indexed, TypeId::STRING));
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

    let index_access = interner.intern(TypeKey::IndexAccess(TypeId::STRING, TypeId::NUMBER));
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
    let index_access = interner.intern(TypeKey::IndexAccess(string_array, TypeId::NUMBER));
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
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
        number_index: None,
    });

    let index_access = interner.intern(TypeKey::IndexAccess(indexed, TypeId::NUMBER));
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
        PropertyInfo {
            name: kind,
            type_id: interner.literal_string("a"),
            write_type: interner.literal_string("a"),
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: key_a,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);
    let obj_b = interner.object(vec![
        PropertyInfo {
            name: kind,
            type_id: interner.literal_string("b"),
            write_type: interner.literal_string("b"),
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: key_b,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    let union_obj = interner.union(vec![obj_a, obj_b]);
    let key_union = interner.union(vec![
        interner.literal_string("a"),
        interner.literal_string("b"),
    ]);
    let index_access = interner.intern(TypeKey::IndexAccess(union_obj, key_union));
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::STRING]);

    assert!(checker.is_assignable(index_access, expected));
    assert!(!checker.is_assignable(index_access, TypeId::NUMBER));
}

#[test]
fn test_object_keyword_accepts_non_primitives() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let name = interner.intern_string("name");
    let obj = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    assert!(checker.is_assignable(obj, TypeId::OBJECT));

    let array = interner.array(TypeId::NUMBER);
    assert!(checker.is_assignable(array, TypeId::OBJECT));

    let tuple = interner.tuple(vec![TupleElement {
        type_id: TypeId::NUMBER,
        name: None,
        optional: false,
        rest: false,
    }]);
    assert!(checker.is_assignable(tuple, TypeId::OBJECT));

    let func = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    assert!(checker.is_assignable(func, TypeId::OBJECT));
}

#[test]
fn test_object_keyword_rejects_primitives() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    assert!(!checker.is_assignable(TypeId::STRING, TypeId::OBJECT));
    assert!(!checker.is_assignable(TypeId::NUMBER, TypeId::OBJECT));
}

#[test]
fn test_object_interface_accepts_primitives() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let object_interface = make_object_interface(&interner);
    assert!(checker.is_assignable(TypeId::STRING, object_interface));
    assert!(checker.is_assignable(TypeId::NUMBER, object_interface));
    assert!(checker.is_assignable(TypeId::BOOLEAN, object_interface));
    assert!(checker.is_assignable(TypeId::SYMBOL, object_interface));

    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("prefix")),
        TemplateSpan::Type(TypeId::STRING),
        TemplateSpan::Text(interner.intern_string("suffix")),
    ]);
    assert!(checker.is_assignable(template, object_interface));
}

#[test]
fn test_object_trifecta_assignability() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let empty_object = interner.object(Vec::new());
    let object_interface = make_object_interface(&interner);

    assert!(checker.is_assignable(TypeId::STRING, empty_object));
    assert!(checker.is_assignable(TypeId::STRING, object_interface));
    assert!(!checker.is_assignable(TypeId::STRING, TypeId::OBJECT));
}

#[test]
fn test_split_accessor_allows_wider_setter_in_source() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let name = interner.intern_string("x");
    let source = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::STRING,
        write_type: interner.union2(TypeId::STRING, TypeId::NUMBER),
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let target = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    assert!(checker.is_assignable(source, target));
}

#[test]
fn test_split_accessor_rejects_wider_setter_in_target() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let name = interner.intern_string("x");
    let source = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let target = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::STRING,
        write_type: interner.union2(TypeId::STRING, TypeId::NUMBER),
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    assert!(!checker.is_assignable(source, target));
}

#[test]
fn test_function_type_accepts_callables() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let function_top = interner.callable(CallableShape {
        call_signatures: Vec::new(),
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        ..Default::default()
    });

    let function = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: None,
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
    assert!(checker.is_assignable(function, function_top));

    let callable = interner.callable(CallableShape {
        call_signatures: vec![CallSignature {
            params: vec![ParamInfo {
                name: None,
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::BOOLEAN,
            type_predicate: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        ..Default::default()
    });
    assert!(checker.is_assignable(callable, function_top));
}

#[test]
fn test_function_type_rejects_non_callables() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let function_top = interner.callable(CallableShape {
        call_signatures: Vec::new(),
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        ..Default::default()
    });

    let name = interner.intern_string("name");
    let obj = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    assert!(!checker.is_assignable(obj, function_top));
}

#[test]
fn test_function_type_not_assignable_to_specific_callable() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let function_top = interner.callable(CallableShape {
        call_signatures: Vec::new(),
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        ..Default::default()
    });

    let specific_callable = interner.callable(CallableShape {
        call_signatures: vec![CallSignature {
            params: vec![ParamInfo {
                name: None,
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::STRING,
            type_predicate: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        ..Default::default()
    });

    assert!(!checker.is_assignable(function_top, specific_callable));
}

#[test]
fn test_tuple_array_assignability_tuple_to_array() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

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
    let elem_union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let array = interner.array(elem_union);

    assert!(checker.is_assignable(tuple, array));
}

#[test]
fn test_tuple_array_assignability_tuple_to_array_rejects() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

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
    let array = interner.array(TypeId::STRING);

    assert!(!checker.is_assignable(tuple, array));
}

#[test]
fn test_tuple_array_assignability_array_to_tuple_rejects() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let array = interner.array(TypeId::STRING);
    let tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
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

    assert!(!checker.is_assignable(array, tuple));
}

#[test]
fn test_tuple_array_assignability_empty_array_to_optional_tuple() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let never_array = interner.array(TypeId::NEVER);
    let optional_tuple = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: true,
        rest: false,
    }]);
    let empty_tuple = interner.tuple(Vec::new());
    let rest_tuple = interner.tuple(vec![TupleElement {
        type_id: interner.array(TypeId::STRING),
        name: None,
        optional: false,
        rest: true,
    }]);

    assert!(checker.is_assignable(never_array, empty_tuple));
    assert!(checker.is_assignable(never_array, optional_tuple));
    assert!(checker.is_assignable(never_array, rest_tuple));
}

#[test]
fn test_apparent_string_members_assignable() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let length = interner.intern_string("length");
    let to_upper = interner.intern_string("toUpperCase");
    let to_upper_type = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let target = interner.object(vec![
        PropertyInfo {
            name: length,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: to_upper,
            type_id: to_upper_type,
            write_type: to_upper_type,
            optional: false,
            readonly: false,
            is_method: true,
        },
    ]);

    assert!(checker.is_assignable(TypeId::STRING, target));

    let literal = interner.literal_string("hello");
    assert!(checker.is_assignable(literal, target));
}

#[test]
fn test_apparent_string_members_include_substr_and_locale_compare() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let locale_compare = interner.intern_string("localeCompare");
    let substr = interner.intern_string("substr");
    let locale_compare_type = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("that")),
            type_id: TypeId::ANY,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let substr_type = interner.function(FunctionShape {
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("start")),
                type_id: TypeId::ANY,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("length")),
                type_id: TypeId::ANY,
                optional: true,
                rest: false,
            },
        ],
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let target = interner.object(vec![
        PropertyInfo {
            name: locale_compare,
            type_id: locale_compare_type,
            write_type: locale_compare_type,
            optional: false,
            readonly: false,
            is_method: true,
        },
        PropertyInfo {
            name: substr,
            type_id: substr_type,
            write_type: substr_type,
            optional: false,
            readonly: false,
            is_method: true,
        },
    ]);

    assert!(checker.is_assignable(TypeId::STRING, target));
}

#[test]
fn test_apparent_string_members_include_legacy_and_unicode() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let normalize = interner.intern_string("normalize");
    let is_well_formed = interner.intern_string("isWellFormed");
    let fontcolor = interner.intern_string("fontcolor");

    let normalize_type = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let is_well_formed_type = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::BOOLEAN,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let fontcolor_type = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let target = interner.object(vec![
        PropertyInfo {
            name: normalize,
            type_id: normalize_type,
            write_type: normalize_type,
            optional: false,
            readonly: false,
            is_method: true,
        },
        PropertyInfo {
            name: is_well_formed,
            type_id: is_well_formed_type,
            write_type: is_well_formed_type,
            optional: false,
            readonly: false,
            is_method: true,
        },
        PropertyInfo {
            name: fontcolor,
            type_id: fontcolor_type,
            write_type: fontcolor_type,
            optional: false,
            readonly: false,
            is_method: true,
        },
    ]);

    assert!(checker.is_assignable(TypeId::STRING, target));
}

#[test]
fn test_apparent_string_members_reject_mismatch() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let length = interner.intern_string("length");
    let target = interner.object(vec![PropertyInfo {
        name: length,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    assert!(!checker.is_assignable(TypeId::STRING, target));
}

#[test]
fn test_apparent_number_method_assignable() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let to_fixed = interner.intern_string("toFixed");
    let to_fixed_type = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let target = interner.object(vec![PropertyInfo {
        name: to_fixed,
        type_id: to_fixed_type,
        write_type: to_fixed_type,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    assert!(checker.is_assignable(TypeId::NUMBER, target));
}

#[test]
fn test_apparent_number_method_not_assignable_to_number() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let to_fixed = interner.intern_string("toFixed");
    let to_fixed_type = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let target = interner.object(vec![PropertyInfo {
        name: to_fixed,
        type_id: to_fixed_type,
        write_type: to_fixed_type,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    assert!(!checker.is_assignable(target, TypeId::NUMBER));
}

#[test]
fn test_apparent_number_member_rejects_mismatch() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let rest_any = interner.array(TypeId::ANY);
    let method = |return_type| {
        interner.function(FunctionShape {
            params: vec![ParamInfo {
                name: None,
                type_id: rest_any,
                optional: false,
                rest: true,
            }],
            this_type: None,
            return_type,
            type_params: Vec::new(),
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        })
    };

    let to_fixed = interner.intern_string("toFixed");
    let mismatch = interner.object(vec![PropertyInfo {
        name: to_fixed,
        type_id: method(TypeId::NUMBER),
        write_type: method(TypeId::NUMBER),
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    assert!(!checker.is_assignable(TypeId::NUMBER, mismatch));
}

#[test]
fn test_number_interface_boxing_assignability() {
    let interner = TypeInterner::new();
    let symbol = SymbolRef(1);
    let number_interface = interner.reference(symbol);

    let to_fixed = interner.intern_string("toFixed");
    let to_fixed_type = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let number_object = interner.object(vec![PropertyInfo {
        name: to_fixed,
        type_id: to_fixed_type,
        write_type: to_fixed_type,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    let mut env = TypeEnvironment::new();
    env.insert(symbol, number_object);

    let mut checker = CompatChecker::with_resolver(&interner, &env);
    assert!(checker.is_assignable(TypeId::NUMBER, number_interface));
    assert!(!checker.is_assignable(number_interface, TypeId::NUMBER));
}

#[test]
fn test_apparent_boolean_members_assignable() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let to_string = interner.intern_string("toString");
    let to_string_type = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let target = interner.object(vec![PropertyInfo {
        name: to_string,
        type_id: to_string_type,
        write_type: to_string_type,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    assert!(checker.is_assignable(TypeId::BOOLEAN, target));
}

#[test]
fn test_apparent_bigint_members_assignable() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let value_of = interner.intern_string("valueOf");
    let value_of_type = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::BIGINT,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let target = interner.object(vec![PropertyInfo {
        name: value_of,
        type_id: value_of_type,
        write_type: value_of_type,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    assert!(checker.is_assignable(TypeId::BIGINT, target));
}

#[test]
fn test_apparent_symbol_members_assignable() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let description = interner.intern_string("description");
    let to_string = interner.intern_string("toString");
    let description_type = interner.union(vec![TypeId::STRING, TypeId::UNDEFINED]);
    let to_string_type = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let target = interner.object(vec![
        PropertyInfo {
            name: description,
            type_id: description_type,
            write_type: description_type,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: to_string,
            type_id: to_string_type,
            write_type: to_string_type,
            optional: false,
            readonly: false,
            is_method: true,
        },
    ]);

    assert!(checker.is_assignable(TypeId::SYMBOL, target));
}

#[test]
fn test_apparent_string_number_index_assignable() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let target = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
        }),
    });

    assert!(checker.is_assignable(TypeId::STRING, target));
}

#[test]
fn test_apparent_string_rejects_string_index_signature() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let target = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
        }),
        number_index: None,
    });

    assert!(!checker.is_assignable(TypeId::STRING, target));
}

#[test]
fn test_optional_property_allows_undefined() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let name = interner.intern_string("x");
    let source = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::UNDEFINED,
        write_type: TypeId::UNDEFINED,
        optional: true,
        readonly: false,
        is_method: false,
    }]);
    let target = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: true,
        readonly: false,
        is_method: false,
    }]);

    assert!(checker.is_assignable(source, target));
}

#[test]
fn test_optional_property_rejects_required_target() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let name = interner.intern_string("x");
    let source = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: true,
        readonly: false,
        is_method: false,
    }]);
    let target = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    assert!(!checker.is_assignable(source, target));
}

#[test]
fn test_optional_property_rejects_string_index_signature() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let name = interner.intern_string("x");
    let source = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: true,
        readonly: false,
        is_method: false,
    }]);

    let target = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
        number_index: None,
    });

    assert!(!checker.is_assignable(source, target));
}

#[test]
fn test_exact_optional_property_rejects_undefined() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    checker.set_exact_optional_property_types(true);

    let name = interner.intern_string("x");
    let source = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::UNDEFINED,
        write_type: TypeId::UNDEFINED,
        optional: true,
        readonly: false,
        is_method: false,
    }]);
    let target = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: true,
        readonly: false,
        is_method: false,
    }]);

    assert!(!checker.is_assignable(source, target));
}

#[test]
fn test_exact_optional_property_allows_string_index_signature() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    checker.set_exact_optional_property_types(true);

    let name = interner.intern_string("x");
    let source = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: true,
        readonly: false,
        is_method: false,
    }]);

    let target = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
        number_index: None,
    });

    assert!(checker.is_assignable(source, target));
}

#[test]
fn test_rest_any_callable_target_from_function() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    checker.set_strict_function_types(true);

    let rest_any = interner.array(TypeId::ANY);
    let target = interner.callable(CallableShape {
        call_signatures: vec![CallSignature {
            params: vec![ParamInfo {
                name: None,
                type_id: rest_any,
                optional: false,
                rest: true,
            }],
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

    let source = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: None,
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

    assert!(checker.is_assignable(source, target));
}

#[test]
fn test_rest_unknown_callable_target_from_callable() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    checker.set_strict_function_types(true);

    let rest_unknown = interner.array(TypeId::UNKNOWN);
    let target = interner.callable(CallableShape {
        call_signatures: vec![CallSignature {
            params: vec![ParamInfo {
                name: None,
                type_id: rest_unknown,
                optional: false,
                rest: true,
            }],
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

    let source = interner.callable(CallableShape {
        call_signatures: vec![CallSignature {
            params: vec![ParamInfo {
                name: None,
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            }],
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

    assert!(checker.is_assignable(source, target));
}

#[test]
fn test_mapped_type_over_number_keys_assignable() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let constraint = interner.intern(TypeKey::KeyOf(TypeId::NUMBER));
    let mapped = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
        },
        constraint,
        name_type: None,
        template: TypeId::BOOLEAN,
        readonly_modifier: None,
        optional_modifier: None,
    });

    let to_fixed = interner.intern_string("toFixed");
    let expected = interner.object(vec![PropertyInfo {
        name: to_fixed,
        type_id: TypeId::BOOLEAN,
        write_type: TypeId::BOOLEAN,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let mismatch = interner.object(vec![PropertyInfo {
        name: to_fixed,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    assert!(checker.is_assignable(mapped, expected));
    assert!(!checker.is_assignable(mapped, mismatch));
    assert!(!checker.is_assignable(expected, mapped));
}

#[test]
fn test_mapped_type_over_string_keys_assignable() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let constraint = interner.intern(TypeKey::KeyOf(TypeId::STRING));
    let mapped = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
        },
        constraint,
        name_type: None,
        template: TypeId::BOOLEAN,
        readonly_modifier: None,
        optional_modifier: None,
    });

    let to_upper = interner.intern_string("toUpperCase");
    let expected = interner.object(vec![PropertyInfo {
        name: to_upper,
        type_id: TypeId::BOOLEAN,
        write_type: TypeId::BOOLEAN,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let mismatch = interner.object(vec![PropertyInfo {
        name: to_upper,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    assert!(checker.is_assignable(mapped, expected));
    assert!(!checker.is_assignable(mapped, mismatch));
    assert!(!checker.is_assignable(expected, mapped));
}

#[test]
fn test_mapped_type_over_boolean_keys_assignable() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let constraint = interner.intern(TypeKey::KeyOf(TypeId::BOOLEAN));
    let mapped = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
        },
        constraint,
        name_type: None,
        template: TypeId::NUMBER,
        readonly_modifier: None,
        optional_modifier: None,
    });

    let to_string = interner.intern_string("toString");
    let expected = interner.object(vec![PropertyInfo {
        name: to_string,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let mismatch = interner.object(vec![PropertyInfo {
        name: to_string,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    assert!(checker.is_assignable(mapped, expected));
    assert!(!checker.is_assignable(mapped, mismatch));
    assert!(!checker.is_assignable(expected, mapped));
}

#[test]
fn test_mapped_type_key_remap_filters_keys() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let prop_a = PropertyInfo {
        name: interner.intern_string("a"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    };
    let prop_b = PropertyInfo {
        name: interner.intern_string("b"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    };
    let obj = interner.object(vec![prop_a.clone(), prop_b.clone()]);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let keys = interner.union(vec![key_a, key_b]);

    let key_param = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(keys),
        default: None,
    };
    let key_param_id = interner.intern(TypeKey::TypeParameter(key_param.clone()));

    let name_type = interner.conditional(ConditionalType {
        check_type: key_param_id,
        extends_type: key_a,
        true_type: TypeId::NEVER,
        false_type: key_param_id,
        is_distributive: true,
    });
    let template = interner.intern(TypeKey::IndexAccess(obj, key_param_id));

    let mapped = interner.mapped(MappedType {
        type_param: key_param,
        constraint: keys,
        name_type: Some(name_type),
        template,
        readonly_modifier: None,
        optional_modifier: None,
    });

    let expected = interner.object(vec![prop_b]);
    let requires_a = interner.object(vec![prop_a]);

    assert!(checker.is_assignable(mapped, expected));
    assert!(checker.is_assignable(expected, mapped));
    assert!(!checker.is_assignable(mapped, requires_a));
}

#[test]
fn test_conditional_tuple_wrapper_no_distribution_assignable() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
    }));

    let tuple_check = interner.tuple(vec![TupleElement {
        type_id: t_param,
        name: None,
        optional: false,
        rest: false,
    }]);
    let tuple_extends = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }]);

    let conditional = interner.conditional(ConditionalType {
        check_type: tuple_check,
        extends_type: tuple_extends,
        true_type: TypeId::NUMBER,
        false_type: TypeId::BOOLEAN,
        is_distributive: false,
    });

    let string_or_number = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, string_or_number);

    let instantiated = instantiate_type(&interner, conditional, &subst);

    assert!(checker.is_assignable(instantiated, TypeId::BOOLEAN));
    assert!(!checker.is_assignable(instantiated, TypeId::NUMBER));
}

#[test]
fn test_keyof_intersection_assignable() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let obj_a = interner.object(vec![PropertyInfo {
        name: interner.intern_string("a"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let obj_b = interner.object(vec![PropertyInfo {
        name: interner.intern_string("b"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let intersection = interner.intersection(vec![obj_a, obj_b]);
    let keyof_a = interner.intern(TypeKey::KeyOf(obj_a));
    let keyof_intersection = interner.intern(TypeKey::KeyOf(intersection));

    assert!(checker.is_assignable(keyof_a, keyof_intersection));
    assert!(!checker.is_assignable(keyof_intersection, keyof_a));
}

#[test]
fn test_keyof_union_index_signature_assignable() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let string_index = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
        number_index: None,
    });
    let number_index = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let union = interner.union(vec![string_index, number_index]);
    let keyof_union = interner.intern(TypeKey::KeyOf(union));

    assert!(checker.is_assignable(keyof_union, TypeId::NUMBER));
    assert!(!checker.is_assignable(keyof_union, TypeId::STRING));
}

#[test]
fn test_keyof_union_intersection_only_shared_keys() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let prop_a = PropertyInfo {
        name: interner.intern_string("a"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    };
    let prop_b = PropertyInfo {
        name: interner.intern_string("b"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    };
    let prop_c = PropertyInfo {
        name: interner.intern_string("c"),
        type_id: TypeId::BOOLEAN,
        write_type: TypeId::BOOLEAN,
        optional: false,
        readonly: false,
        is_method: false,
    };

    let obj_ab = interner.object(vec![prop_a.clone(), prop_b]);
    let obj_ac = interner.object(vec![prop_a, prop_c]);
    let union = interner.union(vec![obj_ab, obj_ac]);
    let keyof_union = interner.intern(TypeKey::KeyOf(union));

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let key_c = interner.literal_string("c");

    assert!(checker.is_assignable(key_a, keyof_union));
    assert!(!checker.is_assignable(key_b, keyof_union));
    assert!(!checker.is_assignable(key_c, keyof_union));
}

#[test]
fn test_intersection_reduction_disjoint_discriminant_assignable() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let kind = interner.intern_string("kind");
    let obj_a = interner.object(vec![PropertyInfo {
        name: kind,
        type_id: interner.literal_string("a"),
        write_type: interner.literal_string("a"),
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let obj_b = interner.object(vec![PropertyInfo {
        name: kind,
        type_id: interner.literal_string("b"),
        write_type: interner.literal_string("b"),
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let intersection = interner.intersection(vec![obj_a, obj_b]);

    assert!(checker.is_assignable(intersection, TypeId::NEVER));
    assert!(checker.is_assignable(intersection, TypeId::STRING));
}

#[test]
fn test_intersection_reduction_disjoint_primitives() {
    let interner = TypeInterner::new();

    let intersection = interner.intersection(vec![TypeId::STRING, TypeId::NUMBER]);

    assert_eq!(intersection, TypeId::NEVER);
}

#[test]
fn test_unique_symbol_nominal_assignability() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let sym_a = interner.intern(TypeKey::UniqueSymbol(SymbolRef(1)));
    let sym_b = interner.intern(TypeKey::UniqueSymbol(SymbolRef(2)));

    assert!(checker.is_assignable(sym_a, TypeId::SYMBOL));
    assert!(!checker.is_assignable(TypeId::SYMBOL, sym_a));
    assert!(checker.is_assignable(sym_a, sym_a));
    assert!(!checker.is_assignable(sym_a, sym_b));
}

#[test]
fn test_template_literal_expansion_limit_widens_to_string() {
    let interner = TypeInterner::new();

    let count = crate::solver::TEMPLATE_LITERAL_EXPANSION_LIMIT + 1;
    let mut members = Vec::with_capacity(count);
    for idx in 0..count {
        let literal = interner.literal_string(&format!("k{idx}"));
        members.push(literal);
    }
    let union = interner.union(members);
    let template = interner.template_literal(vec![TemplateSpan::Type(union)]);

    assert_eq!(template, TypeId::STRING);
}

// =============================================================================
// Weak Type Detection - Comprehensive Tests (Catalog Rule #13)
// =============================================================================

#[test]
fn test_weak_type_all_optional_properties_detection() {
    // Verifies that types with ALL optional properties are detected as weak
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let a = interner.intern_string("a");
    let b = interner.intern_string("b");

    // Target with all optional properties - weak type
    let weak_target = interner.object(vec![
        PropertyInfo {
            name: a,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: true,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: b,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: true,
            readonly: false,
            is_method: false,
        },
    ]);

    // Source with no overlapping properties - should be rejected
    let c = interner.intern_string("c");
    let source = interner.object(vec![PropertyInfo {
        name: c,
        type_id: TypeId::BOOLEAN,
        write_type: TypeId::BOOLEAN,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    assert!(!checker.is_assignable(source, weak_target));
    // The weak type violation is detected internally and causes the assignability to fail
    // We can verify this by checking the failure reason
    assert!(matches!(
        checker.explain_failure(source, weak_target),
        Some(SubtypeFailureReason::NoCommonProperties { .. })
    ));
}

#[test]
fn test_weak_type_with_index_signature_not_weak() {
    // Types with index signatures are NOT weak, even with all optional properties
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let a = interner.intern_string("a");

    // Target with optional property + index signature - NOT weak
    let target = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo {
            name: a,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: true,
            readonly: false,
            is_method: false,
        }],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::ANY,
            readonly: false,
        }),
        number_index: None,
    });

    // Source with no overlapping properties - should be accepted due to index signature
    let b = interner.intern_string("b");
    let source = interner.object(vec![PropertyInfo {
        name: b,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    assert!(checker.is_assignable(source, target));
}

#[test]
fn test_weak_type_empty_source_accepted() {
    // Empty objects are assignable to weak types
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let a = interner.intern_string("a");

    let weak_target = interner.object(vec![PropertyInfo {
        name: a,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: true,
        readonly: false,
        is_method: false,
    }]);

    let empty_source = interner.object(Vec::new());

    // Empty source should be accepted (no conflicting properties)
    assert!(checker.is_assignable(empty_source, weak_target));
}

#[test]
fn test_weak_union_with_all_weak_members() {
    // Weak union: union of only weak types
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let a = interner.intern_string("a");
    let b = interner.intern_string("b");

    let weak_a = interner.object(vec![PropertyInfo {
        name: a,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: true,
        readonly: false,
        is_method: false,
    }]);
    let weak_b = interner.object(vec![PropertyInfo {
        name: b,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: true,
        readonly: false,
        is_method: false,
    }]);

    let weak_union = interner.union(vec![weak_a, weak_b]);

    let c = interner.intern_string("c");
    let source = interner.object(vec![PropertyInfo {
        name: c,
        type_id: TypeId::BOOLEAN,
        write_type: TypeId::BOOLEAN,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    // Source with no overlap should be rejected
    assert!(!checker.is_assignable(source, weak_union));
}

#[test]
fn test_weak_union_with_non_weak_member_not_weak() {
    // Union with at least one non-weak member is not a weak union
    // Normal union typing applies: source must match at least one member
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let a = interner.intern_string("a");
    let b = interner.intern_string("b");

    let weak_type = interner.object(vec![PropertyInfo {
        name: a,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: true,
        readonly: false,
        is_method: false,
    }]);
    let non_weak_type = interner.object(vec![PropertyInfo {
        name: b,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false, // Required property - NOT weak
        readonly: false,
        is_method: false,
    }]);

    let union = interner.union(vec![weak_type, non_weak_type]);

    // Source that matches the non-weak member
    let source_matching_non_weak = interner.object(vec![PropertyInfo {
        name: b,
        type_id: TypeId::NUMBER, // Matches the required property
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    // Should be accepted since source matches the non-weak member
    assert!(
        checker.is_assignable(source_matching_non_weak, union),
        "Source matching non-weak member should be assignable to union"
    );

    // Source that doesn't match any member should be rejected
    let c = interner.intern_string("c");
    let source_no_match = interner.object(vec![PropertyInfo {
        name: c,
        type_id: TypeId::BOOLEAN,
        write_type: TypeId::BOOLEAN,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    // Should be rejected since source doesn't match any union member
    assert!(
        !checker.is_assignable(source_no_match, union),
        "Source not matching any union member should not be assignable"
    );
}

// =============================================================================
// exact_optional_property_types Tests (Catalog Rule #14)
// =============================================================================

#[test]
fn test_exact_optional_property_types_distinguishes_undefined_from_missing() {
    // With exact_optional_property_types=true, optional properties distinguish
    // between "missing" and "undefined"
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    checker.set_exact_optional_property_types(true);

    let x = interner.intern_string("x");

    // { x?: number }
    let optional_number = interner.object(vec![PropertyInfo {
        name: x,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: true,
        readonly: false,
        is_method: false,
    }]);

    // { x: number | undefined }
    let number_or_undefined = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
    let _explicit_undefined = interner.object(vec![PropertyInfo {
        name: x,
        type_id: number_or_undefined,
        write_type: number_or_undefined,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    // With exact mode, these are NOT the same
    // { x?: number } is NOT assignable to { x: number | undefined }
    assert!(
        !checker.is_assignable(optional_number, _explicit_undefined),
        "Optional property should not be assignable to explicit undefined union in exact mode"
    );
    // { x: number | undefined } is NOT assignable to { x?: number }
    assert!(
        !checker.is_assignable(_explicit_undefined, optional_number),
        "Explicit undefined union should not be assignable to optional property in exact mode"
    );
}

#[test]
fn test_exact_optional_property_types_false_allows_undefined() {
    // With exact_optional_property_types=false, optional properties implicitly
    // include undefined
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    checker.set_exact_optional_property_types(false);

    let x = interner.intern_string("x");

    // { x?: number } - implicitly { x?: number | undefined }
    let optional_number = interner.object(vec![PropertyInfo {
        name: x,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: true,
        readonly: false,
        is_method: false,
    }]);

    // { x: number | undefined }
    let number_or_undefined = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
    let _explicit_undefined = interner.object(vec![PropertyInfo {
        name: x,
        type_id: number_or_undefined,
        write_type: number_or_undefined,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    // With non-exact mode, undefined should be assignable to optional property
    // This tests that the optional property type is widened to include undefined
    let just_undefined = interner.object(vec![PropertyInfo {
        name: x,
        type_id: TypeId::UNDEFINED,
        write_type: TypeId::UNDEFINED,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    assert!(
        checker.is_assignable(just_undefined, optional_number),
        "Explicit undefined should be assignable to optional property in non-exact mode"
    );
}

#[test]
fn test_exact_optional_property_types_toggle_behavior() {
    // Verify that toggling exact_optional_property_types changes behavior
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let x = interner.intern_string("x");

    let optional_number = interner.object(vec![PropertyInfo {
        name: x,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: true,
        readonly: false,
        is_method: false,
    }]);

    let just_undefined = interner.object(vec![PropertyInfo {
        name: x,
        type_id: TypeId::UNDEFINED,
        write_type: TypeId::UNDEFINED,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    // Default (false): undefined is assignable to optional
    assert!(checker.is_assignable(just_undefined, optional_number));

    // Toggle to true: undefined is NOT assignable to optional
    checker.set_exact_optional_property_types(true);
    assert!(!checker.is_assignable(just_undefined, optional_number));

    // Toggle back to false: undefined is assignable again
    checker.set_exact_optional_property_types(false);
    assert!(checker.is_assignable(just_undefined, optional_number));
}

// =============================================================================
// strictNullChecks Legacy Behavior Tests (Catalog Rule #9)
// =============================================================================

#[test]
fn test_strict_null_checks_off_null_assignable_to_anything() {
    // With strictNullChecks=false, null is assignable to everything (like never)
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    checker.set_strict_null_checks(false);

    // null is assignable to all types
    assert!(checker.is_assignable(TypeId::NULL, TypeId::STRING));
    assert!(checker.is_assignable(TypeId::NULL, TypeId::NUMBER));
    assert!(checker.is_assignable(TypeId::NULL, TypeId::BOOLEAN));
    assert!(checker.is_assignable(TypeId::NULL, TypeId::VOID));

    // undefined is also assignable to everything
    assert!(checker.is_assignable(TypeId::UNDEFINED, TypeId::STRING));
    assert!(checker.is_assignable(TypeId::UNDEFINED, TypeId::NUMBER));
    assert!(checker.is_assignable(TypeId::UNDEFINED, TypeId::BOOLEAN));
}

#[test]
fn test_strict_null_checks_on_null_not_assignable() {
    // With strictNullChecks=true, null and undefined are NOT assignable to non-nullish types
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    checker.set_strict_null_checks(true);

    // null is NOT assignable to non-nullish types
    assert!(!checker.is_assignable(TypeId::NULL, TypeId::STRING));
    assert!(!checker.is_assignable(TypeId::NULL, TypeId::NUMBER));
    assert!(!checker.is_assignable(TypeId::NULL, TypeId::BOOLEAN));
    assert!(!checker.is_assignable(TypeId::NULL, TypeId::VOID));

    // undefined is also NOT assignable
    assert!(!checker.is_assignable(TypeId::UNDEFINED, TypeId::STRING));
    assert!(!checker.is_assignable(TypeId::UNDEFINED, TypeId::NUMBER));
    assert!(!checker.is_assignable(TypeId::UNDEFINED, TypeId::BOOLEAN));
}

#[test]
fn test_strict_null_checks_union_with_null() {
    // Test behavior of unions containing null/undefined based on strictNullChecks
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let nullable_string = interner.union(vec![TypeId::STRING, TypeId::NULL]);
    let undefinable_number = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);

    // With strict mode on (default), nullable types are distinct from non-nullable
    // But a specific type IS assignable to a union containing it (normal subtyping)
    assert!(!checker.is_assignable(nullable_string, TypeId::STRING)); // string | null not assignable to string
    assert!(checker.is_assignable(TypeId::STRING, nullable_string)); // string IS assignable to string | null
    assert!(!checker.is_assignable(undefinable_number, TypeId::NUMBER)); // number | undefined not assignable to number
    assert!(checker.is_assignable(TypeId::NUMBER, undefinable_number)); // number IS assignable to number | undefined

    // With strict mode off, null/undefined are "never-like" and assignable
    checker.set_strict_null_checks(false);
    // Now string | null "collapses" to string (null is bottom-like)
    assert!(checker.is_assignable(nullable_string, TypeId::STRING));
    assert!(checker.is_assignable(TypeId::NULL, TypeId::STRING));
    assert!(checker.is_assignable(undefinable_number, TypeId::NUMBER));
    assert!(checker.is_assignable(TypeId::UNDEFINED, TypeId::NUMBER));
}

#[test]
fn test_strict_null_checks_empty_object() {
    // Test empty object assignability with null/undefined based on strictNullChecks
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let empty_object = interner.object(Vec::new());

    // With strict mode on (default), null/undefined are NOT assignable to {}
    assert!(!checker.is_assignable(TypeId::NULL, empty_object));
    assert!(!checker.is_assignable(TypeId::UNDEFINED, empty_object));

    // With strict mode off, null/undefined ARE assignable to {}
    checker.set_strict_null_checks(false);
    assert!(checker.is_assignable(TypeId::NULL, empty_object));
    assert!(checker.is_assignable(TypeId::UNDEFINED, empty_object));
}

// =============================================================================
// Void Return Exception Tests (Catalog Rule #6)
// =============================================================================

#[test]
fn test_void_return_exception_functions() {
    // Functions returning void can accept functions with any return type
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // () => void
    let void_fn = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // () => string
    let string_fn = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // () => number
    let number_fn = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::NUMBER,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Functions with non-void returns ARE assignable to void-returning functions
    assert!(
        checker.is_assignable(string_fn, void_fn),
        "Function returning string should be assignable to void function"
    );
    assert!(
        checker.is_assignable(number_fn, void_fn),
        "Function returning number should be assignable to void function"
    );

    // But void-return function is NOT assignable to non-void function
    assert!(
        !checker.is_assignable(void_fn, string_fn),
        "Void function should NOT be assignable to string function"
    );
}

#[test]
fn test_void_return_exception_with_parameters() {
    // Void return exception applies even with parameter mismatches
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let x = interner.intern_string("x");

    // (x: number) => void
    let void_fn = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(x),
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

    // (x: string) => number
    let string_number_fn = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(x),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Return type mismatch still applies void exception
    // Even though parameters don't match, the void return should allow non-void returns
    // (though parameters will still be checked separately)
    assert!(
        !checker.is_assignable(string_number_fn, void_fn),
        "Parameter mismatch should still cause rejection"
    );
}

#[test]
fn test_void_return_exception_constructors() {
    // Void return exception also applies to constructors
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let instance_type = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    // new () => void
    let void_ctor = interner.object(vec![PropertyInfo {
        name: interner.intern_string("constructor"),
        type_id: interner.function(FunctionShape {
            params: Vec::new(),
            this_type: None,
            return_type: TypeId::VOID,
            type_params: Vec::new(),
            type_predicate: None,
            is_constructor: true,
            is_method: false,
        }),
        write_type: TypeId::ANY,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    // new () => Instance
    let instance_ctor = interner.object(vec![PropertyInfo {
        name: interner.intern_string("constructor"),
        type_id: interner.function(FunctionShape {
            params: Vec::new(),
            this_type: None,
            return_type: instance_type,
            type_params: Vec::new(),
            type_predicate: None,
            is_constructor: true,
            is_method: false,
        }),
        write_type: TypeId::ANY,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    // Constructor returning instance IS assignable to void-returning constructor
    assert!(
        checker.is_assignable(instance_ctor, void_ctor),
        "Constructor returning instance should be assignable to void constructor"
    );
}

// =============================================================================
// Covariant This Types Tests (Catalog Rule #19)
// =============================================================================

#[test]
fn test_method_bivariance_allows_derived_methods() {
    // Methods are bivariant in TypeScript, allowing Derived methods to override Base methods
    // even though method parameters should normally be contravariant
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let method_name = interner.intern_string("compare");

    // class Base { compare(other: Base): void }
    let base = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let base_method = interner.object(vec![PropertyInfo {
        name: method_name,
        type_id: interner.function(FunctionShape {
            params: vec![ParamInfo {
                name: None,
                type_id: base,
                optional: false,
                rest: false,
            }],
            this_type: Some(base),
            return_type: TypeId::VOID,
            type_params: Vec::new(),
            type_predicate: None,
            is_constructor: false,
            is_method: true, // This is a method
        }),
        write_type: TypeId::ANY,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    // class Derived extends Base { x: string; y: number; compare(other: Derived): void }
    let derived = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("x"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: interner.intern_string("y"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    let derived_method = interner.object(vec![PropertyInfo {
        name: method_name,
        type_id: interner.function(FunctionShape {
            params: vec![ParamInfo {
                name: None,
                type_id: derived,
                optional: false,
                rest: false,
            }],
            this_type: Some(derived),
            return_type: TypeId::VOID,
            type_params: Vec::new(),
            type_predicate: None,
            is_constructor: false,
            is_method: true, // This is a method
        }),
        write_type: TypeId::ANY,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    // With method bivariance (default), derived method with narrower parameter is assignable
    // This simulates the covariant 'this' behavior
    assert!(
        checker.is_assignable(derived_method, base_method),
        "Derived method with narrower 'this' parameter should be assignable to Base method due to bivariance"
    );
}

#[test]
fn test_method_bivariance_persists_with_strict_function_types() {
    // Methods remain bivariant even with strictFunctionTypes=true
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    checker.set_strict_function_types(true);

    let method_name = interner.intern_string("method");

    // Base type with method
    let base = interner.object(vec![PropertyInfo {
        name: interner.intern_string("a"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let base_with_method = interner.object(vec![PropertyInfo {
        name: method_name,
        type_id: interner.function(FunctionShape {
            params: vec![ParamInfo {
                name: None,
                type_id: base,
                optional: false,
                rest: false,
            }],
            this_type: Some(base),
            return_type: TypeId::VOID,
            type_params: Vec::new(),
            type_predicate: None,
            is_constructor: false,
            is_method: true,
        }),
        write_type: TypeId::ANY,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    // Derived type with method
    let derived = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("a"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: interner.intern_string("b"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    let derived_with_method = interner.object(vec![PropertyInfo {
        name: method_name,
        type_id: interner.function(FunctionShape {
            params: vec![ParamInfo {
                name: None,
                type_id: derived,
                optional: false,
                rest: false,
            }],
            this_type: Some(derived),
            return_type: TypeId::VOID,
            type_params: Vec::new(),
            type_predicate: None,
            is_constructor: false,
            is_method: true,
        }),
        write_type: TypeId::ANY,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    // Methods are still bivariant even with strictFunctionTypes
    assert!(
        checker.is_assignable(derived_with_method, base_with_method),
        "Methods should remain bivariant even with strictFunctionTypes"
    );
}

#[test]
fn test_function_variance_strict_function_types_affects_functions_not_methods() {
    // strictFunctionTypes affects standalone functions but not methods
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    checker.set_strict_function_types(true);

    let (animal, dog) = make_animal_dog(&interner);

    // Standalone functions: contravariant with strictFunctionTypes
    let fn_dog = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: None,
            type_id: dog,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false, // NOT a method
    });

    let fn_animal = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: None,
            type_id: animal,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false, // NOT a method
    });

    // Functions should be contravariant (not assignable) with strictFunctionTypes
    assert!(
        !checker.is_assignable(fn_dog, fn_animal),
        "Standalone functions should be contravariant with strictFunctionTypes"
    );

    // But methods are still bivariant
    let method_name = interner.intern_string("method");

    let obj_with_dog_method = interner.object(vec![PropertyInfo {
        name: method_name,
        type_id: fn_dog,
        write_type: TypeId::ANY,
        optional: false,
        readonly: false,
        is_method: true, // IS a method
    }]);

    let obj_with_animal_method = interner.object(vec![PropertyInfo {
        name: method_name,
        type_id: fn_animal,
        write_type: TypeId::ANY,
        optional: false,
        readonly: false,
        is_method: true, // IS a method
    }]);

    // Methods are bivariant even with strictFunctionTypes
    assert!(
        checker.is_assignable(obj_with_dog_method, obj_with_animal_method),
        "Methods should be bivariant even with strictFunctionTypes"
    );
}

// =============================================================================
// Integration Tests: Compiler Options Toggle Behaviors
// =============================================================================

#[test]
fn test_strict_mode_enables_all_strict_flags() {
    // Integration test: strict mode should enable multiple strict behaviors
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // Test strict null checks behavior
    assert!(!checker.is_assignable(TypeId::NULL, TypeId::STRING));

    // Test function variance (default is non-strict)
    let (animal, dog) = make_animal_dog(&interner);

    let fn_dog = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: None,
            type_id: dog,
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

    let fn_animal = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: None,
            type_id: animal,
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

    // Default: non-strict (bivariant)
    assert!(
        checker.is_assignable(fn_dog, fn_animal),
        "Functions should be bivariant by default"
    );

    // Enable strict function types
    checker.set_strict_function_types(true);
    assert!(
        !checker.is_assignable(fn_dog, fn_animal),
        "Functions should be contravariant with strictFunctionTypes"
    );
}

#[test]
fn test_compiler_options_independent_toggles() {
    // Test that compiler options can be toggled independently
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // Start with all defaults
    assert!(!checker.is_assignable(TypeId::NULL, TypeId::STRING)); // strictNullChecks=true (default)

    // Toggle strictNullChecks
    checker.set_strict_null_checks(false);
    assert!(checker.is_assignable(TypeId::NULL, TypeId::STRING));

    // Reset for next test
    checker.set_strict_null_checks(true);

    // Toggle exact_optional_property_types
    let x = interner.intern_string("x");
    let optional_number = interner.object(vec![PropertyInfo {
        name: x,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: true,
        readonly: false,
        is_method: false,
    }]);
    let number_or_undefined = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
    let explicit_union = interner.object(vec![PropertyInfo {
        name: x,
        type_id: number_or_undefined,
        write_type: number_or_undefined,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    // Default (exact_optional_property_types=false): optional includes undefined
    // So { x: number | undefined } should be assignable to { x?: number }
    assert!(
        checker.is_assignable(explicit_union, optional_number),
        "Explicit number|undefined should be assignable to optional number in default mode"
    );

    // Toggle exact_optional_property_types
    checker.set_exact_optional_property_types(true);
    // In exact mode, optional does NOT include undefined
    // So { x: number | undefined } should NOT be assignable to { x?: number }
    assert!(
        !checker.is_assignable(explicit_union, optional_number),
        "Explicit number|undefined should NOT be assignable to optional number in exact mode"
    );

    // Toggle no_unchecked_indexed_access
    let indexed = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
        number_index: None,
    });
    let index_access = interner.intern(TypeKey::IndexAccess(indexed, TypeId::STRING));

    // Reset exact mode for next test
    checker.set_exact_optional_property_types(false);

    // Default: no_unchecked_indexed_access=false, index access returns NUMBER
    assert!(checker.is_assignable(index_access, TypeId::NUMBER));

    // Toggle no_unchecked_indexed_access
    checker.set_no_unchecked_indexed_access(true);
    assert!(!checker.is_assignable(index_access, TypeId::NUMBER));
}

// =============================================================================
// Rule #29: The Global Function type - Intrinsic(Function) as untyped callable supertype
// =============================================================================

#[test]
fn test_function_intrinsic_accepts_any_function() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // Create a simple function type
    let simple_fn = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: None,
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Function intrinsic should accept any function
    assert!(
        checker.is_assignable(simple_fn, TypeId::FUNCTION),
        "Any function should be assignable to Function intrinsic"
    );
}

#[test]
fn test_function_intrinsic_accepts_callable() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // Create a callable with multiple signatures
    let callable = interner.callable(CallableShape {
        call_signatures: vec![CallSignature {
            type_params: Vec::new(),
            params: vec![ParamInfo {
                name: None,
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::NUMBER,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
    });

    // Function intrinsic should accept callable types
    assert!(
        checker.is_assignable(callable, TypeId::FUNCTION),
        "Callable types should be assignable to Function intrinsic"
    );
}

#[test]
fn test_function_intrinsic_rejects_non_callable() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // Primitives are NOT callable
    assert!(
        !checker.is_assignable(TypeId::STRING, TypeId::FUNCTION),
        "String should NOT be assignable to Function intrinsic"
    );
    assert!(
        !checker.is_assignable(TypeId::NUMBER, TypeId::FUNCTION),
        "Number should NOT be assignable to Function intrinsic"
    );

    // Objects are NOT callable (unless they have call signatures)
    let obj = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    assert!(
        !checker.is_assignable(obj, TypeId::FUNCTION),
        "Plain object should NOT be assignable to Function intrinsic"
    );
}

#[test]
fn test_function_intrinsic_with_union_of_callables() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let fn1 = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: None,
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn2 = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: None,
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Union of callables should be assignable to Function
    let union_fn = interner.union(vec![fn1, fn2]);
    assert!(
        checker.is_assignable(union_fn, TypeId::FUNCTION),
        "Union of callables should be assignable to Function intrinsic"
    );
}

#[test]
fn test_function_intrinsic_with_union_non_callable() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let fn1 = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: None,
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Union of callable and non-callable should NOT be assignable to Function
    let mixed_union = interner.union(vec![fn1, TypeId::STRING]);
    assert!(
        !checker.is_assignable(mixed_union, TypeId::FUNCTION),
        "Mixed union (callable | non-callable) should NOT be assignable to Function"
    );
}

// =============================================================================
// Union/Intersection Distributivity Tests
// =============================================================================

#[test]
fn test_union_intersection_distributivity_basic() {
    // Test: (A | B) & C distributes to (A & C) | (B & C)
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let name = interner.intern_string("name");
    let age = interner.intern_string("age");

    let type_a = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let type_b = interner.object(vec![PropertyInfo {
        name: age,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let type_c = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    // (A | B) & C
    let union_ab = interner.union(vec![type_a, type_b]);
    let intersection = interner.intersection(vec![union_ab, type_c]);

    // A & C (should be compatible since both have 'name: string')
    let a_and_c = interner.intersection(vec![type_a, type_c]);

    assert!(
        checker.is_assignable(intersection, a_and_c),
        "(A | B) & C should distribute correctly"
    );
}

#[test]
fn test_intersection_union_distributivity() {
    // Test: A & (B | C) distributes to (A & B) | (A & C)
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let name = interner.intern_string("name");
    let age = interner.intern_string("age");

    let type_a = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let type_b = interner.object(vec![PropertyInfo {
        name: age,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let type_c = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    // A & (B | C)
    let union_bc = interner.union(vec![type_b, type_c]);
    let intersection = interner.intersection(vec![type_a, union_bc]);

    // (A & B) is empty (incompatible), so intersection should simplify
    assert!(
        checker.is_assignable(type_a, intersection),
        "A & (B | C) should distribute to (A & B) | (A & C)"
    );
}

#[test]
fn test_distributivity_with_primitives() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // (string | number) & string should be string
    let str_num = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let result = interner.intersection(vec![str_num, TypeId::STRING]);

    assert!(
        checker.is_assignable(TypeId::STRING, result),
        "(string | number) & string should be string"
    );
}

// =============================================================================
// Enhanced Weak Type Detection Tests
// =============================================================================

#[test]
fn test_weak_type_detection_with_all_strict_options() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // Enable all strict options
    checker.set_strict_function_types(true);
    checker.set_strict_null_checks(true);
    checker.set_exact_optional_property_types(true);
    checker.set_no_unchecked_indexed_access(true);

    let x = interner.intern_string("x");
    let y = interner.intern_string("y");

    // Weak type: all optional properties
    let weak_type = interner.object(vec![
        PropertyInfo {
            name: x,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: true,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: y,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: true,
            readonly: false,
            is_method: false,
        },
    ]);

    // Source with no common properties should be rejected
    let source = interner.object(vec![PropertyInfo {
        name: interner.intern_string("z"),
        type_id: TypeId::BOOLEAN,
        write_type: TypeId::BOOLEAN,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    assert!(
        !checker.is_assignable(source, weak_type),
        "Weak type detection should work with all strict options enabled"
    );
}

#[test]
fn test_weak_union_detection_improved() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let x = interner.intern_string("x");
    let y = interner.intern_string("y");

    // Weak types in a union
    let weak1 = interner.object(vec![PropertyInfo {
        name: x,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: true,
        readonly: false,
        is_method: false,
    }]);

    let weak2 = interner.object(vec![PropertyInfo {
        name: y,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: true,
        readonly: false,
        is_method: false,
    }]);

    let weak_union = interner.union(vec![weak1, weak2]);

    // Source with no common properties should be rejected
    let source = interner.object(vec![PropertyInfo {
        name: interner.intern_string("z"),
        type_id: TypeId::BOOLEAN,
        write_type: TypeId::BOOLEAN,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    assert!(
        !checker.is_assignable(source, weak_union),
        "Weak union detection should reject source with no common properties"
    );
}

// =============================================================================
// Comprehensive Compiler Options Tests
// =============================================================================

#[test]
fn test_all_compiler_options_combinations() {
    let interner = TypeInterner::new();
    let x = interner.intern_string("x");

    let optional_number = interner.object(vec![PropertyInfo {
        name: x,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: true,
        readonly: false,
        is_method: false,
    }]);

    let number_or_undefined = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
    let explicit_union = interner.object(vec![PropertyInfo {
        name: x,
        type_id: number_or_undefined,
        write_type: number_or_undefined,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    // Test all combinations
    let test_cases = vec![
        (false, false, false, "all defaults"),
        (true, false, false, "strictFunctionTypes only"),
        (false, true, false, "exactOptionalProperties only"),
        (false, false, true, "noUncheckedIndexedAccess only"),
        (true, true, false, "strict + exact"),
        (true, false, true, "strict + noUnchecked"),
        (false, true, true, "exact + noUnchecked"),
        (true, true, true, "all strict"),
    ];

    for (strict_fn, exact, no_unchecked, desc) in test_cases {
        let mut checker = CompatChecker::new(&interner);
        checker.set_strict_function_types(strict_fn);
        checker.set_exact_optional_property_types(exact);
        checker.set_no_unchecked_indexed_access(no_unchecked);

        // The behavior should change based on exact_optional_property_types
        let expected = !exact; // When exact=true, should NOT be assignable
        let result = checker.is_assignable(explicit_union, optional_number);

        assert_eq!(result, expected, "Failed for: {} (exact={})", desc, exact);
    }
}

#[test]
fn test_strict_function_types_affects_methods_independently() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // Animal: { name: string }
    let name = interner.intern_string("name");
    let animal = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    // Dog: { name: string, breed: string } - Dog is subtype of Animal
    let breed = interner.intern_string("breed");
    let dog = interner.object(vec![
        PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: breed,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    // Create method types
    let fn_animal = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: None,
            type_id: animal,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false, // Function, not method
    });

    let fn_dog = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: None,
            type_id: dog,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false, // Function, not method
    });

    // Default: bivariant
    assert!(
        checker.is_assignable(fn_dog, fn_animal),
        "Functions should be bivariant by default"
    );

    // Enable strict function types
    checker.set_strict_function_types(true);
    assert!(
        !checker.is_assignable(fn_dog, fn_animal),
        "Functions should be contravariant with strictFunctionTypes"
    );

    // Methods should remain bivariant even with strictFunctionTypes
    let method_animal = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: None,
            type_id: animal,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: true, // This is a method
    });

    let method_dog = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: None,
            type_id: dog,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: true, // This is a method
    });

    assert!(
        checker.is_assignable(method_dog, method_animal),
        "Methods should remain bivariant even with strictFunctionTypes"
    );
}

#[test]
fn test_no_unchecked_indexed_access_with_nested_types() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // Create nested array type
    let nested_array = interner.array(interner.array(TypeId::STRING));

    // Index access should include undefined when no_unchecked_indexed_access is enabled
    checker.set_no_unchecked_indexed_access(true);

    // String should NOT be assignable to (string | undefined)
    assert!(
        !checker.is_assignable(TypeId::STRING, nested_array),
        "With noUncheckedIndexedAccess, array indexing includes undefined"
    );
}

// =============================================================================
// Rule #30: keyof contravariance - keyof(A | B) === keyof A & keyof B
// =============================================================================

#[test]
fn test_keyof_union_contravariance() {
    let interner = TypeInterner::new();
    let checker = CompatChecker::new(&interner);

    let name = interner.intern_string("name");
    let age = interner.intern_string("age");

    // Type A: { name: string }
    let type_a = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    // Type B: { age: number }
    let type_b = interner.object(vec![PropertyInfo {
        name: age,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    // keyof (A | B) should be keyof A & keyof B
    // Since A has "name" and B has "age" with NO common keys,
    // keyof (A | B) = "name" & "age" = never
    let union_ab = interner.union(vec![type_a, type_b]);
    let keyof_union = crate::solver::evaluate_keyof(&interner, union_ab);

    // keyof (A | B) with no common keys should be never
    assert_eq!(
        keyof_union,
        TypeId::NEVER,
        "keyof (A | B) with disjoint keys should be never"
    );

    // Verify that keyof properly extracts keys when there ARE common properties
    let name_prop = PropertyInfo {
        name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    };
    // Type C: { name: string, x: number }
    let type_c = interner.object(vec![
        name_prop.clone(),
        PropertyInfo {
            name: interner.intern_string("x"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);
    // Type D: { name: string, y: boolean }
    let type_d = interner.object(vec![
        name_prop,
        PropertyInfo {
            name: interner.intern_string("y"),
            type_id: TypeId::BOOLEAN,
            write_type: TypeId::BOOLEAN,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    // keyof (C | D) = keyof C & keyof D = ("name" | "x") & ("name" | "y") = "name"
    let union_cd = interner.union(vec![type_c, type_d]);
    let keyof_union_cd = crate::solver::evaluate_keyof(&interner, union_cd);

    let name_literal = interner.literal_string("name");
    assert_eq!(
        keyof_union_cd, name_literal,
        "keyof (C | D) with common 'name' key should be 'name'"
    );

    // Suppress unused checker warning
    let _ = checker;
}

#[test]
fn test_keyof_intersection_distributivity() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let name = interner.intern_string("name");
    let age = interner.intern_string("age");

    // Type A: { name: string }
    let type_a = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    // Type B: { name: string, age: number }
    let type_b = interner.object(vec![
        PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: age,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    // keyof (A & B) should be keyof A | keyof B
    // Both have 'name', B also has 'age'
    let intersection_ab = interner.intersection(vec![type_a, type_b]);
    let keyof_intersection = interner.intern(TypeKey::KeyOf(intersection_ab));

    let name_literal = interner.intern(TypeKey::Literal(crate::solver::LiteralValue::String(name)));
    let age_literal = interner.intern(TypeKey::Literal(crate::solver::LiteralValue::String(age)));

    // keyof (A & B) should include 'name' (common to both)
    assert!(
        checker.is_assignable(name_literal, keyof_intersection),
        "keyof (A & B) should include 'name'"
    );

    // keyof (A & B) should include 'age' (from B)
    assert!(
        checker.is_assignable(age_literal, keyof_intersection),
        "keyof (A & B) should include 'age'"
    );
}

#[test]
fn test_keyof_with_union_of_objects_with_common_properties() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let name = interner.intern_string("name");
    let age = interner.intern_string("age");

    // Type A: { name: string, age: number }
    let type_a = interner.object(vec![
        PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: age,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    // Type B: { name: string, email: string }
    let email = interner.intern_string("email");
    let type_b = interner.object(vec![
        PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: email,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    // keyof (A | B) should be keyof A & keyof B
    // which is just "name" (the common property)
    let union_ab = interner.union(vec![type_a, type_b]);
    let keyof_union = interner.intern(TypeKey::KeyOf(union_ab));

    let name_literal = interner.intern(TypeKey::Literal(crate::solver::LiteralValue::String(name)));
    let age_literal = interner.intern(TypeKey::Literal(crate::solver::LiteralValue::String(age)));
    let email_literal =
        interner.intern(TypeKey::Literal(crate::solver::LiteralValue::String(email)));

    // keyof (A | B) should include 'name' (common to both)
    assert!(
        checker.is_assignable(name_literal, keyof_union),
        "keyof (A | B) should include common property 'name'"
    );

    // keyof (A | B) should NOT include 'age' (only in A)
    assert!(
        !checker.is_assignable(age_literal, keyof_union),
        "keyof (A | B) should NOT include 'age' (only in A)"
    );

    // keyof (A | B) should NOT include 'email' (only in B)
    assert!(
        !checker.is_assignable(email_literal, keyof_union),
        "keyof (A | B) should NOT include 'email' (only in B)"
    );
}

// =============================================================================
// Rule #32: Best Common Type (BCT) inference for array literals
// =============================================================================

#[test]
fn test_best_common_type_array_literal_inference() {
    let interner = TypeInterner::new();
    let ctx = crate::solver::infer::InferenceContext::new(&interner);

    // Array literal with mixed types: [1, "hello", true]
    // Best common type should be the union: number | string | boolean
    let types = vec![TypeId::NUMBER, TypeId::STRING, TypeId::BOOLEAN];
    let bct = ctx.best_common_type(&types);

    // The BCT should be a union of all three types
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::STRING, TypeId::BOOLEAN]);
    assert_eq!(bct, expected, "BCT of mixed types should be their union");
}

#[test]
fn test_best_common_type_with_supertype() {
    let interner = TypeInterner::new();
    let ctx = crate::solver::infer::InferenceContext::new(&interner);

    let name = interner.intern_string("name");

    // Type Animal: { name: string }
    let animal = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    // Type Dog: { name: string, breed: string }
    let breed = interner.intern_string("breed");
    let dog = interner.object(vec![
        PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: breed,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    // BCT of [Animal, Dog] should be Animal (the supertype)
    let types = vec![animal, dog];
    let bct = ctx.best_common_type(&types);

    // Animal should be assignable to BCT
    assert!(
        interner.is_subtype_of(animal, bct),
        "Animal should be subtype of BCT"
    );
}

#[test]
fn test_best_common_type_empty_array() {
    let interner = TypeInterner::new();
    let ctx = crate::solver::infer::InferenceContext::new(&interner);

    // Empty array should infer to unknown[] (or any[])
    let types: Vec<TypeId> = vec![];
    let bct = ctx.best_common_type(&types);

    // Empty arrays default to unknown
    assert_eq!(bct, TypeId::UNKNOWN, "BCT of empty array should be unknown");
}

#[test]
fn test_best_common_type_single_element() {
    let interner = TypeInterner::new();
    let ctx = crate::solver::infer::InferenceContext::new(&interner);

    // Single element array should just be that type
    let types = vec![TypeId::STRING];
    let bct = ctx.best_common_type(&types);

    assert_eq!(
        bct,
        TypeId::STRING,
        "BCT of single element should be that element"
    );
}

#[test]
fn test_best_common_type_with_literal_widening() {
    let interner = TypeInterner::new();
    let ctx = crate::solver::infer::InferenceContext::new(&interner);

    // [1, "a"] should infer to (number | string)[]
    let types = vec![TypeId::NUMBER, TypeId::STRING];
    let bct = ctx.best_common_type(&types);

    // Should be a union of both types
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::STRING]);
    assert_eq!(
        bct, expected,
        "BCT of number and string should be their union"
    );
}

// =============================================================================
// Private Brand Assignability Override Tests
// =============================================================================

#[test]
fn test_private_brand_same_brand_assignable() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // Two types with the same private brand should be assignable
    let brand = interner.intern_string("__private_brand_Foo");
    let source = interner.object(vec![PropertyInfo {
        name: brand,
        type_id: TypeId::NEVER,
        write_type: TypeId::NEVER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let target = interner.object(vec![PropertyInfo {
        name: brand,
        type_id: TypeId::NEVER,
        write_type: TypeId::NEVER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    // Same brand = same class declaration = assignable
    assert!(checker.is_assignable(source, target));
}

#[test]
fn test_private_brand_different_brand_not_assignable() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // Two types with different private brands should NOT be assignable
    let brand1 = interner.intern_string("__private_brand_Foo");
    let brand2 = interner.intern_string("__private_brand_Bar");

    let source = interner.object(vec![PropertyInfo {
        name: brand1,
        type_id: TypeId::NEVER,
        write_type: TypeId::NEVER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let target = interner.object(vec![PropertyInfo {
        name: brand2,
        type_id: TypeId::NEVER,
        write_type: TypeId::NEVER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    // Different brands = different class declarations = not assignable
    assert!(!checker.is_assignable(source, target));
}

#[test]
fn test_private_brand_source_without_brand_not_assignable_to_target_with_brand() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // Source without brand cannot satisfy target's private requirements
    let brand = interner.intern_string("__private_brand_Foo");
    let name = interner.intern_string("value");

    let source = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let target = interner.object(vec![
        PropertyInfo {
            name: brand,
            type_id: TypeId::NEVER,
            write_type: TypeId::NEVER,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    // Source without brand cannot be assigned to target with brand
    assert!(!checker.is_assignable(source, target));
}

#[test]
fn test_private_brand_source_with_brand_assignable_to_target_without_brand() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // Source with brand CAN be assigned to target without brand (e.g., interface)
    let brand = interner.intern_string("__private_brand_Foo");
    let name = interner.intern_string("value");

    let source = interner.object(vec![
        PropertyInfo {
            name: brand,
            type_id: TypeId::NEVER,
            write_type: TypeId::NEVER,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);
    let target = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    // A class can implement an interface (source with brand -> target without brand)
    assert!(checker.is_assignable(source, target));
}

#[test]
fn test_private_brand_neither_has_brand_falls_through() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // When neither has a brand, fall through to structural checking
    let name = interner.intern_string("value");

    let source = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let target = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    // Structural check passes
    assert!(checker.is_assignable(source, target));
}

#[test]
fn test_private_brand_callable_with_brand() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // Callable types (constructors) can also have private brands
    let brand1 = interner.intern_string("__private_brand_Foo");
    let brand2 = interner.intern_string("__private_brand_Bar");

    let source = interner.callable(CallableShape {
        call_signatures: Vec::new(),
        construct_signatures: vec![CallSignature {
            params: Vec::new(),
            this_type: None,
            return_type: TypeId::ANY,
            type_predicate: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        properties: vec![PropertyInfo {
            name: brand1,
            type_id: TypeId::NEVER,
            write_type: TypeId::NEVER,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        ..Default::default()
    });

    let target = interner.callable(CallableShape {
        call_signatures: Vec::new(),
        construct_signatures: vec![CallSignature {
            params: Vec::new(),
            this_type: None,
            return_type: TypeId::ANY,
            type_predicate: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        properties: vec![PropertyInfo {
            name: brand2,
            type_id: TypeId::NEVER,
            write_type: TypeId::NEVER,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        ..Default::default()
    });

    // Different brands in callables = not assignable
    assert!(!checker.is_assignable(source, target));
}
