//! JSX checker query boundaries.

use tsz_solver::{DefinitionStore, TypeDatabase, TypeId};

pub(crate) fn contains_index_access_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::contains_index_access_type(db, type_id)
}

/// Check whether a type surface contains an explicit-readonly mapped type.
pub(crate) fn contains_mapped_type_with_readonly_modifier(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    tsz_solver::operations::property::contains_mapped_type_with_readonly_modifier(db, type_id)
}

pub(crate) fn index_access_type_arg_alias_hint(
    db: &dyn TypeDatabase,
    def_store: &DefinitionStore,
    type_id: TypeId,
) -> Option<TypeId> {
    tsz_solver::type_queries::index_access_type_arg_alias_hint(db, def_store, type_id)
}

pub(crate) fn contains_anonymous_object_surface(
    db: &dyn TypeDatabase,
    def_store: &DefinitionStore,
    type_id: TypeId,
) -> bool {
    contains_anonymous_object_surface_inner(db, def_store, type_id, &mut Vec::new())
}

fn contains_anonymous_object_surface_inner(
    db: &dyn TypeDatabase,
    def_store: &DefinitionStore,
    type_id: TypeId,
    visited: &mut Vec<TypeId>,
) -> bool {
    if visited.contains(&type_id) {
        return false;
    }
    visited.push(type_id);

    if tsz_solver::type_queries::get_object_shape_id(db, type_id).is_some()
        && def_store.find_def_for_type(type_id).is_none()
    {
        return true;
    }
    if let Some(members) = tsz_solver::type_queries::get_intersection_members(db, type_id)
        && members
            .iter()
            .any(|&member| contains_anonymous_object_surface_inner(db, def_store, member, visited))
    {
        return true;
    }
    tsz_solver::type_queries::get_union_members(db, type_id).is_some_and(|members| {
        members
            .iter()
            .any(|&member| contains_anonymous_object_surface_inner(db, def_store, member, visited))
    })
}
