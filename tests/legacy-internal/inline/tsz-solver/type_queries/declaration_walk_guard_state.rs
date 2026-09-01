//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/type_queries/declaration_walk_guard_state.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 2f1be7be7bbac37e5db552089651a87a9362bb73a343321b5590a2eff00ce399 24 declaration_walk_depth_state_names_limit_boundary
    #[test]
    fn declaration_walk_depth_state_names_limit_boundary() {
        assert_eq!(
            declaration_walk_depth_state(16, 16),
            DeclarationWalkDepthState::Continue
        );
        assert_eq!(
            declaration_walk_depth_state(17, 16),
            DeclarationWalkDepthState::DepthExceeded
        );
    }
// TSZ_INLINE_TEST_END 2f1be7be7bbac37e5db552089651a87a9362bb73a343321b5590a2eff00ce399
