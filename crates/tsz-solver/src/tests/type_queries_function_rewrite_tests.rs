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
