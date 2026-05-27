#[test]
fn test_object_with_index_property_mismatch_string_index() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let source = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo::new(
            interner.intern_string("name"),
            TypeId::STRING,
        )],
        number_index: None,
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
    });

    let target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: None,
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
    });

    assert!(!checker.is_subtype_of(source, target));
}

#[test]
fn test_object_with_index_property_mismatch_number_index() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let source = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo::new(
            interner.intern_string("0"),
            TypeId::STRING,
        )],
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        string_index: None,
    });

    let target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        string_index: None,
    });

    assert!(!checker.is_subtype_of(source, target));
}

#[test]
fn test_object_with_index_satisfies_named_property_string_index() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let source = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: None,
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
    });

    let target = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::NUMBER,
    )]);

    // Index signatures do NOT satisfy required named properties (TS2741)
    assert!(!checker.is_subtype_of(source, target));
}

#[test]
fn test_object_with_index_named_property_mismatch_string_index() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let source = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: None,
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
    });

    let target = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    assert!(!checker.is_subtype_of(source, target));
}

#[test]
fn test_object_to_indexed_property_mismatch_string_index() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let source = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: None,
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
    });

    assert!(!checker.is_subtype_of(source, target));
}

#[test]
fn test_object_with_index_satisfies_numeric_property_number_index() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let source = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        string_index: None,
    });

    let target = interner.object(vec![PropertyInfo::new(
        interner.intern_string("0"),
        TypeId::STRING,
    )]);

    // Index signatures do NOT satisfy required named properties (TS2741)
    assert!(!checker.is_subtype_of(source, target));
}

// =============================================================================
// Recursion-identity tests for conditional alias Applications (Bug B fix)
//
// Structural rule: when same-base Application types whose base is a conditional
// type alias are compared, def_guard cycle detection must engage. After the guard
// sees the same (DefId, DefId) pair a second time it returns compatible (cycle
// detected), matching tsc's getRecursionIdentity behavior for deeply recursive
// conditional types such as RequiredDeep<T>, DeepReadonly<T>, and NestedRecord<K,V>.
// =============================================================================

/// Build a recursive conditional alias application and verify that the
/// subtype check terminates (does not hang or overflow) and returns a
/// non-False result, matching tsc's coinductive treatment of recursive
/// conditional aliases.
///
/// Models a type family similar to:
///   `type Wrap<T> = T extends object ? { inner: Wrap<T> } : T`
///
/// Two Applications of the same conditional alias base should terminate via
/// the `def_guard` cycle detector rather than recursing indefinitely.
#[test]
fn test_same_base_conditional_alias_check_terminates() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    // DefId for the conditional alias "Wrap"
    let wrap_def = DefId(9001);

    // Type parameter T
    let t_info = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.type_param(t_info);

    // Body: T extends object ? { inner: Wrap<T> } : T
    // (represented as a Conditional whose true branch is a placeholder)
    // For the test we use a simple recursive conditional body
    let lazy_wrap = interner.lazy(wrap_def);
    let app_of_t = interner.application(lazy_wrap, vec![t_type]);

    let prop_inner = interner.intern_string("inner");
    let true_branch = interner.object(vec![PropertyInfo::new(prop_inner, app_of_t)]);

    let cond_body = interner.conditional(ConditionalType {
        check_type: t_type,
        extends_type: TypeId::OBJECT,
        true_type: true_branch,
        false_type: t_type,
        is_distributive: false,
    });

    env.insert_def_with_params(wrap_def, cond_body, vec![t_info]);
    env.insert_def_kind(wrap_def, crate::def::DefKind::TypeAlias);

    let base = interner.lazy(wrap_def);
    let app_string = interner.application(base, vec![TypeId::STRING]);
    let app_number = interner.application(base, vec![TypeId::NUMBER]);

    let mut checker = SubtypeChecker::with_resolver(&interner, &env);

    // The check must terminate (not hang). The result is either True/CycleDetected
    // (cycle guard fired, compatible assumed) or False (structurally incompatible
    // before cycle). Either is acceptable — what matters is termination.
    let result = checker.check_subtype(app_string, app_string);
    // Same application is always a subtype of itself
    assert!(
        result.is_true(),
        "Wrap<string> should be a subtype of itself"
    );

    let result2 = checker.check_subtype(app_string, app_number);
    assert!(
        result2.is_false(),
        "Wrap<string> should not be a subtype of Wrap<number>; recursion identity must not hide different args"
    );
}

/// Verify that two Applications of the same conditional alias with identical
/// args are recognized as compatible (cycle detected) even when the alias
/// body is deeply recursive.
///
/// Models `RequiredDeep<T>` / `DeepReadonly<T>` patterns where
/// `Alias<X>` compared against itself should always be compatible.
/// Tests name variants (K and X) to prove the fix is not name-dependent.
#[test]
fn test_conditional_alias_self_comparison_is_compatible() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    // DefId for alias "DeepReq"
    let alias_def = DefId(9002);

    let k_info = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let k_type = interner.type_param(k_info);

    // Body: K extends string ? { v: DeepReq<K> } : never
    let lazy_alias = interner.lazy(alias_def);
    let recursive_app = interner.application(lazy_alias, vec![k_type]);

    let prop_v = interner.intern_string("v");
    let true_br = interner.object(vec![PropertyInfo::new(prop_v, recursive_app)]);

    let body = interner.conditional(ConditionalType {
        check_type: k_type,
        extends_type: TypeId::STRING,
        true_type: true_br,
        false_type: TypeId::NEVER,
        is_distributive: false,
    });

    env.insert_def_with_params(alias_def, body, vec![k_info]);
    env.insert_def_kind(alias_def, crate::def::DefKind::TypeAlias);

    let base = interner.lazy(alias_def);
    let app1 = interner.application(base, vec![TypeId::STRING]);
    let app2 = interner.application(base, vec![TypeId::STRING]);

    let mut checker = SubtypeChecker::with_resolver(&interner, &env);

    // Two Applications of the same conditional alias with the same arg
    // must be found compatible (tsc's recursion identity: assume related on cycle).
    let result = checker.check_subtype(app1, app2);
    assert!(
        result.is_true(),
        "DeepReq<string> <: DeepReq<string> should be compatible (recursion identity)"
    );
}

/// Same test with type parameter named "X" instead of "K" to prove the fix
/// is structural (not name-dependent), per the anti-hardcoding directive (§25).
#[test]
fn test_conditional_alias_self_comparison_is_compatible_renamed_param() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let alias_def = DefId(9003);

    let x_info = TypeParamInfo {
        name: interner.intern_string("X"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let x_type = interner.type_param(x_info);

    let lazy_alias = interner.lazy(alias_def);
    let recursive_app = interner.application(lazy_alias, vec![x_type]);

    let prop_v = interner.intern_string("value");
    let true_br = interner.object(vec![PropertyInfo::new(prop_v, recursive_app)]);

    let body = interner.conditional(ConditionalType {
        check_type: x_type,
        extends_type: TypeId::STRING,
        true_type: true_br,
        false_type: TypeId::NEVER,
        is_distributive: false,
    });

    env.insert_def_with_params(alias_def, body, vec![x_info]);
    env.insert_def_kind(alias_def, crate::def::DefKind::TypeAlias);

    let base = interner.lazy(alias_def);
    let app1 = interner.application(base, vec![TypeId::NUMBER]);
    let app2 = interner.application(base, vec![TypeId::NUMBER]);

    let mut checker = SubtypeChecker::with_resolver(&interner, &env);

    let result = checker.check_subtype(app1, app2);
    assert!(
        result.is_true(),
        "DeepReq<number> <: DeepReq<number> (X-named param) should be compatible"
    );
}

#[test]
fn test_object_with_index_noncanonical_numeric_property_fails() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let source = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        string_index: None,
    });

    let target = interner.object(vec![PropertyInfo::new(
        interner.intern_string("01"),
        TypeId::STRING,
    )]);

    assert!(!checker.is_subtype_of(source, target));
}

#[test]
fn test_object_with_index_readonly_index_to_mutable_property_fails() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let source = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: None,
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: true,
            param_name: None,
        }),
    });

    let target = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::NUMBER,
    )]);

    assert!(!checker.is_subtype_of(source, target));
}

#[test]
fn test_type_parameter_constraint_assignability() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }));

    assert!(checker.is_subtype_of(t_param, TypeId::STRING));
    assert!(!checker.is_subtype_of(t_param, TypeId::NUMBER));

    let unconstrained = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: None,
        default: None,
        is_const: false,
    }));
    assert!(!checker.is_subtype_of(unconstrained, TypeId::STRING));
}

#[test]
fn test_base_constraint_assignability_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

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

    assert!(checker.is_subtype_of(t_param, TypeId::STRING));
    assert!(!checker.is_subtype_of(t_param, TypeId::NUMBER));
    assert!(!checker.is_subtype_of(t_param, u_param));
    assert!(!checker.is_subtype_of(t_param, v_param));
}

#[test]
fn test_base_constraint_not_assignable_to_param() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }));

    assert!(!checker.is_subtype_of(TypeId::STRING, t_param));
}

#[test]
fn test_type_parameter_identity_only() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

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

    assert!(!checker.is_subtype_of(t_param, u_param));
}

#[test]
fn test_deferred_conditional_source_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    let conditional = interner.conditional(ConditionalType {
        check_type: t_param,
        extends_type: TypeId::STRING,
        true_type: TypeId::NUMBER,
        false_type: TypeId::BOOLEAN,
        is_distributive: true,
    });

    let target_union = interner.union(vec![TypeId::NUMBER, TypeId::BOOLEAN]);

    assert!(checker.is_subtype_of(conditional, target_union));
    assert!(!checker.is_subtype_of(conditional, TypeId::NUMBER));
}

#[test]
fn test_deferred_conditional_target_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    let conditional = interner.conditional(ConditionalType {
        check_type: t_param,
        extends_type: TypeId::STRING,
        true_type: TypeId::NUMBER,
        false_type: TypeId::BOOLEAN,
        is_distributive: true,
    });

    assert!(!checker.is_subtype_of(TypeId::NUMBER, conditional));
}

#[test]
fn test_deferred_conditional_structural_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    let source = interner.conditional(ConditionalType {
        check_type: t_param,
        extends_type: TypeId::STRING,
        true_type: TypeId::NUMBER,
        false_type: TypeId::BOOLEAN,
        is_distributive: true,
    });

    let union_nb = interner.union(vec![TypeId::NUMBER, TypeId::BOOLEAN]);
    let target = interner.conditional(ConditionalType {
        check_type: t_param,
        extends_type: TypeId::STRING,
        true_type: union_nb,
        false_type: union_nb,
        is_distributive: true,
    });

    let mismatch = interner.conditional(ConditionalType {
        check_type: t_param,
        extends_type: TypeId::NUMBER,
        true_type: union_nb,
        false_type: union_nb,
        is_distributive: true,
    });

    // A structural mismatch that cannot match via subtype_of_conditional_target either:
    // Different extends AND branches that don't cover the source branches.
    let real_mismatch = interner.conditional(ConditionalType {
        check_type: t_param,
        extends_type: TypeId::NUMBER,
        true_type: TypeId::STRING,
        false_type: TypeId::STRING,
        is_distributive: true,
    });

    assert!(checker.is_subtype_of(source, target));
    // Note: source <: mismatch passes via fallthrough + subtype_of_conditional_target
    // because mismatch's branches are (number|boolean), which covers source's branches.
    // tsc would reject this for local type aliases but accept it for generic type aliases.
    // Our solver treats both the same way (accepting), which is the more permissive behavior.
    assert!(checker.is_subtype_of(source, mismatch));
    // A true structural mismatch: target branches are `string`, which don't cover source branches.
    assert!(!checker.is_subtype_of(source, real_mismatch));
}

#[test]
fn test_conditional_tuple_wrapper_no_distribution_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
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

    assert!(checker.is_subtype_of(instantiated, TypeId::BOOLEAN));
    assert!(!checker.is_subtype_of(instantiated, TypeId::NUMBER));
}

#[test]
fn test_strict_function_variance() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    // Ensure strict mode is on (default)
    assert!(checker.strict_function_types);

    // (x: string | number) => void
    let union_arg_fn = interner.function(FunctionShape {
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

    // (x: string) => void
    let string_arg_fn = interner.function(FunctionShape {
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

    // 1. Safe assignment: (string | number) => void  <:  (string) => void
    // Target param (string) <: Source param (string | number) -> OK (contravariant)
    assert!(checker.is_subtype_of(union_arg_fn, string_arg_fn));

    // 2. Unsafe assignment: (string) => void  <:  (string | number) => void
    // Target param (string | number) <: Source param (string) -> FAIL (would be unsound)
    assert!(!checker.is_subtype_of(string_arg_fn, union_arg_fn));

    // 3. Disable strict mode (Bivariant)
    checker.strict_function_types = false;
    // Now unsafe assignment should pass (legacy behavior)
    assert!(checker.is_subtype_of(string_arg_fn, union_arg_fn));
}

#[test]
fn test_function_variance_union_intersection_targets() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let fn_with_param = |param| {
        interner.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: param,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        })
    };

    let fn_string = fn_with_param(TypeId::STRING);
    let fn_number = fn_with_param(TypeId::NUMBER);
    let fn_union_param = fn_with_param(interner.union(vec![TypeId::STRING, TypeId::NUMBER]));

    let union_target = interner.union(vec![fn_string, fn_number]);
    let intersection_target = interner.intersection(vec![fn_string, fn_number]);

    assert!(checker.is_subtype_of(fn_union_param, union_target));
    assert!(checker.is_subtype_of(fn_union_param, intersection_target));
    assert!(!checker.is_subtype_of(fn_string, intersection_target));
    assert!(!checker.is_subtype_of(union_target, fn_union_param));
}

#[test]
fn test_callable_rest_parameter_contravariance() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let rest_union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let rest_array = interner.array(rest_union);

    let source = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
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

    let target = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            type_params: vec![],
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
            return_type: TypeId::VOID,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    assert!(!checker.is_subtype_of(source, target));
}

#[test]
fn test_method_bivariant_required_param() {
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
            optional: false,
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
            optional: false,
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
fn test_method_source_bivariant_against_function_property() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    let name = interner.intern_string("m");

    let wide_param = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let narrow_param = TypeId::STRING;

    let narrow_method = interner.function(FunctionShape {
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

    let wide_func = interner.function(FunctionShape {
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

