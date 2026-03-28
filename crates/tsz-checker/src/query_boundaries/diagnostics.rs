use super::state::checking as state_checking;
use tsz_solver::TypeId;

pub(crate) use super::common::{callable_shape_for_type, intersection_members, union_members};

pub(crate) fn collect_property_name_atoms_for_diagnostics(
    db: &dyn tsz_solver::TypeDatabase,
    type_id: TypeId,
    max_depth: usize,
) -> Vec<tsz_common::Atom> {
    tsz_solver::type_queries::collect_property_name_atoms_for_diagnostics(db, type_id, max_depth)
}

/// Collect property names accessible on a type for spelling suggestions.
///
/// For union types, only properties present in ALL members are returned (intersection).
pub(crate) fn collect_accessible_property_names_for_suggestion(
    db: &dyn tsz_solver::TypeDatabase,
    type_id: TypeId,
    max_depth: usize,
) -> Vec<tsz_common::Atom> {
    if state_checking::union_members(db, type_id).is_none() {
        return collect_property_name_atoms_for_diagnostics(db, type_id, max_depth);
    }

    tsz_solver::type_queries::collect_accessible_property_names_for_suggestion(
        db, type_id, max_depth,
    )
}

pub(crate) fn function_shape(
    db: &dyn tsz_solver::TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<tsz_solver::FunctionShape>> {
    tsz_solver::type_queries::get_function_shape(db, type_id)
}

pub(crate) fn mapped_type(
    db: &dyn tsz_solver::TypeDatabase,
    type_id: TypeId,
) -> Option<(
    tsz_solver::MappedTypeId,
    std::sync::Arc<tsz_solver::MappedType>,
)> {
    tsz_solver::type_queries::get_mapped_type_with_id(db, type_id)
}

pub(crate) fn type_application(
    db: &dyn tsz_solver::TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<tsz_solver::TypeApplication>> {
    tsz_solver::type_queries::get_type_application(db, type_id)
}

pub(crate) fn preserves_named_application_base(
    db: &dyn tsz_solver::TypeDatabase,
    type_id: TypeId,
) -> bool {
    tsz_solver::type_queries::get_lazy_def_id(db, type_id).is_some()
        || !matches!(
            tsz_solver::type_queries::classify_type_query(db, type_id),
            tsz_solver::type_queries::TypeQueryKind::Other
        )
}
