//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/relations/freshness.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN a43ec6e3e8584b51e8be1aa7625ea7982f80b42266462b58ed63f1a2fda2e590 117 freshness_widen_depth_state_continues_at_limit
    #[test]
    fn freshness_widen_depth_state_continues_at_limit() {
        assert_eq!(
            FreshnessWidenDepthState::from_depth(MAX_FRESHNESS_WIDEN_DEPTH),
            FreshnessWidenDepthState::Continue,
        );
    }
// TSZ_INLINE_TEST_END a43ec6e3e8584b51e8be1aa7625ea7982f80b42266462b58ed63f1a2fda2e590

// TSZ_INLINE_TEST_BEGIN 85194723afda5d5057d71eb5e1f8e5f278de143e64178aa6025499141c903649 125 freshness_widen_depth_state_limits_past_limit
    #[test]
    fn freshness_widen_depth_state_limits_past_limit() {
        assert_eq!(
            FreshnessWidenDepthState::from_depth(MAX_FRESHNESS_WIDEN_DEPTH + 1),
            FreshnessWidenDepthState::LimitExceeded,
        );
    }
// TSZ_INLINE_TEST_END 85194723afda5d5057d71eb5e1f8e5f278de143e64178aa6025499141c903649

// TSZ_INLINE_TEST_BEGIN 203f6269059ee488f31c0b1df3022ba28eaca4b03ae1ecc3bd5c8fa1cc50b645 133 widen_freshness_widens_nested_leaf_at_exact_limit
    #[test]
    fn widen_freshness_widens_nested_leaf_at_exact_limit() {
        let interner = TypeInterner::new();
        let nested = nested_fresh_object(&interner, MAX_FRESHNESS_WIDEN_DEPTH);
        let widened = widen_freshness(&interner, nested);
        let leaf = nested_property_leaf(&interner, widened, MAX_FRESHNESS_WIDEN_DEPTH);

        assert!(!is_fresh_object_type(&interner, leaf));
    }
// TSZ_INLINE_TEST_END 203f6269059ee488f31c0b1df3022ba28eaca4b03ae1ecc3bd5c8fa1cc50b645

// TSZ_INLINE_TEST_BEGIN dea7f427eac46165dc51d3d8e5cf207c229ee8ae13bc9c6e5cc096af0e6fd776 143 widen_freshness_preserves_nested_leaf_past_limit
    #[test]
    fn widen_freshness_preserves_nested_leaf_past_limit() {
        let interner = TypeInterner::new();
        let nested = nested_fresh_object(&interner, MAX_FRESHNESS_WIDEN_DEPTH + 1);
        let widened = widen_freshness(&interner, nested);
        let leaf = nested_property_leaf(&interner, widened, MAX_FRESHNESS_WIDEN_DEPTH + 1);

        assert!(is_fresh_object_type(&interner, leaf));
    }
// TSZ_INLINE_TEST_END dea7f427eac46165dc51d3d8e5cf207c229ee8ae13bc9c6e5cc096af0e6fd776
