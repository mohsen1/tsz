use super::*;
use tsz_solver::{SymbolRef, TypeInterner, TypeParamInfo, Visibility};

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
    let object_with_optional = types.object(vec![tsz_solver::PropertyInfo {
        name: types.intern_string("foo"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: true,
        readonly: false,
        is_method: false,
        visibility: Visibility::Public,
        parent_id: None,
    }]);
    let readonly_array = types.readonly_type(array);
    let union = types.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let null_or_undefined = types.union(vec![TypeId::NULL, TypeId::UNDEFINED]);
    let type_param = types.type_param(TypeParamInfo {
        name: types.intern_string("T"),
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    });
    let query_union = types.union(vec![types.type_query(SymbolRef(42)), TypeId::STRING]);

    assert_eq!(array_element_type(&types, array), Some(TypeId::STRING));
    assert!(object_shape(&types, object).is_some());
    assert_eq!(tuple_elements(&types, tuple).map(|v| v.len()), Some(2));
    assert_eq!(unwrap_readonly_deep(&types, readonly_array), array);
    assert_eq!(
        union_members(&types, union),
        Some(vec![TypeId::NUMBER, TypeId::STRING])
    );
    assert!(is_type_parameter(&types, type_param));
    assert!(is_only_null_or_undefined(&types, null_or_undefined));
    let found = find_property_in_object_by_str(&types, object_with_optional, "foo")
        .expect("expected property in object_with_optional");
    assert_eq!(found.type_id, TypeId::STRING);
    assert!(found.optional);
    assert!(has_type_query_for_symbol(&types, query_union, 42, |ty| ty));
    assert!(!has_type_query_for_symbol(&types, query_union, 7, |ty| ty));
}
