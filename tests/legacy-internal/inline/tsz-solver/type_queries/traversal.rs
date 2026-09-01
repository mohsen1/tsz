//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/type_queries/traversal.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 3907348f97d0a4d4fa791f2d7c0136e6c76cf372f44ac75ff7ee8d9bb4801397 703 conditional_branch_alias_visit_state_names_ignored_sentinels
    #[test]
    fn conditional_branch_alias_visit_state_names_ignored_sentinels() {
        let mut visited = FxHashSet::default();

        assert_eq!(
            ConditionalBranchAliasVisitState::enter(TypeId::ERROR, false, &mut visited),
            ConditionalBranchAliasVisitState::IgnoredSentinel
        );
        assert_eq!(
            ConditionalBranchAliasVisitState::enter(TypeId::ANY, true, &mut visited),
            ConditionalBranchAliasVisitState::IgnoredSentinel
        );
        assert!(visited.is_empty());
    }
// TSZ_INLINE_TEST_END 3907348f97d0a4d4fa791f2d7c0136e6c76cf372f44ac75ff7ee8d9bb4801397

// TSZ_INLINE_TEST_BEGIN edf0d3e9a3876f7de764a0cd042f618ab23fb1d26009425c9990311971954223 718 conditional_branch_alias_visit_state_keys_on_branch_context
    #[test]
    fn conditional_branch_alias_visit_state_keys_on_branch_context() {
        let interner = TypeInterner::new();
        let type_id = interner.object(vec![]);
        let mut visited = FxHashSet::default();

        assert_eq!(
            ConditionalBranchAliasVisitState::enter(type_id, false, &mut visited),
            ConditionalBranchAliasVisitState::Entered
        );
        assert_eq!(
            ConditionalBranchAliasVisitState::enter(type_id, false, &mut visited),
            ConditionalBranchAliasVisitState::AlreadyVisited
        );
        assert_eq!(
            ConditionalBranchAliasVisitState::enter(type_id, true, &mut visited),
            ConditionalBranchAliasVisitState::Entered
        );
    }
// TSZ_INLINE_TEST_END edf0d3e9a3876f7de764a0cd042f618ab23fb1d26009425c9990311971954223

// TSZ_INLINE_TEST_BEGIN 8b97c7c48198b7a3b473d2cb0f873374140fc21093bf6c37b839a4ba80b5f9b6 738 exempt_application_is_not_evaluated_for_declaration_cycle_check
    #[test]
    fn exempt_application_is_not_evaluated_for_declaration_cycle_check() {
        let interner = TypeInterner::new();
        let def_id = DefId(1);
        let app = interner.application(interner.lazy(def_id), vec![TypeId::STRING]);
        let mut host = ExemptApplicationHost {
            exempt_def_id: def_id,
            evaluate_calls: 0,
            resolve_calls: 0,
        };

        assert!(!declaration_type_references_cyclic_structure(
            &interner, &mut host, app
        ));
        assert_eq!(host.evaluate_calls, 0);
        assert_eq!(host.resolve_calls, 0);
    }
// TSZ_INLINE_TEST_END 8b97c7c48198b7a3b473d2cb0f873374140fc21093bf6c37b839a4ba80b5f9b6
