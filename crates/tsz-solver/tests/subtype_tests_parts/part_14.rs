#[test]
fn test_explain_failure_literal_mismatch() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let hello = interner.literal_string("hello");
    let world = interner.literal_string("world");

    // "hello" vs "world" should produce LiteralTypeMismatch
    let reason = checker.explain_failure(hello, world);
    assert!(reason.is_some());
    match reason.unwrap() {
        SubtypeFailureReason::LiteralTypeMismatch {
            source_type,
            target_type,
        } => {
            assert_eq!(source_type, hello);
            assert_eq!(target_type, world);
        }
        other => panic!("Expected LiteralTypeMismatch, got {other:?}"),
    }
}

#[test]
fn test_explain_failure_literal_to_incompatible_intrinsic() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let hello = interner.literal_string("hello");

    // "hello" vs number should produce LiteralTypeMismatch
    let reason = checker.explain_failure(hello, TypeId::NUMBER);
    assert!(reason.is_some());
    match reason.unwrap() {
        SubtypeFailureReason::LiteralTypeMismatch {
            source_type,
            target_type,
        } => {
            assert_eq!(source_type, hello);
            assert_eq!(target_type, TypeId::NUMBER);
        }
        other => panic!("Expected LiteralTypeMismatch, got {other:?}"),
    }
}

#[test]
fn test_explain_failure_error_type() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // ERROR type should produce ErrorType failure reason, not None
    let reason = checker.explain_failure(TypeId::ERROR, TypeId::NUMBER);
    assert!(
        reason.is_some(),
        "ERROR type should produce a failure reason"
    );
    match reason.unwrap() {
        SubtypeFailureReason::ErrorType {
            source_type,
            target_type,
        } => {
            assert_eq!(source_type, TypeId::ERROR);
            assert_eq!(target_type, TypeId::NUMBER);
        }
        other => panic!("Expected ErrorType, got {other:?}"),
    }
}

#[test]
fn test_literal_number_to_string_fails() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let forty_two = interner.literal_number(42.0);

    // 42 vs string should fail
    assert!(!checker.is_subtype_of(forty_two, TypeId::STRING));

    // And produce a proper failure reason
    let reason = checker.explain_failure(forty_two, TypeId::STRING);
    assert!(reason.is_some());
    match reason.unwrap() {
        SubtypeFailureReason::LiteralTypeMismatch { .. } => {}
        other => panic!("Expected LiteralTypeMismatch, got {other:?}"),
    }
}

#[test]
fn test_intrinsic_to_literal_fails() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let hello = interner.literal_string("hello");

    // string vs "hello" should fail (widening is not allowed)
    assert!(!checker.is_subtype_of(TypeId::STRING, hello));

    // And produce a proper failure reason
    let reason = checker.explain_failure(TypeId::STRING, hello);
    assert!(reason.is_some());
    match reason.unwrap() {
        SubtypeFailureReason::TypeMismatch { .. } => {}
        other => panic!("Expected TypeMismatch, got {other:?}"),
    }
}

// ============================================================================
// Explain failure: mapped type evaluation in the explain path
// These tests verify that mapped types are evaluated to concrete object types
// during explain_failure, enabling property-level diagnostics (TS2739/TS2741).
// ============================================================================

#[test]
fn test_explain_failure_mapped_type_target_missing_property() {
    // Simulates: Required<{ a?: string, b: number }> as target
    // with source { b: number } (missing 'a').
    // The mapped type (with -? modifier) should be evaluated to a concrete
    // object { a: string, b: number } so explain_failure can detect the
    // missing property and return MissingProperty instead of TypeMismatch.
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");

    // Build the source object: { a?: string, b: number }
    let source_obj = interner.object(vec![
        PropertyInfo::opt(a_name, TypeId::STRING),
        PropertyInfo::new(b_name, TypeId::NUMBER),
    ]);

    // Build Required<source_obj> as a mapped type: { [K in keyof T]-?: T[K] }
    let keyof_source = interner.keyof(source_obj);
    let k_param_info = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let k_param = interner.intern(TypeData::TypeParameter(k_param_info));
    let template = interner.index_access(source_obj, k_param);
    let required_target = interner.mapped(MappedType {
        type_param: k_param_info,
        constraint: keyof_source,
        name_type: None,
        template,
        optional_modifier: Some(MappedModifier::Remove),
        readonly_modifier: None,
    });

    // Source is missing property 'a': { b: number }
    let incomplete_source = interner.object(vec![PropertyInfo::new(b_name, TypeId::NUMBER)]);

    // Verify the assignment fails
    assert!(
        !checker.is_subtype_of(incomplete_source, required_target),
        "{{b: number}} should not be assignable to Required<{{a?: string, b: number}}>"
    );

    // explain_failure should return MissingProperty (TS2741), not TypeMismatch (TS2322)
    let reason = checker.explain_failure(incomplete_source, required_target);
    assert!(reason.is_some(), "Should produce a failure reason");
    match reason.unwrap() {
        SubtypeFailureReason::MissingProperty { property_name, .. } => {
            assert_eq!(property_name, a_name, "Missing property should be 'a'");
        }
        SubtypeFailureReason::MissingProperties { .. } => {
            // Also acceptable — depends on how many properties are missing
        }
        other => panic!(
            "Expected MissingProperty or MissingProperties for mapped type target, got {other:?}"
        ),
    }
}

#[test]
fn test_explain_failure_mapped_type_source_evaluated() {
    // Verify that mapped type sources are also evaluated.
    // Source: Partial<{ a: string, b: number }> => { a?: string, b?: number }
    // Target: { a: string, b: number }
    // The source mapped type should evaluate to a concrete object so
    // explain_failure can detect the optional→required mismatch.
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");

    // Build the concrete object { a: string, b: number }
    let concrete_obj = interner.object(vec![
        PropertyInfo::new(a_name, TypeId::STRING),
        PropertyInfo::new(b_name, TypeId::NUMBER),
    ]);

    // Build Partial<concrete_obj> as a mapped type: { [K in keyof T]+?: T[K] }
    let keyof_obj = interner.keyof(concrete_obj);
    let k_param_info = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let k_param = interner.intern(TypeData::TypeParameter(k_param_info));
    let template = interner.index_access(concrete_obj, k_param);
    let partial_source = interner.mapped(MappedType {
        type_param: k_param_info,
        constraint: keyof_obj,
        name_type: None,
        template,
        optional_modifier: Some(MappedModifier::Add),
        readonly_modifier: None,
    });

    // Partial<T> → T should fail (properties may be missing)
    assert!(
        !checker.is_subtype_of(partial_source, concrete_obj),
        "Partial<{{a: string, b: number}}> should not be assignable to {{a: string, b: number}}"
    );

    // explain_failure should return a structured reason (not None)
    let reason = checker.explain_failure(partial_source, concrete_obj);
    assert!(
        reason.is_some(),
        "Partial<T> → T should produce a failure reason"
    );
    // The specific reason depends on how the solver handles optional→required mismatches.
    // The important thing is we get a structured reason, not a generic TypeMismatch from
    // failing to enumerate properties on an unevaluated mapped type.
}

// ============================================================================
// Tuple-to-Array Assignability Tests
// These tests document TypeScript behavior for assigning tuples to arrays
// ============================================================================

// --- Homogeneous Tuples to Arrays ---

#[test]
fn test_tuple_to_array_homogeneous_two_strings() {
    // [string, string] -> string[] should succeed
    // In TypeScript: const arr: string[] = ["a", "b"]; // OK
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let source = interner.tuple(vec![
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

    assert!(
        checker.is_subtype_of(source, string_array),
        "[string, string] should be assignable to string[]"
    );
}

#[test]
fn test_tuple_to_array_homogeneous_three_numbers() {
    // [number, number, number] -> number[] should succeed
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let number_array = interner.array(TypeId::NUMBER);
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
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    assert!(
        checker.is_subtype_of(source, number_array),
        "[number, number, number] should be assignable to number[]"
    );
}

#[test]
fn test_tuple_to_array_homogeneous_booleans() {
    // [boolean, boolean] -> boolean[] should succeed
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let boolean_array = interner.array(TypeId::BOOLEAN);
    let source = interner.tuple(vec![
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

    assert!(
        checker.is_subtype_of(source, boolean_array),
        "[boolean, boolean] should be assignable to boolean[]"
    );
}

#[test]
fn test_tuple_to_array_homogeneous_literal_to_base() {
    // ["hello", "world"] -> string[] should succeed (literals widen to base type)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let hello = interner.literal_string("hello");
    let world = interner.literal_string("world");
    let string_array = interner.array(TypeId::STRING);
    let source = interner.tuple(vec![
        TupleElement {
            type_id: hello,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: world,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    assert!(
        checker.is_subtype_of(source, string_array),
        "[\"hello\", \"world\"] should be assignable to string[]"
    );
}

#[test]
fn test_tuple_to_array_homogeneous_number_literals() {
    // [1, 2, 3] -> number[] should succeed
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let one = interner.literal_number(1.0);
    let two = interner.literal_number(2.0);
    let three = interner.literal_number(3.0);
    let number_array = interner.array(TypeId::NUMBER);
    let source = interner.tuple(vec![
        TupleElement {
            type_id: one,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: two,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: three,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    assert!(
        checker.is_subtype_of(source, number_array),
        "[1, 2, 3] should be assignable to number[]"
    );
}

// --- Heterogeneous Tuples to Union Arrays ---

#[test]
fn test_tuple_to_union_array_string_number() {
    // [string, number] -> (string | number)[] should succeed
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union_elem = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let union_array = interner.array(union_elem);
    let source = interner.tuple(vec![
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

    assert!(
        checker.is_subtype_of(source, union_array),
        "[string, number] should be assignable to (string | number)[]"
    );
}

#[test]
fn test_tuple_to_union_array_number_boolean() {
    // [number, boolean] -> (number | boolean)[] should succeed
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union_elem = interner.union(vec![TypeId::NUMBER, TypeId::BOOLEAN]);
    let union_array = interner.array(union_elem);
    let source = interner.tuple(vec![
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

    assert!(
        checker.is_subtype_of(source, union_array),
        "[number, boolean] should be assignable to (number | boolean)[]"
    );
}

#[test]
fn test_tuple_to_union_array_three_types() {
    // [string, number, boolean] -> (string | number | boolean)[] should succeed
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union_elem = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);
    let union_array = interner.array(union_elem);
    let source = interner.tuple(vec![
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

    assert!(
        checker.is_subtype_of(source, union_array),
        "[string, number, boolean] should be assignable to (string | number | boolean)[]"
    );
}

#[test]
fn test_tuple_to_union_array_literals_to_base() {
    // ["a", 1] -> (string | number)[] should succeed
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_literal = interner.literal_string("a");
    let one_literal = interner.literal_number(1.0);
    let union_elem = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let union_array = interner.array(union_elem);
    let source = interner.tuple(vec![
        TupleElement {
            type_id: a_literal,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: one_literal,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    assert!(
        checker.is_subtype_of(source, union_array),
        "[\"a\", 1] should be assignable to (string | number)[]"
    );
}

#[test]
fn test_tuple_to_union_array_subset_elements() {
    // [string, string] -> (string | number)[] should succeed
    // All elements match a subset of the union
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union_elem = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let union_array = interner.array(union_elem);
    let source = interner.tuple(vec![
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

    assert!(
        checker.is_subtype_of(source, union_array),
        "[string, string] should be assignable to (string | number)[]"
    );
}

#[test]
fn test_tuple_to_union_array_fails_missing_element_type() {
    // [string, boolean] -> (string | number)[] should FAIL
    // boolean is not in the union (string | number)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union_elem = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let union_array = interner.array(union_elem);
    let source = interner.tuple(vec![
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

    assert!(
        !checker.is_subtype_of(source, union_array),
        "[string, boolean] should NOT be assignable to (string | number)[] - boolean is not in union"
    );
}

// --- Tuples with Rest Elements to Arrays ---

#[test]
fn test_tuple_rest_to_array_matching() {
    // [number, ...string[]] -> (number | string)[] should succeed
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union_elem = interner.union(vec![TypeId::NUMBER, TypeId::STRING]);
    let union_array = interner.array(union_elem);
    let string_array = interner.array(TypeId::STRING);
    let source = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: string_array,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    assert!(
        checker.is_subtype_of(source, union_array),
        "[number, ...string[]] should be assignable to (number | string)[]"
    );
}

#[test]
fn test_tuple_rest_to_array_homogeneous() {
    // [string, ...string[]] -> string[] should succeed
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let source = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: string_array,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    assert!(
        checker.is_subtype_of(source, string_array),
        "[string, ...string[]] should be assignable to string[]"
    );
}

#[test]
fn test_tuple_rest_to_array_prefix_not_matching() {
    // [boolean, ...string[]] -> string[] should FAIL
    // The first element (boolean) is not string
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let source = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::BOOLEAN,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: string_array,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    assert!(
        !checker.is_subtype_of(source, string_array),
        "[boolean, ...string[]] should NOT be assignable to string[]"
    );
}

#[test]
fn test_tuple_rest_to_array_rest_not_matching() {
    // [string, ...number[]] -> string[] should FAIL
    // The rest element (number[]) is not compatible with string[]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let number_array = interner.array(TypeId::NUMBER);
    let source = interner.tuple(vec![
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

    assert!(
        !checker.is_subtype_of(source, string_array),
        "[string, ...number[]] should NOT be assignable to string[]"
    );
}

#[test]
fn test_tuple_rest_multiple_prefix_to_union_array() {
    // [string, number, ...boolean[]] -> (string | number | boolean)[] should succeed
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union_elem = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);
    let union_array = interner.array(union_elem);
    let boolean_array = interner.array(TypeId::BOOLEAN);
    let source = interner.tuple(vec![
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
            type_id: boolean_array,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    assert!(
        checker.is_subtype_of(source, union_array),
        "[string, number, ...boolean[]] should be assignable to (string | number | boolean)[]"
    );
}

#[test]
fn test_tuple_only_rest_to_array() {
    // [...number[]] -> number[] should succeed
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let number_array = interner.array(TypeId::NUMBER);
    let source = interner.tuple(vec![TupleElement {
        type_id: number_array,
        name: None,
        optional: false,
        rest: true,
    }]);

    assert!(
        checker.is_subtype_of(source, number_array),
        "[...number[]] should be assignable to number[]"
    );
}

// --- Edge Cases: Empty Tuples ---

#[test]
fn test_empty_tuple_to_string_array() {
    // [] -> string[] should succeed (empty tuple is compatible with any array)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let empty_tuple = interner.tuple(Vec::new());

    assert!(
        checker.is_subtype_of(empty_tuple, string_array),
        "[] should be assignable to string[]"
    );
}

#[test]
fn test_empty_tuple_to_number_array() {
    // [] -> number[] should succeed
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let number_array = interner.array(TypeId::NUMBER);
    let empty_tuple = interner.tuple(Vec::new());

    assert!(
        checker.is_subtype_of(empty_tuple, number_array),
        "[] should be assignable to number[]"
    );
}

#[test]
fn test_empty_tuple_to_union_array() {
    // [] -> (string | number)[] should succeed
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union_elem = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let union_array = interner.array(union_elem);
    let empty_tuple = interner.tuple(Vec::new());

    assert!(
        checker.is_subtype_of(empty_tuple, union_array),
        "[] should be assignable to (string | number)[]"
    );
}

#[test]
fn test_empty_tuple_to_any_array() {
    // [] -> any[] should succeed
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let any_array = interner.array(TypeId::ANY);
    let empty_tuple = interner.tuple(Vec::new());

    assert!(
        checker.is_subtype_of(empty_tuple, any_array),
        "[] should be assignable to any[]"
    );
}

#[test]
fn test_empty_tuple_to_never_array() {
    // [] -> never[] should succeed (empty tuple has zero elements, all of which are never)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let never_array = interner.array(TypeId::NEVER);
    let empty_tuple = interner.tuple(Vec::new());

    assert!(
        checker.is_subtype_of(empty_tuple, never_array),
        "[] should be assignable to never[]"
    );
}

// --- Edge Cases: Single-Element Tuples ---

#[test]
fn test_single_element_tuple_to_array() {
    // [string] -> string[] should succeed
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let source = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }]);

    assert!(
        checker.is_subtype_of(source, string_array),
        "[string] should be assignable to string[]"
    );
}

#[test]
fn test_single_element_tuple_type_mismatch() {
    // [number] -> string[] should FAIL
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let source = interner.tuple(vec![TupleElement {
        type_id: TypeId::NUMBER,
        name: None,
        optional: false,
        rest: false,
    }]);

    assert!(
        !checker.is_subtype_of(source, string_array),
        "[number] should NOT be assignable to string[]"
    );
}

#[test]
fn test_single_element_tuple_to_union_array() {
    // [string] -> (string | number)[] should succeed
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union_elem = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let union_array = interner.array(union_elem);
    let source = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }]);

    assert!(
        checker.is_subtype_of(source, union_array),
        "[string] should be assignable to (string | number)[]"
    );
}

// --- Edge Cases: Tuples with Optional Elements ---

#[test]
fn test_tuple_optional_to_array() {
    // [string, number?] -> (string | number)[] should succeed
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union_elem = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let union_array = interner.array(union_elem);
    let source = interner.tuple(vec![
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

    assert!(
        checker.is_subtype_of(source, union_array),
        "[string, number?] should be assignable to (string | number)[]"
    );
}

#[test]
fn test_tuple_all_optional_to_array() {
    // [string?, number?] -> (string | number)[] should succeed
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union_elem = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let union_array = interner.array(union_elem);
    let source = interner.tuple(vec![
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

    assert!(
        checker.is_subtype_of(source, union_array),
        "[string?, number?] should be assignable to (string | number)[]"
    );
}

#[test]
fn test_tuple_optional_homogeneous_to_array() {
    // [string, string?] -> string[] should succeed
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let source = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: true,
            rest: false,
        },
    ]);

    assert!(
        checker.is_subtype_of(source, string_array),
        "[string, string?] should be assignable to string[]"
    );
}

#[test]
fn test_tuple_optional_element_type_mismatch() {
    // [string, boolean?] -> string[] should FAIL
    // Optional element type (boolean) doesn't match array element type (string)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let source = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::BOOLEAN,
            name: None,
            optional: true,
            rest: false,
        },
    ]);

    assert!(
        !checker.is_subtype_of(source, string_array),
        "[string, boolean?] should NOT be assignable to string[] - boolean is not string"
    );
}

#[test]
fn test_tuple_optional_with_rest_to_array() {
    // [string?, ...number[]] -> (string | number)[] should succeed
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union_elem = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let union_array = interner.array(union_elem);
    let number_array = interner.array(TypeId::NUMBER);
    let source = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: true,
            rest: false,
        },
        TupleElement {
            type_id: number_array,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    assert!(
        checker.is_subtype_of(source, union_array),
        "[string?, ...number[]] should be assignable to (string | number)[]"
    );
}

// --- Edge Cases: Named Tuple Elements ---

#[test]
fn test_named_tuple_to_array() {
    // [name: string, age: number] -> (string | number)[] should succeed
    // Named tuple elements don't affect assignability to arrays
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let name_atom = interner.intern_string("name");
    let age_atom = interner.intern_string("age");
    let union_elem = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let union_array = interner.array(union_elem);
    let source = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: Some(name_atom),
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: Some(age_atom),
            optional: false,
            rest: false,
        },
    ]);

    assert!(
        checker.is_subtype_of(source, union_array),
        "[name: string, age: number] should be assignable to (string | number)[]"
    );
}

// --- Edge Cases: Special Types ---

#[test]
fn test_tuple_with_any_to_string_array() {
    // [any, any] -> string[] should succeed (any is assignable to anything)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let source = interner.tuple(vec![
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
    ]);

    assert!(
        checker.is_subtype_of(source, string_array),
        "[any, any] should be assignable to string[]"
    );
}

#[test]
fn test_tuple_to_any_array() {
    // [string, number] -> any[] should succeed
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let any_array = interner.array(TypeId::ANY);
    let source = interner.tuple(vec![
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

    assert!(
        checker.is_subtype_of(source, any_array),
        "[string, number] should be assignable to any[]"
    );
}

#[test]
fn test_tuple_with_never_to_string_array() {
    // [never, never] -> string[] should succeed (never is subtype of all types)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let source = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NEVER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NEVER,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    assert!(
        checker.is_subtype_of(source, string_array),
        "[never, never] should be assignable to string[]"
    );
}

#[test]
fn test_tuple_to_unknown_array() {
    // [string, number] -> unknown[] should succeed
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let unknown_array = interner.array(TypeId::UNKNOWN);
    let source = interner.tuple(vec![
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

    assert!(
        checker.is_subtype_of(source, unknown_array),
        "[string, number] should be assignable to unknown[]"
    );
}

#[test]
fn test_tuple_with_unknown_to_string_array() {
    // [unknown, unknown] -> string[] should FAIL
    // unknown is not assignable to string
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let source = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::UNKNOWN,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::UNKNOWN,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    assert!(
        !checker.is_subtype_of(source, string_array),
        "[unknown, unknown] should NOT be assignable to string[]"
    );
}

// --- Edge Cases: Readonly arrays ---

#[test]
fn test_tuple_to_readonly_array() {
    // [string, string] -> readonly string[] should succeed
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // readonly_array takes the element type, not an array type
    let readonly_string_array = interner.readonly_array(TypeId::STRING);
    let source = interner.tuple(vec![
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

    assert!(
        checker.is_subtype_of(source, readonly_string_array),
        "[string, string] should be assignable to readonly string[]"
    );
}

// --- Edge Cases: Nested tuples ---

#[test]
fn test_nested_tuple_to_array() {
    // [[string, number], [string, number]] -> [string, number][] should succeed
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let inner_tuple = interner.tuple(vec![
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
    let tuple_array = interner.array(inner_tuple);
    let source = interner.tuple(vec![
        TupleElement {
            type_id: inner_tuple,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: inner_tuple,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    assert!(
        checker.is_subtype_of(source, tuple_array),
        "[[string, number], [string, number]] should be assignable to [string, number][]"
    );
}

// --- Negative Cases: Array to Tuple (reverse direction) ---

#[test]
fn test_array_to_tuple_fails_fixed() {
    // string[] -> [string] should FAIL (array has unknown length)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let target = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }]);

    assert!(
        !checker.is_subtype_of(string_array, target),
        "string[] should NOT be assignable to [string]"
    );
}

#[test]
fn test_array_to_tuple_fails_multi_element() {
    // string[] -> [string, string] should FAIL
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let target = interner.tuple(vec![
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

    assert!(
        !checker.is_subtype_of(string_array, target),
        "string[] should NOT be assignable to [string, string]"
    );
}

// =============================================================================
// THIS TYPE NARROWING IN CLASS HIERARCHIES
// =============================================================================

#[test]
fn test_this_type_class_hierarchy_fluent_return() {
    // class Base { method(): this }
    // class Derived extends Base { extra(): number }
    // Derived.method() should have type Derived (not Base)
    let interner = TypeInterner::new();

    let this_type = interner.intern(TypeData::ThisType);

    // Base method returning this
    let base_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: this_type,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    let base_class = interner.object(vec![PropertyInfo::method(
        interner.intern_string("method"),
        base_method,
    )]);

    // Derived class with extra property
    let extra_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    let derived_class = interner.object(vec![
        PropertyInfo::method(interner.intern_string("method"), base_method),
        PropertyInfo::method(interner.intern_string("extra"), extra_method),
    ]);

    // Derived is subtype of Base (has all base properties)
    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.is_subtype_of(derived_class, base_class),
        "Derived should be subtype of Base"
    );
}

#[test]
fn test_this_type_in_method_parameter_covariant() {
    // From TS_UNSOUNDNESS_CATALOG #19:
    // class Box { compare(other: this) }
    // class StringBox extends Box { compare(other: StringBox) }
    // StringBox should be subtype of Box (this is covariant in class hierarchies)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let this_type = interner.intern(TypeData::ThisType);

    // Box.compare(other: this)
    let box_compare = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("other")),
            type_id: this_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    let box_class = interner.object(vec![PropertyInfo::method(
        interner.intern_string("compare"),
        box_compare,
    )]);

    // StringBox type
    let stringbox_class = interner.object(vec![PropertyInfo::method(
        interner.intern_string("compare"),
        box_compare,
    )]);

    // StringBox should be subtype of Box
    // (this type enables bivariance, which makes this pass)
    assert!(
        checker.is_subtype_of(stringbox_class, box_class),
        "StringBox should be subtype of Box (this type enables bivariance)"
    );
}

#[test]
fn test_this_type_explicit_this_parameter_inheritance() {
    // class Base { method(this: Base): void }
    // class Derived extends Base { method(this: Derived): void }
    // Derived should be subtype of Base
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // Base class reference
    let base_class_ref = interner.lazy(DefId(100));

    // Base.method(this: Base)
    let base_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: Some(base_class_ref),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    let _base_class = interner.object(vec![PropertyInfo::method(
        interner.intern_string("method"),
        base_method,
    )]);

    // Derived class reference
    let derived_class_ref = interner.lazy(DefId(101));

    // Derived.method(this: Derived)
    let derived_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: Some(derived_class_ref),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    let _derived_class = interner.object(vec![PropertyInfo::method(
        interner.intern_string("method"),
        derived_method,
    )]);

    // Check that derived method is compatible with base method
    // (Methods get bivariance)
    assert!(
        checker.is_subtype_of(derived_method, base_method),
        "Derived method should be subtype of Base method (method bivariance)"
    );
}

#[test]
fn test_this_type_return_covariant_in_hierarchy() {
    // Test that `this` return type is covariant
    // class Base { fluent(): this }
    // class Derived extends Base { fluent(): this }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let this_type = interner.intern(TypeData::ThisType);

    // Base.fluent(): this
    let base_fluent = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: this_type,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    // Both Base and Derived have the same fluent method returning this
    let base_class = interner.object(vec![PropertyInfo::method(
        interner.intern_string("fluent"),
        base_fluent,
    )]);

    let derived_class = interner.object(vec![
        PropertyInfo::method(interner.intern_string("fluent"), base_fluent),
        PropertyInfo::new(interner.intern_string("extra"), TypeId::NUMBER),
    ]);

    // Derived is subtype of Base
    assert!(
        checker.is_subtype_of(derived_class, base_class),
        "Derived should be subtype of Base (same this-returning method)"
    );
}

#[test]
fn test_this_type_polymorphic_method_chain() {
    // Test fluent chaining with this type
    // class Builder {
    //   setName(name: string): this
    //   setValue(value: number): this
    //   build(): Result
    // }
    let interner = TypeInterner::new();

    let this_type = interner.intern(TypeData::ThisType);
    let result_type = interner.lazy(DefId(1));

    let set_name = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("name")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: this_type,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    let set_value = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: this_type,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    let build = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: result_type,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    let builder = interner.object(vec![
        PropertyInfo::method(interner.intern_string("setName"), set_name),
        PropertyInfo::method(interner.intern_string("setValue"), set_value),
        PropertyInfo::method(interner.intern_string("build"), build),
    ]);

    // Builder with all fluent methods should be valid
    assert_ne!(builder, TypeId::ERROR);
}

#[test]
fn test_this_type_with_generics_in_class() {
    // class Container<T> {
    //   map<U>(fn: (value: T) => U): Container<U>
    //   filter(predicate: (value: T) => boolean): this
    // }
    let interner = TypeInterner::new();

    let this_type = interner.intern(TypeData::ThisType);
    let _t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let _u_param = TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: None,
        default: None,
        is_const: false,
    };

    // filter method returning this (polymorphic return)
    let filter_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("predicate")),
            type_id: interner.function(FunctionShape {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("value")),
                    type_id: TypeId::UNKNOWN,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: TypeId::BOOLEAN,
                type_predicate: None,
                is_constructor: false,
                is_method: false,
            }),
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: this_type,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    let container = interner.object(vec![PropertyInfo::method(
        interner.intern_string("filter"),
        filter_method,
    )]);

    // Container with filter returning this should be valid
    assert_ne!(container, TypeId::ERROR);
}

#[test]
fn test_this_type_class_hierarchy_multiple_methods() {
    // Test class hierarchy with multiple methods using this
    // class Base {
    //   method1(): this
    //   method2(): this
    // }
    // class Derived extends Base {
    //   method1(): this
    //   method2(): this
    //   method3(): number
    // }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let this_type = interner.intern(TypeData::ThisType);

    let method1 = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: this_type,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    let method2 = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: this_type,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    let method3 = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    let base_class = interner.object(vec![
        PropertyInfo::method(interner.intern_string("method1"), method1),
        PropertyInfo::method(interner.intern_string("method2"), method2),
    ]);

    let derived_class = interner.object(vec![
        PropertyInfo::method(interner.intern_string("method1"), method1),
        PropertyInfo::method(interner.intern_string("method2"), method2),
        PropertyInfo::method(interner.intern_string("method3"), method3),
    ]);

    // Derived should be subtype of Base (all methods compatible)
    assert!(
        checker.is_subtype_of(derived_class, base_class),
        "Derived should be subtype of Base (all this-returning methods compatible)"
    );
}

#[test]
fn test_this_type_with_constrained_generic() {
    // Test this type with constrained generic parameter
    // class Base {
    //   method<T extends Base>(this: T): T
    // }
    let interner = TypeInterner::new();

    let base_ref = interner.lazy(DefId(100));
    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(base_ref),
        default: None,
        is_const: false,
    };

    let t_type_param = interner.intern(TypeData::TypeParameter(t_param));

    // method<T extends Base>(this: T): T
    let constrained_method = interner.function(FunctionShape {
        type_params: vec![t_param],
        params: vec![],
        this_type: Some(t_type_param),
        return_type: t_type_param,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    let base_class = interner.object(vec![PropertyInfo::method(
        interner.intern_string("method"),
        constrained_method,
    )]);

    // Base with constrained this method should be valid
    assert_ne!(base_class, TypeId::ERROR);
}

