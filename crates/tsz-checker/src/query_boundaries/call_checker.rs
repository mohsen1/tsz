use tsz_solver::{TupleElement, TypeDatabase, TypeId};

pub(crate) fn array_element_type_for_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<TypeId> {
    tsz_solver::type_queries::get_array_element_type(db, type_id)
}

pub(crate) fn tuple_elements_for_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<Vec<TupleElement>> {
    tsz_solver::type_queries::get_tuple_elements(db, type_id)
}

pub(crate) fn is_type_parameter_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_type_parameter(db, type_id)
}

pub(crate) fn lazy_def_id_for_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::DefId> {
    tsz_solver::type_queries::get_lazy_def_id(db, type_id)
}
