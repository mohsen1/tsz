use super::*;
use tsz_solver::{TypeInterner, TypeParamInfo};

#[test]
fn exposes_state_checking_boundary_queries() {
    let types = TypeInterner::new();

    let array = types.array(TypeId::STRING);
    let tuple = types.tuple(vec![
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
    let object = types.object(vec![]);
    let readonly_array = types.readonly_type(array);
    let union = types.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let type_param = types.type_param(TypeParamInfo {
        name: types.intern_string("T"),
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    });

    assert_eq!(array_element_type(&types, array), Some(TypeId::STRING));
    assert!(object_shape(&types, object).is_some());
    assert_eq!(tuple_elements(&types, tuple).map(|v| v.len()), Some(2));
    assert_eq!(unwrap_readonly_deep(&types, readonly_array), array);
    assert_eq!(
        union_members(&types, union),
        Some(vec![TypeId::NUMBER, TypeId::STRING])
    );
    assert!(is_type_parameter(&types, type_param));
}
