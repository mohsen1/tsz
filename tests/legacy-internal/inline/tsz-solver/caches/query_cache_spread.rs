//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/caches/query_cache_spread.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 923f2b6446856b07464cb2e03ae692d9746b34a3cdc5537bc48b8402b2875a5a 497 object_spread_visit_state_records_first_entry
    #[test]
    fn object_spread_visit_state_records_first_entry() {
        let mut traversal = ObjectSpreadTraversalState::default();

        let state = traversal.enter(TypeId::STRING);

        assert_eq!(state, ObjectSpreadVisitState::Entered);
        assert!(traversal.active.contains(&TypeId::STRING));
        assert!(traversal.is_cacheable());
    }
// TSZ_INLINE_TEST_END 923f2b6446856b07464cb2e03ae692d9746b34a3cdc5537bc48b8402b2875a5a

// TSZ_INLINE_TEST_BEGIN 82712b2919206e132f93c137c7af54b32e2ae344f732c5c3116fa40763010036 508 object_spread_visit_state_records_reentry
    #[test]
    fn object_spread_visit_state_records_reentry() {
        let mut traversal = ObjectSpreadTraversalState::default();

        assert_eq!(
            traversal.enter(TypeId::STRING),
            ObjectSpreadVisitState::Entered
        );
        assert_eq!(
            traversal.enter(TypeId::STRING),
            ObjectSpreadVisitState::AlreadyVisited
        );
        assert_eq!(traversal.active.len(), 1);
        assert!(!traversal.is_cacheable());
    }
// TSZ_INLINE_TEST_END 82712b2919206e132f93c137c7af54b32e2ae344f732c5c3116fa40763010036

// TSZ_INLINE_TEST_BEGIN 9a621a7f8779a27776a61eb2b5f3ea8fa8ca97673642a0bbda1a8a15a91563c7 524 object_spread_traversal_leave_allows_sibling_reentry
    #[test]
    fn object_spread_traversal_leave_allows_sibling_reentry() {
        let mut traversal = ObjectSpreadTraversalState::default();

        assert_eq!(
            traversal.enter(TypeId::STRING),
            ObjectSpreadVisitState::Entered
        );
        traversal.leave(TypeId::STRING);
        assert_eq!(
            traversal.enter(TypeId::STRING),
            ObjectSpreadVisitState::Entered
        );
        assert!(traversal.is_cacheable());
    }
// TSZ_INLINE_TEST_END 9a621a7f8779a27776a61eb2b5f3ea8fa8ca97673642a0bbda1a8a15a91563c7
