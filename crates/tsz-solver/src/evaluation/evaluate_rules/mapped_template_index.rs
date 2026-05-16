//! Helpers for evaluating indexed access over mapped type templates.

use super::mapped::MappedKeys;
use crate::evaluation::evaluate::TypeEvaluator;
use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::relations::subtype::TypeResolver;
use crate::types::{MappedType, TypeId};

/// Evaluate `mapped[constraint]` by substituting each concrete key literal into the
/// mapped template and unioning the results.
///
/// Returns `None` when the constraint cannot be expanded to an all-literal key
/// set, falling back to the constrained-TypeParam approach in `index_access`.
pub(super) fn try_evaluate_mapped_template_per_concrete_key<R: TypeResolver>(
    evaluator: &mut TypeEvaluator<'_, R>,
    mapped: &MappedType,
) -> Option<TypeId> {
    let keys: MappedKeys = evaluator.extract_mapped_keys(mapped.constraint)?;

    if keys.has_string
        || keys.has_number
        || !keys.template_literals.is_empty()
        || !keys.symbol_keys.is_empty()
    {
        return None;
    }

    let results: Vec<TypeId> = keys
        .keys
        .iter()
        .filter_map(|mapped_key| {
            let subst = TypeSubstitution::single(mapped.type_param.name, mapped_key.key_literal);
            let instantiated = instantiate_type(evaluator.interner(), mapped.template, &subst);
            let evaluated = evaluator.evaluate(instantiated);
            (evaluated != TypeId::NEVER).then_some(evaluated)
        })
        .collect();

    Some(crate::utils::union_or_single(evaluator.interner(), results))
}
