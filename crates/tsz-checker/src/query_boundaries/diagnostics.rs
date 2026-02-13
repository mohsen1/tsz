use tsz_solver::TypeId;
use tsz_solver::type_queries::{TypeTraversalKind, classify_for_traversal};

pub(crate) enum PropertyTraversal {
    Object(std::sync::Arc<tsz_solver::ObjectShape>),
    Callable(std::sync::Arc<tsz_solver::CallableShape>),
    Members(Vec<TypeId>),
    Other,
}

pub(crate) fn classify_property_traversal(
    db: &dyn tsz_solver::TypeDatabase,
    type_id: TypeId,
) -> PropertyTraversal {
    match classify_for_traversal(db, type_id) {
        TypeTraversalKind::Object(_) => tsz_solver::type_queries::get_object_shape(db, type_id)
            .map_or(PropertyTraversal::Other, PropertyTraversal::Object),
        TypeTraversalKind::Callable(_) => tsz_solver::type_queries::get_callable_shape(db, type_id)
            .map_or(PropertyTraversal::Other, PropertyTraversal::Callable),
        TypeTraversalKind::Members(members) => PropertyTraversal::Members(members),
        _ => PropertyTraversal::Other,
    }
}
