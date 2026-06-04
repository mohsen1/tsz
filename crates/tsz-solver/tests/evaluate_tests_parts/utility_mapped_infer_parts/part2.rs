#[test]
fn test_readonly_simple_object() {
    // Readonly<{ a: string, b: number }> = { readonly a: string, readonly b: number }
    let interner = TypeInterner::new();

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");

    let readonly_obj = interner.object(vec![
        PropertyInfo {
            name: a_name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: true, // Made readonly
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
            is_symbol_named: false,
            single_quoted_name: false,
        },
        PropertyInfo {
            name: b_name,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: true, // Made readonly
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
            is_symbol_named: false,
            single_quoted_name: false,
        },
    ]);

    match interner.lookup(readonly_obj) {
        Some(TypeData::Object(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            assert!(shape.properties[0].readonly);
            assert!(shape.properties[1].readonly);
        }
        _ => panic!("Expected Object type"),
    }
}

#[test]
fn test_readonly_array() {
    // Readonly<string[]> = readonly string[]
    let interner = TypeInterner::new();

    let string_array = interner.array(TypeId::STRING);
    let readonly_array = interner.intern(TypeData::ReadonlyType(string_array));

    match interner.lookup(readonly_array) {
        Some(TypeData::ReadonlyType(inner)) => {
            assert_eq!(inner, string_array);
        }
        _ => panic!("Expected ReadonlyType"),
    }
}

#[test]
fn test_readonly_tuple() {
    // Readonly<[string, number]> = readonly [string, number]
    let interner = TypeInterner::new();

    let tuple = interner.tuple(vec![
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

    let readonly_tuple = interner.intern(TypeData::ReadonlyType(tuple));

    match interner.lookup(readonly_tuple) {
        Some(TypeData::ReadonlyType(inner)) => {
            assert_eq!(inner, tuple);
            // Verify inner is still a tuple
            match interner.lookup(inner) {
                Some(TypeData::Tuple(_)) => {}
                _ => panic!("Expected Tuple inside ReadonlyType"),
            }
        }
        _ => panic!("Expected ReadonlyType"),
    }
}

#[test]
fn test_readonly_nested() {
    // Readonly<{ items: string[] }> - items property is readonly, not the array
    let interner = TypeInterner::new();

    let items_name = interner.intern_string("items");
    let string_array = interner.array(TypeId::STRING);

    let readonly_obj = interner.object(vec![PropertyInfo {
        name: items_name,
        type_id: string_array, // Array itself isn't readonly
        write_type: string_array,
        optional: false,
        readonly: true, // Property is readonly
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    }]);

    match interner.lookup(readonly_obj) {
        Some(TypeData::Object(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            assert!(shape.properties[0].readonly);
            // The array type itself is not wrapped in ReadonlyType
            match interner.lookup(shape.properties[0].type_id) {
                Some(TypeData::Array(_)) => {} // Regular array
                _ => panic!("Expected Array type"),
            }
        }
        _ => panic!("Expected Object type"),
    }
}

#[test]
fn test_readonly_mapped_type() {
    // Readonly<T> implemented as mapped type with readonly modifier
    let interner = TypeInterner::new();

    let k_name = interner.intern_string("K");

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: k_name,
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: TypeId::STRING,
        name_type: None,
        template: TypeId::NUMBER,
        readonly_modifier: Some(MappedModifier::Add), // +readonly
        optional_modifier: None,
    };

    let mapped_id = interner.mapped(mapped);

    match interner.lookup(mapped_id) {
        Some(TypeData::Mapped(mapped_id)) => {
            let m = interner.mapped_type(mapped_id);
            assert_eq!(m.readonly_modifier, Some(MappedModifier::Add));
        }
        _ => panic!("Expected Mapped type"),
    }
}

#[test]
fn test_record_with_union_value() {
    // Record<string, string | number>
    let interner = TypeInterner::new();

    let value_union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let record = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: value_union,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    match interner.lookup(record) {
        Some(TypeData::ObjectWithIndex(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            let idx = shape.string_index.as_ref().unwrap();
            // Verify value is a union
            match interner.lookup(idx.value_type) {
                Some(TypeData::Union(_)) => {}
                _ => panic!("Expected Union value type"),
            }
        }
        _ => panic!("Expected ObjectWithIndex"),
    }
}

#[test]
fn test_partial_with_methods() {
    // Partial<{ greet(): void }> - methods also become optional
    let interner = TypeInterner::new();

    let greet_name = interner.intern_string("greet");
    let method_type = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let partial_obj = interner.object(vec![PropertyInfo {
        name: greet_name,
        type_id: method_type,
        write_type: method_type,
        optional: true, // Method made optional
        readonly: false,
        is_method: true,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    }]);

    match interner.lookup(partial_obj) {
        Some(TypeData::Object(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            assert!(shape.properties[0].optional);
            assert!(shape.properties[0].is_method);
        }
        _ => panic!("Expected Object type"),
    }
}
