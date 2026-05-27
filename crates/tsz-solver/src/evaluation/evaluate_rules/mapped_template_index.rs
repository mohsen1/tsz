//! Helpers for evaluating indexed access over mapped type templates.

use super::mapped::MappedKeys;
use crate::evaluation::evaluate::TypeEvaluator;
use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::relations::subtype::{SubtypeChecker, TypeResolver};
use crate::types::{MappedModifier, MappedType, TypeData, TypeId};
use smallvec::SmallVec;

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

    let results: SmallVec<[TypeId; 4]> = keys
        .keys
        .iter()
        .filter_map(|mapped_key| {
            let subst = TypeSubstitution::single(mapped.type_param.name, mapped_key.key_literal);
            let instantiated = instantiate_type(evaluator.interner(), mapped.template, &subst);
            let evaluated = evaluator.evaluate(instantiated);
            (evaluated != TypeId::NEVER).then_some(evaluated)
        })
        .collect();

    Some(match results.len() {
        0 => TypeId::NEVER,
        1 => results[0],
        _ => evaluator.interner().union(results.into_vec()),
    })
}

/// Evaluate `mapped[index]` for finite mapped types whose `as` clause remaps
/// source keys to open template-literal patterns.
///
/// For ``{ [K in keyof T as `${K}${string}`]: T[K] }["axyz"]``, the mapped type
/// cannot be materialized as a concrete property named `"axyz"`. Keep the
/// correlation by checking the requested literal index against each remapped key
/// pattern, then substitute the original source key into the template.
pub(super) fn try_evaluate_remapped_mapped_template_for_index<R: TypeResolver>(
    evaluator: &mut TypeEvaluator<'_, R>,
    mapped: &MappedType,
    index_type: TypeId,
) -> Option<TypeId> {
    mapped.name_type?;

    if crate::type_queries::get_literal_property_name(evaluator.interner(), index_type).is_none()
        && crate::literal_number(evaluator.interner(), index_type).is_none()
    {
        return None;
    }

    let keys: MappedKeys = evaluator.extract_mapped_keys(mapped.constraint)?;
    if keys.has_string
        || keys.has_number
        || !keys.template_literals.is_empty()
        || !keys.symbol_keys.is_empty()
    {
        return None;
    }

    let mut results: SmallVec<[TypeId; 4]> = SmallVec::new();

    for mapped_key in keys.keys {
        let remapped_key = match evaluator.remap_key_type_for_mapped(mapped, mapped_key.key_literal)
        {
            Ok(Some(remapped_key)) => remapped_key,
            Ok(None) => continue,
            Err(()) => return None,
        };

        if !remapped_key_matches_index(evaluator, remapped_key, index_type) {
            continue;
        }

        let subst = TypeSubstitution::single(mapped.type_param.name, mapped_key.key_literal);
        let instantiated = instantiate_type(evaluator.interner(), mapped.template, &subst);
        let mut evaluated = evaluator.evaluate(instantiated);
        if matches!(mapped.optional_modifier, Some(MappedModifier::Add)) {
            evaluated = evaluator.interner().union2(evaluated, TypeId::UNDEFINED);
        }
        if evaluated != TypeId::NEVER {
            results.push(evaluated);
        }
    }

    Some(match results.len() {
        0 => TypeId::UNDEFINED,
        1 => results[0],
        _ => evaluator.interner().union(results.into_vec()),
    })
}

fn remapped_key_matches_index<R: TypeResolver>(
    evaluator: &mut TypeEvaluator<'_, R>,
    remapped_key: TypeId,
    index_type: TypeId,
) -> bool {
    if remapped_key == index_type {
        return true;
    }

    if let Some(TypeData::Union(list_id)) = evaluator.interner().lookup(remapped_key) {
        let members = evaluator.interner().type_list(list_id);
        return members
            .iter()
            .any(|&member| remapped_key_matches_index(evaluator, member, index_type));
    }

    let mut checker = SubtypeChecker::with_resolver(evaluator.interner(), evaluator.resolver());
    if let Some(db) = evaluator.query_db() {
        checker = checker.with_query_db(db);
    }
    checker.is_subtype_of(index_type, remapped_key)
}
