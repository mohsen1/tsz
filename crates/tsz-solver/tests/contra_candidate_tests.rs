//! Tests for resolving contravariant inference candidates.

use super::*;
use crate::inference::infer::{InferenceContext, InferenceVar};
use crate::intern::TypeInterner;
use crate::types::InferencePriority;

fn context_with_t_var(interner: &TypeInterner) -> (InferenceContext<'_>, InferenceVar) {
    let mut context = InferenceContext::new(interner);
    let variable = context.fresh_type_param(interner.intern_string("T"), false);
    (context, variable)
}

#[test]
fn test_contra_candidate_basic() {
    let interner = TypeInterner::new();
    let (mut context, variable) = context_with_t_var(&interner);

    context.add_contra_candidate(
        variable,
        TypeId::STRING,
        InferencePriority::NakedTypeVariable,
    );
    context.add_contra_candidate(
        variable,
        TypeId::NUMBER,
        InferencePriority::NakedTypeVariable,
    );

    // Ordinary candidates use tsc's common-subtype reduction. Unrelated
    // candidates retain the first candidate.
    assert_eq!(
        context.resolve_with_constraints(variable).unwrap(),
        TypeId::STRING
    );
}

#[test]
fn test_contra_candidate_related_types_choose_subtype_in_either_order() {
    let interner = TypeInterner::new();
    let literal = interner.literal_number(1.0);

    for candidates in [[TypeId::NUMBER, literal], [literal, TypeId::NUMBER]] {
        let (mut context, variable) = context_with_t_var(&interner);
        for candidate in candidates {
            context.add_contra_candidate(variable, candidate, InferencePriority::NakedTypeVariable);
        }
        assert_eq!(context.resolve_with_constraints(variable).unwrap(), literal);
    }
}

#[test]
fn test_contra_candidate_combination_priority_still_intersects() {
    let interner = TypeInterner::new();
    let (mut context, variable) = context_with_t_var(&interner);

    context.add_contra_candidate(variable, TypeId::STRING, InferencePriority::ReturnType);
    context.add_contra_candidate(variable, TypeId::NUMBER, InferencePriority::ReturnType);

    let expected = interner.intersection(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(
        context.resolve_with_constraints(variable).unwrap(),
        expected
    );
}

#[test]
fn test_contra_candidate_ignores_worse_priority_before_reduction() {
    let interner = TypeInterner::new();
    let (mut context, variable) = context_with_t_var(&interner);

    context.add_contra_candidate(variable, TypeId::STRING, InferencePriority::ReturnType);
    context.add_contra_candidate(
        variable,
        TypeId::NUMBER,
        InferencePriority::NakedTypeVariable,
    );

    assert_eq!(
        context.resolve_with_constraints(variable).unwrap(),
        TypeId::NUMBER
    );
}
