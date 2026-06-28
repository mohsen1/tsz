//! Public convenience wrappers for type evaluation entry points.

use crate::construction::TypeDatabase;
use crate::evaluation::request::EvaluationRequest;
use crate::evaluation::result::EvaluationResult;
use crate::relations::subtype::TypeResolver;
use crate::types::{ConditionalType, MappedType, TypeId};

use super::TypeEvaluator;

/// Convenience function for evaluating conditional types.
pub fn evaluate_conditional(interner: &dyn TypeDatabase, cond: &ConditionalType) -> TypeId {
    let mut evaluator = TypeEvaluator::new(interner);
    evaluator.evaluate_conditional(cond)
}

/// Convenience function for evaluating index access types.
pub fn evaluate_index_access(
    interner: &dyn TypeDatabase,
    object_type: TypeId,
    index_type: TypeId,
) -> TypeId {
    let mut evaluator = TypeEvaluator::new(interner);
    evaluator.evaluate_index_access(object_type, index_type)
}

/// Convenience function for evaluating index access types with options.
pub fn evaluate_index_access_with_options(
    interner: &dyn TypeDatabase,
    object_type: TypeId,
    index_type: TypeId,
    no_unchecked_indexed_access: bool,
) -> TypeId {
    let mut evaluator = TypeEvaluator::new(interner);
    evaluator.set_no_unchecked_indexed_access(no_unchecked_indexed_access);
    evaluator.evaluate_index_access(object_type, index_type)
}

/// Convenience function for full type evaluation.
pub fn evaluate_type(interner: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    evaluate_type_with_request(interner, EvaluationRequest::new(type_id))
}

/// Convenience function for full type evaluation with an explicit resolver.
pub fn evaluate_type_with_resolver(
    interner: &dyn TypeDatabase,
    resolver: &impl TypeResolver,
    type_id: TypeId,
) -> TypeId {
    let mut evaluator = TypeEvaluator::with_resolver(interner, resolver);
    evaluator.evaluate(type_id)
}

/// Convenience function for full type evaluation with explicit request options.
pub fn evaluate_type_with_request(
    interner: &dyn TypeDatabase,
    request: EvaluationRequest,
) -> TypeId {
    evaluate_type_result_with_request(interner, request).into_type_id()
}

/// Convenience function for full type evaluation with explicit request options,
/// preserving the typed termination verdict.
pub fn evaluate_type_result_with_request(
    interner: &dyn TypeDatabase,
    request: EvaluationRequest,
) -> EvaluationResult {
    let mut evaluator = TypeEvaluator::new(interner);
    evaluator.evaluate_request_result(request)
}

/// Convenience function for evaluating mapped types.
pub fn evaluate_mapped(interner: &dyn TypeDatabase, mapped: &MappedType) -> TypeId {
    let mut evaluator = TypeEvaluator::new(interner);
    evaluator.evaluate_mapped(mapped)
}

/// Convenience function for evaluating keyof types.
pub fn evaluate_keyof(interner: &dyn TypeDatabase, operand: TypeId) -> TypeId {
    let mut evaluator = TypeEvaluator::new(interner);
    evaluator.evaluate_keyof(operand)
}
