//! Literal numeric indexing through tuple rest elements.

use crate::relations::subtype::TypeResolver;
use crate::types::{TupleElement, TypeData, TypeId};
use crate::visitor::literal_number;

use super::super::evaluate::TypeEvaluator;

pub(super) fn evaluate_tuple_literal_index<R: TypeResolver>(
    evaluator: &mut TypeEvaluator<'_, R>,
    elements: &[TupleElement],
    index_type: TypeId,
) -> Option<TypeId> {
    let value = literal_number(evaluator.interner(), index_type)?;
    if !value.0.is_finite() || value.0.fract() != 0.0 || value.0 < 0.0 {
        return Some(TypeId::UNDEFINED);
    }
    evaluate_tuple_literal_index_inner(evaluator, elements, value.0 as usize, 0)
}

pub(super) fn literal_index_needs_unchecked_undefined(
    elements: &[TupleElement],
    index_type: TypeId,
    interner: &dyn crate::construction::TypeDatabase,
) -> bool {
    let Some(value) = literal_number(interner, index_type) else {
        return false;
    };
    if !value.0.is_finite() || value.0.fract() != 0.0 || value.0 < 0.0 {
        return false;
    }

    let fixed_prefix_len = elements
        .iter()
        .take_while(|element| !element.rest && element.is_required())
        .count();
    let index = value.0 as usize;
    if index < fixed_prefix_len {
        return false;
    }

    let Some(rest_position) = elements.iter().position(|element| element.rest) else {
        return false;
    };
    let fixed_suffix_len = elements[rest_position + 1..]
        .iter()
        .filter(|element| !element.rest && element.is_required())
        .count();
    if fixed_suffix_len > 0 {
        index == fixed_prefix_len.saturating_add(fixed_suffix_len)
    } else {
        true
    }
}

fn evaluate_tuple_literal_index_inner<R: TypeResolver>(
    evaluator: &mut TypeEvaluator<'_, R>,
    elements: &[TupleElement],
    index: usize,
    depth: usize,
) -> Option<TypeId> {
    const MAX_TUPLE_SPREAD_DEPTH: usize = 64;
    if depth > MAX_TUPLE_SPREAD_DEPTH {
        return None;
    }

    let mut position = 0usize;
    for element in elements {
        if element.rest {
            let rest_index = index.checked_sub(position)?;
            return evaluate_rest_tuple_literal_index(
                evaluator,
                element.type_id,
                rest_index,
                depth,
            );
        }

        if position == index {
            let mut type_id = element.type_id;
            if element.optional {
                type_id = evaluator.interner().union2(type_id, TypeId::UNDEFINED);
            }
            return Some(type_id);
        }
        position = position.checked_add(1)?;
    }

    Some(TypeId::UNDEFINED)
}

fn evaluate_rest_tuple_literal_index<R: TypeResolver>(
    evaluator: &mut TypeEvaluator<'_, R>,
    rest_type: TypeId,
    index: usize,
    depth: usize,
) -> Option<TypeId> {
    if rest_type.is_intrinsic() {
        return None;
    }

    let evaluated = evaluator.evaluate(rest_type);
    let evaluated = crate::type_queries::data::unwrap_readonly(evaluator.interner(), evaluated);
    match evaluator.interner().lookup(evaluated) {
        Some(TypeData::Tuple(tuple_id)) => {
            let elements = evaluator.interner().tuple_list(tuple_id);
            evaluate_tuple_literal_index_inner(evaluator, &elements, index, depth + 1)
        }
        Some(TypeData::Array(element_type)) => Some(element_type),
        Some(TypeData::Union(list_id)) => {
            let members: Vec<_> = evaluator
                .interner()
                .type_list(list_id)
                .iter()
                .copied()
                .collect();
            let mut results = Vec::new();
            for member in members {
                if let Some(result) =
                    evaluate_rest_tuple_literal_index(evaluator, member, index, depth + 1)
                    && result != TypeId::UNDEFINED
                {
                    results.push(result);
                }
            }
            match results.as_slice() {
                [] => Some(TypeId::UNDEFINED),
                [only] => Some(*only),
                _ => Some(evaluator.interner().union(results)),
            }
        }
        _ => None,
    }
}
