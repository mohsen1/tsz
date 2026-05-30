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
