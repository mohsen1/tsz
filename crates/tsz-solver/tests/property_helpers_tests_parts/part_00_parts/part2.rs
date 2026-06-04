#[test]
fn test_object_prototype_members_on_plain_object() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let x = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo::new(x, TypeId::NUMBER)]);

    // Object.prototype methods should be available
    let result = evaluator.resolve_property_access(obj, "hasOwnProperty");
    assert!(result.is_success(), "hasOwnProperty should be found");

    let result = evaluator.resolve_property_access(obj, "toString");
    assert!(result.is_success(), "toString should be found");
}

#[test]
fn test_callable_with_properties() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let version = interner.intern_string("version");
    let callable = interner.callable(CallableShape {
        call_signatures: vec![CallSignature {
            params: vec![],
            return_type: TypeId::VOID,
            type_params: Vec::new(),
            this_type: None,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: vec![],
        properties: vec![PropertyInfo::new(version, TypeId::STRING)],
        string_index: None,
        number_index: None,
        symbol: None,
        is_abstract: false,
    });

    // Access property on callable
    assert_property_success(
        &evaluator.resolve_property_access(callable, "version"),
        TypeId::STRING,
    );

    // Function.prototype members should also be accessible
    let result = evaluator.resolve_property_access(callable, "bind");
    assert!(result.is_success(), "callable.bind should be accessible");
}

#[test]
fn test_union_with_null_and_undefined() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let x = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo::new(x, TypeId::NUMBER)]);
    let union = interner.union(vec![obj, TypeId::NULL, TypeId::UNDEFINED]);

    // Should report PossiblyNullOrUndefined
    let result = evaluator.resolve_property_access(union, "x");
    assert_possibly_null_or_undefined(&result);

    // The nullable result should include the property type from the non-null member
    match &result {
        PropertyAccessResult::PossiblyNullOrUndefined { property_type, .. } => {
            assert!(
                property_type.is_some(),
                "Property type from non-null member should be present"
            );
        }
        _ => unreachable!(),
    }
}

/// Register minimal Array + ReadonlyArray interfaces on `interner` for property-access tests.
fn make_array_and_readonly_array_env(interner: &TypeInterner) {
    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let push_fn = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
            type_id: t_type,
            optional: false,
            rest: false,
        }],
        return_type: TypeId::NUMBER,
        type_params: vec![],
        this_type: None,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });
    let slice_fn = interner.function(FunctionShape {
        params: vec![],
        return_type: interner.array(t_type),
        type_params: vec![],
        this_type: None,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    // Array<T>: length, push, slice
    let array_iface = interner.object(vec![
        PropertyInfo::readonly(interner.intern_string("length"), TypeId::NUMBER),
        PropertyInfo::method(interner.intern_string("push"), push_fn),
        PropertyInfo::method(interner.intern_string("slice"), slice_fn),
    ]);
    interner.set_array_base_type(array_iface, vec![t_param]);

    // ReadonlyArray<T>: length, slice (no push)
    let readonly_array_iface = interner.object(vec![
        PropertyInfo::readonly(interner.intern_string("length"), TypeId::NUMBER),
        PropertyInfo::method(interner.intern_string("slice"), slice_fn),
    ]);
    interner.set_readonly_array_base_type(readonly_array_iface);
}

#[test]
fn test_readonly_array_push_not_found() {
    let interner = TypeInterner::new();
    make_array_and_readonly_array_env(&interner);
    let evaluator = PropertyAccessEvaluator::new(&interner);

    // readonly number[] must NOT have push
    let readonly_num = interner.readonly_array(TypeId::NUMBER);
    assert_property_not_found(&evaluator.resolve_property_access(readonly_num, "push"));
}

#[test]
fn test_readonly_array_push_not_found_different_element_type() {
    // Verify the fix is structural (any element type), not specific to `number`.
    let interner = TypeInterner::new();
    make_array_and_readonly_array_env(&interner);
    let evaluator = PropertyAccessEvaluator::new(&interner);

    // readonly string[] — push absent
    let readonly_str = interner.readonly_array(TypeId::STRING);
    assert_property_not_found(&evaluator.resolve_property_access(readonly_str, "push"));

    // readonly boolean[] — push absent
    let readonly_bool = interner.readonly_array(TypeId::BOOLEAN);
    assert_property_not_found(&evaluator.resolve_property_access(readonly_bool, "push"));
}

#[test]
fn test_readonly_array_length_accessible() {
    // Non-mutating properties must still resolve on readonly arrays.
    let interner = TypeInterner::new();
    make_array_and_readonly_array_env(&interner);
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let readonly_num = interner.readonly_array(TypeId::NUMBER);
    assert_property_success(
        &evaluator.resolve_property_access(readonly_num, "length"),
        TypeId::NUMBER,
    );
}

#[test]
fn test_readonly_array_slice_accessible() {
    // `slice` is in ReadonlyArray — must be found.
    let interner = TypeInterner::new();
    make_array_and_readonly_array_env(&interner);
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let readonly_str = interner.readonly_array(TypeId::STRING);
    let result = evaluator.resolve_property_access(readonly_str, "slice");
    assert!(
        result.is_success(),
        "slice should be accessible on readonly string[]. Got: {result:?}"
    );
}
