//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/relations/subtype/explain_guard.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN b262886c3b9fbbb50ae421b7114292dd8f0b46e48d567016730218683811c6b3 80 fuel_state_reports_exhausted_only_at_zero
    #[test]
    fn fuel_state_reports_exhausted_only_at_zero() {
        assert_eq!(ExplainFuelState::from_fuel(None), ExplainFuelState::Ready);
        assert_eq!(
            ExplainFuelState::from_fuel(Some(1)),
            ExplainFuelState::Ready
        );
        assert_eq!(
            ExplainFuelState::from_fuel(Some(0)),
            ExplainFuelState::Exhausted
        );
    }
// TSZ_INLINE_TEST_END b262886c3b9fbbb50ae421b7114292dd8f0b46e48d567016730218683811c6b3

// TSZ_INLINE_TEST_BEGIN 24c04af83a1989dc92d8700407054c63cf81bc8625c458854d5d8a3c501e4ace 93 exhausted_fuel_builds_type_mismatch_fallback
    #[test]
    fn exhausted_fuel_builds_type_mismatch_fallback() {
        assert!(
            ExplainFuelState::Ready
                .fallback_reason(TypeId::STRING, TypeId::NUMBER)
                .is_none()
        );
        let reason = ExplainFuelState::Exhausted
            .fallback_reason(TypeId::STRING, TypeId::NUMBER)
            .expect("exhausted fuel should produce a coarse fallback");

        assert!(matches!(
            reason,
            crate::diagnostics::SubtypeFailureReason::TypeMismatch {
                source_type: TypeId::STRING,
                target_type: TypeId::NUMBER,
            }
        ));
    }
// TSZ_INLINE_TEST_END 24c04af83a1989dc92d8700407054c63cf81bc8625c458854d5d8a3c501e4ace

// TSZ_INLINE_TEST_BEGIN a88013ae32309313c93e0e6396ae6de907907f07bb9cb0ebf46c1f0fdc8c9dbc 113 recursion_entry_state_preserves_entered_vs_fallback
    #[test]
    fn recursion_entry_state_preserves_entered_vs_fallback() {
        assert_eq!(
            ExplainRecursionEntryState::from_recursion_result(RecursionResult::Entered),
            ExplainRecursionEntryState::Entered
        );
        for denied in [
            RecursionResult::Cycle,
            RecursionResult::DepthExceeded,
            RecursionResult::IterationExceeded,
        ] {
            assert_eq!(
                ExplainRecursionEntryState::from_recursion_result(denied),
                ExplainRecursionEntryState::Fallback
            );
        }
    }
// TSZ_INLINE_TEST_END a88013ae32309313c93e0e6396ae6de907907f07bb9cb0ebf46c1f0fdc8c9dbc

// TSZ_INLINE_TEST_BEGIN 1407108a70ac6e3f35757a03eb7f425daa575a350fc2235d590d18dd098da943 131 explain_funnel_uses_named_guard_states
    #[test]
    fn explain_funnel_uses_named_guard_states() {
        let explain_rs = include_str!("explain.rs");

        assert!(explain_rs.contains("ExplainFuelState::from_fuel"));
        assert!(explain_rs.contains("ExplainRecursionEntryState::from_recursion_result"));
        assert!(!explain_rs.contains("explain_eval_fuel == Some(0)"));
        assert!(!explain_rs.contains("RecursionResult::Cycle"));
    }
// TSZ_INLINE_TEST_END 1407108a70ac6e3f35757a03eb7f425daa575a350fc2235d590d18dd098da943
