#[test]
fn test_index_access_tuple_string_literal_length() {
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
    let length_key = interner.literal_string("length");

    let result = evaluate_index_access(&interner, tuple, length_key);
    // Fixed-length tuples return their literal length (e.g., 2 for [string, number]).
    // This matches tsc behavior: `[string, number]["length"]` is `2`, not `number`.
    let expected = interner.literal_number(2.0);
    assert_eq!(result, expected);
}

#[test]
fn test_index_access_optional_tuple_string_literal_length() {
    let interner = TypeInterner::new();

    let tuple = interner.tuple(vec![TupleElement {
        type_id: TypeId::NUMBER,
        name: None,
        optional: true,
        rest: false,
    }]);
    let length_key = interner.literal_string("length");

    let result = evaluate_index_access(&interner, tuple, length_key);
    let zero = interner.literal_number(0.0);
    let one = interner.literal_number(1.0);
    let expected = interner.union(vec![zero, one]);
    assert_eq!(result, expected);
}

#[test]
fn test_index_access_tuple_string_literal_numeric_key() {
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
    let zero = interner.literal_string("0");

    let result = evaluate_index_access(&interner, tuple, zero);
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_index_access_readonly_tuple_literal() {
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
    let one = interner.literal_number(1.0);

    let result = evaluate_index_access(&interner, readonly_tuple, one);
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_index_access_string_number() {
    let interner = TypeInterner::new();

    let result = evaluate_index_access(&interner, TypeId::STRING, TypeId::NUMBER);
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_index_access_string_literal_numeric_key() {
    let interner = TypeInterner::new();

    let zero = interner.literal_string("0");
    let result = evaluate_index_access(&interner, TypeId::STRING, zero);
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_index_access_string_number_with_no_unchecked_indexed_access() {
    let interner = TypeInterner::new();

    let mut evaluator = TypeEvaluator::new(&interner);
    evaluator.set_no_unchecked_indexed_access(true);

    let result = evaluator.evaluate_index_access(TypeId::STRING, TypeId::NUMBER);
    let expected = interner.union(vec![TypeId::STRING, TypeId::UNDEFINED]);
    assert_eq!(result, expected);
}

#[test]
fn test_index_access_string_literal_numeric_key_with_no_unchecked_indexed_access() {
    let interner = TypeInterner::new();

    let mut evaluator = TypeEvaluator::new(&interner);
    evaluator.set_no_unchecked_indexed_access(true);

    let zero = interner.literal_string("0");
    let result = evaluator.evaluate_index_access(TypeId::STRING, zero);
    let expected = interner.union(vec![TypeId::STRING, TypeId::UNDEFINED]);
    assert_eq!(result, expected);
}

#[test]
fn test_index_access_string_literal_member() {
    let interner = TypeInterner::new();

    let length_key = interner.literal_string("length");
    let length_type = evaluate_index_access(&interner, TypeId::STRING, length_key);
    assert_eq!(length_type, TypeId::NUMBER);

    let to_string_key = interner.literal_string("toString");
    let to_string_type = evaluate_index_access(&interner, TypeId::STRING, to_string_key);
    match interner.lookup(to_string_type) {
        Some(TypeData::Function(func_id)) => {
            let func = interner.function_shape(func_id);
            assert_eq!(func.return_type, TypeId::STRING);
            assert_eq!(func.params.len(), 1);
            assert!(func.params[0].rest);
        }
        other => panic!("Expected function type, got {other:?}"),
    }
}

#[test]
fn test_index_access_template_literal_members() {
    let interner = TypeInterner::new();

    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("prefix")),
        TemplateSpan::Type(TypeId::STRING),
        TemplateSpan::Text(interner.intern_string("suffix")),
    ]);

    let length_key = interner.literal_string("length");
    let length_type = evaluate_index_access(&interner, template, length_key);
    assert_eq!(length_type, TypeId::NUMBER);

    let number_index = evaluate_index_access(&interner, template, TypeId::NUMBER);
    assert_eq!(number_index, TypeId::STRING);
}

#[test]
fn test_keyof_readonly_array() {
    let interner = TypeInterner::new();

    let array = interner.array(TypeId::STRING);
    let readonly_array = interner.intern(TypeData::ReadonlyType(array));

    let result = evaluate_keyof(&interner, readonly_array);
    let key = interner
        .lookup(result)
        .expect("expected union for keyof readonly array");

    match key {
        TypeData::Union(members) => {
            let members = interner.type_list(members);
            let length = interner.literal_string("length");
            let map = interner.literal_string("map");
            assert!(members.contains(&TypeId::NUMBER));
            assert!(members.contains(&length));
            assert!(members.contains(&map));
        }
        other => panic!("Expected union, got {other:?}"),
    }
}

#[test]
fn test_keyof_readonly_tuple() {
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

    let result = evaluate_keyof(&interner, readonly_tuple);
    let key = interner
        .lookup(result)
        .expect("expected union for keyof readonly tuple");

    match key {
        TypeData::Union(members) => {
            let members = interner.type_list(members);
            let key_0 = interner.literal_string("0");
            let key_1 = interner.literal_string("1");
            let length = interner.literal_string("length");
            let map = interner.literal_string("map");
            assert!(members.contains(&key_0));
            assert!(members.contains(&key_1));
            assert!(members.contains(&TypeId::NUMBER));
            assert!(members.contains(&length));
            assert!(members.contains(&map));
        }
        other => panic!("Expected union, got {other:?}"),
    }
}

#[test]
fn test_keyof_type_param_constraint() {
    let interner = TypeInterner::new();

    let constraint = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("y"), TypeId::STRING),
    ]);

    let type_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(constraint),
        default: None,
        is_const: false,
    }));

    let result = evaluate_keyof(&interner, type_param);
    let expected = interner.union(vec![
        interner.literal_string("x"),
        interner.literal_string("y"),
    ]);
    assert_eq!(result, expected);
}

#[test]
fn test_base_constraint_assignability_evaluate_keyof() {
    let interner = TypeInterner::new();

    let constraint = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("y"), TypeId::STRING),
    ]);

    let type_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(constraint),
        default: None,
        is_const: false,
    }));

    let key_of = interner.intern(TypeData::KeyOf(type_param));
    let result = evaluate_type(&interner, key_of);
    let expected = interner.union(vec![
        interner.literal_string("x"),
        interner.literal_string("y"),
    ]);
    assert_eq!(result, expected);
}

#[test]
fn test_keyof_type_param_no_constraint_deferred() {
    let interner = TypeInterner::new();

    let type_param = test_type_param(&interner, "T").1;

    let result = evaluate_keyof(&interner, type_param);
    match interner.lookup(result) {
        Some(TypeData::KeyOf(inner)) => assert_eq!(inner, type_param),
        other => panic!("Expected deferred KeyOf, got {other:?}"),
    }
}

#[test]
fn test_keyof_type_param_with_type_param_constraint_not_collapsed() {
    // When B extends A (both type parameters), keyof B must NOT be
    // collapsed to keyof A. B may have more keys than A, so
    // keyof B ⊇ keyof A — they are distinct types.
    let interner = TypeInterner::new();

    let a_param = test_type_param(&interner, "A").1;

    let b_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("B"),
        constraint: Some(a_param), // B extends A
        default: None,
        is_const: false,
    }));

    let keyof_a = evaluate_keyof(&interner, a_param);
    let keyof_b = evaluate_keyof(&interner, b_param);

    // keyof A should be deferred (preserved as KeyOf)
    assert!(
        matches!(interner.lookup(keyof_a), Some(TypeData::KeyOf(_))),
        "keyof A should be deferred, got {:?}",
        interner.lookup(keyof_a)
    );

    // keyof B should also be deferred and DISTINCT from keyof A
    assert!(
        matches!(interner.lookup(keyof_b), Some(TypeData::KeyOf(_))),
        "keyof B should be deferred, got {:?}",
        interner.lookup(keyof_b)
    );

    // They must be different TypeIds (keyof B ≠ keyof A)
    assert_ne!(
        keyof_a, keyof_b,
        "keyof B should NOT be collapsed to keyof A when B extends A"
    );
}

#[test]
fn test_keyof_resolves_ref() {
    use crate::def::DefId;
    use crate::relations::subtype::TypeEnvironment;

    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("y"), TypeId::STRING),
    ]);

    let def_id = DefId(2);
    env.insert_def(def_id, obj);

    let ref_type = interner.lazy(def_id);
    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate_keyof(ref_type);

    let expected = interner.union(vec![
        interner.literal_string("x"),
        interner.literal_string("y"),
    ]);
    assert_eq!(result, expected);
}

#[test]
fn test_index_access_tuple_second() {
    let interner = TypeInterner::new();

    // [string, number][1] -> number
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
    let one = interner.literal_number(1.0);

    let result = evaluate_index_access(&interner, tuple, one);
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_index_access_tuple_number() {
    let interner = TypeInterner::new();

    // [string, number][number] -> string | number
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

    let result = evaluate_index_access(&interner, tuple, TypeId::NUMBER);

    // Should be string | number
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

#[test]
fn test_index_access_tuple_optional_number() {
    let interner = TypeInterner::new();

    // [string, number?][number] -> string | number | undefined
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
            optional: true,
            rest: false,
        },
    ]);

    let result = evaluate_index_access(&interner, tuple, TypeId::NUMBER);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::UNDEFINED]);
    assert_eq!(result, expected);
}

#[test]
fn test_nested_conditional() {
    let interner = TypeInterner::new();

    // string extends string ? (number extends number ? "yes" : "no") : "outer-no"
    // Inner should resolve to "yes", so result is "yes"
    let yes = interner.literal_string("yes");
    let no = interner.literal_string("no");
    let outer_no = interner.literal_string("outer-no");

    let inner_cond = interner.conditional(ConditionalType {
        check_type: TypeId::NUMBER,
        extends_type: TypeId::NUMBER,
        true_type: yes,
        false_type: no,
        is_distributive: false,
    });

    let cond = ConditionalType {
        check_type: TypeId::STRING,
        extends_type: TypeId::STRING,
        true_type: inner_cond,
        false_type: outer_no,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);

    // Inner conditional should also be evaluated -> "yes"
    assert_eq!(result, yes);
}

#[test]
fn test_evaluate_type_non_meta() {
    let interner = TypeInterner::new();

    // Non-meta types should pass through unchanged
    assert_eq!(evaluate_type(&interner, TypeId::STRING), TypeId::STRING);
    assert_eq!(evaluate_type(&interner, TypeId::NUMBER), TypeId::NUMBER);

    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);
    assert_eq!(evaluate_type(&interner, obj), obj);
}

// =============================================================================
// Keyof Tests
// =============================================================================

#[test]
fn test_keyof_object() {
    let interner = TypeInterner::new();

    // keyof { x: number, y: string } = "x" | "y"
    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("y"), TypeId::STRING),
    ]);

    let result = evaluate_keyof(&interner, obj);

    let key_x = interner.literal_string("x");
    let key_y = interner.literal_string("y");
    let expected = interner.union(vec![key_x, key_y]);
    assert_eq!(result, expected);
}

#[test]
fn test_keyof_object_with_string_index_signature() {
    let interner = TypeInterner::new();

    let key_x = interner.intern_string("x");
    let obj = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo::new(key_x, TypeId::NUMBER)],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::BOOLEAN,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let result = evaluate_keyof(&interner, obj);
    let expected = interner.union(vec![
        interner.literal_string("x"),
        TypeId::STRING,
        TypeId::NUMBER,
    ]);
    assert_eq!(result, expected);
}

#[test]
fn test_keyof_object_with_number_index_signature() {
    let interner = TypeInterner::new();

    let key_x = interner.intern_string("x");
    let obj = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo::new(key_x, TypeId::NUMBER)],
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
    });

    let result = evaluate_keyof(&interner, obj);
    let expected = interner.union(vec![interner.literal_string("x"), TypeId::NUMBER]);
    assert_eq!(result, expected);
}

#[test]
fn test_keyof_union_disjoint_objects() {
    let interner = TypeInterner::new();

    let obj_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::NUMBER,
    )]);
    let obj_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::STRING,
    )]);

    let union = interner.union(vec![obj_a, obj_b]);
    let result = evaluate_keyof(&interner, union);
    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_keyof_union_overlap_objects() {
    let interner = TypeInterner::new();

    let obj_a = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("b"), TypeId::STRING),
    ]);
    let obj_b = interner.object(vec![
        PropertyInfo::new(interner.intern_string("b"), TypeId::BOOLEAN),
        PropertyInfo::new(interner.intern_string("c"), TypeId::STRING),
    ]);

    let union = interner.union(vec![obj_a, obj_b]);
    let result = evaluate_keyof(&interner, union);
    let expected = interner.literal_string("b");
    assert_eq!(result, expected);
}

#[test]
fn test_keyof_intersection_unions_keys() {
    let interner = TypeInterner::new();

    let obj_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::NUMBER,
    )]);
    let obj_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::STRING,
    )]);

    let intersection = interner.intersection(vec![obj_a, obj_b]);
    let result = evaluate_keyof(&interner, intersection);
    let expected = interner.union(vec![
        interner.literal_string("a"),
        interner.literal_string("b"),
    ]);
    assert_eq!(result, expected);
}

#[test]
fn test_keyof_union_string_index_overlap_literal() {
    let interner = TypeInterner::new();

    let obj_index = interner.object_with_index(ObjectShape {
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
    let obj_literal = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::BOOLEAN,
    )]);

    let union = interner.union(vec![obj_index, obj_literal]);
    let result = evaluate_keyof(&interner, union);
    let expected = interner.literal_string("a");
    assert_eq!(result, expected);
}

#[test]
fn test_keyof_union_index_signature_intersection() {
    let interner = TypeInterner::new();

    let string_index = interner.object_with_index(ObjectShape {
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
    let number_index = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
    });

    let union = interner.union(vec![string_index, number_index]);
    let result = evaluate_keyof(&interner, union);

    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_keyof_empty_object() {
    let interner = TypeInterner::new();

    // keyof {} = never
    let obj = interner.object(vec![]);

    let result = evaluate_keyof(&interner, obj);
    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_keyof_array() {
    let interner = TypeInterner::new();

    // keyof string[] includes number and array members
    let arr = interner.array(TypeId::STRING);

    let result = evaluate_keyof(&interner, arr);
    let key = interner
        .lookup(result)
        .expect("expected union for keyof array");

    match key {
        TypeData::Union(members) => {
            let members = interner.type_list(members);
            let length = interner.literal_string("length");
            let map = interner.literal_string("map");
            assert!(members.contains(&TypeId::NUMBER));
            assert!(members.contains(&length));
            assert!(members.contains(&map));
        }
        other => panic!("Expected union, got {other:?}"),
    }
}

#[test]
fn test_keyof_tuple() {
    let interner = TypeInterner::new();

    // keyof [string, number] includes tuple indices and array members
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

    let result = evaluate_keyof(&interner, tuple);

    let key = interner
        .lookup(result)
        .expect("expected union for keyof tuple");

    match key {
        TypeData::Union(members) => {
            let members = interner.type_list(members);
            let key_0 = interner.literal_string("0");
            let key_1 = interner.literal_string("1");
            let length = interner.literal_string("length");
            let map = interner.literal_string("map");
            assert!(members.contains(&key_0));
            assert!(members.contains(&key_1));
            assert!(members.contains(&TypeId::NUMBER));
            assert!(members.contains(&length));
            assert!(members.contains(&map));
        }
        other => panic!("Expected union, got {other:?}"),
    }
}

#[test]
fn test_keyof_tuple_with_rest_tuple() {
    let interner = TypeInterner::new();

    // keyof [string, ...[number, boolean]] includes expanded indices
    let rest_tuple = interner.tuple(vec![
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
    let tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: rest_tuple,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    let result = evaluate_keyof(&interner, tuple);
    let key = interner
        .lookup(result)
        .expect("expected union for keyof tuple with rest");

    match key {
        TypeData::Union(members) => {
            let members = interner.type_list(members);
            let key_0 = interner.literal_string("0");
            let key_1 = interner.literal_string("1");
            let key_2 = interner.literal_string("2");
            let length = interner.literal_string("length");
            assert!(members.contains(&key_0));
            assert!(members.contains(&key_1));
            assert!(members.contains(&key_2));
            assert!(members.contains(&TypeId::NUMBER));
            assert!(members.contains(&length));
        }
        other => panic!("Expected union, got {other:?}"),
    }
}

#[test]
fn test_keyof_any() {
    let interner = TypeInterner::new();

    // keyof any = string | number | symbol
    let result = evaluate_keyof(&interner, TypeId::ANY);

    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::SYMBOL]);
    assert_eq!(result, expected);
}

#[test]
fn test_keyof_unknown() {
    let interner = TypeInterner::new();

    // keyof unknown = never
    let result = evaluate_keyof(&interner, TypeId::UNKNOWN);
    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_keyof_object_keyword() {
    let interner = TypeInterner::new();

    // keyof object = never
    let result = evaluate_keyof(&interner, TypeId::OBJECT);
    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_object_trifecta_keyof_object_interface() {
    use crate::relations::subtype::TypeEnvironment;

    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let object_interface = interner.object(vec![
        PropertyInfo::new(interner.intern_string("toString"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("valueOf"), TypeId::NUMBER),
    ]);

    let def_id = DefId(1);
    env.insert_def(def_id, object_interface);

    let ref_type = interner.lazy(def_id);
    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate_keyof(ref_type);
    let key = interner
        .lookup(result)
        .expect("expected union for keyof Object interface");

    match key {
        TypeData::Union(members) => {
            let members = interner.type_list(members);
            let to_string = interner.literal_string("toString");
            let value_of = interner.literal_string("valueOf");
            assert!(members.contains(&to_string));
            assert!(members.contains(&value_of));
        }
        other => panic!("Expected union, got {other:?}"),
    }
}

#[test]
fn test_keyof_never() {
    let interner = TypeInterner::new();

    // keyof never = string | number | symbol
    let result = evaluate_keyof(&interner, TypeId::NEVER);
    assert_eq!(
        result,
        interner.union3(TypeId::STRING, TypeId::NUMBER, TypeId::SYMBOL)
    );
}

#[test]
fn test_keyof_nullish() {
    let interner = TypeInterner::new();

    // keyof null/undefined/void = never
    assert_eq!(evaluate_keyof(&interner, TypeId::NULL), TypeId::NEVER);
    assert_eq!(evaluate_keyof(&interner, TypeId::UNDEFINED), TypeId::NEVER);
    assert_eq!(evaluate_keyof(&interner, TypeId::VOID), TypeId::NEVER);
}

#[test]
fn test_keyof_string_apparent_members() {
    let interner = TypeInterner::new();

    let result = evaluate_keyof(&interner, TypeId::STRING);
    let key = interner
        .lookup(result)
        .expect("expected union for keyof string");

    match key {
        TypeData::Union(members) => {
            let members = interner.type_list(members);
            let length = interner.literal_string("length");
            let to_string = interner.literal_string("toString");
            assert!(members.contains(&length));
            assert!(members.contains(&to_string));
            assert!(members.contains(&TypeId::NUMBER));
        }
        other => panic!("Expected union, got {other:?}"),
    }
}

#[test]
fn test_apparent_number_keyof_members() {
    let interner = TypeInterner::new();

    let result = evaluate_keyof(&interner, TypeId::NUMBER);
    let key = interner
        .lookup(result)
        .expect("expected union for keyof number");

    match key {
        TypeData::Union(members) => {
            let members = interner.type_list(members);
            let to_fixed = interner.literal_string("toFixed");
            let value_of = interner.literal_string("valueOf");
            assert!(members.contains(&to_fixed));
            assert!(members.contains(&value_of));
        }
        other => panic!("Expected union, got {other:?}"),
    }
}

#[test]
fn test_keyof_template_literal_matches_string() {
    let interner = TypeInterner::new();

    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("prefix")),
        TemplateSpan::Type(TypeId::STRING),
        TemplateSpan::Text(interner.intern_string("suffix")),
    ]);

    let result = evaluate_keyof(&interner, template);
    let expected = evaluate_keyof(&interner, TypeId::STRING);
    assert_eq!(result, expected);
}

// =============================================================================
// KEYOF OPERATOR TESTS
// =============================================================================

/// Test basic keyof on simple object type.
///
/// keyof { a: string, b: number, c: boolean } = "a" | "b" | "c"
#[test]
fn test_keyof_basic_object_type() {
    let interner = TypeInterner::new();

    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("c"), TypeId::BOOLEAN),
    ]);

    let result = evaluate_keyof(&interner, obj);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let key_c = interner.literal_string("c");
    let expected = interner.union(vec![key_a, key_b, key_c]);
    assert_eq!(result, expected);
}

/// Test keyof on single property object.
///
/// keyof { only: string } = "only"
#[test]
fn test_keyof_single_property() {
    let interner = TypeInterner::new();

    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("only"),
        TypeId::STRING,
    )]);

    let result = evaluate_keyof(&interner, obj);

    // Single property should produce the literal key
    let expected = interner.literal_string("only");
    assert_eq!(result, expected);
}

/// Test keyof on intersection produces union of all keys.
///
/// keyof ({ a: string } & { b: number }) = "a" | "b"
#[test]
fn test_keyof_intersection_produces_union() {
    let interner = TypeInterner::new();

    let obj1 = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let obj2 = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);

    let intersection = interner.intersection(vec![obj1, obj2]);
    let result = evaluate_keyof(&interner, intersection);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let expected = interner.union(vec![key_a, key_b]);
    assert_eq!(result, expected);
}

/// Test keyof on intersection with overlapping keys.
///
/// keyof ({ a: string, b: number } & { b: boolean, c: string }) = "a" | "b" | "c"
#[test]
fn test_keyof_intersection_overlapping_keys() {
    let interner = TypeInterner::new();

    let obj1 = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
    ]);

    let obj2 = interner.object(vec![
        PropertyInfo::new(interner.intern_string("b"), TypeId::BOOLEAN),
        PropertyInfo::new(interner.intern_string("c"), TypeId::STRING),
    ]);

    let intersection = interner.intersection(vec![obj1, obj2]);
    let result = evaluate_keyof(&interner, intersection);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let key_c = interner.literal_string("c");
    let expected = interner.union(vec![key_a, key_b, key_c]);
    assert_eq!(result, expected);
}

/// Test keyof on union produces intersection of keys.
///
/// keyof ({ a: string, b: number } | { b: boolean, c: string }) = "b"
/// (only common keys)
#[test]
fn test_keyof_union_common_keys_only() {
    let interner = TypeInterner::new();

    let obj1 = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
    ]);

    let obj2 = interner.object(vec![
        PropertyInfo::new(interner.intern_string("b"), TypeId::BOOLEAN),
        PropertyInfo::new(interner.intern_string("c"), TypeId::STRING),
    ]);

    let union = interner.union(vec![obj1, obj2]);
    let result = evaluate_keyof(&interner, union);

    // Only "b" is common to both
    let expected = interner.literal_string("b");
    assert_eq!(result, expected);
}

/// Test keyof on union with no common keys produces never.
///
/// keyof ({ a: string } | { b: number }) = never
#[test]
fn test_keyof_union_no_common_keys() {
    let interner = TypeInterner::new();

    let obj1 = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let obj2 = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);

    let union = interner.union(vec![obj1, obj2]);
    let result = evaluate_keyof(&interner, union);

    // No common keys = never
    assert_eq!(result, TypeId::NEVER);
}

/// Test keyof with mapped type constraint.
///
/// keyof { [K in "x" | "y"]: number } = "x" | "y"
#[test]
fn test_keyof_mapped_type_basic() {
    let interner = TypeInterner::new();

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

    // First evaluate the mapped type to get the resulting object
    let mapped_result = evaluate_mapped(&interner, &mapped);

    // Then get keyof the result
    let result = evaluate_keyof(&interner, mapped_result);

    let expected = interner.union(vec![key_x, key_y]);
    assert_eq!(result, expected);
}

/// Test keyof with mapped type with remapped keys.
///
/// keyof { [K in "a" | "b" as `${K}_key`]: string } = "`a_key`" | "`b_key`"
#[test]
fn test_keyof_mapped_type_remapped_keys() {
    let interner = TypeInterner::new();

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let keys = interner.union(vec![key_a, key_b]);

    let key_a_key = interner.literal_string("a_key");
    let key_b_key = interner.literal_string("b_key");

    let key_param = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(keys),
        default: None,
        is_const: false,
    };
    let key_param_id = interner.intern(TypeData::TypeParameter(key_param));

    // Create conditional: K extends "a" ? "a_key" : K extends "b" ? "b_key" : never
    let inner_cond = interner.conditional(ConditionalType {
        check_type: key_param_id,
        extends_type: key_b,
        true_type: key_b_key,
        false_type: TypeId::NEVER,
        is_distributive: false,
    });
    let name_type = interner.conditional(ConditionalType {
        check_type: key_param_id,
        extends_type: key_a,
        true_type: key_a_key,
        false_type: inner_cond,
        is_distributive: false,
    });

    let mapped = MappedType {
        type_param: key_param,
        constraint: keys,
        name_type: Some(name_type),
        template: TypeId::STRING,
        readonly_modifier: None,
        optional_modifier: None,
    };

    // First evaluate the mapped type to get the resulting object
    let mapped_result = evaluate_mapped(&interner, &mapped);

    // Then get keyof the result
    let result = evaluate_keyof(&interner, mapped_result);

    let expected = interner.union(vec![key_a_key, key_b_key]);
    assert_eq!(result, expected);
}

/// Test keyof with readonly and optional properties.
///
/// keyof { readonly a: string, b?: number } = "a" | "b"
#[test]
fn test_keyof_readonly_and_optional_properties() {
    let interner = TypeInterner::new();

    let obj = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("a"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: true, // readonly
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
            name: interner.intern_string("b"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: true, // optional
            readonly: false,
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

    let result = evaluate_keyof(&interner, obj);

    // readonly and optional don't affect keyof
    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let expected = interner.union(vec![key_a, key_b]);
    assert_eq!(result, expected);
}

/// Test keyof on triple intersection.
///
/// keyof ({ a: string } & { b: number } & { c: boolean }) = "a" | "b" | "c"
#[test]
fn test_keyof_triple_intersection() {
    let interner = TypeInterner::new();

    let obj1 = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let obj2 = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);

    let obj3 = interner.object(vec![PropertyInfo::new(
        interner.intern_string("c"),
        TypeId::BOOLEAN,
    )]);

    let intersection = interner.intersection(vec![obj1, obj2, obj3]);
    let result = evaluate_keyof(&interner, intersection);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let key_c = interner.literal_string("c");
    let expected = interner.union(vec![key_a, key_b, key_c]);
    assert_eq!(result, expected);
}

/// Test keyof on union with identical keys.
///
/// keyof ({ a: string, b: number } | { a: boolean, b: string }) = "a" | "b"
#[test]
fn test_keyof_union_identical_keys() {
    let interner = TypeInterner::new();

    let obj1 = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
    ]);

    let obj2 = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::BOOLEAN),
        PropertyInfo::new(interner.intern_string("b"), TypeId::STRING),
    ]);

    let union = interner.union(vec![obj1, obj2]);
    let result = evaluate_keyof(&interner, union);

    // Both objects have "a" and "b"
    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let expected = interner.union(vec![key_a, key_b]);
    assert_eq!(result, expected);
}

// =============================================================================
// KEYOF EDGE CASE TESTS
// =============================================================================

#[test]
fn test_keyof_nested_object_only_top_level() {
    // keyof { a: { b: number } } = "a" (not "a" | "b")
    let interner = TypeInterner::new();

    let inner_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);

    let outer_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        inner_obj,
    )]);

    let result = evaluate_keyof(&interner, outer_obj);
    let expected = interner.literal_string("a");
    assert_eq!(result, expected);
}

#[test]
fn test_keyof_both_index_signatures() {
    // keyof { [k: string]: any, [n: number]: any } = string | number
    let interner = TypeInterner::new();

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
            value_type: TypeId::ANY,
            readonly: false,
            param_name: None,
        }),
    });

    let result = evaluate_keyof(&interner, obj);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

#[test]
fn test_keyof_numeric_literal_keys() {
    // keyof { 0: string, 1: number } = 0 | 1 (numeric literals, matching tsc).
    // `PropertyInfo::new` defaults to `is_string_named: false`, which models
    // properties declared with a bare numeric name (`{ 0: ... }` rather than
    // `{ "0": ... }`).
    let interner = TypeInterner::new();

    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("0"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("1"), TypeId::NUMBER),
    ]);

    let result = evaluate_keyof(&interner, obj);
    let key_0 = interner.literal_number(0.0);
    let key_1 = interner.literal_number(1.0);
    let expected = interner.union(vec![key_0, key_1]);
    assert_eq!(result, expected);
}

#[test]
fn test_keyof_mixed_optional_required() {
    // keyof { a: string, b?: number, c: boolean } = "a" | "b" | "c"
    let interner = TypeInterner::new();

    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::opt(interner.intern_string("b"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("c"), TypeId::BOOLEAN),
    ]);

    let result = evaluate_keyof(&interner, obj);
    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let key_c = interner.literal_string("c");
    let expected = interner.union(vec![key_a, key_b, key_c]);
    assert_eq!(result, expected);
}

#[test]
fn test_keyof_function_type() {
    // keyof (() => void) - function has standard Function members
    let interner = TypeInterner::new();

    let func = interner.function(FunctionShape {
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: vec![],
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let result = evaluate_keyof(&interner, func);
    // Functions get apparent Function members (like "call", "apply", "bind", "length", etc.)
    // Result should be never or string | number | symbol (depending on implementation)
    // Just verify it doesn't panic and returns some type
    assert_ne!(result, func);
}

#[test]
fn test_keyof_deeply_nested_union() {
    // keyof (A | (B | C)) = keyof (A | B | C) = common keys
    let interner = TypeInterner::new();

    let obj_a = interner.object(vec![
        PropertyInfo::new(interner.intern_string("shared"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("a"), TypeId::NUMBER),
    ]);

    let obj_b = interner.object(vec![
        PropertyInfo::new(interner.intern_string("shared"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("b"), TypeId::BOOLEAN),
    ]);

    let obj_c = interner.object(vec![
        PropertyInfo::new(interner.intern_string("shared"), TypeId::BOOLEAN),
        PropertyInfo::new(interner.intern_string("c"), TypeId::STRING),
    ]);

    let inner_union = interner.union(vec![obj_b, obj_c]);
    let outer_union = interner.union(vec![obj_a, inner_union]);
    let result = evaluate_keyof(&interner, outer_union);

    // Only "shared" is common to all three
    let expected = interner.literal_string("shared");
    assert_eq!(result, expected);
}

#[test]
fn test_keyof_deeply_nested_intersection() {
    // keyof (A & (B & C)) = keyof A | keyof B | keyof C
    let interner = TypeInterner::new();

    let obj_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let obj_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);

    let obj_c = interner.object(vec![PropertyInfo::new(
        interner.intern_string("c"),
        TypeId::BOOLEAN,
    )]);

    let inner_intersection = interner.intersection(vec![obj_b, obj_c]);
    let outer_intersection = interner.intersection(vec![obj_a, inner_intersection]);
    let result = evaluate_keyof(&interner, outer_intersection);

    // All keys are included: "a" | "b" | "c"
    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let key_c = interner.literal_string("c");
    let expected = interner.union(vec![key_a, key_b, key_c]);
    assert_eq!(result, expected);
}

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

// =============================================================================
// Mapped Type Tests
// =============================================================================

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
