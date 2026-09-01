//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-common/src/module_resolution/mod.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 3a53ef94263824bd986a873ba5a164838e789185efe0ffcd639688e8502d3b71 88 map_only_transforms_the_resolved_payload
    #[test]
    fn map_only_transforms_the_resolved_payload() {
        assert_eq!(
            TargetMatch::Resolved(2).map(|n| n * 3),
            TargetMatch::Resolved(6)
        );
        assert_eq!(
            TargetMatch::<i32>::Blocked.map(|n| n * 3),
            TargetMatch::Blocked
        );
        assert_eq!(
            TargetMatch::<i32>::NotApplicable.map(|n| n * 3),
            TargetMatch::NotApplicable
        );
    }
// TSZ_INLINE_TEST_END 3a53ef94263824bd986a873ba5a164838e789185efe0ffcd639688e8502d3b71

// TSZ_INLINE_TEST_BEGIN 605f1bfe987d5f123c3a58b7b978fbe8d415d14b1eee182b40e5af56a0c97a4e 104 is_blocked_and_into_option_distinguish_block_from_miss
    #[test]
    fn is_blocked_and_into_option_distinguish_block_from_miss() {
        assert!(TargetMatch::<i32>::Blocked.is_blocked());
        assert!(!TargetMatch::Resolved(1).is_blocked());
        assert!(!TargetMatch::<i32>::NotApplicable.is_blocked());

        // `into_option` deliberately collapses the block/miss distinction.
        assert_eq!(TargetMatch::Resolved(7).into_option(), Some(7));
        assert_eq!(TargetMatch::<i32>::Blocked.into_option(), None);
        assert_eq!(TargetMatch::<i32>::NotApplicable.into_option(), None);
    }
// TSZ_INLINE_TEST_END 605f1bfe987d5f123c3a58b7b978fbe8d415d14b1eee182b40e5af56a0c97a4e
