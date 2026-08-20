//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/type_queries/flow/comparability.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 6db0726889cd05895d6c225e4f99eede5e387d7851e9c9b4a5a04ff24956eed5 1162 comparability_depth_state_names_cap_boundary
    #[test]
    fn comparability_depth_state_names_cap_boundary() {
        assert_eq!(
            ComparabilityDepthState::from_depth(MAX_COMPARABILITY_DEPTH),
            ComparabilityDepthState::WithinLimit
        );
        assert_eq!(
            ComparabilityDepthState::from_depth(MAX_COMPARABILITY_DEPTH + 1),
            ComparabilityDepthState::LimitExceeded
        );
        assert_eq!(
            ComparabilityDepthState::LimitExceeded.fallback_result(),
            Some(false)
        );
    }
// TSZ_INLINE_TEST_END 6db0726889cd05895d6c225e4f99eede5e387d7851e9c9b4a5a04ff24956eed5

// TSZ_INLINE_TEST_BEGIN 25f3825bb03b5ac6e0f09ce211696bd94ab4c962169ff8079d40562cd21dbe5e 1178 comparability_depth_state_preserves_strict_and_assertion_fallback
    #[test]
    fn comparability_depth_state_preserves_strict_and_assertion_fallback() {
        let interner = TypeInterner::new();

        assert!(types_are_comparable_inner(
            &interner,
            TypeId::STRING,
            TypeId::STRING,
            MAX_COMPARABILITY_DEPTH
        ));
        assert!(!types_are_comparable_inner(
            &interner,
            TypeId::STRING,
            TypeId::STRING,
            MAX_COMPARABILITY_DEPTH + 1
        ));
        assert!(types_are_comparable_for_assertion_inner(
            &interner,
            TypeId::STRING,
            TypeId::STRING,
            MAX_COMPARABILITY_DEPTH,
            false
        ));
        assert!(!types_are_comparable_for_assertion_inner(
            &interner,
            TypeId::STRING,
            TypeId::STRING,
            MAX_COMPARABILITY_DEPTH + 1,
            false
        ));
    }
// TSZ_INLINE_TEST_END 25f3825bb03b5ac6e0f09ce211696bd94ab4c962169ff8079d40562cd21dbe5e
