//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/inference/infer_matching_guard_state.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN f42e80b1d5253e16f18d600fd96d9c177b7b21ac40bc4ac89616a8e4f94867ec 59 infer_match_entry_state_names_depth_and_revisit_cutoffs
    #[test]
    fn infer_match_entry_state_names_depth_and_revisit_cutoffs() {
        assert_eq!(
            infer_match_entry_state(20, 20, true),
            InferMatchEntryState::DepthExceeded
        );
        assert_eq!(
            infer_match_entry_state(0, 20, false),
            InferMatchEntryState::AlreadyVisited
        );
        assert_eq!(
            infer_match_entry_state(0, 20, true),
            InferMatchEntryState::Entered { depth: 1 }
        );
    }
// TSZ_INLINE_TEST_END f42e80b1d5253e16f18d600fd96d9c177b7b21ac40bc4ac89616a8e4f94867ec

// TSZ_INLINE_TEST_BEGIN ba851145a66a643c8f00994063a1783f9cde547fb2dc15c6fe54d1c8be06a875 75 app_expansion_state_names_depth_cutoff
    #[test]
    fn app_expansion_state_names_depth_cutoff() {
        assert_eq!(app_expansion_state(8, 8), AppExpansionState::DepthExceeded);
        assert_eq!(
            app_expansion_state(7, 8),
            AppExpansionState::Entered { depth: 8 }
        );
    }
// TSZ_INLINE_TEST_END ba851145a66a643c8f00994063a1783f9cde547fb2dc15c6fe54d1c8be06a875

// TSZ_INLINE_TEST_BEGIN 93f265bb20f2a4a8147e503d95b51a357c1b1fc05fb6ffd0b4dcebc7e9c37a24 84 target_param_visit_state_names_cycle_cutoff
    #[test]
    fn target_param_visit_state_names_cycle_cutoff() {
        assert_eq!(
            target_param_visit_state(true),
            TargetParamVisitState::Entered
        );
        assert_eq!(
            target_param_visit_state(false),
            TargetParamVisitState::AlreadyVisited { fallback: false }
        );
    }
// TSZ_INLINE_TEST_END 93f265bb20f2a4a8147e503d95b51a357c1b1fc05fb6ffd0b4dcebc7e9c37a24
