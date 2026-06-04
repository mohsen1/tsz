//! Tests for deferred-conditional failure explanation.

use crate::SubtypeFailureReason;
use crate::intern::TypeInterner;
use crate::relations::subtype::SubtypeChecker;
use crate::types::{ConditionalType, TypeData, TypeId, TypeParamInfo};

fn type_param(interner: &TypeInterner, name: &str) -> TypeId {
    interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string(name),
        constraint: None,
        default: None,
        is_const: false,
    }))
}

fn nested_deferred_conditional(interner: &TypeInterner, depth: usize) -> TypeId {
    let t = type_param(interner, "T");
    let mut current = TypeId::NUMBER;
    for _ in 0..depth {
        current = interner.conditional(ConditionalType {
            check_type: t,
            extends_type: TypeId::STRING,
            true_type: current,
            false_type: TypeId::BOOLEAN,
            is_distributive: true,
        });
    }
    current
}

fn conditional_branch_depth(reason: &SubtypeFailureReason) -> usize {
    match reason {
        SubtypeFailureReason::ConditionalBranchMismatch { nested_reason, .. } => {
            1 + conditional_branch_depth(nested_reason)
        }
        _ => 0,
    }
}

fn is_original_conditional_branch(
    reason: &SubtypeFailureReason,
    source: TypeId,
    target: TypeId,
) -> bool {
    matches!(
        reason,
        SubtypeFailureReason::ConditionalBranchMismatch {
            source_type,
            target_type,
            ..
        } if *source_type == source && *target_type == target
    )
}

#[test]
fn deferred_conditional_explain_uses_default_constraint_before_branches() {
    let interner = TypeInterner::new();
    let source = nested_deferred_conditional(&interner, 140);
    let mut checker = SubtypeChecker::new(&interner);

    let reason = checker
        .explain_failure(source, TypeId::STRING)
        .expect("number leaf must fail against string target");

    let branch_depth = conditional_branch_depth(&reason);
    assert_eq!(
        branch_depth, 0,
        "default-constraint explanation should avoid a conditional branch ladder"
    );
    assert!(
        !matches!(
            reason,
            SubtypeFailureReason::TypeMismatch {
                source_type,
                ..
            } if source_type == source
        ),
        "explanation should describe the conditional constraint, not the original deferred conditional"
    );
}

#[test]
fn conditional_pair_explain_requires_matching_extends_shape() {
    let interner = TypeInterner::new();
    let t = type_param(&interner, "T");
    let source = interner.conditional(ConditionalType {
        check_type: t,
        extends_type: TypeId::STRING,
        true_type: TypeId::NUMBER,
        false_type: TypeId::BOOLEAN,
        is_distributive: true,
    });
    let target = interner.conditional(ConditionalType {
        check_type: t,
        extends_type: TypeId::NUMBER,
        true_type: TypeId::STRING,
        false_type: TypeId::STRING,
        is_distributive: true,
    });
    let mut checker = SubtypeChecker::new(&interner);

    let reason = checker
        .explain_failure(source, target)
        .expect("unmatched conditional shapes should fail");

    assert!(
        !is_original_conditional_branch(&reason, source, target),
        "unmatched conditional shapes should not invent branch-pair diagnostics for the original pair, got {reason:?}"
    );
}

#[test]
fn source_conditional_to_conditional_explain_uses_default_constraint() {
    let interner = TypeInterner::new();
    let t = type_param(&interner, "T");
    let inner_source = interner.conditional(ConditionalType {
        check_type: t,
        extends_type: TypeId::STRING,
        true_type: TypeId::NUMBER,
        false_type: TypeId::BOOLEAN,
        is_distributive: true,
    });
    let source = interner.conditional(ConditionalType {
        check_type: t,
        extends_type: TypeId::STRING,
        true_type: inner_source,
        false_type: TypeId::BOOLEAN,
        is_distributive: true,
    });
    let target = interner.conditional(ConditionalType {
        check_type: t,
        extends_type: TypeId::STRING,
        true_type: TypeId::STRING,
        false_type: TypeId::BOOLEAN,
        is_distributive: true,
    });
    let mut checker = SubtypeChecker::new(&interner);

    let reason = checker
        .explain_failure(source, target)
        .expect("number leaf must fail against string target branch");

    assert!(
        !is_original_conditional_branch(&reason, source, target),
        "source conditional should explain via its default constraint before target branches, got {reason:?}"
    );
}
