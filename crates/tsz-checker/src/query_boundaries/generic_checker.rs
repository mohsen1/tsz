use tsz_solver::{TypeDatabase, TypeId};

pub(crate) use super::common::{callable_shape_for_type, contains_type_parameters};
pub(crate) use tsz_solver::type_queries::TypeArgumentExtractionKind;

pub(crate) fn classify_for_type_argument_extraction(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> TypeArgumentExtractionKind {
    tsz_solver::type_queries::classify_for_type_argument_extraction(db, type_id)
}
