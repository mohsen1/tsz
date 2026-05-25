use super::*;
use crate::intern::TypeInterner;
use crate::relations::compat::CompatChecker;
use crate::types::{FunctionShape, ParamInfo, TupleElement, TypeData, TypeId, TypeParamInfo};

#[test]
fn infer_generic_tuple_rest_from_rest_argument_returns_array() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(interner.array(TypeId::ANY)),
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let tuple_t = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: t_type,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    let func = FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: tuple_t,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    let string_array = interner.array(TypeId::STRING);
    let tuple_arg = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: string_array,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    let result = infer_generic_function(&interner, &mut subtype, &func, &[tuple_arg]);

    assert_eq!(result, string_array, "T should be inferred as string[]");
}
