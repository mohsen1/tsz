//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/type_queries/data/accessors.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 096689142f703346422109d849bcc6ae143c4ee75328864dd1110ca0f1e8b3f9 1660 readonly_unwrap_depth_allows_exact_cap
    #[test]
    fn readonly_unwrap_depth_allows_exact_cap() {
        assert_eq!(
            readonly_unwrap_depth_state(MAX_READONLY_UNWRAP_DEPTH),
            ReadonlyUnwrapDepthState::Continue
        );
    }
// TSZ_INLINE_TEST_END 096689142f703346422109d849bcc6ae143c4ee75328864dd1110ca0f1e8b3f9

// TSZ_INLINE_TEST_BEGIN e5a9f00d6519165c07ad3e22d8df304cff3d92d551a64b94814a45182715a574 1668 readonly_unwrap_depth_limits_past_cap
    #[test]
    fn readonly_unwrap_depth_limits_past_cap() {
        assert_eq!(
            readonly_unwrap_depth_state(MAX_READONLY_UNWRAP_DEPTH + 1),
            ReadonlyUnwrapDepthState::LimitExceeded
        );
    }
// TSZ_INLINE_TEST_END e5a9f00d6519165c07ad3e22d8df304cff3d92d551a64b94814a45182715a574

// TSZ_INLINE_TEST_BEGIN c8e65bbfe473323bdbdb1ba0c23c01c4c04eba5a9ab74fd513887741be4f9907 1676 unwrap_readonly_deep_unwraps_exact_cap
    #[test]
    fn unwrap_readonly_deep_unwraps_exact_cap() {
        let interner = TypeInterner::new();
        let nested = raw_readonly_chain(&interner, TypeId::STRING, MAX_READONLY_UNWRAP_DEPTH);

        assert_eq!(unwrap_readonly_deep(&interner, nested), TypeId::STRING);
    }
// TSZ_INLINE_TEST_END c8e65bbfe473323bdbdb1ba0c23c01c4c04eba5a9ab74fd513887741be4f9907

// TSZ_INLINE_TEST_BEGIN 1a4023eae415d75ec3363d6a9e4dca994f3d53d156652f7f2431870ffc6a8649 1684 unwrap_readonly_deep_preserves_wrapper_past_cap
    #[test]
    fn unwrap_readonly_deep_preserves_wrapper_past_cap() {
        let interner = TypeInterner::new();
        let nested = raw_readonly_chain(&interner, TypeId::STRING, MAX_READONLY_UNWRAP_DEPTH + 1);
        let unwrapped = unwrap_readonly_deep(&interner, nested);

        assert!(
            matches!(
                interner.lookup(unwrapped),
                Some(TypeData::ReadonlyType(inner)) if inner == TypeId::STRING
            ),
            "past the cap, the previous opaque wrapper is preserved"
        );
    }
// TSZ_INLINE_TEST_END 1a4023eae415d75ec3363d6a9e4dca994f3d53d156652f7f2431870ffc6a8649
