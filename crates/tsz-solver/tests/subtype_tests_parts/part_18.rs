// Opaque variadic rest relation tests.
//
// Rule: a bare source `...T` is universally quantified. Relating functions
// must preserve that binder rather than project through `T`'s constraint and
// compare only the resulting array element.

fn scoped_rest_param(
    interner: &TypeInterner,
    name: &str,
    node: u32,
    constraint: TypeId,
) -> TypeId {
    let file = interner.intern_string("opaque-rest-relation.ts");
    interner.fresh_type_param(TypeParamInfo {
        name: interner.intern_string(name),
        constraint: Some(constraint),
        default: None,
        is_const: false,
        origin: TypeParamOrigin::DeclScoped { file, node },
    })
}

fn rest_function(interner: &TypeInterner, rest_type: TypeId) -> TypeId {
    rest_function_with_method(interner, rest_type, false)
}

fn rest_function_with_method(
    interner: &TypeInterner,
    rest_type: TypeId,
    is_method: bool,
) -> TypeId {
    rest_function_full(interner, rest_type, TypeId::VOID, is_method)
}

fn rest_function_full(
    interner: &TypeInterner,
    rest_type: TypeId,
    return_type: TypeId,
    is_method: bool,
) -> TypeId {
    interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("values")),
            type_id: rest_type,
            optional: false,
            rest: true,
arity_only_optional: false,
        }],
        this_type: None,
        return_type,
        type_predicate: None,
        is_constructor: false,
        is_method,
    })
}

#[test]
fn empty_tuple_does_not_satisfy_an_opaque_variadic_target() {
    let interner = TypeInterner::new();
    let unknown_array = interner.array(TypeId::UNKNOWN);
    let target_t = scoped_rest_param(&interner, "Target", 0, unknown_array);
    let empty = interner.tuple(vec![]);
    let spread_target = interner.tuple(vec![TupleElement {
        type_id: target_t,
        name: None,
        optional: false,
        rest: true,
    }]);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        !checker.is_subtype_of(empty, spread_target),
        "a concrete empty tuple cannot witness the universally quantified `[...T]`"
    );
    assert!(
        checker.explain_failure(empty, spread_target).is_some(),
        "failure analysis must agree with the tuple relation"
    );
}

#[test]
fn empty_tuple_keeps_an_inference_placeholder_variadic_provisional() {
    let interner = TypeInterner::new();
    let placeholder = interner.fresh_type_param(TypeParamInfo {
        name: interner.intern_string("PendingPack"),
        constraint: Some(interner.array(TypeId::UNKNOWN)),
        default: None,
        is_const: false,
        origin: TypeParamOrigin::InferPlaceholder { id: 4_201 },
    });
    let empty = interner.tuple(vec![]);
    let spread_target = interner.tuple(vec![TupleElement {
        type_id: placeholder,
        name: None,
        optional: false,
        rest: true,
    }]);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.is_subtype_of(empty, spread_target),
        "an inference placeholder is provisional rather than universally quantified"
    );
    assert!(
        checker.explain_failure(empty, spread_target).is_none(),
        "failure analysis must agree with the provisional tuple relation"
    );
}

#[test]
fn bare_source_rest_does_not_collapse_to_constraint_element() {
    let interner = TypeInterner::new();
    let unknown_array = interner.array(TypeId::UNKNOWN);
    let source_t = scoped_rest_param(&interner, "Values", 1, unknown_array);
    let source = rest_function(&interner, source_t);

    let mut checker = SubtypeChecker::new(&interner);
    checker.allow_bivariant_rest = true;

    for target_rest in [
        unknown_array,
        interner.array(TypeId::STRING),
        interner.array(TypeId::NEVER),
    ] {
        let target = rest_function(&interner, target_rest);
        assert!(
            !checker.is_subtype_of(source, target),
            "bare `...T` must not collapse to target rest {target_rest:?}"
        );
    }
}

#[test]
fn bare_source_rest_accepts_only_same_binder_or_legacy_any_array() {
    let interner = TypeInterner::new();
    let unknown_array = interner.array(TypeId::UNKNOWN);
    let source_t = scoped_rest_param(&interner, "Values", 1, unknown_array);
    let distinct_u = scoped_rest_param(&interner, "Other", 2, unknown_array);
    let source = rest_function(&interner, source_t);

    let mut checker = SubtypeChecker::new(&interner);
    checker.allow_bivariant_rest = true;

    assert!(
        checker.is_subtype_of(source, rest_function(&interner, source_t)),
        "the same outer rest binder must remain assignable"
    );
    let same_binder_spread = interner.tuple(vec![TupleElement {
        type_id: source_t,
        name: None,
        optional: false,
        rest: true,
    }]);
    assert!(
        checker.is_subtype_of(source, rest_function(&interner, same_binder_spread)),
        "a transparent `[...T]` rest keeps the same binder"
    );
    assert!(
        !checker.is_subtype_of(source, rest_function(&interner, distinct_u)),
        "distinct binders with identical constraints must remain distinct"
    );
    assert!(
        checker.is_subtype_of(
            source,
            rest_function(&interner, interner.array(TypeId::ANY)),
        ),
        "concrete `any[]` is the legacy bivariant-rest wildcard"
    );
}

#[test]
fn alpha_equivalent_local_generic_rests_accept_renamed_binders() {
    let interner = TypeInterner::new();
    let unknown_array = interner.array(TypeId::UNKNOWN);
    let source_t = scoped_rest_param(&interner, "SourcePack", 2_001, unknown_array);
    let target_u = scoped_rest_param(&interner, "TargetPack", 2_002, unknown_array);
    let Some(TypeData::TypeParameter(source_info)) = interner.lookup(source_t) else {
        panic!("source rest must be a type parameter");
    };
    let Some(TypeData::TypeParameter(target_info)) = interner.lookup(target_u) else {
        panic!("target rest must be a type parameter");
    };
    let generic_rest = |info, rest_type| {
        interner.function(FunctionShape {
            type_params: vec![info],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("values")),
                type_id: rest_type,
                optional: false,
                rest: true,
arity_only_optional: false,
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        })
    };
    let source = generic_rest(source_info, source_t);
    let target = generic_rest(target_info, target_u);

    let mut checker = SubtypeChecker::new(&interner);
    checker.allow_bivariant_rest = true;
    assert!(
        checker.is_subtype_of(source, target),
        "alpha-equivalent local generic rest binders may be renamed"
    );
    assert!(
        checker.is_subtype_of(target, source),
        "alpha-equivalent local generic rest binders relate symmetrically"
    );
}

#[test]
fn bare_source_rest_respects_function_parameter_variance_for_unknown_array() {
    let interner = TypeInterner::new();
    let unknown_array = interner.array(TypeId::UNKNOWN);
    let source_t = scoped_rest_param(&interner, "Values", 3, unknown_array);
    let source = rest_function(&interner, source_t);
    let function_target = rest_function(&interner, unknown_array);
    let method_target = rest_function_with_method(&interner, unknown_array, true);

    let mut checker = SubtypeChecker::new(&interner);
    checker.allow_bivariant_rest = true;
    assert!(
        !checker.is_subtype_of(source, function_target),
        "strict function-property variance preserves the opaque source rest"
    );

    checker.strict_function_types = false;
    assert!(
        checker.is_subtype_of(source, function_target),
        "non-strict function variance accepts the unknown rest target"
    );

    checker.in_callback_param_check = true;
    assert!(
        !checker.is_subtype_of(source, function_target),
        "callback mode restores strict variance for the immediate signature"
    );

    checker.strict_function_types = true;
    assert!(
        checker.is_subtype_of(source, method_target),
        "method bivariance accepts the unknown rest target"
    );

    checker.disable_method_bivariance = true;
    assert!(
        !checker.is_subtype_of(source, method_target),
        "disabling method bivariance restores the strict relation"
    );
}

#[test]
fn bare_source_rest_rejects_fixed_target_slots_regardless_of_slot_type() {
    let interner = TypeInterner::new();
    let unknown_array = interner.array(TypeId::UNKNOWN);
    let source_t = scoped_rest_param(&interner, "Values", 5, unknown_array);
    let source = rest_function(&interner, source_t);

    let fixed_function = |fixed_type| {
        interner.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo::unnamed(fixed_type)],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        })
    };

    let mut checker = SubtypeChecker::new(&interner);
    checker.allow_bivariant_rest = true;
    assert!(
        !checker.is_subtype_of(source, fixed_function(source_t)),
        "a fixed `x: T` is not a variadic `...T`"
    );
    assert!(
        !checker.is_subtype_of(
            source,
            fixed_function(interner.array(TypeId::ANY)),
        ),
        "the `any[]` wildcard applies only to a rest slot"
    );

    let empty_target = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    assert!(
        checker.is_subtype_of(source, empty_target),
        "a target with no tail slots does not consume the source rest"
    );
}

#[test]
fn bare_source_rest_rejects_fixed_member_of_union_tuple_rest() {
    let interner = TypeInterner::new();
    let unknown_array = interner.array(TypeId::UNKNOWN);
    let source_t = scoped_rest_param(&interner, "Values", 6, unknown_array);
    let source = rest_function(&interner, source_t);
    let fixed_t = interner.tuple(vec![TupleElement::fixed(source_t)]);
    let spread_t = interner.tuple(vec![TupleElement {
        type_id: source_t,
        name: None,
        optional: false,
        rest: true,
    }]);
    let empty = interner.tuple(vec![]);

    let mut checker = SubtypeChecker::new(&interner);
    checker.allow_bivariant_rest = true;
    for (label, target_union) in [
        ("fixed tuple", interner.union(vec![fixed_t, empty])),
        ("spread tuple", interner.union(vec![spread_t, empty])),
        (
            "any-array union",
            interner.union_preserve_members(vec![interner.array(TypeId::ANY), empty]),
        ),
    ] {
        assert!(
            !checker.is_subtype_of(source, rest_function(&interner, target_union)),
            "an opaque source rest must not collapse into {label} rest {target_union:?}"
        );
    }

    let provisional_target = rest_function(
        &interner,
        interner.union_preserve_members(vec![fixed_t, spread_t]),
    );
    checker.allow_provisional_rest_union = true;
    let provisional_reason = checker.explain_failure(source, provisional_target);
    assert!(
        checker.is_subtype_of(source, provisional_target),
        "generic-call aggregate validation keeps the inferred union provisional: {provisional_reason:?}"
    );
    checker.allow_provisional_rest_union = false;
    assert!(
        !checker.is_subtype_of(source, provisional_target),
        "provisional aggregate verdicts must not poison ordinary relation caching"
    );

    let wrapped_provisional_target = rest_function(
        &interner,
        interner.no_infer(interner.union_preserve_members(vec![fixed_t, spread_t])),
    );
    checker.allow_provisional_rest_union = true;
    assert!(
        checker.is_subtype_of(source, wrapped_provisional_target),
        "transparent wrappers around the inferred union must keep the provisional policy"
    );
    assert!(
        checker
            .explain_failure(source, wrapped_provisional_target)
            .is_none(),
        "relation and failure explanation must agree on a wrapped union surface"
    );
    checker.allow_provisional_rest_union = false;
    assert!(
        !checker.is_subtype_of(source, wrapped_provisional_target),
        "a wrapped user-written union remains rigid without call provenance"
    );

    checker.strict_function_types = false;
    assert!(
        checker.is_subtype_of(
            source,
            rest_function(&interner, interner.union(vec![spread_t, empty])),
        ),
        "non-strict function variance accepts a union rest containing the source spread"
    );
    assert!(
        checker.is_subtype_of(
            source,
            rest_function(
                &interner,
                interner.union_preserve_members(vec![fixed_t, spread_t]),
            ),
        ),
        "non-strict whole-rest compatibility also admits the fixed union member"
    );
}

#[test]
fn provisional_rest_union_is_scoped_to_direct_function_relations_and_cache_keys() {
    let interner = TypeInterner::new();
    let unknown_array = interner.array(TypeId::UNKNOWN);
    let source_t = scoped_rest_param(&interner, "Values", 7, unknown_array);
    let source = rest_function(&interner, source_t);
    let fixed_t = interner.tuple(vec![TupleElement::fixed(source_t)]);
    let spread_t = interner.tuple(vec![TupleElement {
        type_id: source_t,
        name: None,
        optional: false,
        rest: true,
    }]);
    let target = rest_function(
        &interner,
        interner.union_preserve_members(vec![fixed_t, spread_t]),
    );
    let outer = |return_type| {
        interner.function(FunctionShape {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        })
    };
    let outer_source = outer(source);
    let outer_target = outer(target);

    let mut nested_first = SubtypeChecker::new(&interner);
    nested_first.allow_bivariant_rest = true;
    nested_first.allow_provisional_rest_union = true;
    assert!(
        !nested_first.is_subtype_of(outer_source, outer_target),
        "a nested user-declared rest union must keep ordinary strict semantics"
    );
    assert!(
        nested_first.is_subtype_of(source, target),
        "the direct provisional relation must not reuse the nested ordinary verdict"
    );

    let mut direct_first = SubtypeChecker::new(&interner);
    direct_first.allow_bivariant_rest = true;
    direct_first.allow_provisional_rest_union = true;
    assert!(direct_first.is_subtype_of(source, target));
    assert!(
        !direct_first.is_subtype_of(outer_source, outer_target),
        "the nested ordinary relation must not reuse the direct provisional verdict"
    );
}

#[test]
fn no_infer_preserves_bare_rest_binder_relation() {
    let interner = TypeInterner::new();
    let unknown_array = interner.array(TypeId::UNKNOWN);
    let source_t = scoped_rest_param(&interner, "Items", 10, unknown_array);
    let source_no_infer = interner.no_infer(source_t);

    let mut checker = SubtypeChecker::new(&interner);
    checker.allow_bivariant_rest = true;

    assert!(
        !checker.is_subtype_of(
            rest_function(&interner, source_no_infer),
            rest_function(&interner, unknown_array),
        ),
        "`NoInfer<T>` must not collapse to the element of T's constraint"
    );
    assert!(
        checker.is_subtype_of(
            rest_function(&interner, source_no_infer),
            rest_function(&interner, source_t),
        ),
        "`NoInfer<T>` and `T` denote the same rest binder"
    );
    assert!(
        checker.is_subtype_of(
            rest_function(&interner, source_t),
            rest_function(&interner, source_no_infer),
        ),
        "the same-binder rule is symmetric across `NoInfer`"
    );
}

#[test]
fn inference_placeholder_rest_keeps_provisional_union_compatibility() {
    let interner = TypeInterner::new();
    let unknown_array = interner.array(TypeId::UNKNOWN);
    let placeholder = interner.fresh_type_param(TypeParamInfo {
        name: interner.intern_string("Pending"),
        constraint: Some(unknown_array),
        default: None,
        is_const: false,
        origin: TypeParamOrigin::InferPlaceholder { id: 31 },
    });
    let fixed = interner.tuple(vec![
        TupleElement::fixed(TypeId::STRING),
        TupleElement::fixed(TypeId::NUMBER),
    ]);
    let spread = interner.tuple(vec![TupleElement {
        type_id: placeholder,
        name: None,
        optional: false,
        rest: true,
    }]);
    let target_rest = interner.union(vec![fixed, spread]);

    let mut checker = SubtypeChecker::new(&interner);
    checker.allow_bivariant_rest = true;
    assert!(
        checker.is_subtype_of(
            rest_function(&interner, placeholder),
            rest_function(&interner, target_rest),
        ),
        "a provisional inference placeholder must keep the aggregate variadic relation"
    );
}

#[test]
fn opaque_rest_failure_reason_uses_raw_rest_types() {
    let interner = TypeInterner::new();
    let unknown_array = interner.array(TypeId::UNKNOWN);
    let source_t = scoped_rest_param(&interner, "Args", 20, unknown_array);
    let source = rest_function(&interner, source_t);
    let target = rest_function(&interner, unknown_array);

    let mut checker = SubtypeChecker::new(&interner);
    checker.allow_bivariant_rest = true;
    assert!(!checker.is_subtype_of(source, target));

    let reason = checker.explain_failure(source, target);
    assert!(
        matches!(
            reason,
            Some(SubtypeFailureReason::ParameterTypeMismatch {
                source_param,
                target_param,
                ..
            }) if source_param == source_t && target_param == unknown_array
        ),
        "expected a raw-rest parameter mismatch, got {reason:?}"
    );
}

#[test]
fn same_binder_spread_reason_reaches_return_mismatch() {
    let interner = TypeInterner::new();
    let unknown_array = interner.array(TypeId::UNKNOWN);
    let source_t = scoped_rest_param(&interner, "Args", 21, unknown_array);
    let target_spread = interner.tuple(vec![TupleElement {
        type_id: source_t,
        name: None,
        optional: false,
        rest: true,
    }]);
    let source = rest_function_full(&interner, source_t, TypeId::STRING, false);
    let target = rest_function_full(&interner, target_spread, TypeId::NUMBER, false);

    let mut checker = SubtypeChecker::new(&interner);
    checker.allow_bivariant_rest = true;
    assert!(!checker.is_subtype_of(source, target));

    let reason = checker.explain_failure(source, target);
    assert!(
        matches!(reason, Some(SubtypeFailureReason::ReturnTypeMismatch { .. })),
        "same-binder `[...T]` parameters must not hide the return mismatch: {reason:?}"
    );
}

fn callable_with_signatures(
    interner: &TypeInterner,
    call_signatures: Vec<CallSignature>,
) -> TypeId {
    interner.callable(CallableShape {
        call_signatures,
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
        symbol: None,
        is_abstract: false,
    })
}

fn call_signature(params: Vec<ParamInfo>) -> CallSignature {
    CallSignature {
        type_params: vec![],
        params,
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_method: false,
    }
}

#[test]
fn bare_rest_visibility_query_covers_fixed_slots_and_union_rests() {
    let interner = TypeInterner::new();
    let unknown_array = interner.array(TypeId::UNKNOWN);
    let source_t = scoped_rest_param(&interner, "Pack", 30, unknown_array);
    let source_params = vec![ParamInfo {
        name: None,
        type_id: source_t,
        optional: false,
        rest: true,
arity_only_optional: false,
    }];
    let target_params = vec![ParamInfo::unnamed(source_t)];
    let source_callable =
        callable_with_signatures(&interner, vec![call_signature(source_params.clone())]);
    let target_callable =
        callable_with_signatures(&interner, vec![call_signature(target_params.clone())]);
    let source_function = rest_function(&interner, source_t);
    let target_function = interner.function(FunctionShape {
        type_params: vec![],
        params: target_params.clone(),
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    for (source, target) in [
        (source_callable, target_callable),
        (source_callable, target_function),
        (source_function, target_callable),
    ] {
        assert!(
            crate::type_queries::bare_source_rest_requires_visible_relation_failure(
                &interner,
                &crate::relations::subtype::NoopResolver,
                source,
                target,
            ),
            "the query must cover every direct function/callable pairing"
        );
    }

    let fixed = interner.tuple(vec![TupleElement::fixed(source_t)]);
    let spread = interner.tuple(vec![TupleElement {
        type_id: source_t,
        name: None,
        optional: false,
        rest: true,
    }]);
    let union_rest = interner.union_preserve_members(vec![fixed, spread]);
    let union_params = vec![ParamInfo {
        name: None,
        type_id: union_rest,
        optional: false,
        rest: true,
arity_only_optional: false,
    }];
    let union_target_callable =
        callable_with_signatures(&interner, vec![call_signature(union_params.clone())]);
    let union_target_function = rest_function(&interner, union_rest);
    for (source, target) in [
        (source_callable, union_target_callable),
        (source_callable, union_target_function),
        (source_function, union_target_callable),
        (source_function, union_target_function),
    ] {
        assert!(
            crate::type_queries::bare_source_rest_requires_visible_relation_failure(
                &interner,
                &crate::relations::subtype::NoopResolver,
                source,
                target,
            ),
            "a target rest union must keep the solver's failed relation visible"
        );
    }

    let overloaded_source = callable_with_signatures(
        &interner,
        vec![
            call_signature(source_params.clone()),
            call_signature(source_params),
        ],
    );
    assert!(
        crate::type_queries::bare_source_rest_requires_visible_relation_failure(
            &interner,
            &crate::relations::subtype::NoopResolver,
            overloaded_source,
            target_callable,
        ),
        "overloaded sources must not hide a fixed-slot relation failure"
    );
    let overloaded_target = callable_with_signatures(
        &interner,
        vec![
            call_signature(target_params),
            call_signature(union_params),
        ],
    );
    assert!(
        crate::type_queries::bare_source_rest_requires_visible_relation_failure(
            &interner,
            &crate::relations::subtype::NoopResolver,
            source_callable,
            overloaded_target,
        ),
        "overloaded targets must not hide a union-rest relation failure"
    );
}

#[test]
fn nested_overloaded_callable_tries_matching_source_after_rigid_rest_failure() {
    let interner = TypeInterner::new();
    let unknown_array = interner.array(TypeId::UNKNOWN);
    let pack = scoped_rest_param(&interner, "Pack", 40, unknown_array);
    let rest_params = vec![ParamInfo {
        name: None,
        type_id: pack,
        optional: false,
        rest: true,
arity_only_optional: false,
    }];
    let fixed_params = vec![ParamInfo::unnamed(pack)];
    let single_target =
        callable_with_signatures(&interner, vec![call_signature(rest_params.clone())]);
    let overloaded_source = callable_with_signatures(
        &interner,
        vec![
            call_signature(fixed_params),
            call_signature(rest_params),
        ],
    );

    let mut direct = SubtypeChecker::new(&interner);
    direct.strict_function_types = true;
    direct.allow_bivariant_rest = true;
    assert!(
        direct.is_subtype_of(overloaded_source, single_target),
        "one matching source overload must satisfy the sole target signature"
    );

    let consumer = |callback| {
        interner.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo::unnamed(callback)],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        })
    };
    let mut nested = SubtypeChecker::new(&interner);
    nested.strict_function_types = true;
    nested.allow_bivariant_rest = true;
    assert!(
        nested.is_subtype_of(consumer(single_target), consumer(overloaded_source)),
        "contravariant callback comparison must preserve callable overload quantification"
    );
}

#[test]
fn reset_clears_provisional_rest_function_depth() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.provisional_rest_union_function_depth = 7;
    checker.reset();
    assert_eq!(checker.provisional_rest_union_function_depth, 0);
}
