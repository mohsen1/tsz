use tsz_solver::TypeId;

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

/// Thin wrapper around `tsz_solver::BinaryOpEvaluator::evaluate`.
///
/// Evaluates a binary operation (e.g., `+`, `-`, `*`, `&&`, `||`) on two types
/// and returns the result type or an error.
pub(crate) fn evaluate_binary_op(
    db: &dyn tsz_solver::QueryDatabase,
    left: TypeId,
    right: TypeId,
    op: &'static str,
) -> BinaryOpResult {
    tsz_solver::BinaryOpEvaluator::new(db).evaluate(left, right, op)
}

/// Thin wrapper around `tsz_solver::BinaryOpEvaluator::is_arithmetic_operand`.
///
/// Checks whether a type is valid as an operand in arithmetic operations.
pub(crate) fn is_arithmetic_operand(db: &dyn tsz_solver::QueryDatabase, type_id: TypeId) -> bool {
    tsz_solver::BinaryOpEvaluator::new(db).is_arithmetic_operand(type_id)
}

/// Thin wrapper around `tsz_solver::BinaryOpEvaluator::is_valid_instanceof_left_operand`.
///
/// Checks whether a type is valid for the left side of an `instanceof` expression.
pub(crate) fn is_valid_instanceof_left_operand(
    db: &dyn tsz_solver::QueryDatabase,
    type_id: TypeId,
) -> bool {
    tsz_solver::BinaryOpEvaluator::new(db).is_valid_instanceof_left_operand(type_id)
}

/// Thin wrapper around `tsz_solver::BinaryOpEvaluator::is_symbol_like`.
///
/// Checks whether a type is symbol-like (relevant for operator validation).
pub(crate) fn is_symbol_like(db: &dyn tsz_solver::QueryDatabase, type_id: TypeId) -> bool {
    tsz_solver::BinaryOpEvaluator::new(db).is_symbol_like(type_id)
}

/// Thin wrapper around `tsz_solver::BinaryOpEvaluator::is_boolean_like`.
///
/// Checks whether a type is boolean-like (relevant for logical operator evaluation).
pub(crate) fn is_boolean_like(db: &dyn tsz_solver::QueryDatabase, type_id: TypeId) -> bool {
    tsz_solver::BinaryOpEvaluator::new(db).is_boolean_like(type_id)
}

/// Thin wrapper around `tsz_solver::BinaryOpEvaluator::is_bigint_like`.
///
/// Checks whether a type is bigint-like (relevant for numeric operator validation).
pub(crate) fn is_bigint_like(db: &dyn tsz_solver::QueryDatabase, type_id: TypeId) -> bool {
    tsz_solver::BinaryOpEvaluator::new(db).is_bigint_like(type_id)
}

/// Thin wrapper around `tsz_solver::BinaryOpEvaluator::is_valid_computed_property_name_type`.
///
/// Checks whether a type is valid for use as a computed property name.
pub(crate) fn is_valid_computed_property_name_type(
    db: &dyn tsz_solver::QueryDatabase,
    type_id: TypeId,
) -> bool {
    tsz_solver::BinaryOpEvaluator::new(db).is_valid_computed_property_name_type(type_id)
}

/// Thin wrapper around `tsz_solver::BinaryOpEvaluator::is_valid_mapped_type_key_type`.
///
/// Checks whether a type is valid as a key type in a mapped type.
pub(crate) fn is_valid_mapped_type_key_type(
    db: &dyn tsz_solver::QueryDatabase,
    type_id: TypeId,
) -> bool {
    tsz_solver::BinaryOpEvaluator::new(db).is_valid_mapped_type_key_type(type_id)
}
