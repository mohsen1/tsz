//! Index-union distribution for indexed access types.

use crate::intern::TEMPLATE_LITERAL_EXPANSION_LIMIT;
use crate::relations::subtype::TypeResolver;
use crate::types::{TypeData, TypeId};

use super::super::evaluate::TypeEvaluator;

pub(super) const MAX_UNION_INDEX_SIZE: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum UnionIndexSizeState {
    Continue,
    LimitExceeded,
}

impl UnionIndexSizeState {
    pub(super) const fn for_member_count(member_count: usize) -> Self {
        if member_count > MAX_UNION_INDEX_SIZE {
            Self::LimitExceeded
        } else {
            Self::Continue
        }
    }

    pub(super) const fn is_limit_exceeded(self) -> bool {
        matches!(self, Self::LimitExceeded)
    }
}

pub(super) fn evaluate_index_union_distribution<R: TypeResolver>(
    evaluator: &mut TypeEvaluator<'_, R>,
    object_type: TypeId,
    members: &[TypeId],
) -> TypeId {
    // Limit to prevent OOM with large unions.
    match UnionIndexSizeState::for_member_count(members.len()) {
        UnionIndexSizeState::Continue => {}
        UnionIndexSizeState::LimitExceeded => {
            evaluator.mark_depth_exceeded_for_request();
            return TypeId::ERROR;
        }
    }

    // TS2590 union-complexity bail (parity with tsc's `getUnionType`, which
    // returns `errorType` once a union reaches ~100k constituents).
    //
    // Distributing an index union over an object whose members are themselves
    // large unions grows the assembled union factorially. Stop accumulating
    // once the running member total crosses the limit instead of materializing
    // the whole union; the sticky `union_too_complex` flag then drives TS2590
    // in the checker. A nested distribution that already overflowed is detected
    // via the pre-loop flag snapshot so deeper keys are not re-expanded.
    let union_complex_before_index = evaluator.interner().is_union_too_complex();
    let mut cumulative_members: usize = 0;
    let mut results = Vec::new();
    for &member in members {
        if evaluator.is_depth_exceeded() {
            return TypeId::ERROR;
        }
        if !union_complex_before_index && evaluator.interner().is_union_too_complex() {
            break;
        }
        let result = evaluator.recurse_index_access(object_type, member);
        if result == TypeId::ERROR && evaluator.is_depth_exceeded() {
            return TypeId::ERROR;
        }
        if result != TypeId::UNDEFINED || evaluator.no_unchecked_indexed_access() {
            cumulative_members =
                cumulative_members.saturating_add(evaluator.count_union_members(result));
            results.push(result);
            if cumulative_members >= TEMPLATE_LITERAL_EXPANSION_LIMIT {
                evaluator.interner().mark_union_too_complex();
                break;
            }
        }
    }

    if results.is_empty() {
        return TypeId::UNDEFINED;
    }

    // When the distribution overflowed the union-complexity budget and every
    // collected result is string-typed, widen to `string` instead of interning
    // the oversized literal union. This mirrors the template-literal expansion
    // cap's widening, keeps the already-marked TS2590 flag visible to the
    // checker, and collapses the result so an enclosing recursion does not
    // re-expand it.
    if cumulative_members >= TEMPLATE_LITERAL_EXPANSION_LIMIT
        && results
            .iter()
            .all(|&result| index_result_is_string_like(evaluator, result))
    {
        return TypeId::STRING;
    }

    evaluator.interner().union(results)
}

fn index_result_is_string_like<R: TypeResolver>(
    evaluator: &TypeEvaluator<'_, R>,
    ty: TypeId,
) -> bool {
    if let Some(TypeData::Union(list_id)) = evaluator.interner().lookup(ty) {
        return evaluator
            .interner()
            .type_list(list_id)
            .iter()
            .all(|&member| index_result_is_string_like(evaluator, member));
    }
    crate::type_queries::is_string_like_type(evaluator.interner(), ty)
}

#[cfg(test)]
mod union_index_size_state_tests {
    use super::*;

    #[test]
    fn union_index_size_state_names_exact_cap_and_overflow() {
        assert_eq!(
            UnionIndexSizeState::for_member_count(MAX_UNION_INDEX_SIZE),
            UnionIndexSizeState::Continue
        );
        assert_eq!(
            UnionIndexSizeState::for_member_count(MAX_UNION_INDEX_SIZE + 1),
            UnionIndexSizeState::LimitExceeded
        );
    }
}
