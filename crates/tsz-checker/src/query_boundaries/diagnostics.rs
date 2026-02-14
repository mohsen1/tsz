use tsz_solver::TypeId;
pub(crate) use tsz_solver::type_queries::PropertyTraversalKind as PropertyTraversal;

pub(crate) fn classify_property_traversal(
    db: &dyn tsz_solver::TypeDatabase,
    type_id: TypeId,
) -> PropertyTraversal {
    tsz_solver::type_queries::classify_property_traversal(db, type_id)
}
