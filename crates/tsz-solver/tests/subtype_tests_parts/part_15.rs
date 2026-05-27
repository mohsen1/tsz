#[test]
fn test_rest_param_flag_is_preserved() {
    let interner = TypeInterner::new();

    // Create target function with rest parameter
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

    // Verify the rest flag is preserved
    if let Some(TypeData::Function(shape_id)) = interner.lookup(target) {
        let shape = interner.function_shape(shape_id);
        assert_eq!(shape.params.len(), 3, "Should have 3 params");
        assert!(!shape.params[0].rest, "First param should not be rest");
        assert!(!shape.params[1].rest, "Second param should not be rest");
        assert!(shape.params[2].rest, "Third param SHOULD be rest");
    } else {
        panic!("Target is not a function type");
    }
}

#[test]
fn test_rest_param_any_with_extra_fixed_params() {
    // Test case from conformance: (a, b, c) => R <: (a, b, ...rest: any[]) => R
    let interner = TypeInterner::new();

    // Source: (name: string, mixed: any, args_0: any) => any
    let source = interner.function(FunctionShape {
        params: vec![
            ParamInfo::unnamed(TypeId::STRING),
            ParamInfo::unnamed(TypeId::ANY),
            ParamInfo::unnamed(TypeId::ANY),
        ],
        this_type: None,
        return_type: TypeId::ANY,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Target: (name: string, mixed: any, ...args: any[]) => any
    let rest_any = interner.array(TypeId::ANY);
    let target = interner.function(FunctionShape {
        params: vec![
            ParamInfo::unnamed(TypeId::STRING),
            ParamInfo::unnamed(TypeId::ANY),
            ParamInfo {
                name: None,
                type_id: rest_any,
                optional: false,
                rest: true,
            },
        ],
        this_type: None,
        return_type: TypeId::ANY,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let mut checker = SubtypeChecker::new(&interner);

    // TypeScript allows this assignment because the extra fixed param (args_0: any)
    // is compatible with the rest element type (any). When the target has rest params,
    // the arity check is skipped entirely and compatibility is checked per-param.
    assert!(checker.is_subtype_of(source, target));

    // Should still work with allow_bivariant_rest
    checker.allow_bivariant_rest = true;
    assert!(checker.is_subtype_of(source, target));
}

#[test]
fn test_intersection_target_produces_type_mismatch_not_missing_property() {
    // When the target is an intersection type (T & U), explain_failure should
    // return TypeMismatch (→ TS2322) instead of MissingProperty (→ TS2741).
    // TSC always emits TS2322 for intersection targets because intersection
    // types combine constraints from multiple sources.
    //
    // We use type parameters because the interner merges anonymous object
    // intersections into a single object (losing the intersection information).
    use crate::types::TypeData;
    use crate::types::TypeParamInfo;

    let interner = TypeInterner::new();

    // Create constrained type params to make an intersection that won't be merged
    let a_prop = interner.intern_string("a");
    let b_prop = interner.intern_string("b");

    let obj_a = interner.object(vec![PropertyInfo::new(a_prop, TypeId::STRING)]);
    let obj_b = interner.object(vec![PropertyInfo::new(b_prop, TypeId::STRING)]);

    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(obj_a),
        default: None,
        is_const: false,
    }));
    let u_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: Some(obj_b),
        default: None,
        is_const: false,
    }));

    // Source: { a: string } — satisfies T but not U
    let source = interner.object(vec![PropertyInfo::new(a_prop, TypeId::STRING)]);

    // Target: T & U (intersection of type parameters — interner keeps this as Intersection)
    let target = interner.intersection(vec![t_param, u_param]);
    assert!(
        crate::is_intersection_type(&interner, target),
        "Target should be an intersection type"
    );

    let mut checker = SubtypeChecker::new(&interner);

    // Source should NOT be a subtype of the intersection target
    assert!(!checker.is_subtype_of(source, target));

    // explain_failure should return TypeMismatch, NOT MissingProperty
    let reason = checker.explain_failure(source, target);
    assert!(
        matches!(reason, Some(SubtypeFailureReason::TypeMismatch { .. })),
        "Expected TypeMismatch for intersection target, got: {reason:?}"
    );
}

#[test]
fn test_plain_object_target_produces_missing_property() {
    // When the target is a plain object (not an intersection), explain_failure
    // should still return MissingProperty (→ TS2741) as before.
    let interner = TypeInterner::new();

    let a_prop = interner.intern_string("a");
    let b_prop = interner.intern_string("b");

    // Source: { a: string }
    let source = interner.object(vec![PropertyInfo::new(a_prop, TypeId::STRING)]);

    // Target: { a: string, b: string } (plain object, not intersection)
    let target = interner.object(vec![
        PropertyInfo::new(a_prop, TypeId::STRING),
        PropertyInfo::new(b_prop, TypeId::STRING),
    ]);

    let mut checker = SubtypeChecker::new(&interner);

    assert!(!checker.is_subtype_of(source, target));

    // For plain object targets, should produce MissingProperty
    let reason = checker.explain_failure(source, target);
    assert!(
        matches!(reason, Some(SubtypeFailureReason::MissingProperty { .. })),
        "Expected MissingProperty for plain object target, got: {reason:?}"
    );
}

// =========================================================================
// Enum namespace implicit index signature tests
// =========================================================================

#[test]
fn test_enum_namespace_satisfies_string_index_target() {
    // An enum namespace type (flagged with ENUM_NAMESPACE) should have an
    // implicit string index signature derived from its property types.
    // This matches tsc: `typeof E1` (numeric enum) is assignable to
    // `{ [x: string]: T }` when all property types are compatible.
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // Source: enum namespace { A: number, B: number } with ENUM_NAMESPACE flag
    let source = {
        let shape = ObjectShape {
            properties: vec![
                PropertyInfo::new(interner.intern_string("A"), TypeId::NUMBER),
                PropertyInfo::new(interner.intern_string("B"), TypeId::NUMBER),
            ],
            flags: ObjectFlags::ENUM_NAMESPACE,
            symbol: Some(SymbolId(100)),
            ..Default::default()
        };
        interner.object_with_index(shape)
    };

    // Target: { [x: string]: number }
    let target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    // Enum namespace should satisfy string index target via implicit index
    assert!(
        checker.is_subtype_of(source, target),
        "Enum namespace with all-number properties should satisfy {{ [x: string]: number }}"
    );
}

#[test]
fn test_enum_namespace_rejects_incompatible_string_index() {
    // When enum namespace has mixed types, it should NOT satisfy a specific
    // string index target.
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // Source: enum namespace { A: number, B: string } with ENUM_NAMESPACE flag
    let source = {
        let shape = ObjectShape {
            properties: vec![
                PropertyInfo::new(interner.intern_string("A"), TypeId::NUMBER),
                PropertyInfo::new(interner.intern_string("B"), TypeId::STRING),
            ],
            flags: ObjectFlags::ENUM_NAMESPACE,
            symbol: Some(SymbolId(101)),
            ..Default::default()
        };
        interner.object_with_index(shape)
    };

    // Target: { [x: string]: number } — string property B is incompatible
    let target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    assert!(
        !checker.is_subtype_of(source, target),
        "Enum namespace with mixed types should NOT satisfy {{ [x: string]: number }}"
    );
}

#[test]
fn test_regular_named_object_still_rejects_number_index() {
    // Named objects without ENUM_NAMESPACE flag should still reject
    // implicit number index signatures (existing behavior preserved).
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let source = interner.object_with_flags_and_symbol(
        vec![PropertyInfo::new(
            interner.intern_string("one"),
            TypeId::NUMBER,
        )],
        ObjectFlags::empty(),
        Some(SymbolId(1)),
    );

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

    assert!(
        !checker.is_subtype_of(source, target),
        "Regular named object (no ENUM_NAMESPACE flag) should NOT satisfy number index target"
    );
}

// ============================================================================
// TypeQuery (typeof) explain_failure: resolve to structural forms for TS2741
// ============================================================================

#[test]
fn test_explain_failure_resolves_typequery_to_structural_form() {
    // Simulates: typeof Outer vs typeof Outer.instantiated
    //
    // `typeof Outer` has properties: { instantiated: ..., uninstantiated: ... }
    // `typeof Outer.instantiated` has properties: { C: typeof C }
    //
    // Assignment `x5 = Outer` where `x5: typeof importInst` should produce
    // MissingProperty for 'C' (TS2741), not generic TypeMismatch (TS2322).
    use crate::SymbolRef;
    use crate::relations::subtype::TypeEnvironment;
    use crate::types::TypeData;

    let interner = TypeInterner::new();

    let c_name = interner.intern_string("C");
    let inst_name = interner.intern_string("instantiated");
    let uninst_name = interner.intern_string("uninstantiated");

    // Build typeof Outer.instantiated: { C: typeof C }
    let inner_obj = interner.object(vec![PropertyInfo::new(c_name, TypeId::OBJECT)]);

    // Build typeof Outer: { instantiated: ..., uninstantiated: ... }
    let outer_obj = interner.object(vec![
        PropertyInfo::new(inst_name, inner_obj),
        PropertyInfo::new(uninst_name, TypeId::OBJECT),
    ]);

    // Create TypeQuery types referencing symbols
    let sym_outer = SymbolRef(100);
    let sym_inner = SymbolRef(200);

    let tq_outer = interner.intern(TypeData::TypeQuery(sym_outer));
    let tq_inner = interner.intern(TypeData::TypeQuery(sym_inner));

    // Set up environment: symbols resolve to the object types
    let mut env = TypeEnvironment::new();
    env.insert(sym_outer, outer_obj);
    env.insert(sym_inner, inner_obj);

    let mut checker = SubtypeChecker::with_resolver(&interner, &env);

    // typeof Outer is NOT assignable to typeof Outer.instantiated
    // (outer has {instantiated, uninstantiated} but inner needs {C})
    assert!(
        !checker.is_subtype_of(tq_outer, tq_inner),
        "typeof Outer should not be assignable to typeof Outer.instantiated"
    );

    // explain_failure should produce MissingProperty for 'C' (TS2741)
    let reason = checker.explain_failure(tq_outer, tq_inner);
    assert!(reason.is_some(), "Should produce a failure reason");
    match reason.unwrap() {
        SubtypeFailureReason::MissingProperty { property_name, .. } => {
            assert_eq!(property_name, c_name, "Missing property should be 'C'");
        }
        SubtypeFailureReason::MissingProperties { .. } => {
            // Also acceptable
        }
        other => panic!("Expected MissingProperty for 'C' on typeof namespace, got {other:?}"),
    }

    // And the reverse: typeof Outer.instantiated is NOT assignable to typeof Outer
    assert!(
        !checker.is_subtype_of(tq_inner, tq_outer),
        "typeof Outer.instantiated should not be assignable to typeof Outer"
    );

    // explain_failure should produce MissingProperty for 'instantiated' (TS2741)
    let reason2 = checker.explain_failure(tq_inner, tq_outer);
    assert!(reason2.is_some(), "Should produce a failure reason");
    match reason2.unwrap() {
        SubtypeFailureReason::MissingProperty { property_name, .. } => {
            assert_eq!(
                property_name, inst_name,
                "Missing property should be 'instantiated'"
            );
        }
        SubtypeFailureReason::MissingProperties { .. } => {
            // Also acceptable
        }
        other => {
            panic!("Expected MissingProperty for 'instantiated' on typeof namespace, got {other:?}")
        }
    }
}

#[test]
fn test_callback_with_readonly_tuple_union_rest_param() {
    // Reproduces: contextualTupleTypeParameterReadonly.ts
    // Source: (a: 1 | 2, b: "1" | "2") => void
    // Target: (...args: readonly [1, "1"] | readonly [2, "2"]) => any
    // Expected: source is NOT assignable to target (TS2345)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

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
        !checker.is_subtype_of(source, target),
        "callback (a: 1|2, b: '1'|'2') => void should NOT be assignable to (...args: readonly [1, '1'] | readonly [2, '2']) => any"
    );

    checker.strict_function_types = false;
    assert!(
        !checker.is_subtype_of(source, target),
        "Even with bivariant callbacks, should NOT be assignable due to readonly tuple constraint"
    );
}

#[test]
fn test_type_param_extends_never_assignable_to_never() {
    // tsc accepts `T extends never` as assignable to `never` because the constraint
    // is vacuously inhabited only by `never` itself, so the type parameter carries
    // the same bottom-type semantics as `never` in all subtype positions.
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(TypeId::NEVER),
        default: None,
        is_const: false,
    };
    let t_type = interner.type_param(t_param);

    assert!(
        checker.is_subtype_of(t_type, TypeId::NEVER),
        "T extends never should be assignable to never"
    );
    assert!(
        checker.is_subtype_of(t_type, TypeId::UNKNOWN),
        "T extends never should be assignable to unknown"
    );
    assert!(
        checker.is_subtype_of(t_type, TypeId::STRING),
        "T extends never should be assignable to any type (never extends everything)"
    );

    // Name-independence: same constraint, different type parameter name.
    let n_param = TypeParamInfo {
        name: interner.intern_string("N"),
        constraint: Some(TypeId::NEVER),
        default: None,
        is_const: false,
    };
    let n_type = interner.type_param(n_param);
    assert!(
        checker.is_subtype_of(n_type, TypeId::NEVER),
        "N extends never should also be assignable to never (name-independent)"
    );
}

#[test]
fn test_type_param_extends_string_not_assignable_to_never() {
    // Negative: T extends string → T is NOT assignable to never.
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    };
    let t_type = interner.type_param(t_param);

    assert!(
        !checker.is_subtype_of(t_type, TypeId::NEVER),
        "T extends string should NOT be assignable to never"
    );
}

#[test]
fn test_unconstrained_type_param_not_assignable_to_never() {
    // Negative: unconstrained T is NOT assignable to never.
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.type_param(t_param);

    assert!(
        !checker.is_subtype_of(t_type, TypeId::NEVER),
        "Unconstrained T should NOT be assignable to never"
    );
}

#[test]
fn global_function_intrinsic_assignability_is_one_way() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let specific_fn = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(TypeId::NUMBER)],
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    assert!(
        checker.is_subtype_of(specific_fn, TypeId::FUNCTION),
        "specific callable values assign to the global Function type"
    );
    assert!(
        !checker.is_subtype_of(TypeId::FUNCTION, specific_fn),
        "the global Function type does not assign to a specific call signature"
    );
}

// =============================================================================
// Mapped Type Key Constraint Contravariance Tests
// =============================================================================
//
// Structural rule: when `type M<K, V> = { [P in K]: V }` is used,
// K is CONTRAVARIANT — a source with wider keys (K1 ⊇ K2) is assignable to
// a target with narrower keys (K2) because the source provides every property
// the target requires.  `M<"a"|"b"|"c", number> <: M<"a"|"b", number>` = TRUE.

fn unconstrained_type_param(interner: &TypeInterner, name: &str) -> TypeParamInfo {
    TypeParamInfo {
        name: interner.intern_string(name),
        constraint: None,
        default: None,
        is_const: false,
    }
}

fn mapped_type_alias_env(
    def_id: DefId,
    body: TypeId,
    params: Vec<TypeParamInfo>,
) -> TypeEnvironment {
    let mut env = TypeEnvironment::new();
    env.insert_def_with_params(def_id, body, params);
    env.insert_def_kind(def_id, crate::def::DefKind::TypeAlias);
    env
}

/// Core case: `type M<K, V> = { [P in K]: V }` — Application with wider key
/// constraint should be assignable to one with narrower key constraint.
#[test]
fn test_mapped_key_constraint_application_wider_source_subtype_of_narrower_target() {
    // M<"a"|"b"|"c", number> <: M<"a"|"b", number>   ← TRUE  (wider ⊇ narrower)
    // M<"a"|"b", number>      <: M<"a"|"b"|"c", number> ← FALSE (narrower ⊄ wider)
    let interner = TypeInterner::new();

    let def_id = DefId(9200);
    let k_param = unconstrained_type_param(&interner, "K");
    let v_param = unconstrained_type_param(&interner, "V");
    let k_type = interner.type_param(k_param);
    let v_type = interner.type_param(v_param);

    let body = interner.mapped(MappedType {
        type_param: unconstrained_type_param(&interner, "P"),
        constraint: k_type,
        name_type: None,
        template: v_type,
        readonly_modifier: None,
        optional_modifier: None,
    });

    let env = mapped_type_alias_env(def_id, body, vec![k_param, v_param]);
    let base = interner.lazy(def_id);

    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let lit_c = interner.literal_string("c");
    let keys_ab = interner.union(vec![lit_a, lit_b]);
    let keys_abc = interner.union(vec![lit_a, lit_b, lit_c]);

    let app_abc = interner.application(base, vec![keys_abc, TypeId::NUMBER]);
    let app_ab = interner.application(base, vec![keys_ab, TypeId::NUMBER]);

    let mut checker = SubtypeChecker::with_resolver(&interner, &env);

    assert!(
        checker.is_subtype_of(app_abc, app_ab),
        "M<'a'|'b'|'c', number> must be assignable to M<'a'|'b', number> — \
         key constraint is contravariant (wider source covers narrower target)"
    );
    assert!(
        !checker.is_subtype_of(app_ab, app_abc),
        "M<'a'|'b', number> must NOT be assignable to M<'a'|'b'|'c', number> — \
         narrower source does not cover wider target"
    );
}

/// Renamed params (X, Y instead of K, V) prove the fix is structural, not name-dependent.
#[test]
fn test_mapped_key_constraint_application_wider_source_renamed_params() {
    let interner = TypeInterner::new();

    let def_id = DefId(9201);
    let x_param = unconstrained_type_param(&interner, "X");
    let y_param = unconstrained_type_param(&interner, "Y");
    let x_type = interner.type_param(x_param);
    let y_type = interner.type_param(y_param);

    let body = interner.mapped(MappedType {
        type_param: unconstrained_type_param(&interner, "Q"),
        constraint: x_type,
        name_type: None,
        template: y_type,
        readonly_modifier: None,
        optional_modifier: None,
    });

    let env = mapped_type_alias_env(def_id, body, vec![x_param, y_param]);
    let base = interner.lazy(def_id);

    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let lit_c = interner.literal_string("c");
    let lit_d = interner.literal_string("d");
    let keys_ab = interner.union(vec![lit_a, lit_b]);
    let keys_abcd = interner.union(vec![lit_a, lit_b, lit_c, lit_d]);

    let app_abcd = interner.application(base, vec![keys_abcd, TypeId::STRING]);
    let app_ab = interner.application(base, vec![keys_ab, TypeId::STRING]);

    let mut checker = SubtypeChecker::with_resolver(&interner, &env);

    assert!(
        checker.is_subtype_of(app_abcd, app_ab),
        "M<4 keys, string> must be assignable to M<2 keys, string>"
    );
    assert!(
        !checker.is_subtype_of(app_ab, app_abcd),
        "M<2 keys, string> must NOT be assignable to M<4 keys, string>"
    );
}

/// Raw mapped types (not wrapped in Application): wider constraint ⊇ narrower.
#[test]
fn test_mapped_key_constraint_raw_wider_subtype_of_narrower() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let iter_var = unconstrained_type_param(&interner, "K");

    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let lit_c = interner.literal_string("c");
    let keys_ab = interner.union(vec![lit_a, lit_b]);
    let keys_abc = interner.union(vec![lit_a, lit_b, lit_c]);

    let mapped_abc = interner.mapped(MappedType {
        type_param: iter_var,
        constraint: keys_abc,
        name_type: None,
        template: TypeId::NUMBER,
        readonly_modifier: None,
        optional_modifier: None,
    });
    let mapped_ab = interner.mapped(MappedType {
        type_param: iter_var,
        constraint: keys_ab,
        name_type: None,
        template: TypeId::NUMBER,
        readonly_modifier: None,
        optional_modifier: None,
    });

    assert!(
        checker.is_subtype_of(mapped_abc, mapped_ab),
        "{{[K in 'a'|'b'|'c']: number}} must be assignable to {{[K in 'a'|'b']: number}}"
    );
    assert!(
        !checker.is_subtype_of(mapped_ab, mapped_abc),
        "{{[K in 'a'|'b']: number}} must NOT be assignable to {{[K in 'a'|'b'|'c']: number}}"
    );
}

/// Value type (template) mismatch must still be rejected even when key sets match.
#[test]
fn test_mapped_key_constraint_value_mismatch_rejected() {
    let interner = TypeInterner::new();

    let def_id = DefId(9202);
    let k_param = unconstrained_type_param(&interner, "K");
    let v_param = unconstrained_type_param(&interner, "V");
    let k_type = interner.type_param(k_param);
    let v_type = interner.type_param(v_param);

    let body = interner.mapped(MappedType {
        type_param: unconstrained_type_param(&interner, "P"),
        constraint: k_type,
        name_type: None,
        template: v_type,
        readonly_modifier: None,
        optional_modifier: None,
    });

    let env = mapped_type_alias_env(def_id, body, vec![k_param, v_param]);
    let base = interner.lazy(def_id);

    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let keys_ab = interner.union(vec![lit_a, lit_b]);

    let app_num = interner.application(base, vec![keys_ab, TypeId::NUMBER]);
    let app_str = interner.application(base, vec![keys_ab, TypeId::STRING]);

    let mut checker = SubtypeChecker::with_resolver(&interner, &env);

    assert!(
        !checker.is_subtype_of(app_num, app_str),
        "M<keys, number> must NOT be assignable to M<keys, string> — V is covariant"
    );
    assert!(
        !checker.is_subtype_of(app_str, app_num),
        "M<keys, string> must NOT be assignable to M<keys, number> — V is covariant"
    );
}
