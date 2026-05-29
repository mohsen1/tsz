/// Build a `<param_name>(arg_name: param_name): param_name` identity `FunctionShape`.
/// Reused across multiple tests that verify unconstrained-T inference behavior.
fn make_identity_shape(interner: &TypeInterner, param_name: &str, arg_name: &str) -> FunctionShape {
    let t_param = TypeParamInfo {
        name: interner.intern_string(param_name),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.type_param(t_param);
    FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string(arg_name)),
            type_id: t_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    }
}

#[test]
fn test_call_simple_function() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    // function(x: number): string
    let func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
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

    // Call with correct args
    let result = evaluator.resolve_call(func, &[TypeId::NUMBER]);
    match result {
        CallResult::Success(ret) => assert_eq!(ret, TypeId::STRING),
        _ => panic!("Expected success, got {result:?}"),
    }
}

#[test]
fn call_evaluator_cache_statistics_account_for_contextual_sensitivity() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let evaluator = CallEvaluator::new(&interner, &mut subtype);

    let empty = evaluator.cache_statistics();
    assert_eq!(empty.contextual_sensitivity_entries, 0);
    assert_eq!(empty.estimated_size_bytes(), 0);

    let func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::ANY,
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

    assert!(evaluator.is_contextually_sensitive(func));
    let populated = evaluator.cache_statistics();
    assert_eq!(populated.contextual_sensitivity_entries, 1);
    assert!(
        populated.estimated_size_bytes() > empty.estimated_size_bytes(),
        "populated call evaluator cache should report nonzero estimated residency"
    );

    assert!(evaluator.is_contextually_sensitive(func));
    let repeated = evaluator.cache_statistics();
    assert_eq!(
        repeated.contextual_sensitivity_entries,
        populated.contextual_sensitivity_entries
    );
    assert_eq!(
        repeated.estimated_size_bytes(),
        populated.estimated_size_bytes()
    );
}

#[test]
fn test_call_argument_count_mismatch() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    // function(x: number): string
    let func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
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

    // Call with no args
    let result = evaluator.resolve_call(func, &[]);
    match result {
        CallResult::ArgumentCountMismatch {
            expected_min,
            actual,
            ..
        } => {
            assert_eq!(expected_min, 1);
            assert_eq!(actual, 0);
        }
        _ => panic!("Expected ArgumentCountMismatch, got {result:?}"),
    }
}

#[test]
fn test_call_argument_type_mismatch() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    // function(x: number): string
    let func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
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

    // Call with wrong type
    let result = evaluator.resolve_call(func, &[TypeId::STRING]);
    match result {
        CallResult::ArgumentTypeMismatch {
            index,
            expected,
            actual,
            ..
        } => {
            assert_eq!(index, 0);
            assert_eq!(expected, TypeId::NUMBER);
            assert_eq!(actual, TypeId::STRING);
        }
        _ => panic!("Expected ArgumentTypeMismatch, got {result:?}"),
    }
}

#[test]
fn test_call_assignability_respects_strict_function_types_toggle() {
    let interner = TypeInterner::new();

    let name = interner.intern_string("name");
    let breed = interner.intern_string("breed");

    let animal = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
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
    let dog = interner.object(vec![
        PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
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
        },
        PropertyInfo::new(breed, TypeId::STRING),
    ]);

    let fn_animal = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(animal)],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let fn_dog = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(dog)],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let accepts_fn = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("cb")),
            type_id: fn_animal,
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

    let mut checker = CompatChecker::new(&interner);
    {
        let mut evaluator = CallEvaluator::new(&interner, &mut checker);
        let result = evaluator.resolve_call(accepts_fn, &[fn_dog]);
        assert!(matches!(result, CallResult::Success(_)));
    }

    checker.set_strict_function_types(true);
    {
        let mut evaluator = CallEvaluator::new(&interner, &mut checker);
        let result = evaluator.resolve_call(accepts_fn, &[fn_dog]);
        assert!(matches!(result, CallResult::ArgumentTypeMismatch { .. }));
    }
}

#[test]
fn test_call_weak_type_with_compat_checker() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);

    let weak_target = interner.object(vec![PropertyInfo::opt(
        interner.intern_string("a"),
        TypeId::NUMBER,
    )]);
    let func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("arg")),
            type_id: weak_target,
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

    let arg = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);

    let result = evaluator.resolve_call(func, &[arg]);
    assert!(matches!(result, CallResult::ArgumentTypeMismatch { .. }));
}

#[test]
fn test_generic_call_resets_constraint_step_budget() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);

    let tp_name = interner.intern_string("T");
    let tp = TypeParamInfo {
        is_const: false,
        name: tp_name,
        constraint: None,
        default: None,
    };
    let tp_id = interner.type_param(tp);
    let identity = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(tp_id)],
        this_type: None,
        return_type: tp_id,
        type_params: vec![tp],
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    evaluator.constraint_step_count.set(MAX_CONSTRAINT_STEPS);

    let result = evaluator.resolve_call(identity, &[TypeId::STRING]);
    match result {
        CallResult::Success(ret) => assert_eq!(ret, TypeId::STRING),
        _ => panic!("Expected successful generic inference, got {result:?}"),
    }
}

/// When a non-const type parameter has a constraint that the widened argument
/// type would violate, the solver should fall back to the unwidened (literal)
/// argument type. This prevents false TS2322 errors like:
///   `<T extends [string, string, 'a' | 'b']>(x: T): T`
///   called with `["x", "y", "a"]` → T should be `["x", "y", "a"]` not `[string, string, string]`
#[test]
fn test_generic_call_widening_falls_back_when_constraint_violated() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);

    // Build constraint: [string, 'a' | 'b']
    let a_lit = interner.literal_string("a");
    let b_lit = interner.literal_string("b");
    let ab_union = interner.union(vec![a_lit, b_lit]);
    let constraint = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: ab_union,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    // Build: <T extends [string, 'a' | 'b']>(x: T): T
    let tp_name = interner.intern_string("T");
    let tp = TypeParamInfo {
        is_const: false,
        name: tp_name,
        constraint: Some(constraint),
        default: None,
    };
    let tp_id = interner.type_param(tp);
    let func = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(tp_id)],
        this_type: None,
        return_type: tp_id,
        type_params: vec![tp],
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Call with ["hello", "a"] — literal tuple
    let hello_lit = interner.literal_string("hello");
    let arg = interner.tuple(vec![
        TupleElement {
            type_id: hello_lit,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: a_lit,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let result = evaluator.resolve_call(func, &[arg]);
    match result {
        CallResult::Success(ret) => {
            // The return type should be the unwidened literal tuple,
            // because widening to [string, string] would violate the constraint
            assert_eq!(ret, arg, "Expected unwidened literal tuple as return type");
        }
        other => panic!("Expected Success with literal tuple, got {other:?}"),
    }
}

/// When widening does NOT violate the constraint, the widened type should be used.
#[test]
fn test_generic_call_widening_applies_when_constraint_satisfied() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);

    // Build constraint: [string, string]
    let constraint = interner.tuple(vec![
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

    // Build: <T extends [string, string]>(x: T): T
    let tp_name = interner.intern_string("T");
    let tp = TypeParamInfo {
        is_const: false,
        name: tp_name,
        constraint: Some(constraint),
        default: None,
    };
    let tp_id = interner.type_param(tp);
    let func = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(tp_id)],
        this_type: None,
        return_type: tp_id,
        type_params: vec![tp],
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Call with ["hello", "world"] — literal tuple
    let hello_lit = interner.literal_string("hello");
    let world_lit = interner.literal_string("world");
    let arg = interner.tuple(vec![
        TupleElement {
            type_id: hello_lit,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: world_lit,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let result = evaluator.resolve_call(func, &[arg]);
    match result {
        CallResult::Success(ret) => {
            // Widening ["hello", "world"] → [string, string] satisfies constraint,
            // so the widened type should be used
            let expected = interner.tuple(vec![
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
            assert_eq!(ret, expected, "Expected widened tuple as return type");
        }
        other => panic!("Expected Success with widened tuple, got {other:?}"),
    }
}

#[test]
fn test_generic_call_widens_fresh_object_union_inferred_type() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);

    let t_param = TypeParamInfo {
        is_const: false,
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
    };
    let t_type = interner.type_param(t_param);
    let func = interner.function(FunctionShape {
        params: vec![
            ParamInfo::unnamed(t_type),
            ParamInfo::unnamed(t_type),
            ParamInfo::unnamed(t_type),
        ],
        this_type: None,
        return_type: t_type,
        type_params: vec![t_param],
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let x = interner.intern_string("x");
    let y = interner.intern_string("y");
    let z = interner.intern_string("z");
    let six = interner.literal_number(6.0);
    let seven = interner.literal_number(7.0);
    let empty = interner.literal_string("");

    let left = interner.object_fresh(vec![PropertyInfo::new(x, six), PropertyInfo::new(z, seven)]);
    let right = interner.object_fresh(vec![PropertyInfo::new(x, six), PropertyInfo::new(y, empty)]);

    let result = evaluator.resolve_call(func, &[TypeId::UNDEFINED, left, right]);
    let ret = match result {
        CallResult::Success(ret) => ret,
        other => panic!("Expected Success for fresh-object generic inference, got {other:?}"),
    };

    let Some(TypeData::Union(list_id)) = interner.lookup(ret) else {
        panic!("Expected union return type, got {:?}", interner.lookup(ret));
    };
    let members = interner.type_list(list_id);
    assert!(
        members.contains(&TypeId::UNDEFINED),
        "Expected undefined in inferred union, got {members:?}"
    );

    let mut saw_left_shape = false;
    let mut saw_right_shape = false;

    for &member in members.iter() {
        let shape = match interner.lookup(member) {
            Some(TypeData::Object(shape_id)) | Some(TypeData::ObjectWithIndex(shape_id)) => {
                interner.object_shape(shape_id)
            }
            _ => continue,
        };

        assert!(
            !shape
                .flags
                .contains(crate::types::ObjectFlags::FRESH_LITERAL),
            "Widened inference result should not retain fresh-object flags"
        );

        let find_prop = |name| shape.properties.iter().find(|prop| prop.name == name);
        let x_prop = find_prop(x).expect("normalized member should keep x");
        assert_eq!(x_prop.type_id, TypeId::NUMBER);
        assert!(!x_prop.optional);

        let y_prop = find_prop(y).expect("normalized member should include y");
        let z_prop = find_prop(z).expect("normalized member should include z");

        if !z_prop.optional {
            saw_left_shape = true;
            assert_eq!(z_prop.type_id, TypeId::NUMBER);
            assert!(y_prop.optional);
            assert_eq!(y_prop.type_id, TypeId::UNDEFINED);
        } else if !y_prop.optional {
            saw_right_shape = true;
            assert_eq!(y_prop.type_id, TypeId::STRING);
            assert!(z_prop.optional);
            assert_eq!(z_prop.type_id, TypeId::UNDEFINED);
        }
    }

    assert!(
        saw_left_shape,
        "Expected widened left object member in inferred union"
    );
    assert!(
        saw_right_shape,
        "Expected widened right object member in inferred union"
    );
}

fn type_union_members(interner: &TypeInterner, type_id: TypeId) -> Vec<TypeId> {
    match interner.lookup(type_id) {
        Some(TypeData::Union(list_id)) => interner.type_list(list_id).to_vec(),
        _ => vec![type_id],
    }
}

#[test]
fn object_spread_property_merge_later_required_overrides() {
    let interner = TypeInterner::new();
    let prop_name = interner.intern_string("value");
    let earlier = PropertyInfo::readonly(prop_name, TypeId::STRING);
    let mut spread = PropertyInfo::new(prop_name, TypeId::NUMBER);
    spread.declaration_order = 42;

    let merged = merge_object_spread_property(&interner, false, Some(&earlier), &spread);

    assert_eq!(merged.type_id, TypeId::NUMBER);
    assert_eq!(merged.write_type, TypeId::NUMBER);
    assert!(!merged.optional);
    assert!(!merged.readonly);
    assert_eq!(merged.declaration_order, 42);
}

#[test]
fn object_spread_property_merge_optional_later_unions_without_undefined_when_inexact() {
    let interner = TypeInterner::new();
    let prop_name = interner.intern_string("value");
    let earlier = PropertyInfo::new(prop_name, TypeId::STRING);
    let optional_number = interner.union2(TypeId::NUMBER, TypeId::UNDEFINED);
    let spread = PropertyInfo::opt(prop_name, optional_number);

    let merged = merge_object_spread_property(&interner, false, Some(&earlier), &spread);
    let members = type_union_members(&interner, merged.type_id);

    assert!(
        !merged.optional,
        "earlier required property keeps merge required"
    );
    assert!(members.contains(&TypeId::STRING));
    assert!(members.contains(&TypeId::NUMBER));
    assert!(
        !members.contains(&TypeId::UNDEFINED),
        "inexact optional spread merge should remove undefined from the later optional contribution"
    );
}

#[test]
fn object_spread_property_merge_optional_later_preserves_undefined_when_exact() {
    let interner = TypeInterner::new();
    let prop_name = interner.intern_string("value");
    let earlier = PropertyInfo::new(prop_name, TypeId::STRING);
    let optional_number = interner.union2(TypeId::NUMBER, TypeId::UNDEFINED);
    let spread = PropertyInfo::opt(prop_name, optional_number);

    let merged = merge_object_spread_property(&interner, true, Some(&earlier), &spread);
    let members = type_union_members(&interner, merged.type_id);

    assert!(
        !merged.optional,
        "earlier required property keeps merge required"
    );
    assert!(members.contains(&TypeId::STRING));
    assert!(members.contains(&TypeId::NUMBER));
    assert!(
        members.contains(&TypeId::UNDEFINED),
        "exact optional spread merge should preserve undefined in the later optional contribution"
    );
}

#[test]
fn test_generic_call_uninferred_callback_param_mismatch_uses_unknown() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);

    let t_param = TypeParamInfo {
        is_const: false,
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
    };
    let u_param = TypeParamInfo {
        is_const: false,
        name: interner.intern_string("U"),
        constraint: None,
        default: None,
    };
    let t_type = interner.type_param(t_param);
    let u_type = interner.type_param(u_param);

    let callback = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(u_type)],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: vec![],
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let func = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(t_type), ParamInfo::unnamed(callback)],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: vec![t_param, u_param],
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let result = evaluator.resolve_call(func, &[TypeId::NULL, TypeId::NULL]);
    match result {
        CallResult::ArgumentTypeMismatch {
            index,
            expected,
            actual,
            ..
        } => {
            assert_eq!(index, 1);
            assert_eq!(actual, TypeId::NULL);

            let Some(TypeData::Function(shape_id)) = interner.lookup(expected) else {
                panic!(
                    "Expected instantiated callback type in mismatch, got {:?}",
                    interner.lookup(expected)
                );
            };
            let shape = interner.function_shape(shape_id);
            assert!(shape.type_params.is_empty());
            assert_eq!(shape.params.len(), 1);
            assert_eq!(shape.params[0].type_id, TypeId::UNKNOWN);
            assert_eq!(shape.return_type, TypeId::VOID);
        }
        other => panic!("Expected callback mismatch with unknown parameter, got {other:?}"),
    }
}

#[test]
fn test_get_contextual_signature_with_compat_checker_matches_call_evaluator() {
    let interner = TypeInterner::new();
    let contextual = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(TypeId::STRING)],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let via_helper = get_contextual_signature_with_compat_checker(&interner, contextual);
    let via_evaluator =
        CallEvaluator::<CompatChecker>::get_contextual_signature(&interner, contextual);

    assert_eq!(via_helper, via_evaluator);
    let sig = via_helper.expect("expected contextual signature");
    assert_eq!(sig.params.len(), 1);
    assert_eq!(sig.params[0].type_id, TypeId::STRING);
    assert_eq!(sig.return_type, TypeId::NUMBER);
}

#[test]
fn test_get_contextual_signature_union_ignores_noncallable_and_constructor_members_when_call_exists()
 {
    let interner = TypeInterner::new();
    let props_type = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        TypeId::STRING,
    )]);
    let call_member = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(props_type)],
        this_type: None,
        return_type: TypeId::ANY,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let construct_member = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(props_type)],
        this_type: None,
        return_type: TypeId::ANY,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });
    let contextual = interner.union(vec![call_member, construct_member, TypeId::STRING]);

    let sig = CallEvaluator::<CompatChecker>::get_contextual_signature(&interner, contextual)
        .expect("expected contextual signature from callable union member");
    assert_eq!(sig.params.len(), 1);
    assert_eq!(sig.params[0].type_id, props_type);
    assert!(!sig.is_constructor);
}

#[test]
fn test_call_rest_parameter_allows_zero_args() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    // function(...args: number[]): string
    let rest_array = interner.array(TypeId::NUMBER);
    let func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: rest_array,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let result = evaluator.resolve_call(func, &[]);
    match result {
        CallResult::Success(ret) => assert_eq!(ret, TypeId::STRING),
        _ => panic!("Expected success, got {result:?}"),
    }
}

#[test]
fn test_call_rest_parameter_min_args_with_required() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    // function(x: string, ...args: number[]): string
    let rest_array = interner.array(TypeId::NUMBER);
    let func = interner.function(FunctionShape {
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("args")),
                type_id: rest_array,
                optional: false,
                rest: true,
            },
        ],
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let result = evaluator.resolve_call(func, &[]);
    match result {
        CallResult::ArgumentCountMismatch {
            expected_min,
            actual,
            ..
        } => {
            assert_eq!(expected_min, 1);
            assert_eq!(actual, 0);
        }
        _ => panic!("Expected ArgumentCountMismatch, got {result:?}"),
    }
}

#[test]
fn test_binary_equality_disjoint_primitives_returns_boolean() {
    // Equality operators always return boolean regardless of operand types.
    // TS2367 diagnostics are the checker's responsibility, not the evaluator's.
    let interner = TypeInterner::new();
    let evaluator = BinaryOpEvaluator::new(&interner);

    let result = evaluator.evaluate(TypeId::STRING, TypeId::NUMBER, "===");
    assert!(matches!(result, BinaryOpResult::Success(TypeId::BOOLEAN)));

    let result = evaluator.evaluate(TypeId::NUMBER, TypeId::UNDEFINED, "!==");
    assert!(matches!(result, BinaryOpResult::Success(TypeId::BOOLEAN)));

    let result = evaluator.evaluate(TypeId::STRING, TypeId::NULL, "===");
    assert!(matches!(result, BinaryOpResult::Success(TypeId::BOOLEAN)));
}

#[test]
fn test_binary_equality_disjoint_primitives_loose_returns_boolean() {
    let interner = TypeInterner::new();
    let evaluator = BinaryOpEvaluator::new(&interner);

    let result = evaluator.evaluate(TypeId::STRING, TypeId::NUMBER, "==");
    assert!(matches!(result, BinaryOpResult::Success(TypeId::BOOLEAN)));

    let result = evaluator.evaluate(TypeId::BOOLEAN, TypeId::UNDEFINED, "!=");
    assert!(matches!(result, BinaryOpResult::Success(TypeId::BOOLEAN)));
}

#[test]
fn test_binary_equality_disjoint_literals_returns_boolean() {
    let interner = TypeInterner::new();
    let evaluator = BinaryOpEvaluator::new(&interner);

    let one = interner.literal_number(1.0);
    let two = interner.literal_number(2.0);

    let result = evaluator.evaluate(one, two, "===");
    assert!(matches!(result, BinaryOpResult::Success(TypeId::BOOLEAN)));

    let result = evaluator.evaluate(one, two, "!==");
    assert!(matches!(result, BinaryOpResult::Success(TypeId::BOOLEAN)));
}

#[test]
fn test_binary_overlap_union_literals() {
    let interner = TypeInterner::new();
    let evaluator = BinaryOpEvaluator::new(&interner);

    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let lit_c = interner.literal_string("c");

    let left = interner.union(vec![lit_a, lit_b]);
    let right = interner.union(vec![lit_b, lit_c]);

    let result = evaluator.evaluate(left, right, "===");
    match result {
        BinaryOpResult::Success(result_type) => assert_eq!(result_type, TypeId::BOOLEAN),
        _ => panic!("Expected boolean result, got {result:?}"),
    }
}

#[test]
fn test_binary_overlap_with_any_unknown_never() {
    let interner = TypeInterner::new();
    let evaluator = BinaryOpEvaluator::new(&interner);

    let any_result = evaluator.evaluate(TypeId::ANY, TypeId::NUMBER, "===");
    assert!(matches!(
        any_result,
        BinaryOpResult::Success(TypeId::BOOLEAN)
    ));

    let unknown_result = evaluator.evaluate(TypeId::UNKNOWN, TypeId::NUMBER, "===");
    assert!(matches!(
        unknown_result,
        BinaryOpResult::Success(TypeId::BOOLEAN)
    ));

    // `never` is the bottom type — any operation on `never` produces `never`, not a type error.
    let never_result = evaluator.evaluate(TypeId::NEVER, TypeId::NUMBER, "===");
    assert!(matches!(
        never_result,
        BinaryOpResult::Success(TypeId::NEVER)
    ));
}

#[test]
fn test_binary_overlap_template_literal() {
    let interner = TypeInterner::new();
    let evaluator = BinaryOpEvaluator::new(&interner);

    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("prefix")),
        TemplateSpan::Type(TypeId::STRING),
        TemplateSpan::Text(interner.intern_string("suffix")),
    ]);

    let ok_result = evaluator.evaluate(template, TypeId::STRING, "===");
    assert!(matches!(
        ok_result,
        BinaryOpResult::Success(TypeId::BOOLEAN)
    ));

    // Even non-overlapping equality comparisons produce boolean
    let non_overlap_result = evaluator.evaluate(template, TypeId::NUMBER, "===");
    assert!(matches!(
        non_overlap_result,
        BinaryOpResult::Success(TypeId::BOOLEAN)
    ));
}

#[test]
fn test_binary_equality_generic_constraint_disjoint_still_boolean() {
    // Even when a type parameter's constraint is disjoint from the other operand,
    // equality operators still produce boolean. TS2367 is the checker's job.
    let interner = TypeInterner::new();
    let evaluator = BinaryOpEvaluator::new(&interner);

    let type_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }));

    let result = evaluator.evaluate(type_param, TypeId::NUMBER, "===");
    assert!(matches!(result, BinaryOpResult::Success(TypeId::BOOLEAN)));
}

#[test]
fn test_binary_overlap_generic_constraint_overlap() {
    let interner = TypeInterner::new();
    let evaluator = BinaryOpEvaluator::new(&interner);

    let type_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }));

    let result = evaluator.evaluate(type_param, TypeId::STRING, "===");
    match result {
        BinaryOpResult::Success(result_type) => assert_eq!(result_type, TypeId::BOOLEAN),
        _ => panic!("Expected boolean result, got {result:?}"),
    }
}

#[test]
fn test_binary_overlap_unconstrained_type_param() {
    let interner = TypeInterner::new();
    let evaluator = BinaryOpEvaluator::new(&interner);

    let type_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    let result = evaluator.evaluate(type_param, TypeId::NUMBER, "===");
    match result {
        BinaryOpResult::Success(result_type) => assert_eq!(result_type, TypeId::BOOLEAN),
        _ => panic!("Expected boolean result, got {result:?}"),
    }
}

#[test]
fn test_binary_equality_union_constraint_disjoint_still_boolean() {
    // Same principle: disjoint union constraint doesn't affect equality result type.
    let interner = TypeInterner::new();
    let evaluator = BinaryOpEvaluator::new(&interner);

    let constraint = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let type_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(constraint),
        default: None,
        is_const: false,
    }));

    let result = evaluator.evaluate(type_param, TypeId::BOOLEAN, "===");
    assert!(matches!(result, BinaryOpResult::Success(TypeId::BOOLEAN)));
}

#[test]
fn test_binary_overlap_union_constraint_overlap() {
    let interner = TypeInterner::new();
    let evaluator = BinaryOpEvaluator::new(&interner);

    let constraint = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let type_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(constraint),
        default: None,
        is_const: false,
    }));

    let result = evaluator.evaluate(type_param, TypeId::NUMBER, "===");
    match result {
        BinaryOpResult::Success(result_type) => assert_eq!(result_type, TypeId::BOOLEAN),
        _ => panic!("Expected boolean result, got {result:?}"),
    }
}

#[test]
fn test_binary_logical_and_contextual_callable_result() {
    let interner = TypeInterner::new();
    let evaluator = BinaryOpEvaluator::new(&interner);

    let contextual_fn = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(TypeId::STRING)],
        this_type: None,
        return_type: TypeId::STRING,
        type_params: vec![],
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let right_fn = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(TypeId::STRING)],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_params: vec![],
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let left_true = interner.literal_boolean(true);

    let result = evaluator.evaluate_with_context(left_true, right_fn, "&&", Some(contextual_fn));
    match result {
        BinaryOpResult::Success(result_type) => assert_eq!(result_type, right_fn),
        _ => panic!("Expected callable result, got {result:?}"),
    }
}

#[test]
fn test_binary_logical_and_contextual_callable_false_left_preserves_false() {
    let interner = TypeInterner::new();
    let evaluator = BinaryOpEvaluator::new(&interner);

    let contextual_fn = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(TypeId::STRING)],
        this_type: None,
        return_type: TypeId::STRING,
        type_params: vec![],
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let right_fn = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(TypeId::STRING)],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_params: vec![],
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let left_false = interner.literal_boolean(false);

    let result = evaluator.evaluate_with_context(left_false, right_fn, "&&", Some(contextual_fn));
    match result {
        BinaryOpResult::Success(result_type) => assert_eq!(result_type, left_false),
        _ => panic!("Expected false result, got {result:?}"),
    }
}

#[test]
fn test_binary_logical_and_with_boolean_produces_false_union() {
    // Verifies that `boolean && object_type` produces `false | object_type`,
    // which is critical for spread patterns like `...condition && { prop: value }`.
    // The spread checker filters out definitely-falsy types from unions, so
    // `false | { a: string }` is a valid spread type, but `unknown | { a: string }` is not.
    let interner = TypeInterner::new();
    let evaluator = BinaryOpEvaluator::new(&interner);

    let a_name = interner.intern_string("a");
    let obj = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    // boolean && { a: string } should produce false | { a: string }
    let result = evaluator.evaluate(TypeId::BOOLEAN, obj, "&&");
    match result {
        BinaryOpResult::Success(result_type) => {
            // The result should be a union containing false and the object type
            let data = interner.lookup(result_type);
            assert!(
                matches!(data, Some(TypeData::Union(_))),
                "Expected union type, got {data:?}"
            );
        }
        _ => panic!("Expected success result, got {result:?}"),
    }
}

#[test]
fn test_call_rest_parameter_type_match() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    // function(...args: number[]): string
    let rest_array = interner.array(TypeId::NUMBER);
    let func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: rest_array,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let result = evaluator.resolve_call(func, &[TypeId::NUMBER, TypeId::NUMBER]);
    match result {
        CallResult::Success(ret) => assert_eq!(ret, TypeId::STRING),
        _ => panic!("Expected success, got {result:?}"),
    }
}

#[test]
fn test_call_rest_parameter_type_mismatch() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    // function(...args: number[]): string
    let rest_array = interner.array(TypeId::NUMBER);
    let func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: rest_array,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let result = evaluator.resolve_call(func, &[TypeId::NUMBER, TypeId::STRING]);
    match result {
        CallResult::ArgumentTypeMismatch {
            index,
            expected,
            actual,
            ..
        } => {
            assert_eq!(index, 1);
            assert_eq!(expected, TypeId::NUMBER);
            assert_eq!(actual, TypeId::STRING);
        }
        _ => panic!("Expected ArgumentTypeMismatch, got {result:?}"),
    }
}

#[test]
fn test_call_tuple_rest_argument_count_mismatch() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    let tuple_rest = interner.tuple(vec![
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

    let func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: tuple_rest,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let result = evaluator.resolve_call(func, &[TypeId::NUMBER]);
    match result {
        CallResult::ArgumentCountMismatch {
            expected_min,
            actual,
            ..
        } => {
            assert_eq!(expected_min, 2);
            assert_eq!(actual, 1);
        }
        _ => panic!("Expected ArgumentCountMismatch, got {result:?}"),
    }
}

#[test]
fn test_call_tuple_rest_argument_type_mismatch() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    let tuple_rest = interner.tuple(vec![
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

    let func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: tuple_rest,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let result = evaluator.resolve_call(func, &[TypeId::NUMBER, TypeId::BOOLEAN]);
    match result {
        CallResult::ArgumentTypeMismatch {
            index,
            expected,
            actual,
            ..
        } => {
            assert_eq!(index, 1);
            assert_eq!(expected, TypeId::STRING);
            assert_eq!(actual, TypeId::BOOLEAN);
        }
        _ => panic!("Expected ArgumentTypeMismatch, got {result:?}"),
    }
}

#[test]
fn test_call_tuple_rest_argument_success() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    let tuple_rest = interner.tuple(vec![
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

    let func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: tuple_rest,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let result = evaluator.resolve_call(func, &[TypeId::NUMBER, TypeId::STRING]);
    match result {
        CallResult::Success(ret) => assert_eq!(ret, TypeId::STRING),
        _ => panic!("Expected success, got {result:?}"),
    }
}

#[test]
fn test_call_tuple_rest_with_fixed_tail() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    let rest_array = interner.array(TypeId::STRING);
    let tuple_rest = interner.tuple(vec![
        TupleElement {
            type_id: rest_array,
            name: None,
            optional: false,
            rest: true,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: tuple_rest,
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

    let result = evaluator.resolve_call(func, &[TypeId::NUMBER]);
    match result {
        CallResult::Success(ret) => assert_eq!(ret, TypeId::VOID),
        _ => panic!("Expected success, got {result:?}"),
    }

    let result = evaluator.resolve_call(func, &[TypeId::STRING, TypeId::STRING, TypeId::NUMBER]);
    match result {
        CallResult::Success(ret) => assert_eq!(ret, TypeId::VOID),
        _ => panic!("Expected success, got {result:?}"),
    }

    let result = evaluator.resolve_call(func, &[TypeId::STRING, TypeId::NUMBER, TypeId::STRING]);
    match result {
        CallResult::ArgumentTypeMismatch {
            index,
            expected,
            actual,
            ..
        } => {
            assert_eq!(index, 0);
            assert_eq!(expected, tuple_rest);
            let Some(TypeData::Tuple(actual_elements)) = interner.lookup(actual) else {
                panic!(
                    "expected aggregate tuple actual, got {:?}",
                    interner.lookup(actual)
                );
            };
            let actual_elements = interner.tuple_list(actual_elements);
            assert_eq!(actual_elements.len(), 3);
            assert_eq!(actual_elements[1].type_id, TypeId::NUMBER);
        }
        _ => panic!("Expected ArgumentTypeMismatch, got {result:?}"),
    }
}

/// Calling a variadic tuple rest param function with too few args should produce
/// `ArgumentTypeMismatch` (TS2345), not `ArgumentCountMismatch` (TS2555).
/// E.g. `f1(...args: [...T[], Required])` called as `f1()` → TS2345.
#[test]
fn test_call_variadic_tuple_rest_empty_args_produces_type_mismatch() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    // Build tuple type: [...((arg: number) => void)[], (arg: string) => void]
    let num_fn = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("arg")),
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
    let str_fn = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("arg")),
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

    let rest_array = interner.array(num_fn);
    let tuple_type = interner.tuple(vec![
        TupleElement {
            type_id: rest_array,
            name: None,
            optional: false,
            rest: true,
        },
        TupleElement {
            type_id: str_fn,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: tuple_type,
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

    // Call with 0 args — should get ArgumentTypeMismatch (TS2345), not ArgumentCountMismatch
    let result = evaluator.resolve_call(func, &[]);
    match result {
        CallResult::ArgumentTypeMismatch {
            expected, actual, ..
        } => {
            // Expected: the variadic tuple type
            assert_eq!(expected, tuple_type);
            // Actual: an empty tuple []
            assert!(
                matches!(interner.lookup(actual), Some(TypeData::Tuple(elems)) if interner.tuple_list(elems).is_empty()),
                "Expected empty tuple for actual, got {:?}",
                interner.lookup(actual)
            );
        }
        _ => panic!(
            "Expected ArgumentTypeMismatch for empty args to variadic tuple rest, got {result:?}"
        ),
    }

    // Call with 1 arg (the required trailing element) — should succeed
    let result = evaluator.resolve_call(func, &[str_fn]);
    match result {
        CallResult::Success(ret) => assert_eq!(ret, TypeId::VOID),
        _ => panic!("Expected success with 1 arg to variadic tuple rest, got {result:?}"),
    }
}

#[test]
fn test_call_variadic_tuple_rest_with_trailing_element_uses_aggregate_mismatch() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    let rest_array = interner.array(TypeId::STRING);
    let tuple_type = interner.tuple(vec![
        TupleElement {
            type_id: rest_array,
            name: None,
            optional: false,
            rest: true,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: tuple_type,
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

    let result = evaluator.resolve_call(func, &[TypeId::STRING, TypeId::NUMBER, TypeId::STRING]);
    match result {
        CallResult::ArgumentTypeMismatch {
            index,
            expected,
            actual,
            ..
        } => {
            assert_eq!(index, 0);
            assert_eq!(expected, tuple_type);
            let Some(TypeData::Tuple(actual_elements)) = interner.lookup(actual) else {
                panic!(
                    "expected aggregate tuple actual, got {:?}",
                    interner.lookup(actual)
                );
            };
            let actual_elements = interner.tuple_list(actual_elements);
            assert_eq!(actual_elements.len(), 3);
            assert_eq!(actual_elements[1].type_id, TypeId::NUMBER);
        }
        _ => panic!("Expected aggregate ArgumentTypeMismatch, got {result:?}"),
    }
}

#[test]
fn test_property_access_on_never_returns_never() {
    // never is the bottom type — all property accesses are vacuously valid
    // and return never (the code is unreachable). tsc does not emit TS2339 on never.
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let result = evaluator.resolve_property_access(TypeId::NEVER, "anything");
    match result {
        PropertyAccessResult::Success { type_id: t, .. } => assert_eq!(t, TypeId::NEVER),
        _ => panic!("Property access on never should succeed with never, got {result:?}"),
    }

    let result = evaluator.resolve_property_access(TypeId::NEVER, "nonexistent");
    match result {
        PropertyAccessResult::Success { type_id: t, .. } => assert_eq!(t, TypeId::NEVER),
        _ => panic!("Any property on never should return never, got {result:?}"),
    }
}

#[test]
fn test_property_access_object() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    // { x: number, y: string }
    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("y"), TypeId::STRING),
    ]);

    // Access existing property
    let result = evaluator.resolve_property_access(obj, "x");
    match result {
        PropertyAccessResult::Success { type_id: t, .. } => assert_eq!(t, TypeId::NUMBER),
        _ => panic!("Expected success, got {result:?}"),
    }

    // Access non-existent property
    let result = evaluator.resolve_property_access(obj, "z");
    match result {
        PropertyAccessResult::PropertyNotFound { .. } => {}
        _ => panic!("Expected PropertyNotFound, got {result:?}"),
    }
}

#[test]
fn test_property_access_function_members() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let func = interner.function(FunctionShape {
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let result = evaluator.resolve_property_access(func, "call");
    match result {
        PropertyAccessResult::Success { type_id, .. } => {
            let Some(TypeData::Function(shape_id)) = interner.lookup(type_id) else {
                panic!("Expected call to resolve to function type");
            };
            let shape = interner.function_shape(shape_id);
            let rest_array = interner.array(TypeId::ANY);
            assert_eq!(shape.return_type, TypeId::ANY);
            assert_eq!(shape.params.len(), 1);
            assert!(shape.params[0].rest);
            assert_eq!(shape.params[0].type_id, rest_array);
        }
        _ => panic!("Expected success, got {result:?}"),
    }

    let result = evaluator.resolve_property_access(func, "length");
    match result {
        PropertyAccessResult::Success { type_id: t, .. } => assert_eq!(t, TypeId::NUMBER),
        _ => panic!("Expected success, got {result:?}"),
    }

    let result = evaluator.resolve_property_access(func, "toString");
    match result {
        PropertyAccessResult::Success { type_id, .. } => {
            let Some(TypeData::Function(shape_id)) = interner.lookup(type_id) else {
                panic!("Expected toString to resolve to function type");
            };
            let shape = interner.function_shape(shape_id);
            assert_eq!(shape.return_type, TypeId::STRING);
        }
        _ => panic!("Expected success, got {result:?}"),
    }
}

#[test]
fn test_property_access_callable_members() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let call_sig = CallSignature {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_method: false,
    };
    let callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![call_sig],
        construct_signatures: vec![],
        properties: vec![],
        ..Default::default()
    });

    let result = evaluator.resolve_property_access(callable, "bind");
    match result {
        PropertyAccessResult::Success { type_id, .. } => {
            let Some(TypeData::Function(shape_id)) = interner.lookup(type_id) else {
                panic!("Expected bind to resolve to function type");
            };
            let shape = interner.function_shape(shape_id);
            assert_eq!(shape.return_type, TypeId::ANY);
        }
        _ => panic!("Expected success, got {result:?}"),
    }
}

#[test]
fn test_property_access_optional_property() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let obj = interner.object(vec![PropertyInfo::opt(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    let result = evaluator.resolve_property_access(obj, "x");
    match result {
        PropertyAccessResult::Success {
            type_id,
            write_type: _,
            from_index_signature,
        } => {
            let expected = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
            assert_eq!(type_id, expected);
            assert!(!from_index_signature);
        }
        _ => panic!("Expected success, got {result:?}"),
    }
}
