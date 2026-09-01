//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/visitors/visitor_predicates/identity_comparable.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN da63d0b19060a20b7324e1143997f3f1e633ecd41c73f3c61b309b3a4bb811da 70 identity_comparable_depth_state_names_exact_cap_and_limit
    #[test]
    fn identity_comparable_depth_state_names_exact_cap_and_limit() {
        assert_eq!(
            IdentityComparableDepthState::for_depth(MAX_IDENTITY_COMPARABLE_DEPTH),
            IdentityComparableDepthState::Continue
        );
        assert_eq!(
            IdentityComparableDepthState::for_depth(MAX_IDENTITY_COMPARABLE_DEPTH + 1),
            IdentityComparableDepthState::LimitExceeded
        );
    }
// TSZ_INLINE_TEST_END da63d0b19060a20b7324e1143997f3f1e633ecd41c73f3c61b309b3a4bb811da

// TSZ_INLINE_TEST_BEGIN b9f4467843c3bded17acc888dfb0782b5f181f5260cb9afa10f17f990b9c4637 82 identity_comparable_depth_limit_preserves_false_fallback
    #[test]
    fn identity_comparable_depth_limit_preserves_false_fallback() {
        let interner = TypeInterner::new();

        assert!(is_identity_comparable_type(&interner, TypeId::BOOLEAN_TRUE));
        assert!(!is_identity_comparable_type_impl(
            &interner,
            TypeId::BOOLEAN_TRUE,
            MAX_IDENTITY_COMPARABLE_DEPTH + 1
        ));
    }
// TSZ_INLINE_TEST_END b9f4467843c3bded17acc888dfb0782b5f181f5260cb9afa10f17f990b9c4637
