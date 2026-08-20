//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/evaluation/evaluate_rules/infer_match_expansion.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 3180b9bcab5ee3ee24e2840ef0f8f9ff734b2eba3964b934ae627622ff1bee18 331 infer_match_expansion_state_limits_at_budget
    #[test]
    fn infer_match_expansion_state_limits_at_budget() {
        let session = EvaluationSession::new();
        let mut held = Vec::new();
        for expected_prev in 0..InferMatchExpansionDepthEntry::limit() {
            let entry = session
                .enter_infer_match_expansion_depth()
                .expect("enter within budget must succeed");
            assert_eq!(entry.prior_depth(), expected_prev);
            held.push(entry);
            assert_eq!(session.infer_match_expansion_depth(), expected_prev + 1);
        }

        assert!(
            matches!(
                session.enter_infer_match_expansion_depth(),
                Err(InferMatchExpansionDepthState::LimitExceeded)
            ),
            "enter at the budget must be denied so the caller stops expanding"
        );

        held.clear();
        assert_eq!(session.infer_match_expansion_depth(), 0);
        assert!(
            session.enter_infer_match_expansion_depth().is_ok(),
            "after unwinding, a fresh expansion must be allowed again"
        );
    }
// TSZ_INLINE_TEST_END 3180b9bcab5ee3ee24e2840ef0f8f9ff734b2eba3964b934ae627622ff1bee18

// TSZ_INLINE_TEST_BEGIN e5634ce84a41b21ea1e9965c52a79b48b55e868571ef02077af80bdb68396c56 360 infer_match_expansion_depth_is_session_local
    #[test]
    fn infer_match_expansion_depth_is_session_local() {
        let saturated = EvaluationSession::new();
        let separate = EvaluationSession::new();
        let mut held = Vec::new();
        for _ in 0..InferMatchExpansionDepthEntry::limit() {
            held.push(
                saturated
                    .enter_infer_match_expansion_depth()
                    .expect("saturating session enters"),
            );
        }

        assert!(matches!(
            saturated.enter_infer_match_expansion_depth(),
            Err(InferMatchExpansionDepthState::LimitExceeded)
        ));
        assert!(
            separate.enter_infer_match_expansion_depth().is_ok(),
            "a saturated infer-match depth in one session must not block another session"
        );
    }
// TSZ_INLINE_TEST_END e5634ce84a41b21ea1e9965c52a79b48b55e868571ef02077af80bdb68396c56
