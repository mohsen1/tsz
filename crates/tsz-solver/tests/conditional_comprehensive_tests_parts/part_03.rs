// Regression coverage for issue #10881: diagnostic chains lose nested relation
// details in conditional failures.
//
// Structural rule: when a subtype/assignability check involving a deferred
// conditional type (`T extends U ? X : Y`) fails, the failure reason should
// preserve the branch that caused the failure and the nested reason explaining
// why that branch failed, rather than collapsing to a generic `TypeMismatch`.
//
// The tests below cover the three structural shapes the explain path can see
// — source-side conditional, target-side conditional, and conditional-vs-
// conditional — and exercise different branch-failure reasons (literal vs.
// intrinsic mismatch) so the assertions are about structural identity, not
// about a particular spelling. Branch identity is derived structurally
// (`branch_target == cond.true_type`), not from a label, per CLAUDE.md §25.

/// Deferred-conditional **target**: `S <: (T extends U ? X : Y)`.
/// Both branches must accept the source. When the true branch fails the
/// relation, the explain path should surface that branch and the underlying
/// literal mismatch instead of a bare `TypeMismatch`.
#[test]
fn test_explain_target_conditional_true_branch_mismatch_preserves_chain() {
    let interner = TypeInterner::new();

    // T extends string ? "yes" : "no"
    let t_param = interner.type_param(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });
    let yes = interner.literal_string("yes");
    let no = interner.literal_string("no");
    let cond = interner.conditional(ConditionalType {
        check_type: t_param,
        extends_type: TypeId::STRING,
        true_type: yes,
        false_type: no,
        is_distributive: true,
    });

    let source = interner.literal_string("x");
    let mut checker = SubtypeChecker::new(&interner);

    // Sanity: the relation actually fails — "x" satisfies neither branch.
    assert!(
        !checker.is_subtype_of(source, cond),
        "deferred conditional should reject an unrelated literal source"
    );

    let reason = checker.explain_failure(source, cond);
    let Some(SubtypeFailureReason::ConditionalBranchMismatch {
        source_type,
        target_type,
        branch_source,
        branch_target,
        nested_reason,
    }) = reason
    else {
        panic!(
            "deferred conditional target should produce a ConditionalBranchMismatch with the failing branch chained, got {reason:?}"
        );
    };
    assert_eq!(source_type, source, "source pair preserved");
    assert_eq!(target_type, cond, "target pair preserved");
    assert_eq!(
        branch_target, yes,
        "true branch fails first ({source:?} -> {yes:?}); explain must surface the true branch as the failing branch"
    );
    assert_eq!(
        branch_source, source,
        "concrete source carried into branch relation"
    );
    assert!(
        matches!(*nested_reason, SubtypeFailureReason::LiteralTypeMismatch { .. }),
        "nested reason should preserve the underlying literal mismatch, got {nested_reason:?}"
    );
}

/// Deferred-conditional target where the **false** branch is the one that
/// fails. Verifies the variant carries the false branch type as `branch_target`.
#[test]
fn test_explain_target_conditional_false_branch_mismatch_preserves_chain() {
    let interner = TypeInterner::new();

    // T extends string ? "yes" : 42
    // Source = "yes" satisfies the true branch but not the false branch.
    let t_param = interner.type_param(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });
    let yes = interner.literal_string("yes");
    let forty_two = interner.literal_number(42.0);
    let cond = interner.conditional(ConditionalType {
        check_type: t_param,
        extends_type: TypeId::STRING,
        true_type: yes,
        false_type: forty_two,
        is_distributive: true,
    });

    let source = yes;
    let mut checker = SubtypeChecker::new(&interner);

    assert!(
        !checker.is_subtype_of(source, cond),
        "\"yes\" should not be assignable to deferred conditional whose false branch is 42"
    );

    let reason = checker.explain_failure(source, cond);
    let Some(SubtypeFailureReason::ConditionalBranchMismatch {
        branch_target,
        nested_reason,
        ..
    }) = reason
    else {
        panic!(
            "deferred conditional target with false-branch failure should yield ConditionalBranchMismatch, got {reason:?}"
        );
    };
    assert_eq!(
        branch_target, forty_two,
        "true branch (\"yes\") accepts the source so explain must surface the false branch (42)"
    );
    assert!(
        matches!(*nested_reason, SubtypeFailureReason::TypeMismatch { .. }
            | SubtypeFailureReason::LiteralTypeMismatch { .. }
            | SubtypeFailureReason::IntrinsicTypeMismatch { .. }),
        "nested reason should preserve a structural mismatch, got {nested_reason:?}"
    );
}

/// Deferred-conditional **source**: `(T extends U ? X : Y) <: T'`.
/// Both branches must be `<:` the concrete target. Exercise the path with the
/// true branch failing.
#[test]
fn test_explain_source_conditional_true_branch_mismatch_preserves_chain() {
    let interner = TypeInterner::new();

    // T extends string ? boolean : "no"
    // Target = "no" — false branch matches, but true branch (boolean) fails.
    let t_param = interner.type_param(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });
    let no = interner.literal_string("no");
    let cond = interner.conditional(ConditionalType {
        check_type: t_param,
        extends_type: TypeId::STRING,
        true_type: TypeId::BOOLEAN,
        false_type: no,
        is_distributive: true,
    });

    let target = no;
    let mut checker = SubtypeChecker::new(&interner);

    assert!(
        !checker.is_subtype_of(cond, target),
        "deferred conditional with boolean true branch should not be assignable to \"no\""
    );

    let reason = checker.explain_failure(cond, target);
    let Some(SubtypeFailureReason::ConditionalBranchMismatch {
        source_type,
        target_type,
        branch_source,
        branch_target,
        nested_reason,
    }) = reason
    else {
        panic!(
            "deferred conditional source with branch failure should yield ConditionalBranchMismatch, got {reason:?}"
        );
    };
    assert_eq!(source_type, cond);
    assert_eq!(target_type, target);
    assert_eq!(
        branch_source, TypeId::BOOLEAN,
        "true branch (boolean) is the failing branch source"
    );
    assert_eq!(branch_target, target, "concrete target carried into branch relation");
    assert!(
        matches!(*nested_reason, SubtypeFailureReason::TypeMismatch { .. }
            | SubtypeFailureReason::IntrinsicTypeMismatch { .. }
            | SubtypeFailureReason::LiteralTypeMismatch { .. }),
        "nested reason should preserve the structural mismatch on the failing branch, got {nested_reason:?}"
    );
}

/// Branch identity must be structural, not name-based: renaming the type
/// parameter from `T` to `K` should not change the failure shape.
/// Hardening per CLAUDE.md §25 (anti-hardcoding directive).
#[test]
fn test_explain_conditional_branch_identity_is_name_independent() {
    fn build_failure(param_name: &str) -> (TypeId, SubtypeFailureReason) {
        let interner = TypeInterner::new();
        let t_param = interner.type_param(TypeParamInfo {
            name: interner.intern_string(param_name),
            constraint: None,
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        });
        let yes = interner.literal_string("yes");
        let cond = interner.conditional(ConditionalType {
            check_type: t_param,
            extends_type: TypeId::STRING,
            true_type: yes,
            false_type: interner.literal_string("no"),
            is_distributive: true,
        });
        let source = interner.literal_string("x");
        let mut checker = SubtypeChecker::new(&interner);
        let reason = checker
            .explain_failure(source, cond)
            .expect("relation fails so explain must produce a reason");
        (yes, reason)
    }

    fn unwrap_branch(reason: SubtypeFailureReason) -> (TypeId, Box<SubtypeFailureReason>) {
        match reason {
            SubtypeFailureReason::ConditionalBranchMismatch {
                branch_target,
                nested_reason,
                ..
            } => (branch_target, nested_reason),
            other => panic!("expected ConditionalBranchMismatch, got {other:?}"),
        }
    }

    let (t_yes, t_reason) = build_failure("T");
    let (k_yes, k_reason) = build_failure("K");
    let (t_branch_target, t_nested) = unwrap_branch(t_reason);
    let (k_branch_target, k_nested) = unwrap_branch(k_reason);

    // The failing branch must be the true branch in both cases — derived from
    // structure (`branch_target == true_type`), not param name.
    assert_eq!(
        t_branch_target, t_yes,
        "branch direction must be derived from structure (true branch), not param name"
    );
    assert_eq!(
        k_branch_target, k_yes,
        "renaming T -> K must not change the failing-branch identity"
    );
    assert!(
        matches!(*t_nested, SubtypeFailureReason::LiteralTypeMismatch { .. })
            && matches!(*k_nested, SubtypeFailureReason::LiteralTypeMismatch { .. }),
        "nested reason kind must be identical for renamed type parameters"
    );
}
