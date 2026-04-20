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
    use crate::types::TypeData;
    use crate::{SymbolRef, TypeEnvironment};

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
