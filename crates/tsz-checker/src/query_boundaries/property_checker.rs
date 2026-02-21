use tsz_solver::{TypeDatabase, TypeId};

pub(crate) fn is_type_usable_as_property_name(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_type_usable_as_property_name(db, type_id)
}

#[cfg(test)]
#[path = "../../tests/property_checker_boundaries.rs"]
mod tests;
