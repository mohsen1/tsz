#[test]
fn test_string_enum_unicode() {
    // enum E { EMOJI = "🎉", SYMBOL = "→" }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let emoji = interner.literal_string("🎉");
    let symbol = interner.literal_string("→");
    let enum_type = interner.union(vec![emoji, symbol]);

    // Unicode strings work
    assert!(checker.is_subtype_of(emoji, TypeId::STRING));
    assert!(checker.is_subtype_of(symbol, enum_type));
}

#[test]
fn test_enum_in_mapped_type_context() {
    // { [K in E]: K } where E = "a" | "b"
    let interner = TypeInterner::new();
    let _checker = SubtypeChecker::new(&interner);

    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");

    // Result object has properties "a" and "b"
    let result = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), lit_a),
        PropertyInfo::new(interner.intern_string("b"), lit_b),
    ]);

    assert!(result != TypeId::ERROR);
}

#[test]
fn test_index_signature_string_to_string() {
    // { [key: string]: number } is subtype of { [key: string]: number }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj_a = interner.object_with_index(ObjectShape {
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

    let obj_b = interner.object_with_index(ObjectShape {
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

    assert!(checker.is_subtype_of(obj_a, obj_b));
}

#[test]
fn test_index_signature_number_to_number() {
    // { [key: number]: string } is subtype of { [key: number]: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj_a = interner.object_with_index(ObjectShape {
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

    let obj_b = interner.object_with_index(ObjectShape {
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

    assert!(checker.is_subtype_of(obj_a, obj_b));
}

#[test]
fn test_index_signature_covariant_value_type() {
    // { [key: string]: "a" | "b" } is subtype of { [key: string]: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let literal_union = interner.union(vec![
        interner.literal_string("a"),
        interner.literal_string("b"),
    ]);

    let obj_specific = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: literal_union,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let obj_general = interner.object_with_index(ObjectShape {
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

    assert!(checker.is_subtype_of(obj_specific, obj_general));
    assert!(!checker.is_subtype_of(obj_general, obj_specific));
}

#[test]
fn test_index_signature_both_string_and_number() {
    // { [key: string]: any, [key: number]: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj_both = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::ANY,
            readonly: false,
            param_name: None,
        }),
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
    });

    let obj_string_only = interner.object_with_index(ObjectShape {
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

    // Object with both is subtype of object with just string
    assert!(checker.is_subtype_of(obj_both, obj_string_only));
}

#[test]
fn test_index_signature_number_subtype_of_string() {
    // Number index signature value must be subtype of string index signature value
    // { [key: string]: any, [key: number]: string } - string is subtype of any
    let interner = TypeInterner::new();
    let _checker = SubtypeChecker::new(&interner);

    let obj = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::ANY,
            readonly: false,
            param_name: None,
        }),
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
    });

    // This should be valid - string is subtype of any
    assert!(obj != TypeId::ERROR);
}

#[test]
fn test_index_signature_intersection_combines() {
    // { [key: string]: A } & { [key: string]: B } = { [key: string]: A & B }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj_a = interner.object_with_index(ObjectShape {
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

    let obj_b = interner.object_with_index(ObjectShape {
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

    let intersection = interner.intersection(vec![obj_a, obj_b]);

    // Intersection should be assignable to either
    assert!(checker.is_subtype_of(intersection, obj_a));
    assert!(checker.is_subtype_of(intersection, obj_b));
}
