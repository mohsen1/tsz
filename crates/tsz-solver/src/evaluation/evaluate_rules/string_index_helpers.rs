//! Helpers for string index-signature applicability during indexed access.

use crate::evaluation::evaluate::TypeEvaluator;
use crate::relations::subtype::{SubtypeChecker, TypeResolver};
use crate::types::{IndexSignature, LiteralValue, TypeData, TypeId};

pub(super) fn string_index_signature_applies<R: TypeResolver>(
    evaluator: &TypeEvaluator<'_, R>,
    string_index: &IndexSignature,
    index_type: TypeId,
) -> bool {
    if string_index.key_type == TypeId::STRING {
        return index_type == TypeId::STRING
            || matches!(
                evaluator.interner().lookup(index_type),
                Some(TypeData::Literal(LiteralValue::String(_)))
            )
            || is_string_like_index(evaluator, index_type);
    }

    if index_type == TypeId::STRING
        && matches!(
            evaluator.interner().lookup(string_index.key_type),
            Some(TypeData::TemplateLiteral(_) | TypeData::StringIntrinsic { .. })
        )
    {
        return true;
    }

    let mut checker = SubtypeChecker::with_resolver(evaluator.interner(), evaluator.resolver());
    checker.is_subtype_of(index_type, string_index.key_type)
}

fn is_string_like_index<R: TypeResolver>(
    evaluator: &TypeEvaluator<'_, R>,
    index_type: TypeId,
) -> bool {
    if index_type.is_intrinsic() {
        return false;
    }
    match evaluator.interner().lookup(index_type) {
        Some(TypeData::TemplateLiteral(_) | TypeData::StringIntrinsic { .. }) => true,
        Some(TypeData::Intersection(list_id)) => evaluator
            .interner()
            .type_list(list_id)
            .iter()
            .any(|&member| is_string_like_intersection_member(evaluator, member)),
        _ => false,
    }
}

fn is_string_like_intersection_member<R: TypeResolver>(
    evaluator: &TypeEvaluator<'_, R>,
    member: TypeId,
) -> bool {
    if member == TypeId::STRING {
        return true;
    }
    if member.is_intrinsic() {
        return false;
    }
    matches!(
        evaluator.interner().lookup(member),
        Some(
            TypeData::Literal(LiteralValue::String(_))
                | TypeData::TemplateLiteral(_)
                | TypeData::StringIntrinsic { .. }
        )
    )
}
