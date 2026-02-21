use crate::type_queries::{
    get_function_shape, replace_function_return_type, rewrite_function_error_slots_to_any,
};
use crate::{FunctionShape, ParamInfo, TypeId, TypeInterner};

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
