use tsz_common::Atom;
use tsz_solver::{TypeDatabase, TypeId};

pub(crate) use super::common::tuple_elements;

pub(crate) fn literal_property_name(db: &dyn TypeDatabase, type_id: TypeId) -> Option<Atom> {
    tsz_solver::type_queries::get_literal_property_name(db, type_id)
}

pub(crate) fn is_valid_spread_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_valid_spread_type(db, type_id)
}

#[cfg(test)]
#[path = "../../tests/type_computation_access_boundaries.rs"]
mod tests;
