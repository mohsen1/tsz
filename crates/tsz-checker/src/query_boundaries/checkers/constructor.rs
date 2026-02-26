use tsz_solver::{TypeDatabase, TypeId};

pub(crate) use super::super::common::has_construct_signatures;
pub(crate) use tsz_solver::type_queries::{
    AbstractConstructorAnchor, ConstructorAccessKind, ConstructorReturnMergeKind, InstanceTypeKind,
};

pub(crate) fn classify_for_instance_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> InstanceTypeKind {
    tsz_solver::type_queries::classify_for_instance_type(db, type_id)
}

pub(crate) fn classify_for_constructor_return_merge(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> ConstructorReturnMergeKind {
    tsz_solver::type_queries::classify_for_constructor_return_merge(db, type_id)
}

pub(crate) fn resolve_abstract_constructor_anchor(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> AbstractConstructorAnchor {
    tsz_solver::type_queries::resolve_abstract_constructor_anchor(db, type_id)
}

pub(crate) fn classify_for_constructor_access(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> ConstructorAccessKind {
    tsz_solver::type_queries::classify_for_constructor_access(db, type_id)
}

/// Get the construct return type for a single constructor type member.
/// Returns the raw return type (possibly Lazy) without resolution,
/// suitable for display name formatting that preserves named type references.
pub(crate) fn construct_return_type_for_display(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<TypeId> {
    match tsz_solver::type_queries::classify_for_instance_type(db, type_id) {
        InstanceTypeKind::Callable(shape_id) => {
            tsz_solver::type_queries::get_construct_return_type_union(db, shape_id)
        }
        InstanceTypeKind::Function(shape_id) => {
            let shape = db.function_shape(shape_id);
            if shape.is_constructor {
                Some(shape.return_type)
            } else {
                None
            }
        }
        _ => None,
    }
}
