use super::*;
use crate::TypeInterner;
use crate::def::DefId;
use crate::{SubtypeChecker, TypeSubstitution, instantiate_type};
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

