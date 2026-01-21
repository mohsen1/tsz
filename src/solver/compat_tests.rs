use super::*;
use crate::solver::subtype::SubtypeFailureReason;
use crate::solver::{
    CallableShape, CallSignature, ConditionalType, FunctionShape, IndexSignature, MappedType,
    ObjectShape, ParamInfo, PropertyInfo, SymbolRef, TemplateSpan, TupleElement, TypeEnvironment,
    TypeParamInfo, TypeSubstitution, instantiate_type,
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
    // Test that deep recursion doesn't crash and produces correct results.
    // string[][][] should NOT be assignable to number[][][] regardless of depth.
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

    // Incompatible array types should NOT be assignable (type safety)
    assert!(!checker.is_assignable(deep_string, deep_number));
    // Same types at same depth should be assignable
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
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
        number_index: None,
    });
    let number_index = interner.object_with_index(ObjectShape {
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
