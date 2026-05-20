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
