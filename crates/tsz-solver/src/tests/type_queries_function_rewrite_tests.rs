use crate::type_queries::{
    get_function_shape, replace_function_return_type, rewrite_function_error_slots_to_any,
    unpack_tuple_rest_parameter,
};
use crate::{FunctionShape, ParamInfo, TupleElement, TypeId, TypeInterner};

#[test]
fn rewrite_function_error_slots_to_any_rewrites_error_param_and_return() {
    let db = TypeInterner::new();
    let fn_ty = db.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: None,
            type_id: TypeId::ERROR,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::ERROR,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let rewritten = rewrite_function_error_slots_to_any(&db, fn_ty);
    let shape = get_function_shape(&db, rewritten).expect("function expected");
    assert_eq!(shape.params[0].type_id, TypeId::ANY);
    assert_eq!(shape.return_type, TypeId::ANY);
}

#[test]
fn replace_function_return_type_updates_return_without_touching_params() {
    let db = TypeInterner::new();
    let fn_ty = db.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: None,
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let replaced = replace_function_return_type(&db, fn_ty, TypeId::BOOLEAN);
    let shape = get_function_shape(&db, replaced).expect("function expected");
    assert_eq!(shape.params[0].type_id, TypeId::STRING);
    assert_eq!(shape.return_type, TypeId::BOOLEAN);
}

#[test]
fn unpack_tuple_rest_parameter_flattens_nested_tuple_rest_elements() {
    let db = TypeInterner::new();
    let nested_tail = db.tuple(vec![
        TupleElement {
            type_id: TypeId::BOOLEAN,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: db.array(TypeId::STRING),
            name: None,
            optional: false,
            rest: true,
        },
    ]);
    let source_rest = ParamInfo {
        name: None,
        type_id: db.tuple(vec![
            TupleElement {
                type_id: TypeId::NUMBER,
                name: None,
                optional: false,
                rest: false,
            },
            TupleElement {
                type_id: nested_tail,
                name: None,
                optional: false,
                rest: true,
            },
        ]),
        optional: false,
        rest: true,
    };

    let unpacked = unpack_tuple_rest_parameter(&db, &source_rest);
    assert_eq!(unpacked.len(), 3);
    assert_eq!(unpacked[0].type_id, TypeId::NUMBER);
    assert!(!unpacked[0].rest);
    assert_eq!(unpacked[1].type_id, TypeId::BOOLEAN);
    assert!(!unpacked[1].rest);
    assert_eq!(unpacked[2].type_id, db.array(TypeId::STRING));
    assert!(unpacked[2].rest);
}

/// `(...args: [] | [X])` is the lib pattern used by `Iterator.next` /
/// `AsyncIterator.next`. tsc treats it as equivalent to `(value?: X)` for
/// signature compat. The unpacker must recognize the prefix-aligned union of
/// fixed tuples and emit one optional parameter.
#[test]
fn unpack_tuple_rest_parameter_handles_empty_or_single_tuple_union() {
    let db = TypeInterner::new();
    let empty_tuple = db.tuple(vec![]);
    let single_tuple = db.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }]);
    let union_ty = db.union(vec![empty_tuple, single_tuple]);

    let rest_param = ParamInfo {
        name: None,
        type_id: union_ty,
        optional: false,
        rest: true,
    };

    let unpacked = unpack_tuple_rest_parameter(&db, &rest_param);
    assert_eq!(
        unpacked.len(),
        1,
        "[] | [X] should flatten to a single optional param, got {unpacked:?}"
    );
    assert_eq!(unpacked[0].type_id, TypeId::STRING);
    assert!(
        unpacked[0].optional,
        "shorter member missing position 0 → optional"
    );
    assert!(!unpacked[0].rest, "fixed-position param, not rest");
}

/// `[X] | [X, Y]` where positions agree on prefix should flatten to
/// `[required X, optional Y]`.
#[test]
fn unpack_tuple_rest_parameter_handles_prefix_aligned_two_member_union() {
    let db = TypeInterner::new();
    let one = db.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }]);
    let two = db.tuple(vec![
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
    let union_ty = db.union(vec![one, two]);

    let rest_param = ParamInfo {
        name: None,
        type_id: union_ty,
        optional: false,
        rest: true,
    };

    let unpacked = unpack_tuple_rest_parameter(&db, &rest_param);
    assert_eq!(unpacked.len(), 2);
    assert_eq!(unpacked[0].type_id, TypeId::STRING);
    assert!(!unpacked[0].optional, "position 0 covered by both members");
    assert_eq!(unpacked[1].type_id, TypeId::NUMBER);
    assert!(unpacked[1].optional, "position 1 only in the longer member");
}

/// Disagreeing first elements must NOT collapse — `[X] | [Y]` stays as a
/// rest-typed param so callers fall back to the union-of-tuple handling at
/// the call site (which iterates each tuple variant separately).
#[test]
fn unpack_tuple_rest_parameter_keeps_disagreeing_union_as_rest() {
    let db = TypeInterner::new();
    let s_tuple = db.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }]);
    let n_tuple = db.tuple(vec![TupleElement {
        type_id: TypeId::NUMBER,
        name: None,
        optional: false,
        rest: false,
    }]);
    let union_ty = db.union(vec![s_tuple, n_tuple]);

    let rest_param = ParamInfo {
        name: None,
        type_id: union_ty,
        optional: false,
        rest: true,
    };

    let unpacked = unpack_tuple_rest_parameter(&db, &rest_param);
    assert_eq!(
        unpacked.len(),
        1,
        "non-prefix-aligned union must stay as a rest param"
    );
    assert!(unpacked[0].rest);
    assert_eq!(unpacked[0].type_id, union_ty);
}
