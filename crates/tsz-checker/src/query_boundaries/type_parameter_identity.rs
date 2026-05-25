use tsz_solver::TypeId;
use tsz_solver::construction::TypeDatabase;

pub(crate) fn contains_type_parameter_identity_shallow(
    db: &dyn TypeDatabase,
    def_store: &tsz_solver::def::DefinitionStore,
    root: TypeId,
    target: TypeId,
) -> bool {
    tsz_solver::visitor::contains_type_parameter_identity_shallow(db, def_store, root, target)
}

pub(crate) fn constraint_references_type_param_identity_in_resolution_path(
    db: &dyn TypeDatabase,
    def_store: &tsz_solver::def::DefinitionStore,
    root: TypeId,
    target: TypeId,
) -> bool {
    tsz_solver::visitor::constraint_references_type_param_identity_in_resolution_path(
        db, def_store, root, target,
    )
}

/// Whether `type_id` contains a generic `Application` reachable along the
/// base-constraint resolution path (union/intersection members, mapped key
/// source, index access). Used to gate alias expansion in TS2313 circular
/// constraint detection without touching the contextual-inference predicate.
pub(crate) fn contains_application_in_constraint_resolution_path(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    tsz_solver::type_queries::contains_application_in_constraint_resolution_path(db, type_id)
}
