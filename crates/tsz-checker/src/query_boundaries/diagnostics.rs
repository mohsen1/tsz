use tsz_solver::TypeId;

#[allow(dead_code)] // Used by architecture contract tests
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
    tsz_solver::type_queries::collect_accessible_property_names_for_suggestion(
        db, type_id, max_depth,
    )
}
