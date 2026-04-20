use super::*;
#[test]
fn test_index_access_tuple_fractional_string_literal() {
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
    let fractional = interner.literal_string("1.5");

    let result = evaluate_index_access(&interner, tuple, fractional);
    assert_eq!(result, TypeId::UNDEFINED);
}

#[test]
fn test_index_access_tuple_string_index() {
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
    let map_key = interner.literal_string("map");
    let map_type = evaluate_index_access(&interner, tuple, map_key);

    let result = evaluate_index_access(&interner, tuple, TypeId::STRING);
    let key = interner
        .lookup(result)
        .expect("expected union for tuple[string]");

    match key {
        TypeData::Union(members) => {
            let members = interner.type_list(members);
            assert!(members.contains(&TypeId::STRING));
            assert!(members.contains(&TypeId::NUMBER));
            assert!(members.contains(&map_type));
        }
        other => panic!("Expected union, got {other:?}"),
    }
}

#[test]
fn test_index_access_tuple_string_index_with_no_unchecked_indexed_access() {
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
    let mut evaluator = TypeEvaluator::new(&interner);
    evaluator.set_no_unchecked_indexed_access(true);

    let result = evaluator.evaluate_index_access(tuple, TypeId::STRING);
    let key = interner
        .lookup(result)
        .expect("expected union for tuple[string]");

    match key {
        TypeData::Union(members) => {
            let members = interner.type_list(members);
            assert!(members.contains(&TypeId::UNDEFINED));
        }
        other => panic!("Expected union, got {other:?}"),
    }
}

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

    let type_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }));

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

    let a_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("A"),
        constraint: None,
        default: None,
        is_const: false,
    }));

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
    use crate::TypeEnvironment;
    use crate::def::DefId;

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
    use crate::TypeEnvironment;

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

