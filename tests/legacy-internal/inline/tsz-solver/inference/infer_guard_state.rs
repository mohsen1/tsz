//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/inference/infer_guard_state.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN d6ccb30a4a30a5377872ae1049d08fbeeb24feed89e1407c988e18bc858c47d9 43 type_graph_visit_state_names_revisit_cutoff
    #[test]
    fn type_graph_visit_state_names_revisit_cutoff() {
        assert_eq!(type_graph_visit_state(true), TypeGraphVisitState::Entered);
        assert_eq!(
            type_graph_visit_state(false),
            TypeGraphVisitState::AlreadyVisited
        );
    }
// TSZ_INLINE_TEST_END d6ccb30a4a30a5377872ae1049d08fbeeb24feed89e1407c988e18bc858c47d9

// TSZ_INLINE_TEST_BEGIN 5e9ca1ede8efcb5322b5a3db25b563296d4a044edc56a6a44da66cae3c5eef0c 52 param_dependency_state_prioritizes_target_before_revisit
    #[test]
    fn param_dependency_state_prioritizes_target_before_revisit() {
        assert_eq!(
            param_dependency_state(true, false),
            ParamDependencyState::TargetReached
        );
        assert_eq!(
            param_dependency_state(false, true),
            ParamDependencyState::Entered
        );
        assert_eq!(
            param_dependency_state(false, false),
            ParamDependencyState::AlreadyVisited
        );
    }
// TSZ_INLINE_TEST_END 5e9ca1ede8efcb5322b5a3db25b563296d4a044edc56a6a44da66cae3c5eef0c
