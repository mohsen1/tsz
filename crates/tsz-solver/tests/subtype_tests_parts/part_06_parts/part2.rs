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
