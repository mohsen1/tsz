use tsz_common::Atom;
use tsz_solver::{TupleElement, TypeDatabase, TypeId};

pub(crate) fn tuple_elements(db: &dyn TypeDatabase, type_id: TypeId) -> Option<Vec<TupleElement>> {
    tsz_solver::type_queries::get_tuple_elements(db, type_id)
}

pub(crate) fn literal_property_name(db: &dyn TypeDatabase, type_id: TypeId) -> Option<Atom> {
    tsz_solver::type_queries::get_literal_property_name(db, type_id)
}

pub(crate) fn is_valid_spread_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_valid_spread_type(db, type_id)
}

#[cfg(test)]
#[path = "../../tests/type_computation_access_boundaries.rs"]
mod tests;
