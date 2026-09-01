//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/evaluation/evaluate_rules/infer_pattern_guard_state.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN e964f1537f8b0c1050be3f3bd5fd3c952dcd4c6be4a48fcb4a85f445d81403a6 79 entering_same_pair_reports_converged_revisit
    #[test]
    fn entering_same_pair_reports_converged_revisit() {
        let mut guard = InferPatternGuardState::default();

        assert_eq!(
            guard.enter_pair(TypeId::STRING, TypeId::UNKNOWN),
            InferPatternVisitDecision::Entered
        );
        assert_eq!(
            guard.enter_pair(TypeId::STRING, TypeId::UNKNOWN),
            InferPatternVisitDecision::RevisitedConverged
        );
    }
// TSZ_INLINE_TEST_END e964f1537f8b0c1050be3f3bd5fd3c952dcd4c6be4a48fcb4a85f445d81403a6

// TSZ_INLINE_TEST_BEGIN f92d0c159d8ff4cad993e78b8262641fd0a28a5725b7f9be0ef85f19f0bddc37 93 checkpoint_rollback_preserves_parent_entries
    #[test]
    fn checkpoint_rollback_preserves_parent_entries() {
        let mut guard = InferPatternGuardState::default();
        let parent = (TypeId::STRING, TypeId::UNKNOWN);
        let branch = (TypeId::NUMBER, TypeId::ANY);
        let sibling = (TypeId::BOOLEAN, TypeId::VOID);

        assert_eq!(
            guard.enter_pair(parent.0, parent.1),
            InferPatternVisitDecision::Entered
        );
        let checkpoint = guard.checkpoint();
        assert_eq!(
            guard.enter_pair(branch.0, branch.1),
            InferPatternVisitDecision::Entered
        );
        assert_eq!(
            guard.enter_pair(sibling.0, sibling.1),
            InferPatternVisitDecision::Entered
        );

        guard.rollback_to(checkpoint);

        assert!(guard.contains(&parent));
        assert!(!guard.contains(&branch));
        assert!(!guard.contains(&sibling));
        assert_eq!(
            guard.enter_pair(branch.0, branch.1),
            InferPatternVisitDecision::Entered
        );
    }
// TSZ_INLINE_TEST_END f92d0c159d8ff4cad993e78b8262641fd0a28a5725b7f9be0ef85f19f0bddc37

// TSZ_INLINE_TEST_BEGIN 2488be3c848117cc51bdc1659b3e74ae0ef102a2949e4577c2744e73cbe73d12 125 clear_resets_entries_and_log
    #[test]
    fn clear_resets_entries_and_log() {
        let mut guard = InferPatternGuardState::default();
        let pair = (TypeId::STRING, TypeId::UNKNOWN);

        assert_eq!(
            guard.enter_pair(pair.0, pair.1),
            InferPatternVisitDecision::Entered
        );
        guard.clear();

        assert!(!guard.contains(&pair));
        assert_eq!(
            guard.enter_pair(pair.0, pair.1),
            InferPatternVisitDecision::Entered
        );
    }
// TSZ_INLINE_TEST_END 2488be3c848117cc51bdc1659b3e74ae0ef102a2949e4577c2744e73cbe73d12
