use tsz_solver::TypeId;

pub(crate) fn collect_property_name_atoms_for_diagnostics(
    db: &dyn tsz_solver::TypeDatabase,
    type_id: TypeId,
    max_depth: usize,
) -> Vec<tsz_common::Atom> {
    tsz_solver::type_queries::collect_property_name_atoms_for_diagnostics(db, type_id, max_depth)
}
