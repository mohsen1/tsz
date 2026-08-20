//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/evaluation/evaluate_rules/index_access_union_distribution.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN f43772972d3a9989136ee8755f91f03e7a3285833381a33d4ab1f8aa5a22a359 121 union_index_size_state_names_exact_cap_and_overflow
    #[test]
    fn union_index_size_state_names_exact_cap_and_overflow() {
        assert_eq!(
            UnionIndexSizeState::for_member_count(MAX_UNION_INDEX_SIZE),
            UnionIndexSizeState::Continue
        );
        assert_eq!(
            UnionIndexSizeState::for_member_count(MAX_UNION_INDEX_SIZE + 1),
            UnionIndexSizeState::LimitExceeded
        );
    }
// TSZ_INLINE_TEST_END f43772972d3a9989136ee8755f91f03e7a3285833381a33d4ab1f8aa5a22a359
