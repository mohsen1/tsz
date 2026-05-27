#[test]
fn test_private_brand_callable_with_brand() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // Callable types (constructors) can also have private brands
    let brand1 = interner.intern_string("__private_brand_Foo");
    let brand2 = interner.intern_string("__private_brand_Bar");

    let source = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: Vec::new(),
        construct_signatures: vec![CallSignature {
            params: Vec::new(),
            this_type: None,
            return_type: TypeId::ANY,
            type_predicate: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        properties: vec![PropertyInfo::new(brand1, TypeId::NEVER)],
        ..Default::default()
    });

    let target = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: Vec::new(),
        construct_signatures: vec![CallSignature {
            params: Vec::new(),
            this_type: None,
            return_type: TypeId::ANY,
            type_predicate: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        properties: vec![PropertyInfo::new(brand2, TypeId::NEVER)],
        ..Default::default()
    });

    // Different brands in callables = not assignable
    assert!(!checker.is_assignable(source, target));
}

/// Test: Mapped types with same constraint but different modifiers should be
/// structurally comparable (Readonly<T> assignable to Partial<T>).
#[test]
fn test_mapped_to_mapped_readonly_assignable_to_partial() {
    use crate::MappedModifier;

    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // Create a type parameter T (represented as a TypeParam)
    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Create keyof T
    let keyof_t = interner.intern(TypeData::KeyOf(t_param));

    // Create K (iteration parameter)
    let k_name = interner.intern_string("K");

    // Create T[K] (index access as template)
    let t_k = interner.intern(TypeData::IndexAccess(
        t_param,
        interner.intern(TypeData::TypeParameter(TypeParamInfo {
            name: k_name,
            constraint: None,
            default: None,
            is_const: false,
        })),
    ));

    // Readonly<T>: { readonly [K in keyof T]: T[K] }
    let readonly_t = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: k_name,
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keyof_t,
        name_type: None,
        template: t_k,
        readonly_modifier: Some(MappedModifier::Add),
        optional_modifier: None,
    });

    // Partial<T>: { [K in keyof T]?: T[K] }
    let partial_t = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: k_name,
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keyof_t,
        name_type: None,
        template: t_k,
        readonly_modifier: None,
        optional_modifier: Some(MappedModifier::Add),
    });

    // Readonly<T> should be assignable to Partial<T>
    // Because the template T[K] is assignable to T[K] | undefined
    assert!(
        checker.is_assignable(readonly_t, partial_t),
        "Readonly<T> should be assignable to Partial<T>"
    );
}

// ===========================================================================
// Tests for object→tuple explain: TS2741 for missing numeric properties
// ===========================================================================

#[test]
fn test_explain_object_to_tuple_missing_property() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // Source: object { 0: string, 1: number } (like StrNum interface)
    // with a number index signature (to qualify as array-like)
    let prop0 = PropertyInfo::new(interner.intern_string("0"), TypeId::STRING);
    let prop1 = PropertyInfo::new(interner.intern_string("1"), TypeId::NUMBER);
    let source = interner.object_with_index(ObjectShape {
        properties: vec![prop0, prop1],
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: interner.union(vec![TypeId::STRING, TypeId::NUMBER]),
            readonly: false,
            param_name: None,
        }),
        string_index: None,
        flags: ObjectFlags::empty(),
        symbol: None,
    });

    // Target: tuple [number, number, number] — has required element at index 2
    let target = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
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
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let reason = checker.explain_failure(source, target);
    let expected_prop = interner.intern_string("2");
    assert!(
        matches!(reason, Some(SubtypeFailureReason::MissingProperty { property_name, .. })
            if property_name == expected_prop),
        "Expected MissingProperty for index '2', got: {reason:?}"
    );
}

#[test]
fn test_explain_tuple_element_drills_into_missing_property() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // Source tuple: [{}]  (empty object at index 0)
    let empty_obj = interner.object(vec![]);
    let source = interner.tuple(vec![TupleElement {
        type_id: empty_obj,
        name: None,
        optional: false,
        rest: false,
    }]);

    // Target tuple: [{a: string}]  (object with required 'a' at index 0)
    let obj_with_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);
    let target = interner.tuple(vec![TupleElement {
        type_id: obj_with_a,
        name: None,
        optional: false,
        rest: false,
    }]);

    let reason = checker.explain_failure(source, target);
    let expected_prop = interner.intern_string("a");
    assert!(
        matches!(reason, Some(SubtypeFailureReason::MissingProperty { property_name, .. })
            if property_name == expected_prop),
        "Expected MissingProperty for 'a' (drilled into element), got: {reason:?}"
    );
}

// ===========================================================================
// Tests for tuple↔array comparability (TS2352 type assertion checking)
// ===========================================================================

#[test]
fn test_tuple_to_array_comparable() {
    use crate::type_queries::flow::types_are_comparable;

    let interner = TypeInterner::new();

    // [number, string] should be comparable to number[] (because number overlaps)
    let tuple = interner.tuple(vec![
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
    let num_array = interner.array(TypeId::NUMBER);

    assert!(
        types_are_comparable(&interner, tuple, num_array),
        "[number, string] should be comparable to number[]"
    );
}

#[test]
fn test_tuple_to_array_not_comparable_disjoint_types() {
    use crate::type_queries::flow::types_are_comparable;

    let interner = TypeInterner::new();

    // [string, string] should NOT be comparable to number[]
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
    let num_array = interner.array(TypeId::NUMBER);

    assert!(
        !types_are_comparable(&interner, tuple, num_array),
        "[string, string] should NOT be comparable to number[]"
    );
}

#[test]
fn test_array_to_tuple_comparable() {
    use crate::type_queries::flow::types_are_comparable;

    let interner = TypeInterner::new();

    // number[] should be comparable to [number, string] (symmetric)
    let num_array = interner.array(TypeId::NUMBER);
    let tuple = interner.tuple(vec![
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

    assert!(
        types_are_comparable(&interner, num_array, tuple),
        "number[] should be comparable to [number, string]"
    );
}

#[test]
fn test_readonly_to_mutable_explain_failure_ts4104() {
    // readonly number[] → boolean[] should produce ReadonlyToMutableAssignment
    let interner = TypeInterner::new();
    let readonly_num_array = interner.readonly_array(TypeId::NUMBER);
    let bool_array = interner.array(TypeId::BOOLEAN);

    let mut checker = CompatChecker::new(&interner);
    checker.strict_null_checks = true;
    assert!(
        !checker.is_assignable(readonly_num_array, bool_array),
        "readonly number[] should not be assignable to boolean[]"
    );
    let reason = checker.explain_failure(readonly_num_array, bool_array);
    assert!(
        matches!(
            reason,
            Some(SubtypeFailureReason::ReadonlyToMutableAssignment { .. })
        ),
        "Expected ReadonlyToMutableAssignment, got {reason:?}"
    );
}

#[test]
fn test_readonly_to_mutable_array_same_element_type() {
    // readonly number[] → number[] should produce ReadonlyToMutableAssignment
    let interner = TypeInterner::new();
    let readonly_num_array = interner.readonly_array(TypeId::NUMBER);
    let num_array = interner.array(TypeId::NUMBER);

    let mut checker = CompatChecker::new(&interner);
    checker.strict_null_checks = true;
    assert!(
        !checker.is_assignable(readonly_num_array, num_array),
        "readonly number[] should not be assignable to number[]"
    );
    let reason = checker.explain_failure(readonly_num_array, num_array);
    assert!(
        matches!(
            reason,
            Some(SubtypeFailureReason::ReadonlyToMutableAssignment { .. })
        ),
        "Expected ReadonlyToMutableAssignment for same element type, got {reason:?}"
    );
}

#[test]
fn test_readonly_tuple_to_mutable_tuple_explain_failure() {
    // readonly [number] → [boolean] should produce ReadonlyToMutableAssignment
    let interner = TypeInterner::new();
    let readonly_tuple = interner.readonly_tuple(vec![TupleElement {
        type_id: TypeId::NUMBER,
        name: None,
        optional: false,
        rest: false,
    }]);
    let mutable_tuple = interner.tuple(vec![TupleElement {
        type_id: TypeId::BOOLEAN,
        name: None,
        optional: false,
        rest: false,
    }]);

    let mut checker = CompatChecker::new(&interner);
    checker.strict_null_checks = true;
    assert!(
        !checker.is_assignable(readonly_tuple, mutable_tuple),
        "readonly [number] should not be assignable to [boolean]"
    );
    let reason = checker.explain_failure(readonly_tuple, mutable_tuple);
    assert!(
        matches!(
            reason,
            Some(SubtypeFailureReason::ReadonlyToMutableAssignment { .. })
        ),
        "Expected ReadonlyToMutableAssignment for tuples, got {reason:?}"
    );
}

#[test]
fn test_readonly_to_readonly_no_ts4104() {
    // readonly number[] → readonly boolean[] should NOT produce ReadonlyToMutableAssignment
    // (both are readonly, so it's a regular type mismatch)
    let interner = TypeInterner::new();
    let readonly_num_array = interner.readonly_array(TypeId::NUMBER);
    let readonly_bool_array = interner.readonly_array(TypeId::BOOLEAN);

    let mut checker = CompatChecker::new(&interner);
    checker.strict_null_checks = true;
    assert!(
        !checker.is_assignable(readonly_num_array, readonly_bool_array),
        "readonly number[] should not be assignable to readonly boolean[]"
    );
    let reason = checker.explain_failure(readonly_num_array, readonly_bool_array);
    assert!(
        !matches!(
            reason,
            Some(SubtypeFailureReason::ReadonlyToMutableAssignment { .. })
        ),
        "Should NOT be ReadonlyToMutableAssignment when target is also readonly, got {reason:?}"
    );
}

#[test]
fn test_mutable_to_readonly_no_ts4104() {
    // number[] → readonly number[] should be assignable (adding readonly is fine)
    let interner = TypeInterner::new();
    let num_array = interner.array(TypeId::NUMBER);
    let readonly_num_array = interner.readonly_array(TypeId::NUMBER);

    let mut checker = CompatChecker::new(&interner);
    checker.strict_null_checks = true;
    assert!(
        checker.is_assignable(num_array, readonly_num_array),
        "number[] should be assignable to readonly number[]"
    );
}

#[test]
fn test_readonly_spread_tuple_to_type_param_is_ts2322() {
    // Source: readonly [...T]. Target: T extends unknown[].
    // tsc emits TS2322 (generic "not assignable") — NOT TS4104 — when the target
    // is a type parameter and the source is a readonly *tuple* (not a plain
    // readonly array). See variadicTuples1.ts:160 where
    //   function f11<T extends unknown[]>(t: T, m: [...T], r: readonly [...T]) {
    //     t = r;  // TS2322 (target is T, a type parameter)
    //     m = r;  // TS4104 (target is [...T], a concrete tuple)
    //   }
    // The plain `readonly number[] → T extends unknown[]` case is preserved and
    // still yields TS4104 (exercised by
    // `test_readonly_to_type_param_with_array_constraint_still_ts4104`).
    let interner = TypeInterner::new();

    let unknown_array = interner.array(TypeId::UNKNOWN);
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(unknown_array),
        default: None,
        is_const: false,
    }));

    let spread_tuple = interner.tuple(vec![TupleElement {
        type_id: t_param,
        name: None,
        optional: false,
        rest: true,
    }]);
    let readonly_spread = interner.intern(TypeData::ReadonlyType(spread_tuple));

    let mut checker = CompatChecker::new(&interner);
    checker.strict_null_checks = true;
    assert!(
        !checker.is_assignable(readonly_spread, t_param),
        "readonly [...T] should not be assignable to T extends unknown[]"
    );
    let reason = checker.explain_failure(readonly_spread, t_param);
    assert!(
        !matches!(
            reason,
            Some(SubtypeFailureReason::ReadonlyToMutableAssignment { .. })
        ),
        "Expected non-TS4104 failure for readonly-tuple source with type-param target \
         (tsc emits TS2322), got {reason:?}"
    );
}

#[test]
fn test_readonly_to_type_param_with_array_constraint_still_ts4104() {
    // Source: readonly unknown[] (plain readonly array, not a tuple).
    // Target: T extends unknown[]. tsc short-circuits this to TS4104, matching
    // the behavior tsz already relied on. This test locks in that the narrowing
    // applied for readonly-tuple sources does not affect the plain readonly-array
    // case.
    let interner = TypeInterner::new();

    let unknown_array = interner.array(TypeId::UNKNOWN);
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(unknown_array),
        default: None,
        is_const: false,
    }));

    let readonly_source = interner.readonly_array(TypeId::UNKNOWN);

    let mut checker = CompatChecker::new(&interner);
    checker.strict_null_checks = true;
    let reason = checker.explain_failure(readonly_source, t_param);
    assert!(
        matches!(
            reason,
            Some(SubtypeFailureReason::ReadonlyToMutableAssignment { .. })
        ),
        "Expected ReadonlyToMutableAssignment for readonly array → type-param with \
         array constraint, got {reason:?}"
    );
}

#[test]
fn test_readonly_to_unconstrained_type_param_no_ts4104() {
    // readonly number[] → T (unconstrained) should NOT produce
    // ReadonlyToMutableAssignment. Without an array/tuple constraint,
    // tsc emits a generic TypeMismatch, not TS4104.
    let interner = TypeInterner::new();

    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    let readonly_source = interner.readonly_array(TypeId::NUMBER);

    let mut checker = CompatChecker::new(&interner);
    checker.strict_null_checks = true;
    let reason = checker.explain_failure(readonly_source, t_param);
    assert!(
        !matches!(
            reason,
            Some(SubtypeFailureReason::ReadonlyToMutableAssignment { .. })
        ),
        "Should NOT be ReadonlyToMutableAssignment for unconstrained type param, got {reason:?}"
    );
}

#[test]
fn test_readonly_spread_tuple_to_mutable_spread_tuple_is_ts4104() {
    // Source: readonly [...T]. Target: [...T].
    // Both are concrete tuple types with a single rest element — target is a
    // mutable tuple, so tsc emits TS4104 (readonly-to-mutable). Mirrors
    // variadicTuples1.ts:162 where `m = r;` with `m: [...T]` and
    // `r: readonly [...T]` yields TS4104.
    let interner = TypeInterner::new();

    let unknown_array = interner.array(TypeId::UNKNOWN);
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(unknown_array),
        default: None,
        is_const: false,
    }));

    let spread_tuple = interner.tuple(vec![TupleElement {
        type_id: t_param,
        name: None,
        optional: false,
        rest: true,
    }]);
    let readonly_spread = interner.intern(TypeData::ReadonlyType(spread_tuple));

    let mut checker = CompatChecker::new(&interner);
    checker.strict_null_checks = true;
    let reason = checker.explain_failure(readonly_spread, spread_tuple);
    assert!(
        matches!(
            reason,
            Some(SubtypeFailureReason::ReadonlyToMutableAssignment { .. })
        ),
        "Expected ReadonlyToMutableAssignment (TS4104) for mutable tuple target, got {reason:?}"
    );
}

#[test]
fn test_explain_intersection_source_missing_properties() {
    // Intersection source (like `number & { __brand: T }`) assigned to an object
    // target should produce MissingProperties, not TypeMismatch.
    // Matches tsc behavior for branded types: TS2739 instead of TS2322.
    let interner = TypeInterner::new();

    let view = interner.intern_string("view");
    let style_media = interner.intern_string("styleMedia");
    let brand = interner.intern_string("__brand");

    // Target: { view: number; styleMedia: string }
    let target = interner.object(vec![
        PropertyInfo::new(view, TypeId::NUMBER),
        PropertyInfo::new(style_media, TypeId::STRING),
    ]);

    // Source: number & { __brand: { view: number; styleMedia: string } }
    // (branded type pattern — the intersection has no `view` or `styleMedia` at top level)
    let brand_obj = interner.object(vec![PropertyInfo::new(brand, target)]);
    let source = interner.intersection2(TypeId::NUMBER, brand_obj);

    let mut checker = CompatChecker::new(&interner);
    let reason = checker.explain_failure(source, target);

    // Should get MissingProperties with view and styleMedia
    match reason {
        Some(SubtypeFailureReason::MissingProperties {
            property_names,
            source_type,
            target_type,
        }) => {
            assert_eq!(source_type, source);
            assert_eq!(target_type, target);
            assert_eq!(property_names.len(), 2);
            assert!(property_names.contains(&view));
            assert!(property_names.contains(&style_media));
        }
        other => panic!("Expected MissingProperties with view and styleMedia, got {other:?}"),
    }
}

#[test]
fn test_explain_intersection_source_single_missing_property() {
    // Intersection with only one missing property should produce MissingProperty (TS2741).
    let interner = TypeInterner::new();

    let a = interner.intern_string("a");
    let b = interner.intern_string("b");

    // Target: { a: string; b: number }
    let target = interner.object(vec![
        PropertyInfo::new(a, TypeId::STRING),
        PropertyInfo::new(b, TypeId::NUMBER),
    ]);

    // Source: string & { a: string }  (missing `b` but has `a`)
    let partial_obj = interner.object(vec![PropertyInfo::new(a, TypeId::STRING)]);
    let source = interner.intersection2(TypeId::STRING, partial_obj);

    let mut checker = CompatChecker::new(&interner);
    let reason = checker.explain_failure(source, target);

    match reason {
        Some(SubtypeFailureReason::MissingProperty {
            property_name,
            source_type,
            target_type,
        }) => {
            assert_eq!(source_type, source);
            assert_eq!(target_type, target);
            assert_eq!(property_name, b);
        }
        other => panic!("Expected MissingProperty for 'b', got {other:?}"),
    }
}

#[test]
fn test_explain_normalized_mapped_application_missing_property() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let enum_def = DefId(1000);
    let enum_member_a = interner.intern(crate::TypeData::Enum(
        enum_def,
        interner.literal_number(0.0),
    ));
    let enum_member_b = interner.intern(crate::TypeData::Enum(
        enum_def,
        interner.literal_number(1.0),
    ));

    let t_name = interner.intern_string("T");
    let t_param_info = TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_param = interner.intern(crate::TypeData::TypeParameter(t_param_info));

    let v_name = interner.intern_string("v");
    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");

    let gen_body = interner.intersection(vec![
        interner.object(vec![PropertyInfo::new(v_name, t_param)]),
        interner.union(vec![
            interner.object(vec![
                PropertyInfo::new(v_name, enum_member_a),
                PropertyInfo::new(a_name, TypeId::STRING),
            ]),
            interner.object(vec![
                PropertyInfo::new(v_name, enum_member_b),
                PropertyInfo::new(b_name, TypeId::STRING),
            ]),
        ]),
    ]);

    let gen_def = DefId(1001);
    env.insert_def_with_params(gen_def, gen_body, vec![t_param_info]);

    let key_param_name = interner.intern_string("K");
    let key_param_info = TypeParamInfo {
        name: key_param_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let key_param = interner.intern(crate::TypeData::TypeParameter(key_param_info));
    let gen_t = interner.application(interner.lazy(gen_def), vec![t_param]);
    let gen2_body = interner.mapped(MappedType {
        type_param: key_param_info,
        constraint: interner.keyof(gen_t),
        name_type: None,
        template: interner.index_access(gen_t, key_param),
        readonly_modifier: None,
        optional_modifier: None,
    });

    let gen2_def = DefId(1002);
    env.insert_def_with_params(gen2_def, gen2_body, vec![t_param_info]);

    let source = interner.application(interner.lazy(gen2_def), vec![enum_member_b]);
    let target = interner.application(interner.lazy(gen2_def), vec![enum_member_a]);

    let mut checker = CompatChecker::with_resolver(&interner, &env);
    assert!(!checker.is_assignable(source, target));

    let reason = checker.explain_failure(source, target);
    match reason {
        Some(SubtypeFailureReason::MissingProperty {
            property_name,
            source_type,
            target_type,
        }) => {
            assert_eq!(property_name, a_name);
            assert_eq!(source_type, source);
            assert_eq!(target_type, target);
        }
        other => panic!("Expected MissingProperty for mapped application 'a', got {other:?}"),
    }
}

#[test]
fn test_explain_includes_late_bound_symbols_for_non_array_target() {
    // For non-array-like targets (e.g., ArrayConstructor), tsc includes
    // symbol-keyed names in the missing-property list alongside named
    // properties. The checker must report all missing properties so the
    // emitted TS2322 message matches tsc.
    let interner = TypeInterner::new();

    let length = interner.intern_string("length");
    let iterator = interner.intern_string("[Symbol.iterator]");
    let unscopables = interner.intern_string("[Symbol.unscopables]");

    let source = interner.object(vec![]);
    let target = interner.object(vec![
        PropertyInfo::new(length, TypeId::NUMBER),
        PropertyInfo::new(iterator, TypeId::ANY),
        PropertyInfo::new(unscopables, TypeId::ANY),
    ]);

    let mut checker = CompatChecker::new(&interner);
    let reason = checker.explain_failure(source, target);

    match reason {
        Some(SubtypeFailureReason::MissingProperties {
            property_names,
            source_type,
            target_type,
        }) => {
            assert_eq!(property_names, vec![length, iterator, unscopables]);
            assert_eq!(source_type, source);
            assert_eq!(target_type, target);
        }
        other => panic!("Expected MissingProperties for all three, got {other:?}"),
    }
}

/// tsc rejects `null` and `undefined` as arguments to type parameter `T` even
/// Without strictNullChecks, null/undefined are assignable to ALL types
/// including type parameters.  In tsc, non-strict mode treats null and
/// undefined as being in the domain of every type.
#[test]
fn test_null_assignable_to_unconstrained_type_param_without_strict() {
    let interner = TypeInterner::new();
    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // With strictNullChecks (default for CompatChecker::new): null/undefined
    // are not assignable to type parameters.
    let mut strict_checker = CompatChecker::new(&interner);
    assert!(
        !strict_checker.is_assignable(TypeId::NULL, t_param),
        "null should not be assignable to T with strictNullChecks"
    );
    assert!(
        !strict_checker.is_assignable(TypeId::UNDEFINED, t_param),
        "undefined should not be assignable to T with strictNullChecks"
    );

    // Without strictNullChecks: null/undefined ARE assignable to type
    // parameters, matching tsc behavior where non-strict mode treats
    // null/undefined as part of every type's domain.
    let mut non_strict_checker = CompatChecker::new(&interner);
    non_strict_checker.set_strict_null_checks(false);
    assert!(
        non_strict_checker.is_assignable(TypeId::NULL, t_param),
        "null should be assignable to T without strictNullChecks"
    );
    assert!(
        non_strict_checker.is_assignable(TypeId::UNDEFINED, t_param),
        "undefined should be assignable to T without strictNullChecks"
    );

    // Sanity: null IS still assignable to concrete types without strictNullChecks
    assert!(
        non_strict_checker.is_assignable(TypeId::NULL, TypeId::STRING),
        "null should be assignable to string without strictNullChecks"
    );
}

/// Regression: genericFunctionCallSignatureReturnTypeMismatch.ts
#[test]
fn test_generic_callable_return_type_mismatch_compat_layer() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let s_param = TypeParamInfo {
        name: interner.intern_string("S"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let s_type = interner.type_param(s_param);
    let s_array = interner.array(s_type);
    let source = interner.callable(CallableShape {
        call_signatures: vec![CallSignature {
            type_params: vec![s_param],
            params: vec![],
            this_type: None,
            return_type: s_array,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: vec![],
        properties: vec![],
        ..Default::default()
    });

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.type_param(t_param);
    let target = interner.callable(CallableShape {
        call_signatures: vec![CallSignature {
            type_params: vec![t_param],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: t_type,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: t_type,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: vec![],
        properties: vec![],
        ..Default::default()
    });

    assert!(
        !checker.is_assignable(source, target),
        "generic callable with incompatible return type should not be assignable"
    );
}

#[test]
fn test_callback_readonly_tuple_union_rest_not_assignable() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let lit_1 = interner.literal_number(1.0);
    let lit_2 = interner.literal_number(2.0);
    let lit_s1 = interner.literal_string("1");
    let lit_s2 = interner.literal_string("2");

    let num_union = interner.union2(lit_1, lit_2);
    let str_union = interner.union2(lit_s1, lit_s2);

    let source = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("a")),
                type_id: num_union,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("b")),
                type_id: str_union,
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

    let tuple1 = interner.tuple(vec![
        TupleElement {
            type_id: lit_1,
            optional: false,
            rest: false,
            name: None,
        },
        TupleElement {
            type_id: lit_s1,
            optional: false,
            rest: false,
            name: None,
        },
    ]);
    let readonly_tuple1 = interner.readonly_type(tuple1);

    let tuple2 = interner.tuple(vec![
        TupleElement {
            type_id: lit_2,
            optional: false,
            rest: false,
            name: None,
        },
        TupleElement {
            type_id: lit_s2,
            optional: false,
            rest: false,
            name: None,
        },
    ]);
    let readonly_tuple2 = interner.readonly_type(tuple2);

    let union_of_tuples = interner.union2(readonly_tuple1, readonly_tuple2);

    let target = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: union_of_tuples,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    assert!(
        !checker.is_assignable(source, target),
        "callback should NOT be assignable: readonly tuple union prevents element-wise matching"
    );

    checker.set_strict_function_types(false);
    assert!(
        !checker.is_assignable(source, target),
        "callback should NOT be assignable even with bivariant mode"
    );
}

#[test]
fn test_intersection_with_primitive_weak_type_check_not_suppressed() {
    // { __typename?: 'TypeTwo' } & string should NOT be assignable to
    // { __typename?: 'TypeOne' } & string — the __typename literal types conflict.
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let typename = interner.intern_string("__typename");
    let type_one_lit = interner.literal_string("TypeOne");
    let type_two_lit = interner.literal_string("TypeTwo");

    let obj_one = interner.object(vec![PropertyInfo {
        name: typename,
        type_id: interner.union2(type_one_lit, TypeId::UNDEFINED),
        write_type: interner.union2(type_one_lit, TypeId::UNDEFINED),
        optional: true,
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

    let obj_two = interner.object(vec![PropertyInfo {
        name: typename,
        type_id: interner.union2(type_two_lit, TypeId::UNDEFINED),
        write_type: interner.union2(type_two_lit, TypeId::UNDEFINED),
        optional: true,
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

    let source = interner.intersection(vec![obj_two, TypeId::STRING]);
    let target = interner.intersection(vec![obj_one, TypeId::STRING]);

    let result = checker.is_assignable(source, target);

    assert!(
        !result,
        "intersection with conflicting optional literal properties should not be assignable"
    );
}

#[test]
fn test_explain_function_to_callable_with_properties_produces_missing_properties() {
    // When a function type is assigned to a callable type with additional properties
    // (like ArrayConstructor with isArray, from, of), the failure should be
    // MissingProperties, not TypeMismatch. This matches tsc's behavior of emitting
    // TS2739 instead of TS2322 for `Array = function(n, s) { return n; }`.
    let interner = TypeInterner::new();

    let is_array = interner.intern_string("isArray");
    let from = interner.intern_string("from");
    let of = interner.intern_string("of");

    // Source: (n: number, s: string) => number (a simple function type)
    let source = interner.function(FunctionShape {
        params: vec![
            ParamInfo::unnamed(TypeId::NUMBER),
            ParamInfo::unnamed(TypeId::STRING),
        ],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Target: callable with properties (like ArrayConstructor)
    // Has call signatures and properties: isArray, from, of
    let target = interner.callable(CallableShape {
        call_signatures: vec![CallSignature {
            params: vec![],
            type_params: Vec::new(),
            return_type: TypeId::ANY,
            this_type: None,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: vec![CallSignature {
            params: vec![],
            type_params: Vec::new(),
            return_type: TypeId::ANY,
            this_type: None,
            type_predicate: None,
            is_method: false,
        }],
        properties: vec![
            PropertyInfo::new(is_array, TypeId::BOOLEAN),
            PropertyInfo::new(from, TypeId::NUMBER),
            PropertyInfo::new(of, TypeId::NUMBER),
        ],
        string_index: None,
        number_index: None,
        symbol: None,
        is_abstract: false,
    });

    let mut checker = CompatChecker::new(&interner);
    let reason = checker.explain_failure(source, target);

    match reason {
        Some(SubtypeFailureReason::MissingProperties { property_names, .. }) => {
            assert!(
                property_names.contains(&is_array),
                "Expected isArray in missing properties, got: {property_names:?}"
            );
            assert!(
                property_names.contains(&from),
                "Expected from in missing properties, got: {property_names:?}"
            );
            assert!(
                property_names.contains(&of),
                "Expected of in missing properties, got: {property_names:?}"
            );
        }
        Some(SubtypeFailureReason::MissingProperty { property_name, .. }) => {
            // If only one property is reported, that's also acceptable
            assert!(
                property_name == is_array || property_name == from || property_name == of,
                "Expected a constructor property in MissingProperty, got: {property_name:?}"
            );
        }
        other => {
            panic!(
                "Expected MissingProperties or MissingProperty for function assigned to \
                 callable with properties, got: {other:?}"
            );
        }
    }
}

/// Regression: when a closed source tuple has more elements than a closed
/// target tuple, the failure reason must be the arity mismatch — not an
/// element-level type mismatch — even if some interior element would also
/// fail to assign. tsc reports
/// "Source has N element(s) but target allows only M." in this case and
/// stops there; tsz must do the same so that the call-site diagnostic is the
/// outer TS2345 (with the correct `Source has ...` sub-message) instead of a
/// drilled-in TS2322 at a single tuple element. Without this rule, the
/// conformance test
/// `destructuringParameterDeclaration3ES5.ts` fingerprints differently from
/// tsc on the call `a9([1, 2, [["string"]], false, true])` because the inner
/// `[["string"]]` vs `[[any]]` element comparison fires before the length
/// check.
#[test]
fn test_explain_tuple_arity_takes_priority_over_element_mismatch() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // Inner tuple types so the element-level check would otherwise drill in.
    let inner_string = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }]);
    let nested_string = interner.tuple(vec![TupleElement {
        type_id: inner_string,
        name: None,
        optional: false,
        rest: false,
    }]);
    let inner_any = interner.tuple(vec![TupleElement {
        type_id: TypeId::ANY,
        name: None,
        optional: false,
        rest: false,
    }]);
    let nested_any = interner.tuple(vec![TupleElement {
        type_id: inner_any,
        name: None,
        optional: false,
        rest: false,
    }]);

    // Source: [number, number, [[string]], boolean, boolean] — length 5.
    let source = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
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
        TupleElement {
            type_id: nested_string,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::BOOLEAN,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::BOOLEAN,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    // Target: [any, any, [[any]]] — length 3.
    let target = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::ANY,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::ANY,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: nested_any,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    assert!(!checker.is_assignable(source, target));

    match checker.explain_failure(source, target) {
        Some(SubtypeFailureReason::TupleElementMismatch {
            source_count,
            target_count,
        }) => {
            assert_eq!(source_count, 5);
            assert_eq!(target_count, 3);
        }
        other => panic!(
            "Expected TupleElementMismatch (arity) to take priority over an inner \
             TupleElementTypeMismatch, got: {other:?}"
        ),
    }
}

// ===========================================================================
// Tests for unknown -> unknown-like union assignability
// (tsc's `isUnknownLikeUnionType`: a union containing `{}`, `null`, AND
// `undefined` is semantically equivalent to `unknown`, even if it has extra
// non-nullish members like `{ x: string }` that are absorbed by `{}`.)
// ===========================================================================

#[test]
fn test_unknown_assignable_to_canonical_unknown_like_union() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let empty_obj = interner.object(vec![]);
    let union = interner.union(vec![empty_obj, TypeId::NULL, TypeId::UNDEFINED]);

    assert!(
        checker.is_assignable(TypeId::UNKNOWN, union),
        "unknown should be assignable to `{{}} | null | undefined`"
    );
}

#[test]
fn test_unknown_assignable_to_unknown_like_union_with_extra_object_member() {
    // Repro: `let x3: {} | { x: string } | null | undefined = u;` where u: unknown.
    // tsc accepts this because `{} | { x: string } | null | undefined` is unknown-like
    // — `{ x: string }` is a subtype of `{}`, so the union still covers the entire
    // unknown space (`{}` + `null` + `undefined`).
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let empty_obj = interner.object(vec![]);
    let obj_with_x = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::STRING,
    )]);
    let union = interner.union(vec![empty_obj, obj_with_x, TypeId::NULL, TypeId::UNDEFINED]);

    assert!(
        checker.is_assignable(TypeId::UNKNOWN, union),
        "unknown should be assignable to `{{}} | {{ x: string }} | null | undefined`"
    );
}

#[test]
fn test_unknown_not_assignable_to_union_missing_null() {
    // `{} | undefined` is NOT unknown-like (no null constituent), so unknown is
    // not assignable to it.
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let empty_obj = interner.object(vec![]);
    let union = interner.union(vec![empty_obj, TypeId::UNDEFINED]);

    assert!(
        !checker.is_assignable(TypeId::UNKNOWN, union),
        "unknown should not be assignable to `{{}} | undefined` (missing null)"
    );
}

#[test]
fn test_unknown_not_assignable_to_union_missing_undefined() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let empty_obj = interner.object(vec![]);
    let union = interner.union(vec![empty_obj, TypeId::NULL]);

    assert!(
        !checker.is_assignable(TypeId::UNKNOWN, union),
        "unknown should not be assignable to `{{}} | null` (missing undefined)"
    );
}

#[test]
fn test_unknown_not_assignable_to_union_missing_empty_object() {
    // `string | null | undefined` does not contain `{}`, so unknown is not assignable.
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let union = interner.union(vec![TypeId::STRING, TypeId::NULL, TypeId::UNDEFINED]);

    assert!(
        !checker.is_assignable(TypeId::UNKNOWN, union),
        "unknown should not be assignable to `string | null | undefined` (no `{{}}` member)"
    );
}
