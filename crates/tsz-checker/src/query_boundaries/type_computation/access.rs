use tsz_common::Atom;
use tsz_solver::{QueryDatabase, TypeDatabase, TypeId};

pub(crate) use super::super::common::tuple_elements;

pub(crate) fn literal_property_name(db: &dyn TypeDatabase, type_id: TypeId) -> Option<Atom> {
    tsz_solver::type_queries::get_literal_property_name(db, type_id)
}

pub(crate) fn is_valid_spread_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_valid_spread_type(db, type_id)
}

pub(crate) struct GenericIndexedAccessSubstitution {
    pub(crate) index_access: TypeId,
    pub(crate) type_to_evaluate: TypeId,
}

pub(crate) fn generic_index_access_substitution(
    db: &dyn QueryDatabase,
    raw_object_type: TypeId,
    pre_resolution_object_type: TypeId,
    index_type: TypeId,
    mut resolve_lazy: impl FnMut(TypeId) -> TypeId,
) -> Option<GenericIndexedAccessSubstitution> {
    if !tsz_solver::type_queries::is_type_parameter(db.as_type_database(), index_type) {
        return None;
    }

    let substitution_object = [raw_object_type, pre_resolution_object_type]
        .into_iter()
        .find_map(|candidate| {
            let resolved_candidate = resolve_lazy(candidate);
            let is_application_receiver =
                tsz_solver::application_id(db.as_type_database(), candidate).is_some()
                    || db.get_display_alias(candidate).is_some_and(|alias| {
                        tsz_solver::application_id(db.as_type_database(), alias).is_some()
                    });
            if is_application_receiver {
                Some(candidate)
            } else if tsz_solver::mapped_type_id(db.as_type_database(), resolved_candidate)
                .is_some()
            {
                Some(resolved_candidate)
            } else {
                None
            }
        })?;

    let index_access = db.factory().index_access(substitution_object, index_type);
    let type_to_evaluate = index_access;
    Some(GenericIndexedAccessSubstitution {
        index_access,
        type_to_evaluate,
    })
}

#[cfg(test)]
#[path = "../../../tests/type_computation_access_boundaries.rs"]
mod tests;
