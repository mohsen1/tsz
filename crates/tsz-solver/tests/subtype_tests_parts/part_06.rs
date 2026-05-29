#[test]
fn test_fn_optional_param_multiple_optional() {
    // (a: string) => void is NOT subtype of (a?: string, b?: number) => void
    // Contravariant: string|undefined <: string fails
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let fn_one_required = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("a")),
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

    let fn_two_optional = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("a")),
                type_id: TypeId::STRING,
                optional: true,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("b")),
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

    // Required IS subtype of optional — tsc compares declared types without
    // | undefined widening, so (x: string) => void <: (x?: string, y?: number) => void.
    assert!(checker.is_subtype_of(fn_one_required, fn_two_optional));
}

#[test]
fn test_fn_optional_param_mixed_required_optional() {
    // (a: string, b: number) => void is NOT subtype of (a: string, b?: number) => void
    // Contravariant on b: number|undefined <: number fails
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let fn_both_required = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("a")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("b")),
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

    let fn_one_optional = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("a")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("b")),
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

    // Required IS subtype of optional — tsc compares b's declared type (number),
    // not number | undefined, so (a: string, b: number) <: (a: string, b?: number).
    assert!(checker.is_subtype_of(fn_both_required, fn_one_optional));
}

#[test]
fn test_fn_optional_param_with_undefined_union() {
    // (x: string | undefined) => void vs (x?: string) => void
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_or_undefined = interner.union(vec![TypeId::STRING, TypeId::UNDEFINED]);

    let fn_union_param = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: string_or_undefined,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_optional_param = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: true,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // These should be related - exact relationship depends on implementation
    // At minimum, check they don't crash
    let _union_to_optional = checker.is_subtype_of(fn_union_param, fn_optional_param);
    let _optional_to_union = checker.is_subtype_of(fn_optional_param, fn_union_param);
}

// -----------------------------------------------------------------------------
// Rest Parameter Assignability
// -----------------------------------------------------------------------------

#[test]
fn test_fn_rest_param_basic() {
    // (...args: string[]) => void
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);

    let fn_rest = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: string_array,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_no_params = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // No params should be subtype of rest (can be called with zero args)
    assert!(checker.is_subtype_of(fn_no_params, fn_rest));
}

#[test]
fn test_fn_rest_param_fixed_params_to_rest() {
    // (a: string, b: string) => void <: (...args: string[]) => void
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);

    let fn_two_strings = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("a")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("b")),
                type_id: TypeId::STRING,
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

    let fn_rest = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: string_array,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Fixed string params should be subtype of rest strings
    assert!(checker.is_subtype_of(fn_two_strings, fn_rest));
}

#[test]
fn test_fn_rest_param_wider_element_type() {
    // (...args: unknown[]) => void <: (...args: string[]) => void
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let unknown_array = interner.array(TypeId::UNKNOWN);

    let fn_rest_string = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: string_array,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_rest_unknown = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: unknown_array,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // unknown[] accepts more, so it's subtype (contravariance)
    assert!(checker.is_subtype_of(fn_rest_unknown, fn_rest_string));
}

#[test]
fn test_fn_rest_param_with_leading_params() {
    // (a: string, ...rest: number[]) => void
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let number_array = interner.array(TypeId::NUMBER);

    let fn_with_rest = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("a")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("rest")),
                type_id: number_array,
                optional: false,
                rest: true,
            },
        ],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_just_string = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("a")),
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

    // Just string param should be subtype (rest can be empty)
    assert!(checker.is_subtype_of(fn_just_string, fn_with_rest));
}

#[test]
fn test_fn_rest_param_union_element_type() {
    // (...args: (string | number)[]) => void
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let union_type = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let union_array = interner.array(union_type);

    let fn_rest_string = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: string_array,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_rest_union = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: union_array,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Union array accepts more types, so it's subtype
    assert!(checker.is_subtype_of(fn_rest_union, fn_rest_string));
}

#[test]
fn test_fn_rest_to_rest_same_type() {
    // (...args: string[]) => void <: (...args: string[]) => void
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);

    let fn_rest1 = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: string_array,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_rest2 = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: string_array,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Same rest type should be bidirectionally subtype
    assert!(checker.is_subtype_of(fn_rest1, fn_rest2));
    assert!(checker.is_subtype_of(fn_rest2, fn_rest1));
}

#[test]
fn test_fn_rest_combined_with_optional() {
    // (a?: string, ...rest: number[]) => void
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let number_array = interner.array(TypeId::NUMBER);

    let fn_optional_and_rest = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("a")),
                type_id: TypeId::STRING,
                optional: true,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("rest")),
                type_id: number_array,
                optional: false,
                rest: true,
            },
        ],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_no_params = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // No params should be subtype (both optional and rest can be empty)
    assert!(checker.is_subtype_of(fn_no_params, fn_optional_and_rest));
}

// =============================================================================
// Object Literal Type Tests
// =============================================================================

// -----------------------------------------------------------------------------
// Excess Property Checking
// -----------------------------------------------------------------------------

#[test]
fn test_excess_property_structural_subtype() {
    // { a: string, b: number } <: { a: string } (structural subtyping)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");

    let obj_a = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    let obj_ab = interner.object(vec![
        PropertyInfo::new(a_name, TypeId::STRING),
        PropertyInfo::new(b_name, TypeId::NUMBER),
    ]);

    // Object with extra property is subtype (structural)
    assert!(checker.is_subtype_of(obj_ab, obj_a));
    // Object missing property is NOT subtype
    assert!(!checker.is_subtype_of(obj_a, obj_ab));
}

#[test]
fn test_excess_property_three_extra() {
    // { a, b, c, d } <: { a }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");
    let c_name = interner.intern_string("c");
    let d_name = interner.intern_string("d");

    let obj_a = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    let obj_abcd = interner.object(vec![
        PropertyInfo::new(a_name, TypeId::STRING),
        PropertyInfo::new(b_name, TypeId::NUMBER),
        PropertyInfo::new(c_name, TypeId::BOOLEAN),
        PropertyInfo::new(d_name, TypeId::STRING),
    ]);

    // Multiple extra properties still subtype
    assert!(checker.is_subtype_of(obj_abcd, obj_a));
}

#[test]
fn test_excess_property_different_required() {
    // { a: string, b: number } is NOT subtype of { a: string, c: boolean }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");
    let c_name = interner.intern_string("c");

    let obj_ab = interner.object(vec![
        PropertyInfo::new(a_name, TypeId::STRING),
        PropertyInfo::new(b_name, TypeId::NUMBER),
    ]);

    let obj_ac = interner.object(vec![
        PropertyInfo::new(a_name, TypeId::STRING),
        PropertyInfo::new(c_name, TypeId::BOOLEAN),
    ]);

    // Missing required property c
    assert!(!checker.is_subtype_of(obj_ab, obj_ac));
    // Missing required property b
    assert!(!checker.is_subtype_of(obj_ac, obj_ab));
}

#[test]
fn test_excess_property_with_method() {
    // { a: string, method(): void } <: { a: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let method_name = interner.intern_string("method");

    let method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let obj_a = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    let obj_a_method = interner.object(vec![
        PropertyInfo::new(a_name, TypeId::STRING),
        PropertyInfo::method(method_name, method),
    ]);

    // Extra method is still subtype
    assert!(checker.is_subtype_of(obj_a_method, obj_a));
}

#[test]
fn test_excess_property_narrower_type() {
    // { a: "hello" } <: { a: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let hello = interner.literal_string("hello");

    let obj_a_literal = interner.object(vec![PropertyInfo::new(a_name, hello)]);

    let obj_a_string = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    // Literal type is subtype of wider type
    assert!(checker.is_subtype_of(obj_a_literal, obj_a_string));
    // Wider type is NOT subtype of literal
    assert!(!checker.is_subtype_of(obj_a_string, obj_a_literal));
}

#[test]
fn test_excess_property_empty_object() {
    // { a: string } <: {} (empty object accepts all)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");

    let obj_a = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    let empty_obj = interner.object(vec![]);

    // Any object is subtype of empty object
    assert!(checker.is_subtype_of(obj_a, empty_obj));
}

// -----------------------------------------------------------------------------
// Optional Property Matching
// -----------------------------------------------------------------------------

#[test]
fn test_optional_property_required_to_optional() {
    // { a: string } <: { a?: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");

    let obj_required = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    let obj_optional = interner.object(vec![PropertyInfo::opt(a_name, TypeId::STRING)]);

    // Required is subtype of optional
    assert!(checker.is_subtype_of(obj_required, obj_optional));
}

#[test]
fn test_optional_property_optional_to_required_not_subtype() {
    // { a?: string } is NOT subtype of { a: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");

    let obj_required = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    let obj_optional = interner.object(vec![PropertyInfo::opt(a_name, TypeId::STRING)]);

    // Optional is NOT subtype of required
    assert!(!checker.is_subtype_of(obj_optional, obj_required));
}

#[test]
fn test_optional_property_missing_optional() {
    // {} <: { a?: string } (missing optional property is OK)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");

    let empty_obj = interner.object(vec![]);

    let obj_optional = interner.object(vec![PropertyInfo::opt(a_name, TypeId::STRING)]);

    // Empty object is subtype of object with only optional properties
    assert!(checker.is_subtype_of(empty_obj, obj_optional));
}

#[test]
fn test_optional_property_mixed_required_optional() {
    // { a: string, b: number } <: { a: string, b?: number }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");

    let obj_both_required = interner.object(vec![
        PropertyInfo::new(a_name, TypeId::STRING),
        PropertyInfo::new(b_name, TypeId::NUMBER),
    ]);

    let obj_b_optional = interner.object(vec![
        PropertyInfo::new(a_name, TypeId::STRING),
        PropertyInfo::opt(b_name, TypeId::NUMBER),
    ]);

    // Both required is subtype of one optional
    assert!(checker.is_subtype_of(obj_both_required, obj_b_optional));
}

#[test]
fn test_optional_property_all_optional() {
    // { a?: string, b?: number } <: { a?: string, b?: number }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");

    let obj = interner.object(vec![
        PropertyInfo::opt(a_name, TypeId::STRING),
        PropertyInfo::opt(b_name, TypeId::NUMBER),
    ]);

    // Same optional properties - bidirectional subtype
    assert!(checker.is_subtype_of(obj, obj));
}

#[test]
fn test_optional_property_type_mismatch() {
    // { a?: string } is NOT subtype of { a?: number }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");

    let obj_optional_string = interner.object(vec![PropertyInfo::opt(a_name, TypeId::STRING)]);

    let obj_optional_number = interner.object(vec![PropertyInfo::opt(a_name, TypeId::NUMBER)]);

    // Different types - not subtypes
    assert!(!checker.is_subtype_of(obj_optional_string, obj_optional_number));
    assert!(!checker.is_subtype_of(obj_optional_number, obj_optional_string));
}

// -----------------------------------------------------------------------------
// Index Signature Assignability
// -----------------------------------------------------------------------------

#[test]
fn test_index_signature_string_basic() {
    // { [key: string]: number } - string index signature
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let indexed_number = interner.object_with_index(ObjectShape {
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

    // Different value types - not subtypes
    assert!(!checker.is_subtype_of(indexed_number, indexed_string));
    assert!(!checker.is_subtype_of(indexed_string, indexed_number));
}

#[test]
fn test_index_signature_covariant_value() {
    // { [key: string]: "hello" } <: { [key: string]: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let hello = interner.literal_string("hello");

    let indexed_literal = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: hello,
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

    // Literal value type is subtype of wider value type
    assert!(checker.is_subtype_of(indexed_literal, indexed_string));
}

#[test]
fn test_index_signature_with_known_property() {
    // { a: string, [key: string]: string } <: { [key: string]: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");

    let indexed_with_prop = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo::new(a_name, TypeId::STRING)],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let indexed_only = interner.object_with_index(ObjectShape {
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

    // Object with known property and index signature is subtype
    assert!(checker.is_subtype_of(indexed_with_prop, indexed_only));
}

#[test]
fn test_index_signature_number_index() {
    // { [key: number]: string } - number index signature (array-like)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

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

    let string_indexed = interner.object_with_index(ObjectShape {
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

    // Number index and string index are different
    // In TypeScript, number index must be subtype of string index value
    let _result = checker.is_subtype_of(number_indexed, string_indexed);
}

#[test]
fn test_index_signature_union_value() {
    // { [key: string]: string | number }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union_value = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let indexed_union = interner.object_with_index(ObjectShape {
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

    // string is subtype of string | number
    // So { [k: string]: string } <: { [k: string]: string | number }
    assert!(checker.is_subtype_of(indexed_string, indexed_union));
}

#[test]
fn test_index_signature_object_to_indexed() {
    // { a: string, b: string } <: { [key: string]: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");

    let obj_ab = interner.object(vec![
        PropertyInfo::new(a_name, TypeId::STRING),
        PropertyInfo::new(b_name, TypeId::STRING),
    ]);

    let indexed = interner.object_with_index(ObjectShape {
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

    // Object with matching property types is subtype of index signature
    assert!(checker.is_subtype_of(obj_ab, indexed));
}

// -----------------------------------------------------------------------------
// Readonly Property Handling
// -----------------------------------------------------------------------------

#[test]
fn test_readonly_mutable_to_readonly() {
    // { a: string } <: { readonly a: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");

    let obj_mutable = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    let obj_readonly = interner.object(vec![PropertyInfo::readonly(a_name, TypeId::STRING)]);

    // Mutable is subtype of readonly (can read from both)
    assert!(checker.is_subtype_of(obj_mutable, obj_readonly));
}

#[test]
fn test_readonly_to_mutable() {
    // { readonly a: string } may or may not be subtype of { a: string }
    // This depends on whether we allow readonly-to-mutable assignment
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");

    let obj_mutable = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    let obj_readonly = interner.object(vec![PropertyInfo::readonly(a_name, TypeId::STRING)]);

    // Check both directions - implementation-dependent
    let _readonly_to_mutable = checker.is_subtype_of(obj_readonly, obj_mutable);
    let _mutable_to_readonly = checker.is_subtype_of(obj_mutable, obj_readonly);
}

#[test]
fn test_readonly_both_readonly() {
    // { readonly a: string } <: { readonly a: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");

    let obj_readonly = interner.object(vec![PropertyInfo::readonly(a_name, TypeId::STRING)]);

    // Same readonly - bidirectional subtype
    assert!(checker.is_subtype_of(obj_readonly, obj_readonly));
}

#[test]
fn test_readonly_mixed_properties() {
    // { a: string, readonly b: number } <: { a: string, readonly b: number }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");

    let obj = interner.object(vec![
        PropertyInfo::new(a_name, TypeId::STRING),
        PropertyInfo::readonly(b_name, TypeId::NUMBER),
    ]);

    // Same object - bidirectional subtype
    assert!(checker.is_subtype_of(obj, obj));
}

#[test]
fn test_readonly_narrower_type() {
    // { readonly a: "hello" } <: { readonly a: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let hello = interner.literal_string("hello");

    let obj_literal = interner.object(vec![PropertyInfo::readonly(a_name, hello)]);

    let obj_string = interner.object(vec![PropertyInfo::readonly(a_name, TypeId::STRING)]);

    // Readonly literal is subtype of readonly wider type
    assert!(checker.is_subtype_of(obj_literal, obj_string));
}

#[test]
fn test_readonly_with_optional() {
    // { readonly a?: string } - both readonly and optional
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");

    let obj_readonly_optional = interner.object(vec![PropertyInfo {
        name: a_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: true,
        readonly: true,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    }]);

    let obj_readonly_required =
        interner.object(vec![PropertyInfo::readonly(a_name, TypeId::STRING)]);

    // Required is subtype of optional (even with readonly)
    assert!(checker.is_subtype_of(obj_readonly_required, obj_readonly_optional));
    // Optional is NOT subtype of required
    assert!(!checker.is_subtype_of(obj_readonly_optional, obj_readonly_required));
}

#[test]
fn test_readonly_array_like() {
    // ReadonlyArray<T> pattern - readonly with index signature
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let length_name = interner.intern_string("length");

    let readonly_array_like =
        interner.object(vec![PropertyInfo::readonly(length_name, TypeId::NUMBER)]);

    let mutable_array_like = interner.object(vec![PropertyInfo::new(length_name, TypeId::NUMBER)]);

    // Mutable is subtype of readonly
    assert!(checker.is_subtype_of(mutable_array_like, readonly_array_like));
}

#[test]
fn test_readonly_method_property() {
    // { readonly method(): void }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let method_name = interner.intern_string("method");

    let method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let obj_readonly_method = interner.object(vec![PropertyInfo {
        name: method_name,
        type_id: method,
        write_type: method,
        optional: false,
        readonly: true,
        is_method: true,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    }]);

    let obj_mutable_method = interner.object(vec![PropertyInfo::method(method_name, method)]);

    // Mutable method is subtype of readonly method
    assert!(checker.is_subtype_of(obj_mutable_method, obj_readonly_method));
}

// =============================================================================
// Tuple Type Subtype Tests
// =============================================================================

// -----------------------------------------------------------------------------
// Fixed Length Tuple Assignability
// -----------------------------------------------------------------------------

#[test]
fn test_tuple_fixed_same_length_same_types() {
    // [string, number] <: [string, number]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let tuple1 = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
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

    let tuple2 = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
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

    // Same types - bidirectional subtype
    assert!(checker.is_subtype_of(tuple1, tuple2));
    assert!(checker.is_subtype_of(tuple2, tuple1));
}

#[test]
fn test_tuple_fixed_covariant_elements() {
    // ["hello", 42] <: [string, number]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let hello = interner.literal_string("hello");
    let forty_two = interner.literal_number(42.0);

    let literal_tuple = interner.tuple(vec![
        TupleElement {
            type_id: hello,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: forty_two,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let wide_tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
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

    // Literal tuple is subtype of wider tuple
    assert!(checker.is_subtype_of(literal_tuple, wide_tuple));
    // Wider tuple is NOT subtype of literal
    assert!(!checker.is_subtype_of(wide_tuple, literal_tuple));
}

#[test]
fn test_tuple_fixed_different_lengths_not_subtype() {
    // [string, number, boolean] is NOT subtype of [string, number]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let tuple_3 = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
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
            type_id: TypeId::BOOLEAN,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let tuple_2 = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
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

    // Extra element - not subtype of fixed tuple
    assert!(!checker.is_subtype_of(tuple_3, tuple_2));
    // Missing element - not subtype
    assert!(!checker.is_subtype_of(tuple_2, tuple_3));
}

#[test]
fn test_tuple_fixed_type_mismatch() {
    // [string, string] is NOT subtype of [string, number]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let tuple_ss = interner.tuple(vec![
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

    let tuple_sn = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
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

    // Different element types - not subtypes
    assert!(!checker.is_subtype_of(tuple_ss, tuple_sn));
    assert!(!checker.is_subtype_of(tuple_sn, tuple_ss));
}

#[test]
fn test_tuple_fixed_empty_tuple() {
    // [] <: []
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let empty_tuple = interner.tuple(vec![]);

    // Empty tuple is subtype of itself
    assert!(checker.is_subtype_of(empty_tuple, empty_tuple));
}

#[test]
fn test_tuple_fixed_single_element() {
    // [string] <: [string]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let single = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }]);

    assert!(checker.is_subtype_of(single, single));
}

#[test]
fn test_tuple_fixed_union_element() {
    // [string | number] <: [string | number]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let tuple_union = interner.tuple(vec![TupleElement {
        type_id: union,
        name: None,
        optional: false,
        rest: false,
    }]);

    let tuple_string = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }]);

    // [string] <: [string | number]
    assert!(checker.is_subtype_of(tuple_string, tuple_union));
    // [string | number] is NOT subtype of [string]
    assert!(!checker.is_subtype_of(tuple_union, tuple_string));
}

// -----------------------------------------------------------------------------
// Rest Element Handling
// -----------------------------------------------------------------------------

#[test]
fn test_tuple_rest_basic() {
    // [string, ...number[]] - tuple with rest
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let number_array = interner.array(TypeId::NUMBER);

    let tuple_with_rest = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: number_array,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    let tuple_string_number = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
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

    // Fixed tuple with matching types is subtype of rest tuple
    assert!(checker.is_subtype_of(tuple_string_number, tuple_with_rest));
}

#[test]
fn test_tuple_rest_accepts_multiple() {
    // [string, number, number, number] <: [string, ...number[]]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let number_array = interner.array(TypeId::NUMBER);

    let tuple_with_rest = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: number_array,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    let tuple_four = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
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
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    // Multiple numbers match rest
    assert!(checker.is_subtype_of(tuple_four, tuple_with_rest));
}

#[test]
fn test_tuple_rest_accepts_zero() {
    // [string] <: [string, ...number[]]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let number_array = interner.array(TypeId::NUMBER);

    let tuple_with_rest = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: number_array,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    let tuple_one = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }]);

    // Zero rest elements is valid
    assert!(checker.is_subtype_of(tuple_one, tuple_with_rest));
}

#[test]
fn test_tuple_rest_type_mismatch() {
    // [string, boolean] is NOT subtype of [string, ...number[]]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let number_array = interner.array(TypeId::NUMBER);

    let tuple_with_rest = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: number_array,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    let tuple_bool = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
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

    // boolean doesn't match number rest
    assert!(!checker.is_subtype_of(tuple_bool, tuple_with_rest));
}

#[test]
fn test_tuple_rest_to_rest() {
    // [...string[]] <: [...string[]]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);

    let tuple_rest1 = interner.tuple(vec![TupleElement {
        type_id: string_array,
        name: None,
        optional: false,
        rest: true,
    }]);

    let tuple_rest2 = interner.tuple(vec![TupleElement {
        type_id: string_array,
        name: None,
        optional: false,
        rest: true,
    }]);

    // Same rest types - bidirectional subtype
    assert!(checker.is_subtype_of(tuple_rest1, tuple_rest2));
    assert!(checker.is_subtype_of(tuple_rest2, tuple_rest1));
}

#[test]
fn test_tuple_rest_covariant() {
    // [...("hello")[]] <: [...string[]]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let hello = interner.literal_string("hello");
    let hello_array = interner.array(hello);
    let string_array = interner.array(TypeId::STRING);

    let tuple_literal_rest = interner.tuple(vec![TupleElement {
        type_id: hello_array,
        name: None,
        optional: false,
        rest: true,
    }]);

    let tuple_string_rest = interner.tuple(vec![TupleElement {
        type_id: string_array,
        name: None,
        optional: false,
        rest: true,
    }]);

    // Literal rest is subtype of string rest
    assert!(checker.is_subtype_of(tuple_literal_rest, tuple_string_rest));
}

#[test]
fn test_tuple_rest_middle_position() {
    // [string, ...number[], boolean] - rest in middle
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let number_array = interner.array(TypeId::NUMBER);

    let tuple_middle_rest = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: number_array,
            name: None,
            optional: false,
            rest: true,
        },
        TupleElement {
            type_id: TypeId::BOOLEAN,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let tuple_three = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
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
            type_id: TypeId::BOOLEAN,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    // Fixed tuple matches middle rest
    assert!(checker.is_subtype_of(tuple_three, tuple_middle_rest));
}

// -----------------------------------------------------------------------------
// Optional Element Patterns
// -----------------------------------------------------------------------------

#[test]
fn test_tuple_optional_basic() {
    // [string, number?] - optional second element
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let tuple_optional = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: true,
            rest: false,
        },
    ]);

    let tuple_one = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }]);

    // Shorter tuple matches optional
    assert!(checker.is_subtype_of(tuple_one, tuple_optional));
}

#[test]
fn test_tuple_optional_provided() {
    // [string, number] <: [string, number?]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let tuple_optional = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: true,
            rest: false,
        },
    ]);

    let tuple_both = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
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

    // Full tuple with optional provided is subtype
    assert!(checker.is_subtype_of(tuple_both, tuple_optional));
}

#[test]
fn test_tuple_optional_all_optional() {
    // [string?, number?]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let tuple_all_optional = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: true,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: true,
            rest: false,
        },
    ]);

    let empty_tuple = interner.tuple(vec![]);

    // Empty tuple matches all optional
    assert!(checker.is_subtype_of(empty_tuple, tuple_all_optional));
}

#[test]
fn test_tuple_optional_type_mismatch() {
    // [string, boolean] is NOT subtype of [string, number?]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let tuple_optional_number = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: true,
            rest: false,
        },
    ]);

    let tuple_with_bool = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
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

    // Wrong type for optional slot
    assert!(!checker.is_subtype_of(tuple_with_bool, tuple_optional_number));
}

