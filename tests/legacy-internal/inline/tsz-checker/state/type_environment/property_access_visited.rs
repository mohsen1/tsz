//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/state/type_environment/property_access_visited.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 2e090bf8d733862770049b1ba7ebe1fa69e06f9369cfdfc3a7e46037294dc597 45 rollback_removes_branch_insertions
    #[test]
    fn rollback_removes_branch_insertions() {
        let mut visited = PropertyAccessVisited::default();
        assert!(visited.insert(TypeId(1)));

        let checkpoint = visited.checkpoint();
        assert!(visited.insert(TypeId(2)));
        assert!(!visited.insert(TypeId(2)));

        visited.rollback_to(checkpoint);

        assert!(visited.insert(TypeId(2)));
    }
// TSZ_INLINE_TEST_END 2e090bf8d733862770049b1ba7ebe1fa69e06f9369cfdfc3a7e46037294dc597

// TSZ_INLINE_TEST_BEGIN 84d07078039719f4a731813435407c3431f4a5cb1595f5851fe3c33993a5766d 59 rollback_preserves_ancestor_insertions
    #[test]
    fn rollback_preserves_ancestor_insertions() {
        let mut visited = PropertyAccessVisited::default();
        assert!(visited.insert(TypeId(1)));

        let checkpoint = visited.checkpoint();
        assert!(visited.insert(TypeId(2)));
        visited.rollback_to(checkpoint);

        assert!(!visited.insert(TypeId(1)));
    }
// TSZ_INLINE_TEST_END 84d07078039719f4a731813435407c3431f4a5cb1595f5851fe3c33993a5766d
