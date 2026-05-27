#[test]
fn test_index_signature_with_properties() {
    // { x: number, [key: string]: number | string }
    let interner = TypeInterner::new();
    let _checker = SubtypeChecker::new(&interner);

    let union_type = interner.union(vec![TypeId::NUMBER, TypeId::STRING]);

    let obj = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo::new(
            interner.intern_string("x"),
            TypeId::NUMBER,
        )],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: union_type,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    // Object has both property and index signature
    assert!(obj != TypeId::ERROR);
}

#[test]
fn test_index_signature_property_must_match_index() {
    // Property type must be subtype of index signature value type
    // { x: string, [key: string]: string } is valid
    let interner = TypeInterner::new();
    let _checker = SubtypeChecker::new(&interner);

    let obj_valid = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo::new(
            interner.intern_string("x"),
            TypeId::STRING,
        )],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    assert!(obj_valid != TypeId::ERROR);
}

#[test]
fn test_index_signature_readonly_to_mutable() {
    // { readonly [key: string]: T } is NOT subtype of { [key: string]: T }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj_readonly = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: true,
            param_name: None,
        }),
        number_index: None,
    });

    let obj_mutable = interner.object_with_index(ObjectShape {
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

    // Per tsc behavior, readonly on index signatures does NOT affect assignability.
    // A readonly index signature IS assignable to a mutable index signature.
    assert!(checker.is_subtype_of(obj_readonly, obj_mutable));
}

#[test]
fn test_index_signature_mutable_to_readonly() {
    // { [key: string]: T } is subtype of { readonly [key: string]: T }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj_mutable = interner.object_with_index(ObjectShape {
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

    let obj_readonly = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: true,
            param_name: None,
        }),
        number_index: None,
    });

    // Mutable is assignable to readonly (can read)
    assert!(checker.is_subtype_of(obj_mutable, obj_readonly));
}

#[test]
fn test_index_signature_union_value_subtyping() {
    // { [key: string]: A | B } - specific member is subtype of union
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union_value = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let obj = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: union_value,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let obj_string = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    // { [k: string]: string } is subtype of { [k: string]: string | number }
    assert!(checker.is_subtype_of(obj_string, obj));
}

#[test]
fn test_index_signature_intersection_value() {
    // { [key: string]: A & B }
    let interner = TypeInterner::new();
    let _checker = SubtypeChecker::new(&interner);

    let obj_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::NUMBER,
    )]);

    let obj_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::STRING,
    )]);

    let intersection_value = interner.intersection(vec![obj_a, obj_b]);

    let obj = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: intersection_value,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    // Object with intersection value type
    assert!(obj != TypeId::ERROR);
}

#[test]
fn test_index_signature_empty_object_to_indexed() {
    // {} is NOT subtype of { [key: string]: T } unless T allows undefined
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let empty_obj = interner.object(vec![]);

    let indexed_obj = interner.object_with_index(ObjectShape {
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

    // Empty object may not be subtype of indexed object
    // This depends on strictness settings
    let result = checker.is_subtype_of(empty_obj, indexed_obj);
    // Just ensure it doesn't panic
    let _ = result;
}

#[test]
fn test_index_signature_object_with_extra_props() {
    // { a: number, b: string } is subtype of { [key: string]: number | string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj_with_props = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("b"), TypeId::STRING),
    ]);

    let union_value = interner.union(vec![TypeId::NUMBER, TypeId::STRING]);
    let indexed_obj = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: union_value,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    assert!(checker.is_subtype_of(obj_with_props, indexed_obj));
}

#[test]
fn test_index_signature_numeric_string_key() {
    // { "0": T, "1": T } should be compatible with { [key: number]: T }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj_with_numeric_props = interner.object(vec![
        PropertyInfo::new(interner.intern_string("0"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("1"), TypeId::STRING),
    ]);

    let number_indexed = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
    });

    // Numeric string properties should be compatible
    assert!(checker.is_subtype_of(obj_with_numeric_props, number_indexed));
}

#[test]
fn test_index_signature_any_value() {
    // { [key: string]: any } accepts anything
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let indexed_any = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::ANY,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let obj_with_props = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::BOOLEAN,
    )]);

    assert!(checker.is_subtype_of(obj_with_props, indexed_any));
}

#[test]
fn test_object_with_named_props_satisfies_number_index_any() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let source = interner.object(vec![PropertyInfo::new(
        interner.intern_string("one"),
        TypeId::NUMBER,
    )]);

    let target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::ANY,
            readonly: false,
            param_name: None,
        }),
    });

    assert!(checker.is_subtype_of(source, target));
    assert_eq!(checker.explain_failure(source, target), None);
}

#[test]
fn test_string_is_not_subtype_of_string_index_any() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::ANY,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    assert!(!checker.is_subtype_of(TypeId::STRING, target));
    assert!(matches!(
        checker.explain_failure(TypeId::STRING, target),
        Some(SubtypeFailureReason::TypeMismatch { .. })
    ));
}

#[test]
fn test_boolean_is_not_subtype_of_number_index_any() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::ANY,
            readonly: false,
            param_name: None,
        }),
    });

    assert!(!checker.is_subtype_of(TypeId::BOOLEAN, target));
    assert!(matches!(
        checker.explain_failure(TypeId::BOOLEAN, target),
        Some(SubtypeFailureReason::TypeMismatch { .. })
    ));
}

#[test]
fn test_index_signature_unknown_value() {
    // { [key: string]: unknown } - safe unknown
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let indexed_unknown = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::UNKNOWN,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let indexed_string = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    // { [k: string]: string } is subtype of { [k: string]: unknown }
    assert!(checker.is_subtype_of(indexed_string, indexed_unknown));
}

#[test]
fn test_index_signature_never_value() {
    // { [key: string]: never } - impossible to add properties
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let indexed_never = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NEVER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    // Empty object might be subtype of { [k: string]: never }
    let empty_obj = interner.object(vec![]);
    let result = checker.is_subtype_of(empty_obj, indexed_never);
    // Just ensure it handles the case
    let _ = result;
}

#[test]
fn test_index_signature_function_value() {
    // { [key: string]: () => void }
    let interner = TypeInterner::new();
    let _checker = SubtypeChecker::new(&interner);

    let fn_type = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let indexed_fn = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: fn_type,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    assert!(indexed_fn != TypeId::ERROR);
}

#[test]
fn test_index_signature_array_value() {
    // { [key: string]: T[] }
    let interner = TypeInterner::new();
    let _checker = SubtypeChecker::new(&interner);

    let array_type = interner.array(TypeId::NUMBER);

    let indexed_array = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: array_type,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    assert!(indexed_array != TypeId::ERROR);
}

#[test]
fn test_index_signature_tuple_value() {
    // { [key: number]: [string, number] }
    let interner = TypeInterner::new();
    let _checker = SubtypeChecker::new(&interner);

    let tuple_type = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
            name: None,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
            name: None,
        },
    ]);

    let indexed_tuple = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: tuple_type,
            readonly: false,
            param_name: None,
        }),
    });

    assert!(indexed_tuple != TypeId::ERROR);
}

#[test]
fn test_index_signature_nested_object_value() {
    // { [key: string]: { x: number } }
    let interner = TypeInterner::new();
    let _checker = SubtypeChecker::new(&interner);

    let nested_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    let indexed_nested = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: nested_obj,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    assert!(indexed_nested != TypeId::ERROR);
}

#[test]
fn test_index_signature_intersection_objects() {
    // { [key: string]: A } & { x: B }
    let interner = TypeInterner::new();
    let _checker = SubtypeChecker::new(&interner);

    let indexed_obj = interner.object_with_index(ObjectShape {
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

    let prop_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    let intersection = interner.intersection(vec![indexed_obj, prop_obj]);

    // Intersection should have both index signature and property
    assert!(intersection != TypeId::ERROR);
}

#[test]
fn test_index_signature_literal_key_subset() {
    // { [key: "a" | "b"]: T } - template literal pattern index
    let interner = TypeInterner::new();
    let _checker = SubtypeChecker::new(&interner);

    let _literal_keys = interner.union(vec![
        interner.literal_string("a"),
        interner.literal_string("b"),
    ]);

    // This would be like a Pick pattern or mapped type result
    let obj_with_literal_props = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
    ]);

    assert!(obj_with_literal_props != TypeId::ERROR);
}

// =============================================================================
// COVARIANCE / CONTRAVARIANCE EDGE CASE TESTS
// =============================================================================

#[test]
fn test_variance_nested_function_contravariance() {
    // (f: (x: string) => void) => void  <:  (f: (x: string | number) => void) => void
    // The callback parameter is contravariant, so callbacks with wider params are subtypes
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // Callback with narrow param
    let narrow_callback = interner.function(FunctionShape {
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

    // Callback with wide param
    let wide_param = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let wide_callback = interner.function(FunctionShape {
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

    // HOF taking narrow callback
    let hof_narrow = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("f")),
            type_id: narrow_callback,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // HOF taking wide callback
    let hof_wide = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("f")),
            type_id: wide_callback,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // HOF with wide callback <: HOF with narrow callback (double contravariance = covariance)
    // In strict variance: hof_wide <: hof_narrow only
    // Current behavior: bivariant for callback parameters - both directions work
    assert!(!checker.is_subtype_of(hof_wide, hof_narrow));
    assert!(checker.is_subtype_of(hof_narrow, hof_wide));
}

#[test]
fn test_variance_callback_return_type() {
    // (f: () => string) => void  vs  (f: () => string | number) => void
    // Callback return is covariant within callback, but callback is contravariant
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // Callback returning narrow type
    let narrow_returning = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Callback returning wide type
    let wide_return = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let wide_returning = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: wide_return,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // HOF taking narrow-returning callback
    let hof_narrow_return = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("f")),
            type_id: narrow_returning,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // HOF taking wide-returning callback
    let hof_wide_return = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("f")),
            type_id: wide_returning,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // HOF with narrow-returning <: HOF with wide-returning (contravariant flip of covariant)
    // In strict variance: hof_narrow_return <: hof_wide_return only
    // Current behavior: bivariant for callback parameters - both directions work
    assert!(!checker.is_subtype_of(hof_narrow_return, hof_wide_return));
    assert!(checker.is_subtype_of(hof_wide_return, hof_narrow_return));
}

#[test]
fn test_variance_readonly_property_covariant() {
    // { readonly x: string } <: { readonly x: string | number }
    // Readonly properties are covariant (only read, never written)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let wide_type = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let narrow_readonly = interner.object(vec![PropertyInfo::readonly(
        interner.intern_string("x"),
        TypeId::STRING,
    )]);

    let wide_readonly = interner.object(vec![PropertyInfo::readonly(
        interner.intern_string("x"),
        wide_type,
    )]);

    // Narrow readonly <: wide readonly (covariant)
    assert!(checker.is_subtype_of(narrow_readonly, wide_readonly));
}

#[test]
fn test_variance_mutable_property_invariant() {
    // { x: string } should not be subtype of { x: string | number } (invariant for mutable)
    // In TypeScript this is unsound - arrays are covariant even when mutable
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let wide_type = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let narrow_mutable = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::STRING,
    )]);

    let wide_mutable = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        wide_type,
    )]);

    // TypeScript allows this (unsound covariance), so we match behavior
    assert!(checker.is_subtype_of(narrow_mutable, wide_mutable));
}

#[test]
fn test_variance_tuple_element_covariant() {
    // [string, number] <: [string | number, number | boolean]
    // Tuple elements are covariant for reading
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let wide_first = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let wide_second = interner.union(vec![TypeId::NUMBER, TypeId::BOOLEAN]);

    let narrow_tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            optional: false,
            name: None,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            optional: false,
            name: None,
            rest: false,
        },
    ]);

    let wide_tuple = interner.tuple(vec![
        TupleElement {
            type_id: wide_first,
            optional: false,
            name: None,
            rest: false,
        },
        TupleElement {
            type_id: wide_second,
            optional: false,
            name: None,
            rest: false,
        },
    ]);

    // Narrow tuple <: wide tuple (covariant elements)
    assert!(checker.is_subtype_of(narrow_tuple, wide_tuple));
    assert!(!checker.is_subtype_of(wide_tuple, narrow_tuple));
}

#[test]
fn test_variance_function_returning_function() {
    // () => (x: string) => void  vs  () => (x: string | number) => void
    // Outer return is covariant, inner callback param is contravariant
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let wide_param = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // Inner function with narrow param
    let inner_narrow = interner.function(FunctionShape {
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

    // Inner function with wide param
    let inner_wide = interner.function(FunctionShape {
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

    // Factory returning narrow-param function
    let factory_narrow = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: inner_narrow,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Factory returning wide-param function
    let factory_wide = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: inner_wide,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Factory returning wide-param <: factory returning narrow-param
    // Return is covariant, and wide-param callback <: narrow-param callback
    assert!(checker.is_subtype_of(factory_wide, factory_narrow));
    assert!(!checker.is_subtype_of(factory_narrow, factory_wide));
}

#[test]
fn test_variance_union_in_contravariant_position() {
    // (x: A | B) => void  <:  (x: A) => void  (contravariance)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union_ab = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let fn_union_param = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: union_ab,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_single_param = interner.function(FunctionShape {
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

    // Union param <: single param (contravariance)
    assert!(checker.is_subtype_of(fn_union_param, fn_single_param));
    // Single param should NOT be subtype of union param
    assert!(!checker.is_subtype_of(fn_single_param, fn_union_param));
}

#[test]
fn test_variance_intersection_in_covariant_position() {
    // () => A & B  <:  () => A  (covariance)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let obj_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);

    let intersection_ab = interner.intersection(vec![obj_a, obj_b]);

    let fn_returns_intersection = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: intersection_ab,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_returns_a = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: obj_a,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Returns A & B <: returns A (covariance, intersection subtype of member)
    assert!(checker.is_subtype_of(fn_returns_intersection, fn_returns_a));
}

#[test]
fn test_variance_array_element_unsound_covariance() {
    // string[] <: (string | number)[] - TypeScript's unsound covariance
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let wide_element = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let narrow_array = interner.array(TypeId::STRING);
    let wide_array = interner.array(wide_element);

    // TypeScript allows this (unsound)
    assert!(checker.is_subtype_of(narrow_array, wide_array));
}

#[test]
fn test_variance_method_bivariant_params() {
    // Methods are bivariant in their parameters (TypeScript unsoundness)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let wide_type = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // Object with method taking narrow param
    let narrow_method_obj = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![],
        construct_signatures: vec![],
        properties: vec![PropertyInfo {
            name: interner.intern_string("handle"),
            type_id: interner.function(FunctionShape {
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
            }),
            write_type: TypeId::VOID,
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
        }],
        string_index: None,
        number_index: None,
    });

    // Object with method taking wide param
    let wide_method_obj = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![],
        construct_signatures: vec![],
        properties: vec![PropertyInfo {
            name: interner.intern_string("handle"),
            type_id: interner.function(FunctionShape {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("x")),
                    type_id: wide_type,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: TypeId::VOID,
                type_predicate: None,
                is_constructor: false,
                is_method: false,
            }),
            write_type: TypeId::VOID,
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
        }],
        string_index: None,
        number_index: None,
    });

    // Methods are bivariant - both directions should work
    assert!(checker.is_subtype_of(narrow_method_obj, wide_method_obj));
    assert!(checker.is_subtype_of(wide_method_obj, narrow_method_obj));
}

#[test]
fn test_variance_function_property_contravariant() {
    // Function properties are strictly contravariant (not bivariant like methods)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let wide_type = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // Object with function property taking narrow param
    let narrow_fn_obj = interner.object(vec![PropertyInfo {
        name: interner.intern_string("handle"),
        type_id: interner.function(FunctionShape {
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
        }),
        write_type: TypeId::VOID,
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

    // Object with function property taking wide param
    let wide_fn_obj = interner.object(vec![PropertyInfo {
        name: interner.intern_string("handle"),
        type_id: interner.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: wide_type,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        }),
        write_type: TypeId::VOID,
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

    // Wide param function <: narrow param function (contravariant)
    assert!(checker.is_subtype_of(wide_fn_obj, narrow_fn_obj));
}

#[test]
fn test_variance_promise_covariant() {
    // Promise<string> <: Promise<string | number> (covariant)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let wide_type = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // Simulate Promise<string> as { then: (cb: (value: string) => void) => void }
    let then_narrow = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("cb")),
            type_id: interner.function(FunctionShape {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("value")),
                    type_id: TypeId::STRING,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: TypeId::VOID,
                type_predicate: None,
                is_constructor: false,
                is_method: false,
            }),
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let then_wide = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("cb")),
            type_id: interner.function(FunctionShape {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("value")),
                    type_id: wide_type,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: TypeId::VOID,
                type_predicate: None,
                is_constructor: false,
                is_method: false,
            }),
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let promise_narrow = interner.object(vec![PropertyInfo::method(
        interner.intern_string("then"),
        then_narrow,
    )]);

    let promise_wide = interner.object(vec![PropertyInfo::method(
        interner.intern_string("then"),
        then_wide,
    )]);

    // Promise<string> <: Promise<string | number> (covariant in T)
    // then callback param is contravariant, then is contravariant in object = covariant overall
    assert!(checker.is_subtype_of(promise_narrow, promise_wide));
}

#[test]
fn test_recursive_promise_then_assignable_to_promise_like() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let promise_like_def = DefId(3000);
    let promise_def = DefId(3001);

    let outer_t = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };

    let inner_u = TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: None,
        default: None,
        is_const: false,
    };

    let outer_t_ty = interner.type_param(outer_t);
    let inner_u_ty = interner.type_param(inner_u);

    let promise_like_u = interner.application(interner.lazy(promise_like_def), vec![inner_u_ty]);
    let promise_u = interner.application(interner.lazy(promise_def), vec![inner_u_ty]);

    let onfulfilled_promise_like = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
            type_id: outer_t_ty,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: interner.union(vec![inner_u_ty, promise_like_u]),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let then_promise_like = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            type_params: vec![inner_u],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("onfulfilled")),
                type_id: interner.union(vec![onfulfilled_promise_like, TypeId::UNDEFINED]),
                optional: true,
                rest: false,
            }],
            this_type: None,
            return_type: promise_like_u,
            type_predicate: None,
            is_method: true,
        }],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    let onfulfilled_promise = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
            type_id: outer_t_ty,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: interner.union(vec![inner_u_ty, promise_like_u]),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let then_promise = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            type_params: vec![inner_u],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("onfulfilled")),
                type_id: interner.union(vec![onfulfilled_promise, TypeId::UNDEFINED]),
                optional: true,
                rest: false,
            }],
            this_type: None,
            return_type: promise_u,
            type_predicate: None,
            is_method: true,
        }],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    let promise_like_body = interner.object(vec![PropertyInfo::method(
        interner.intern_string("then"),
        then_promise_like,
    )]);
    let promise_body = interner.object(vec![PropertyInfo::method(
        interner.intern_string("then"),
        then_promise,
    )]);

    env.insert_def_with_params(promise_like_def, promise_like_body, vec![outer_t]);
    env.insert_def_kind(promise_like_def, crate::def::DefKind::Interface);
    env.insert_def_with_params(promise_def, promise_body, vec![outer_t]);
    env.insert_def_kind(promise_def, crate::def::DefKind::Interface);

    let mut checker = SubtypeChecker::with_resolver(&interner, &env);
    let promise_number = interner.application(interner.lazy(promise_def), vec![TypeId::NUMBER]);
    let promise_like_number =
        interner.application(interner.lazy(promise_like_def), vec![TypeId::NUMBER]);

    assert!(
        checker.is_subtype_of(promise_number, promise_like_number),
        "Promise<T> should be assignable to PromiseLike<T> in recursive then comparison"
    );
}

#[test]
fn test_recursive_promise_then_actual_lib_shape_assignable_to_promise_like() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let promise_like_def = DefId(3010);
    let promise_def = DefId(3011);

    let outer_t = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let result1 = TypeParamInfo {
        name: interner.intern_string("TResult1"),
        constraint: None,
        default: Some(interner.type_param(outer_t)),
        is_const: false,
    };
    let result2 = TypeParamInfo {
        name: interner.intern_string("TResult2"),
        constraint: None,
        default: Some(TypeId::NEVER),
        is_const: false,
    };

    let outer_t_ty = interner.type_param(outer_t);
    let result1_ty = interner.type_param(result1);
    let result2_ty = interner.type_param(result2);
    let result_union = interner.union(vec![result1_ty, result2_ty]);
    let promise_like_result =
        interner.application(interner.lazy(promise_like_def), vec![result_union]);
    let promise_result = interner.application(interner.lazy(promise_def), vec![result_union]);
    let promise_like_result1 =
        interner.application(interner.lazy(promise_like_def), vec![result1_ty]);
    let promise_like_result2 =
        interner.application(interner.lazy(promise_like_def), vec![result2_ty]);

    let onfulfilled = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
            type_id: outer_t_ty,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: interner.union(vec![result1_ty, promise_like_result1]),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let onrejected = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("reason")),
            type_id: TypeId::ANY,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: interner.union(vec![result2_ty, promise_like_result2]),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let nullable_onfulfilled = interner.union(vec![onfulfilled, TypeId::UNDEFINED, TypeId::NULL]);
    let nullable_onrejected = interner.union(vec![onrejected, TypeId::UNDEFINED, TypeId::NULL]);

    let then_promise_like = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            type_params: vec![result1, result2],
            params: vec![
                ParamInfo {
                    name: Some(interner.intern_string("onfulfilled")),
                    type_id: nullable_onfulfilled,
                    optional: true,
                    rest: false,
                },
                ParamInfo {
                    name: Some(interner.intern_string("onrejected")),
                    type_id: nullable_onrejected,
                    optional: true,
                    rest: false,
                },
            ],
            this_type: None,
            return_type: promise_like_result,
            type_predicate: None,
            is_method: true,
        }],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    let then_promise = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            type_params: vec![result1, result2],
            params: vec![
                ParamInfo {
                    name: Some(interner.intern_string("onfulfilled")),
                    type_id: nullable_onfulfilled,
                    optional: true,
                    rest: false,
                },
                ParamInfo {
                    name: Some(interner.intern_string("onrejected")),
                    type_id: nullable_onrejected,
                    optional: true,
                    rest: false,
                },
            ],
            this_type: None,
            return_type: promise_result,
            type_predicate: None,
            is_method: true,
        }],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    let promise_like_body = interner.object(vec![PropertyInfo::method(
        interner.intern_string("then"),
        then_promise_like,
    )]);
    let promise_body = interner.object(vec![PropertyInfo::method(
        interner.intern_string("then"),
        then_promise,
    )]);

    env.insert_def_with_params(promise_like_def, promise_like_body, vec![outer_t]);
    env.insert_def_kind(promise_like_def, crate::def::DefKind::Interface);
    env.insert_def_with_params(promise_def, promise_body, vec![outer_t]);
    env.insert_def_kind(promise_def, crate::def::DefKind::Interface);

    let mut checker = SubtypeChecker::with_resolver(&interner, &env);
    let promise_number = interner.application(interner.lazy(promise_def), vec![TypeId::NUMBER]);
    let promise_like_number =
        interner.application(interner.lazy(promise_like_def), vec![TypeId::NUMBER]);

    assert!(
        checker.is_subtype_of(promise_number, promise_like_number),
        "Promise<T> should be assignable to PromiseLike<T> for the real lib then shape"
    );
}

#[test]
fn test_variance_triple_nested_contravariance() {
    // Three levels of contravariance: ((f: (g: (x: T) => void) => void) => void)
    // Three contravariants = contravariant overall
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let wide_type = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // Innermost: (x: T) => void
    let inner_narrow = interner.function(FunctionShape {
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

    let inner_wide = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: wide_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Middle: (g: innermost) => void
    let middle_narrow = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("g")),
            type_id: inner_narrow,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let middle_wide = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("g")),
            type_id: inner_wide,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Outermost: (f: middle) => void
    let outer_narrow = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("f")),
            type_id: middle_narrow,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let outer_wide = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("f")),
            type_id: middle_wide,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Three levels of contravariance = contravariant (in strict mode)
    // outer_narrow <: outer_wide (narrow at innermost becomes wide at triple-contravariant)
    // Current behavior: bivariant for callback parameters - only one direction works
    assert!(!checker.is_subtype_of(outer_narrow, outer_wide));
    assert!(checker.is_subtype_of(outer_wide, outer_narrow));
}

#[test]
fn test_variance_constructor_param_bivariant() {
    // Construct signatures use bivariant parameter checking (like methods).
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let wide_type = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // Instance type
    let instance = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        TypeId::STRING,
    )]);

    let ctor_narrow = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![],
        construct_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: instance,
            type_predicate: None,
            is_method: false,
        }],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    let ctor_wide = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![],
        construct_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: wide_type,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: instance,
            type_predicate: None,
            is_method: false,
        }],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    // Both directions work (bivariant for construct signatures)
    assert!(checker.is_subtype_of(ctor_wide, ctor_narrow));
    assert!(checker.is_subtype_of(ctor_narrow, ctor_wide));
}

#[test]
fn test_variance_rest_param_contravariant() {
    // (...args: (string | number)[]) => void  <:  (...args: string[]) => void
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let wide_type = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let narrow_array = interner.array(TypeId::STRING);
    let wide_array = interner.array(wide_type);

    let fn_narrow_rest = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: narrow_array,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_wide_rest = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: wide_array,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Wide rest param <: narrow rest param (contravariant)
    assert!(checker.is_subtype_of(fn_wide_rest, fn_narrow_rest));
}

