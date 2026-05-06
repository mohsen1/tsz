//! JSX checker query boundaries.

use tsz_solver::{DefinitionStore, TypeDatabase, TypeId};

pub(crate) fn contains_index_access_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::contains_index_access_type(db, type_id)
}

pub(crate) fn index_access_type_arg_alias_hint(
    db: &dyn TypeDatabase,
    def_store: &DefinitionStore,
    type_id: TypeId,
) -> Option<TypeId> {
    tsz_solver::type_queries::index_access_type_arg_alias_hint(db, def_store, type_id)
}
