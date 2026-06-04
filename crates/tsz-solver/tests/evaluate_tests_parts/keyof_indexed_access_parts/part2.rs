#[test]
fn test_keyof_with_method_property() {
    // keyof { fn(): void, prop: string } = "fn" | "prop"
    let interner = TypeInterner::new();

    let method_type = interner.function(FunctionShape {
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: vec![],
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let obj = interner.object(vec![
        PropertyInfo::method(interner.intern_string("fn"), method_type),
        PropertyInfo::new(interner.intern_string("prop"), TypeId::STRING),
    ]);

    let result = evaluate_keyof(&interner, obj);
    let key_fn = interner.literal_string("fn");
    let key_prop = interner.literal_string("prop");
    let expected = interner.union(vec![key_fn, key_prop]);
    assert_eq!(result, expected);
}

#[test]
fn test_keyof_union_with_index_signature_and_literal() {
    // keyof ({ a: string } | { [k: string]: number }) = "a" & string = "a"
    let interner = TypeInterner::new();

    let obj_literal = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let obj_indexed = interner.object_with_index(ObjectShape {
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

    let union = interner.union(vec![obj_literal, obj_indexed]);
    let result = evaluate_keyof(&interner, union);

    // Common keys: "a" is in first, and string covers "a" in second
    // Result should be "a" (the intersection of "a" and string)
    let expected = interner.literal_string("a");
    assert_eq!(result, expected);
}

#[test]
fn test_keyof_intersection_with_index_signature() {
    // keyof ({ a: string } & { [k: string]: number }) = "a" | string = string
    let interner = TypeInterner::new();

    let obj_literal = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let obj_indexed = interner.object_with_index(ObjectShape {
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

    let intersection = interner.intersection(vec![obj_literal, obj_indexed]);
    let result = evaluate_keyof(&interner, intersection);

    // Keys: "a" | string | number = string | number (since "a" is subtype of string)
    // Simplified: should contain string
    let key = interner.lookup(result);
    match key {
        Some(TypeData::Intrinsic(IntrinsicKind::String)) | Some(TypeData::Union(_)) => (),
        _ => panic!("Expected string or union type, got {key:?}"),
    }
}

#[test]
fn test_keyof_single_property_equals_literal() {
    // keyof { only: string } = "only" (not a union)
    let interner = TypeInterner::new();

    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("only"),
        TypeId::STRING,
    )]);

    let result = evaluate_keyof(&interner, obj);
    let expected = interner.literal_string("only");
    assert_eq!(result, expected);
}

#[test]
fn test_keyof_readonly_properties_included() {
    // keyof { readonly a: string, b: number } = "a" | "b"
    let interner = TypeInterner::new();

    let obj = interner.object(vec![
        PropertyInfo::readonly(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
    ]);

    let result = evaluate_keyof(&interner, obj);
    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let expected = interner.union(vec![key_a, key_b]);
    assert_eq!(result, expected);
}

#[test]
fn test_keyof_bigint() {
    // keyof bigint - should get apparent BigInt members
    let interner = TypeInterner::new();
    let result = evaluate_keyof(&interner, TypeId::BIGINT);
    // bigint has apparent members like "toString", "valueOf", etc.
    // Just verify it doesn't panic and returns some union
    assert_ne!(result, TypeId::BIGINT);
}

#[test]
fn test_keyof_symbol() {
    // keyof symbol - should get apparent Symbol members
    let interner = TypeInterner::new();
    let result = evaluate_keyof(&interner, TypeId::SYMBOL);
    // symbol has apparent members like "toString", "valueOf", "description"
    assert_ne!(result, TypeId::SYMBOL);
}

#[test]
fn test_keyof_boolean() {
    // keyof boolean - should get apparent Boolean members
    let interner = TypeInterner::new();
    let result = evaluate_keyof(&interner, TypeId::BOOLEAN);
    // boolean has apparent members from Boolean interface
    assert_ne!(result, TypeId::BOOLEAN);
}

#[test]
fn test_intersection_reduction_disjoint_discriminant_evaluates_never() {
    let interner = TypeInterner::new();

    let kind = interner.intern_string("kind");
    let obj_a = interner.object(vec![PropertyInfo::new(kind, interner.literal_string("a"))]);
    let obj_b = interner.object(vec![PropertyInfo::new(kind, interner.literal_string("b"))]);

    let intersection = interner.intersection(vec![obj_a, obj_b]);
    let result = evaluate_type(&interner, intersection);

    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_mapped_type_basic() {
    let interner = TypeInterner::new();

    // { [K in "x" | "y"]: number }
    // Should produce { x: number, y: number }
    let key_x = interner.literal_string("x");
    let key_y = interner.literal_string("y");
    let keys = interner.union(vec![key_x, key_y]);

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keys,
        name_type: None,
        template: TypeId::NUMBER,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);

    // Result should be { x: number, y: number }
    let expected = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("y"), TypeId::NUMBER),
    ]);
    assert_eq!(result, expected);
}

#[test]
fn test_mapped_type_any_keys_with_never_template_produces_indexes() {
    let interner = TypeInterner::new();

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: TypeId::ANY,
        name_type: None,
        template: TypeId::NEVER,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);
    let Some(TypeData::ObjectWithIndex(shape_id)) = interner.lookup(result) else {
        panic!("expected mapped any keys to evaluate to object indexes, got {result:?}");
    };
    let shape = interner.object_shape(shape_id);

    assert_eq!(shape.properties.len(), 0);
    assert_eq!(
        shape
            .string_index
            .expect("expected string index")
            .value_type,
        TypeId::NEVER
    );
    assert_eq!(
        shape
            .number_index
            .expect("expected number index")
            .value_type,
        TypeId::NEVER
    );
}

#[test]
fn test_mapped_type_over_string_keys() {
    let interner = TypeInterner::new();

    let constraint = interner.intern(TypeData::KeyOf(TypeId::STRING));
    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint,
        name_type: None,
        template: TypeId::BOOLEAN,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);
    let key = interner.lookup(result).expect("Expected object type");

    match key {
        TypeData::ObjectWithIndex(shape_id) => {
            let shape = interner.object_shape(shape_id);
            let length = interner.intern_string("length");
            let to_string = interner.intern_string("toString");
            let mut saw_length = false;
            let mut saw_to_string = false;

            for prop in &shape.properties {
                if prop.name == length {
                    assert_eq!(prop.type_id, TypeId::BOOLEAN);
                    saw_length = true;
                }
                if prop.name == to_string {
                    assert_eq!(prop.type_id, TypeId::BOOLEAN);
                    saw_to_string = true;
                }
            }

            assert!(saw_length, "missing length property");
            assert!(saw_to_string, "missing toString property");
            let number_index = shape
                .number_index
                .as_ref()
                .expect("expected number index signature");
            assert_eq!(number_index.key_type, TypeId::NUMBER);
            assert_eq!(number_index.value_type, TypeId::BOOLEAN);
        }
        other => panic!("Expected object type, got {other:?}"),
    }
}
