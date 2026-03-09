use tsz_solver::{TypeDatabase, TypeId};

pub(crate) use super::super::common::{callable_shape_for_type, contains_type_parameters};
pub(crate) use tsz_solver::type_queries::TypeArgumentExtractionKind;

pub(crate) fn classify_for_type_argument_extraction(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> TypeArgumentExtractionKind {
    tsz_solver::type_queries::classify_for_type_argument_extraction(db, type_id)
}

/// Check if a type is a bare type parameter (`TypeParameter` or `Infer`).
pub(crate) fn is_bare_type_parameter(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::visitor::is_type_parameter(db, type_id)
}

/// Get the base constraint of a type for TS2344 checking.
///
/// For `TypeParameter` with constraint: returns the constraint.
/// For `TypeParameter` without constraint: returns `UNKNOWN`.
/// For all other types (including `Infer`): returns the type unchanged.
pub(crate) fn base_constraint_of_type(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    tsz_solver::type_queries::get_base_constraint_of_type(db, type_id)
}

/// Get the object and index types of an `IndexAccess` type.
pub(crate) fn index_access_components(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<(TypeId, TypeId)> {
    tsz_solver::type_queries::get_index_access_types(db, type_id)
}
