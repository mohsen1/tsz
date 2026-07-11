// Tests for IndexSignatureMismatch nested failure reason elaboration.
//
// Rule: when two index signatures are structurally incompatible, the solver
// captures WHY via `nested_reason` so the checker can render a chained
// diagnostic (matching tsc's elaboration output).

#[test]
fn test_string_index_sig_mismatch_carries_nested_property_reason() {
    // { [key: string]: { x: number } }  vs  { [key: string]: { x: string } }
    // The nested failure should explain that property `x` is incompatible.
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let src_val = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);
    let tgt_val = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::STRING,
    )]);

    let source = interner.object_with_index(ObjectShape {
        symbol_index: None,
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: None,
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: src_val,
            readonly: false,
            param_name: None,
        }),
    });
    let target = interner.object_with_index(ObjectShape {
        symbol_index: None,
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: None,
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: tgt_val,
            readonly: false,
            param_name: None,
        }),
    });

    assert!(!checker.is_subtype_of(source, target));

    let reason = checker.explain_failure(source, target);
    let Some(SubtypeFailureReason::IndexSignatureMismatch {
        index_kind,
        nested_reason: Some(nested),
        ..
    }) = reason
    else {
        panic!("expected IndexSignatureMismatch with nested reason, got: {reason:?}");
    };
    assert_eq!(index_kind, "string");
    assert!(
        matches!(
            *nested,
            SubtypeFailureReason::PropertyTypeMismatch { .. }
        ),
        "nested reason should be PropertyTypeMismatch, got: {nested:?}"
    );
}

#[test]
fn test_string_index_sig_mismatch_nested_reason_is_name_independent() {
    // Same structural shape but with property name `value` instead of `x`,
    // proving the fix operates on structure not identifier spellings.
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let src_val = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        TypeId::NUMBER,
    )]);
    let tgt_val = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        TypeId::STRING,
    )]);

    let source = interner.object_with_index(ObjectShape {
        symbol_index: None,
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: None,
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: src_val,
            readonly: false,
            param_name: None,
        }),
    });
    let target = interner.object_with_index(ObjectShape {
        symbol_index: None,
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: None,
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: tgt_val,
            readonly: false,
            param_name: None,
        }),
    });

    assert!(!checker.is_subtype_of(source, target));

    let reason = checker.explain_failure(source, target);
    assert!(
        matches!(
            reason,
            Some(SubtypeFailureReason::IndexSignatureMismatch {
                index_kind: "string",
                nested_reason: Some(_),
                ..
            })
        ),
        "expected IndexSignatureMismatch with nested reason, got: {reason:?}"
    );
}

#[test]
fn test_number_index_sig_mismatch_carries_nested_property_reason() {
    // { [i: number]: { x: number } }  vs  { [i: number]: { x: string } }
    // Validates the number-index path captures nested reasons too.
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let src_val = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);
    let tgt_val = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::STRING,
    )]);

    let source = interner.object_with_index(ObjectShape {
        symbol_index: None,
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: src_val,
            readonly: false,
            param_name: None,
        }),
        string_index: None,
    });
    let target = interner.object_with_index(ObjectShape {
        symbol_index: None,
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: tgt_val,
            readonly: false,
            param_name: None,
        }),
        string_index: None,
    });

    assert!(!checker.is_subtype_of(source, target));

    let reason = checker.explain_failure(source, target);
    let Some(SubtypeFailureReason::IndexSignatureMismatch {
        index_kind,
        nested_reason: Some(nested),
        ..
    }) = reason
    else {
        panic!("expected number IndexSignatureMismatch with nested reason, got: {reason:?}");
    };
    assert_eq!(index_kind, "number");
    assert!(
        matches!(
            *nested,
            SubtypeFailureReason::PropertyTypeMismatch { .. }
        ),
        "nested reason should be PropertyTypeMismatch, got: {nested:?}"
    );
}

#[test]
fn test_index_sig_mismatch_primitive_value_type_carries_intrinsic_nested_reason() {
    // { [key: string]: number }  vs  { [key: string]: string }
    // Even primitive value types get an IntrinsicTypeMismatch nested reason,
    // so the diagnostic chain can always explain the incompatibility.
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let source = interner.object_with_index(ObjectShape {
        symbol_index: None,
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
    let target = interner.object_with_index(ObjectShape {
        symbol_index: None,
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: None,
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
    });

    assert!(!checker.is_subtype_of(source, target));

    let reason = checker.explain_failure(source, target);
    assert!(
        matches!(
            reason,
            Some(SubtypeFailureReason::IndexSignatureMismatch {
                index_kind: "string",
                nested_reason: Some(_),
                ..
            })
        ),
        "primitive value mismatch should produce IndexSignatureMismatch with nested IntrinsicTypeMismatch, got: {reason:?}"
    );
}

#[test]
fn test_missing_property_in_index_sig_target_returns_missing_property_directly() {
    // { [key: string]: { x: number } } is not assignable to { [key: string]: { x: number; y: string } }
    // The nested failure is MissingProperty, which should surface directly (not wrapped in IndexSignatureMismatch).
    // This preserves existing behavior: missing-property elaboration takes priority.
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let src_val = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);
    let tgt_val = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("y"), TypeId::STRING),
    ]);

    let source = interner.object_with_index(ObjectShape {
        symbol_index: None,
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: None,
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: src_val,
            readonly: false,
            param_name: None,
        }),
    });
    let target = interner.object_with_index(ObjectShape {
        symbol_index: None,
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: None,
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: tgt_val,
            readonly: false,
            param_name: None,
        }),
    });

    assert!(!checker.is_subtype_of(source, target));

    let reason = checker.explain_failure(source, target);
    // MissingProperty surfaces directly because it takes priority over IndexSignatureMismatch.
    assert!(
        matches!(
            reason,
            Some(
                SubtypeFailureReason::MissingProperty { .. }
                    | SubtypeFailureReason::MissingProperties { .. }
            )
        ),
        "missing-property case should surface the missing property reason directly, got: {reason:?}"
    );
}

fn symbol_indexed_object(
    interner: &TypeInterner,
    symbol_value: TypeId,
    readonly: bool,
) -> TypeId {
    interner.object_with_index(ObjectShape {
        symbol_index: Some(IndexSignature {
            key_type: TypeId::SYMBOL,
            value_type: symbol_value,
            readonly,
            param_name: None,
        }),
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: None,
        string_index: None,
    })
}

#[test]
fn test_symbol_index_sig_mismatch_carries_symbol_reason() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    let source = symbol_indexed_object(&interner, TypeId::NUMBER, false);
    let target = symbol_indexed_object(&interner, TypeId::STRING, false);

    assert!(!checker.is_subtype_of(source, target));
    let reason = checker.explain_failure(source, target);
    assert!(
        matches!(
            reason,
            Some(SubtypeFailureReason::IndexSignatureMismatch {
                index_kind: "symbol",
                source_value_type: TypeId::NUMBER,
                target_value_type: TypeId::STRING,
                ..
            })
        ),
        "symbol index value mismatch should retain the symbol key-space reason, got: {reason:?}"
    );
}

#[test]
fn test_symbol_index_sig_matching_value_ignores_readonly() {
    let interner = TypeInterner::new();
    let source = symbol_indexed_object(&interner, TypeId::NUMBER, true);
    let target = symbol_indexed_object(&interner, TypeId::NUMBER, false);
    let mut checker = SubtypeChecker::new(&interner);
    assert!(checker.is_subtype_of(source, target));

    let mut reverse = SubtypeChecker::new(&interner);
    assert!(reverse.is_subtype_of(target, source));
}

#[test]
fn test_mixed_indexes_report_symbol_value_mismatch() {
    let interner = TypeInterner::new();
    let shape = |symbol_value| ObjectShape {
        symbol_index: Some(IndexSignature {
            key_type: TypeId::SYMBOL,
            value_type: symbol_value,
            readonly: false,
            param_name: None,
        }),
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: None,
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::BOOLEAN,
            readonly: false,
            param_name: None,
        }),
    };
    let source = interner.object_with_index(shape(TypeId::NUMBER));
    let target = interner.object_with_index(shape(TypeId::STRING));
    let mut checker = SubtypeChecker::new(&interner);

    assert!(!checker.is_subtype_of(source, target));
    let reason = checker.explain_failure(source, target);
    assert!(
        matches!(
            reason,
            Some(SubtypeFailureReason::IndexSignatureMismatch {
                index_kind: "symbol",
                ..
            })
        ),
        "matching string indexes must not hide an incompatible symbol index, got: {reason:?}"
    );
}

#[test]
fn test_anonymous_string_property_vacuously_satisfies_symbol_index() {
    let interner = TypeInterner::new();
    let source = interner.object(vec![PropertyInfo::new(
        interner.intern_string("label"),
        TypeId::STRING,
    )]);
    let target = symbol_indexed_object(&interner, TypeId::NUMBER, false);
    let mut checker = SubtypeChecker::new(&interner);

    assert!(checker.is_subtype_of(source, target));
}

#[test]
fn test_symbol_index_does_not_capture_user_string_resembling_internal_atom() {
    let interner = TypeInterner::new();
    let source = interner.object(vec![PropertyInfo::new(
        interner.intern_string("__unique_1"),
        TypeId::NUMBER,
    )]);
    let target = interner.object_with_index(ObjectShape {
        symbol_index: Some(IndexSignature {
            key_type: TypeId::SYMBOL,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: None,
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
    });
    let mut checker = SubtypeChecker::new(&interner);

    assert!(!checker.is_subtype_of(source, target));
    assert!(matches!(
        checker.explain_failure(source, target),
        Some(SubtypeFailureReason::IndexSignatureMismatch {
            index_kind: "string",
            ..
        })
    ));
}

// ----------------------------------------------------------------------
// #13609: two applications of the SAME opaque/unresolvable base that differ
// ONLY in a type parameter's optional `default` denote the same type and must
// relate reflexively on EVERY relation path — including those built without a
// `QueryDatabase` (instanceof, element-access, contextual, property lookup,
// `CompatChecker` before `set_query_db`), where the canonical-identity fast
// path in `check_subtype` is unavailable. Before the fix the no-`query_db`
// path dropped two structurally identical applications to a false `False`.
// A genuinely different type argument must still be rejected (no unsound
// covariant/identity fallback over the opaque base).
// ----------------------------------------------------------------------
#[test]
fn application_over_opaque_base_relates_reflexively_despite_type_param_default_13609() {
    use crate::caches::db::QueryDatabase;

    let interner = TypeInterner::new();
    let cache = QueryCache::new(&interner);

    // Name-agnostic: the binder spelling must not influence the outcome.
    for raw_name in ["R", "Elem", "TKey"] {
        let name = interner.intern_string(raw_name);
        let mk_param = |default| {
            interner.type_param(TypeParamInfo {
                name,
                constraint: Some(TypeId::STRING),
                default,
                is_const: false,
                origin: crate::types::TypeParamOrigin::User,
            })
        };
        // Same logical parameter, fragmented only on the optional `default`.
        let p_with = mk_param(Some(TypeId::NUMBER));
        let p_without = mk_param(None);
        assert_ne!(p_with, p_without, "precondition: default fragments interning");

        // An opaque/unresolvable generic base (the resolver has no body for it),
        // mirroring a cross-arena `Lazy` not resolvable in this generation.
        let base = interner.lazy(DefId(90_001));
        let app_with = interner.application(base, vec![p_with]);
        let app_without = interner.application(base, vec![p_without]);
        assert_ne!(app_with, app_without);

        // Precondition: they canonicalize to one identity (the `default` drop).
        assert_eq!(cache.canonical_id(app_with), cache.canonical_id(app_without));

        // With a query database (the production checker path): reflexive both ways.
        let mut with_db = SubtypeChecker::with_resolver(&interner, &cache).with_query_db(&cache);
        assert!(with_db.is_subtype_of(app_with, app_without));
        let mut with_db_rev = SubtypeChecker::with_resolver(&interner, &cache).with_query_db(&cache);
        assert!(with_db_rev.is_subtype_of(app_without, app_with));

        // WITHOUT a query database (the regressing path): the canonical fast path
        // is absent, yet structurally identical applications must still relate.
        let mut no_db = SubtypeChecker::with_resolver(&interner, &cache);
        assert!(
            no_db.is_subtype_of(app_with, app_without),
            "{raw_name}: identical applications must relate without a query_db"
        );
        let mut no_db_rev = SubtypeChecker::with_resolver(&interner, &cache);
        assert!(no_db_rev.is_subtype_of(app_without, app_with));

        // Negative control: a genuinely different type argument must NOT relate,
        // even over the opaque base — the recovery is strict structural identity,
        // not an unsound covariant/identity fallback.
        let app_string = interner.application(base, vec![TypeId::STRING]);
        let app_number = interner.application(base, vec![TypeId::NUMBER]);
        let mut neg = SubtypeChecker::with_resolver(&interner, &cache);
        assert!(
            !neg.is_subtype_of(app_string, app_number),
            "{raw_name}: differing concrete args over an opaque base must stay unrelated"
        );
    }
}
