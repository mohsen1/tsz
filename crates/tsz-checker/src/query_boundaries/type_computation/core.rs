use tsz_solver::{TypeDatabase, TypeId, TypeResolver};

/// Re-export of the solver's binary operation result type.
///
/// Wraps `tsz_solver::BinaryOpResult`.
/// This is the result enum returned by binary operation evaluation.
pub(crate) use tsz_solver::BinaryOpResult;

pub(crate) fn evaluate_contextual_structure_with(
    db: &dyn tsz_solver::QueryDatabase,
    type_id: TypeId,
    evaluate_leaf: &mut dyn FnMut(TypeId) -> TypeId,
) -> TypeId {
    tsz_solver::type_queries::evaluate_contextual_structure_with(db, type_id, evaluate_leaf)
}

pub(crate) fn evaluate_plus_chain(
    db: &dyn tsz_solver::QueryDatabase,
    operand_types: &[TypeId],
) -> Option<TypeId> {
    tsz_solver::BinaryOpEvaluator::new(db).evaluate_plus_chain(operand_types)
}

pub(crate) fn is_arithmetic_operand(db: &dyn tsz_solver::QueryDatabase, type_id: TypeId) -> bool {
    tsz_solver::BinaryOpEvaluator::new(db).is_arithmetic_operand(type_id)
}

pub(crate) fn is_bigint_like(db: &dyn tsz_solver::QueryDatabase, type_id: TypeId) -> bool {
    tsz_solver::BinaryOpEvaluator::new(db).is_bigint_like(type_id)
}

// ---------------------------------------------------------------------------
// Expression operation boundary wrappers
// ---------------------------------------------------------------------------

/// Compute the result type of a conditional (ternary) expression.
pub(crate) fn compute_conditional_expression_type(
    db: &dyn TypeDatabase,
    condition: TypeId,
    true_type: TypeId,
    false_type: TypeId,
) -> TypeId {
    tsz_solver::expression_ops::compute_conditional_expression_type(
        db, condition, true_type, false_type,
    )
}

/// Compute the best common type from a list of element types.
pub(crate) fn compute_best_common_type<R: TypeResolver>(
    db: &dyn TypeDatabase,
    types: &[TypeId],
    resolver: Option<&R>,
) -> TypeId {
    tsz_solver::expression_ops::compute_best_common_type(db, types, resolver)
}

/// Cache-aware variant: thread `&dyn QueryDatabase` so the cross-call
/// subtype-reduction cache on `QueryCache` can collapse the O(N²) loop
/// in `remove_subtypes_for_bct` for repeated BCT call sites.
pub(crate) fn compute_best_common_type_cached<R: TypeResolver>(
    db: &dyn TypeDatabase,
    query_db: Option<&dyn tsz_solver::QueryDatabase>,
    types: &[TypeId],
    resolver: Option<&R>,
) -> TypeId {
    tsz_solver::expression_ops::compute_best_common_type_cached(db, query_db, types, resolver)
}

/// Check whether a contextual type is suitable for template literal narrowing.
pub(crate) fn is_template_literal_contextual_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::expression_ops::is_template_literal_contextual_type(db, type_id)
}

/// Compute the type of a template literal expression with contextual typing.
pub(crate) fn compute_template_expression_type_contextual(
    db: &dyn TypeDatabase,
    texts: &[String],
    parts: &[TypeId],
) -> TypeId {
    tsz_solver::expression_ops::compute_template_expression_type_contextual(db, texts, parts)
}

/// Compute the type of a template literal expression without contextual typing.
pub(crate) fn compute_template_expression_type(
    db: &dyn TypeDatabase,
    texts: &[String],
    parts: &[TypeId],
) -> TypeId {
    tsz_solver::expression_ops::compute_template_expression_type(db, texts, parts)
}
