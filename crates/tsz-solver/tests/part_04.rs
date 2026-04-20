use super::*;
#[test]
fn test_conditional_infer_object_string_index_signature() {
    let interner = TypeInterner::new();

    let r_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: r_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let source = interner.object_with_index(ObjectShape {
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
    let pattern = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: infer_r,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    // { [key: string]: number } extends { [key: string]: infer R } ? R : never -> number
    let cond = ConditionalType {
        check_type: source,
        extends_type: pattern,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_index_access_object_literal() {
    let interner = TypeInterner::new();

    // { x: number, y: string }["x"] -> number
    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("y"), TypeId::STRING),
    ]);
    let key_x = interner.literal_string("x");

    let result = evaluate_index_access(&interner, obj, key_x);
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_index_access_object_string_key() {
    let interner = TypeInterner::new();

    // { x: number, y: string }["y"] -> string
    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("y"), TypeId::STRING),
    ]);
    let key_y = interner.literal_string("y");

    let result = evaluate_index_access(&interner, obj, key_y);
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_index_access_object_string_index_optional_properties() {
    let interner = TypeInterner::new();

    let obj = interner.object(vec![
        PropertyInfo::opt(interner.intern_string("x"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("y"), TypeId::STRING),
    ]);

    let result = evaluate_index_access(&interner, obj, TypeId::STRING);
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::STRING, TypeId::UNDEFINED]);
    assert_eq!(result, expected);
}

#[test]
fn test_index_access_object_missing_key() {
    let interner = TypeInterner::new();

    // { x: number }["z"] -> undefined
    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);
    let key_z = interner.literal_string("z");

    let result = evaluate_index_access(&interner, obj, key_z);
    assert_eq!(result, TypeId::UNDEFINED);
}

#[test]
fn test_index_access_object_union_key() {
    let interner = TypeInterner::new();

    // { x: number, y: string }["x" | "y"] -> number | string
    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("y"), TypeId::STRING),
    ]);
    let key_x = interner.literal_string("x");
    let key_y = interner.literal_string("y");
    let key_union = interner.union(vec![key_x, key_y]);

    let result = evaluate_index_access(&interner, obj, key_union);

    // Should be number | string
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::STRING]);
    assert_eq!(result, expected);
}

#[test]
fn test_index_access_union_object_literal_key() {
    let interner = TypeInterner::new();

    let obj_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);
    let obj_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("y"),
        TypeId::STRING,
    )]);
    let union_obj = interner.union(vec![obj_a, obj_b]);
    let key_x = interner.literal_string("x");

    let result = evaluate_index_access(&interner, union_obj, key_x);
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_index_access_union_object_union_key() {
    let interner = TypeInterner::new();

    let obj_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);
    let obj_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("y"),
        TypeId::STRING,
    )]);
    let union_obj = interner.union(vec![obj_a, obj_b]);
    let key_x = interner.literal_string("x");
    let key_y = interner.literal_string("y");
    let key_union = interner.union(vec![key_x, key_y]);

    let result = evaluate_index_access(&interner, union_obj, key_union);
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::STRING]);
    assert_eq!(result, expected);
}

#[test]
fn test_correlated_union_index_access_cross_product() {
    let interner = TypeInterner::new();

    let kind = interner.intern_string("kind");
    let key_a = interner.intern_string("a");
    let key_b = interner.intern_string("b");

    let obj_a = interner.object(vec![
        PropertyInfo::new(kind, interner.literal_string("a")),
        PropertyInfo::new(key_a, TypeId::NUMBER),
    ]);
    let obj_b = interner.object(vec![
        PropertyInfo::new(kind, interner.literal_string("b")),
        PropertyInfo::new(key_b, TypeId::STRING),
    ]);

    let union_obj = interner.union(vec![obj_a, obj_b]);
    let key_union = interner.union(vec![
        interner.literal_string("a"),
        interner.literal_string("b"),
    ]);

    let result = evaluate_index_access(&interner, union_obj, key_union);
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::STRING]);
    assert_eq!(result, expected);
}

#[test]
fn test_index_access_union_object_union_key_no_unchecked() {
    let interner = TypeInterner::new();

    let obj_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);
    let obj_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("y"),
        TypeId::STRING,
    )]);
    let union_obj = interner.union(vec![obj_a, obj_b]);
    let key_x = interner.literal_string("x");
    let key_y = interner.literal_string("y");
    let key_union = interner.union(vec![key_x, key_y]);

    let mut evaluator = TypeEvaluator::new(&interner);
    evaluator.set_no_unchecked_indexed_access(true);
    let result = evaluator.evaluate_index_access(union_obj, key_union);
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::STRING, TypeId::UNDEFINED]);
    assert_eq!(result, expected);
}

#[test]
fn test_index_access_union_object_literal_key_no_unchecked() {
    let interner = TypeInterner::new();

    let obj_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);
    let obj_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("y"),
        TypeId::STRING,
    )]);
    let union_obj = interner.union(vec![obj_a, obj_b]);
    let key_x = interner.literal_string("x");

    let mut evaluator = TypeEvaluator::new(&interner);
    evaluator.set_no_unchecked_indexed_access(true);

    let result = evaluator.evaluate_index_access(union_obj, key_x);
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
    assert_eq!(result, expected);
}

#[test]
fn test_index_access_object_with_string_index_signature() {
    let interner = TypeInterner::new();

    let key_x = interner.intern_string("x");
    let key_y = interner.literal_string("y");

    let obj = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo::new(key_x, TypeId::STRING)],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let key_x_literal = interner.literal_string("x");
    let result = evaluate_index_access(&interner, obj, key_x_literal);
    assert_eq!(result, TypeId::STRING);

    let result = evaluate_index_access(&interner, obj, key_y);
    assert_eq!(result, TypeId::NUMBER);

    let result = evaluate_index_access(&interner, obj, TypeId::STRING);
    assert_eq!(result, TypeId::NUMBER);

    let key_union = interner.union(vec![key_x_literal, key_y]);
    let result = evaluate_index_access(&interner, obj, key_union);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

#[test]
fn test_index_access_object_with_string_index_signature_optional_property() {
    let interner = TypeInterner::new();

    let key_x = interner.intern_string("x");
    let key_y = interner.literal_string("y");

    let obj = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo::opt(key_x, TypeId::NUMBER)],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::BOOLEAN,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let key_x_literal = interner.literal_string("x");
    let result = evaluate_index_access(&interner, obj, key_x_literal);
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
    assert_eq!(result, expected);

    let result = evaluate_index_access(&interner, obj, key_y);
    assert_eq!(result, TypeId::BOOLEAN);

    let key_union = interner.union(vec![key_x_literal, key_y]);
    let result = evaluate_index_access(&interner, obj, key_union);
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED, TypeId::BOOLEAN]);
    assert_eq!(result, expected);
}

#[test]
fn test_index_access_object_with_string_index_signature_optional_property_no_unchecked() {
    let interner = TypeInterner::new();

    let obj = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo::opt(
            interner.intern_string("x"),
            TypeId::NUMBER,
        )],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::BOOLEAN,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let mut evaluator = TypeEvaluator::new(&interner);
    evaluator.set_no_unchecked_indexed_access(true);

    let key_x = interner.literal_string("x");
    let result = evaluator.evaluate_index_access(obj, key_x);
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
    assert_eq!(result, expected);

    let key_y = interner.literal_string("y");
    let result = evaluator.evaluate_index_access(obj, key_y);
    let expected = interner.union(vec![TypeId::BOOLEAN, TypeId::UNDEFINED]);
    assert_eq!(result, expected);

    let result = evaluator.evaluate_index_access(obj, TypeId::STRING);
    let expected = interner.union(vec![TypeId::BOOLEAN, TypeId::UNDEFINED]);
    assert_eq!(result, expected);

    let key_union = interner.union(vec![key_x, key_y]);
    let result = evaluator.evaluate_index_access(obj, key_union);
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::BOOLEAN, TypeId::UNDEFINED]);
    assert_eq!(result, expected);
}

#[test]
fn test_no_unchecked_object_index_signature_evaluate() {
    let interner = TypeInterner::new();

    let obj = interner.object_with_index(ObjectShape {
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

    let mut evaluator = TypeEvaluator::new(&interner);
    evaluator.set_no_unchecked_indexed_access(true);

    let result = evaluator.evaluate_index_access(obj, TypeId::NUMBER);
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
    assert_eq!(result, expected);
}

#[test]
fn test_index_access_object_with_number_index_signature() {
    let interner = TypeInterner::new();

    let obj = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::BOOLEAN,
            readonly: false,
            param_name: None,
        }),
    });

    let result = evaluate_index_access(&interner, obj, TypeId::NUMBER);
    assert_eq!(result, TypeId::BOOLEAN);

    let one = interner.literal_number(1.0);
    let result = evaluate_index_access(&interner, obj, one);
    assert_eq!(result, TypeId::BOOLEAN);
}

#[test]
fn test_index_access_object_with_number_index_signature_no_unchecked() {
    let interner = TypeInterner::new();

    let obj = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::BOOLEAN,
            readonly: false,
            param_name: None,
        }),
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
    });

    let mut evaluator = TypeEvaluator::new(&interner);
    evaluator.set_no_unchecked_indexed_access(true);

    let result = evaluator.evaluate_index_access(obj, TypeId::NUMBER);
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
    assert_eq!(result, expected);

    let zero = interner.literal_number(0.0);
    let result = evaluator.evaluate_index_access(obj, zero);
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
    assert_eq!(result, expected);

    let zero_str = interner.literal_string("0");
    let result = evaluator.evaluate_index_access(obj, zero_str);
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
    assert_eq!(result, expected);
}

#[test]
fn test_index_access_resolves_ref() {
    use crate::TypeEnvironment;
    use crate::def::DefId;

    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    let def_id = DefId(1);
    env.insert_def(def_id, obj);

    let ref_type = interner.lazy(def_id);
    let key_x = interner.literal_string("x");

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate_index_access(ref_type, key_x);
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_index_access_type_param_constraint() {
    let interner = TypeInterner::new();

    let constraint = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    let type_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(constraint),
        default: None,
        is_const: false,
    }));

    let key_x = interner.literal_string("x");
    let result = evaluate_index_access(&interner, type_param, key_x);
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_index_access_type_param_no_constraint_deferred() {
    let interner = TypeInterner::new();

    let type_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    let key_x = interner.literal_string("x");
    let result = evaluate_index_access(&interner, type_param, key_x);

    match interner.lookup(result) {
        Some(TypeData::IndexAccess(obj, idx)) => {
            assert_eq!(obj, type_param);
            assert_eq!(idx, key_x);
        }
        other => panic!("Expected deferred IndexAccess, got {other:?}"),
    }
}

#[test]
fn test_index_access_optional_property() {
    let interner = TypeInterner::new();

    let obj = interner.object(vec![PropertyInfo::opt(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    let key_x = interner.literal_string("x");
    let result = evaluate_index_access(&interner, obj, key_x);
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
    assert_eq!(result, expected);
}

#[test]
fn test_index_access_any_is_any() {
    let interner = TypeInterner::new();

    let result = evaluate_index_access(&interner, TypeId::ANY, TypeId::STRING);
    assert_eq!(result, TypeId::ANY);

    let result = evaluate_index_access(&interner, TypeId::NUMBER, TypeId::ANY);
    assert_eq!(result, TypeId::ANY);
}

#[test]
fn test_index_access_with_no_unchecked_indexed_access() {
    let interner = TypeInterner::new();

    let indexed = interner.object_with_index(ObjectShape {
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

    let array = interner.array(TypeId::STRING);

    let mut evaluator = TypeEvaluator::new(&interner);
    evaluator.set_no_unchecked_indexed_access(true);

    let result = evaluator.evaluate_index_access(indexed, TypeId::STRING);
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
    assert_eq!(result, expected);

    let result = evaluator.evaluate_index_access(array, TypeId::NUMBER);
    let expected = interner.union(vec![TypeId::STRING, TypeId::UNDEFINED]);
    assert_eq!(result, expected);
}

#[test]
fn test_index_access_with_options_helper_no_unchecked_indexed_access() {
    let interner = TypeInterner::new();

    let indexed = interner.object_with_index(ObjectShape {
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

    let result = evaluate_index_access_with_options(&interner, indexed, TypeId::STRING, true);
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
    assert_eq!(result, expected);
}

#[test]
fn test_index_access_array_literal_with_no_unchecked_indexed_access() {
    let interner = TypeInterner::new();

    let array = interner.array(TypeId::STRING);
    let zero = interner.literal_number(0.0);

    let mut evaluator = TypeEvaluator::new(&interner);
    evaluator.set_no_unchecked_indexed_access(true);

    let result = evaluator.evaluate_index_access(array, zero);
    let expected = interner.union(vec![TypeId::STRING, TypeId::UNDEFINED]);
    assert_eq!(result, expected);
}

#[test]
fn test_index_access_array() {
    let interner = TypeInterner::new();

    // string[][number] -> string
    let string_array = interner.array(TypeId::STRING);

    let result = evaluate_index_access(&interner, string_array, TypeId::NUMBER);
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_no_unchecked_indexed_access_array_union_key() {
    let interner = TypeInterner::new();

    let string_array = interner.array(TypeId::STRING);
    let length_key = interner.literal_string("length");
    let key_union = interner.union(vec![TypeId::NUMBER, length_key]);

    let mut evaluator = TypeEvaluator::new(&interner);
    let result = evaluator.evaluate_index_access(string_array, key_union);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);

    evaluator.set_no_unchecked_indexed_access(true);
    let result = evaluator.evaluate_index_access(string_array, key_union);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::UNDEFINED]);
    assert_eq!(result, expected);
}

#[test]
fn test_index_access_array_string_index() {
    let interner = TypeInterner::new();

    let string_array = interner.array(TypeId::STRING);

    // tsc: Array<T>[string] returns T (the element type) because the
    // numeric index signature (returning T) is available under string
    // indexing.  String keys subsume numeric keys.
    let result = evaluate_index_access(&interner, string_array, TypeId::STRING);
    assert_eq!(
        result,
        TypeId::STRING,
        "string[][string] should be string (element type)"
    );
}

#[test]
fn test_index_access_array_string_index_with_no_unchecked_indexed_access() {
    let interner = TypeInterner::new();

    let string_array = interner.array(TypeId::STRING);
    let mut evaluator = TypeEvaluator::new(&interner);
    evaluator.set_no_unchecked_indexed_access(true);

    // tsc: Array<T>[string] returns T | undefined with noUncheckedIndexedAccess.
    let result = evaluator.evaluate_index_access(string_array, TypeId::STRING);
    let key = interner
        .lookup(result)
        .expect("expected union for array[string] with noUncheckedIndexedAccess");

    match key {
        TypeData::Union(members) => {
            let members = interner.type_list(members);
            assert!(
                members.contains(&TypeId::STRING),
                "should contain STRING (element type)"
            );
            assert!(
                members.contains(&TypeId::UNDEFINED),
                "should contain UNDEFINED (noUncheckedIndexedAccess)"
            );
        }
        other => panic!("Expected union (string | undefined), got {other:?}"),
    }
}

#[test]
fn test_index_access_array_string_literal_length() {
    let interner = TypeInterner::new();

    let string_array = interner.array(TypeId::STRING);
    let length_key = interner.literal_string("length");

    let result = evaluate_index_access(&interner, string_array, length_key);
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_index_access_array_string_literal_method() {
    let interner = TypeInterner::new();

    let string_array = interner.array(TypeId::STRING);
    let includes_key = interner.literal_string("includes");

    let result = evaluate_index_access(&interner, string_array, includes_key);
    match interner.lookup(result) {
        Some(TypeData::Function(func_id)) => {
            let func = interner.function_shape(func_id);
            assert_eq!(func.return_type, TypeId::BOOLEAN);
            assert_eq!(func.params.len(), 1);
            assert!(func.params[0].rest);
        }
        other => panic!("Expected function type, got {other:?}"),
    }
}

#[test]
fn test_index_access_array_string_literal_numeric_key_with_no_unchecked_indexed_access() {
    let interner = TypeInterner::new();

    let string_array = interner.array(TypeId::STRING);
    let zero = interner.literal_string("0");

    let mut evaluator = TypeEvaluator::new(&interner);
    evaluator.set_no_unchecked_indexed_access(true);

    let result = evaluator.evaluate_index_access(string_array, zero);
    let expected = interner.union(vec![TypeId::STRING, TypeId::UNDEFINED]);
    assert_eq!(result, expected);
}

#[test]
fn test_index_access_readonly_array() {
    let interner = TypeInterner::new();

    let array = interner.array(TypeId::STRING);
    let readonly_array = interner.intern(TypeData::ReadonlyType(array));

    let result = evaluate_index_access(&interner, readonly_array, TypeId::NUMBER);
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_index_access_tuple_literal() {
    let interner = TypeInterner::new();

    // [string, number][0] -> string
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
    let zero = interner.literal_number(0.0);

    let result = evaluate_index_access(&interner, tuple, zero);
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_index_access_tuple_rest_array_literal() {
    let interner = TypeInterner::new();

    // [string, ...number[]][1] -> number
    let number_array = interner.array(TypeId::NUMBER);
    let tuple = interner.tuple(vec![
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
    let one = interner.literal_number(1.0);
    let two = interner.literal_number(2.0);

    assert_eq!(evaluate_index_access(&interner, tuple, one), TypeId::NUMBER);
    assert_eq!(evaluate_index_access(&interner, tuple, two), TypeId::NUMBER);
}

#[test]
fn test_index_access_tuple_rest_tuple_literal() {
    let interner = TypeInterner::new();

    // [string, ...[number, boolean]][1] -> number
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

    let one = interner.literal_number(1.0);
    let two = interner.literal_number(2.0);
    let three = interner.literal_number(3.0);

    assert_eq!(evaluate_index_access(&interner, tuple, one), TypeId::NUMBER);
    assert_eq!(
        evaluate_index_access(&interner, tuple, two),
        TypeId::BOOLEAN
    );
    assert_eq!(
        evaluate_index_access(&interner, tuple, three),
        TypeId::UNDEFINED
    );
}

#[test]
fn test_index_access_tuple_optional_literal() {
    let interner = TypeInterner::new();

    // [string, number?][1] -> number | undefined
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
    let one = interner.literal_number(1.0);

    let result = evaluate_index_access(&interner, tuple, one);
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
    assert_eq!(result, expected);
}

#[test]
fn test_index_access_tuple_negative_literal() {
    let interner = TypeInterner::new();

    let number_array = interner.array(TypeId::NUMBER);
    let tuple = interner.tuple(vec![
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
    let negative = interner.literal_number(-1.0);

    let result = evaluate_index_access(&interner, tuple, negative);
    assert_eq!(result, TypeId::UNDEFINED);
}

#[test]
fn test_index_access_tuple_fractional_literal() {
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
    let fractional = interner.literal_number(1.5);

    let result = evaluate_index_access(&interner, tuple, fractional);
    assert_eq!(result, TypeId::UNDEFINED);
}

#[test]
fn test_index_access_tuple_negative_string_literal() {
    let interner = TypeInterner::new();

    let number_array = interner.array(TypeId::NUMBER);
    let tuple = interner.tuple(vec![
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
    let negative = interner.literal_string("-1");

    let result = evaluate_index_access(&interner, tuple, negative);
    assert_eq!(result, TypeId::UNDEFINED);
}

