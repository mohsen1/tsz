//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/evaluation/evaluate_rules/mapped/key_extraction.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 2b5a91f2dc8b4c39cb9a0911e1f4ddbcb7ea302bc60911219715bcaf88bc890f 787 template_source_depth_state_continues_at_limit
    #[test]
    fn template_source_depth_state_continues_at_limit() {
        assert_eq!(
            TemplateSourceDepthState::from_depth(16, 16),
            TemplateSourceDepthState::Continue
        );
    }
// TSZ_INLINE_TEST_END 2b5a91f2dc8b4c39cb9a0911e1f4ddbcb7ea302bc60911219715bcaf88bc890f

// TSZ_INLINE_TEST_BEGIN 210bbaab56ac9fce7ce1e661347b292827c2d9893efaab413be1cb01bf7746b4 795 template_source_depth_state_stops_past_limit
    #[test]
    fn template_source_depth_state_stops_past_limit() {
        assert_eq!(
            TemplateSourceDepthState::from_depth(17, 16),
            TemplateSourceDepthState::LimitExceeded
        );
    }
// TSZ_INLINE_TEST_END 210bbaab56ac9fce7ce1e661347b292827c2d9893efaab413be1cb01bf7746b4
