//! Enum and enum-adjacent checker queries.
//!
//! These wrappers keep enum utility code off the broad `common` quarantine
//! barrel while the underlying solver queries remain the semantic owner.

use std::sync::Arc;

use tsz_solver::construction::TypeDatabase;
use tsz_solver::{ObjectShape, TypeId};

pub(crate) fn enum_def_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::def::DefId> {
    super::common::enum_def_id(db, type_id)
}

pub(crate) fn type_parameter_constraint(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    super::common::type_parameter_constraint(db, type_id)
}

pub(crate) fn object_shape_for_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<Arc<ObjectShape>> {
    super::common::object_shape_for_type(db, type_id)
}
