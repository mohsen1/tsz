use tsz_solver::{ObjectShape, TupleElement, TypeDatabase, TypeId};

pub(crate) fn union_members(db: &dyn TypeDatabase, type_id: TypeId) -> Option<Vec<TypeId>> {
    tsz_solver::type_queries::get_union_members(db, type_id)
}

pub(crate) fn is_type_parameter(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_type_parameter(db, type_id)
}

pub(crate) fn array_element_type(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    tsz_solver::type_queries::get_array_element_type(db, type_id)
}

pub(crate) fn object_shape(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<ObjectShape>> {
    tsz_solver::type_queries::get_object_shape(db, type_id)
}

pub(crate) fn tuple_elements(db: &dyn TypeDatabase, type_id: TypeId) -> Option<Vec<TupleElement>> {
    tsz_solver::type_queries::get_tuple_elements(db, type_id)
}

pub(crate) fn unwrap_readonly_deep(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    tsz_solver::type_queries::unwrap_readonly_deep(db, type_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsz_solver::{TupleElement, TypeInterner, TypeKey, TypeParamInfo};

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
        let type_param = types.intern(TypeKey::TypeParameter(TypeParamInfo {
            name: types.intern_string("T"),
            constraint: Some(TypeId::STRING),
            default: None,
            is_const: false,
        }));

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
}
