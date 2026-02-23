use tsz_solver::{TypeDatabase, TypeId};

pub(crate) use super::common::{
    callable_shape_for_type, construct_signatures_for_type, is_generic_type, is_mapped_type,
    object_shape_for_type,
};

pub(crate) fn type_includes_undefined(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::type_includes_undefined(db, type_id)
}
