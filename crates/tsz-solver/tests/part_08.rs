use super::*;
use crate::TypeInterner;
use crate::def::DefId;
use crate::{SubtypeChecker, TypeSubstitution, instantiate_type};
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
