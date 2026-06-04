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
