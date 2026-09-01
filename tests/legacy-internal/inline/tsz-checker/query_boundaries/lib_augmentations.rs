//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/query_boundaries/lib_augmentations.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 5dbecfadca477c4776c7c42e2a703c777a5e73a888bdbef3b7866815e6611abc 82 thin_body_dropping_inherited_member_is_rejected
    #[test]
    fn thin_body_dropping_inherited_member_is_rejected() {
        let types = TypeInterner::new();
        // `SetIterator` heritage-complete: own `[Symbol.iterator]` + inherited
        // `next` (from `IteratorObject` -> `Iterator`).
        let complete = obj(&types, &["__@iterator", "next"]);
        // Heritage-thin re-derivation: dropped the inherited `next`.
        let thin = obj(&types, &["__@iterator"]);
        assert!(
            lib_body_strictly_loses_members(&types, complete, thin),
            "a thin body that drops an inherited member must be rejected",
        );
        // Completion in the other order (thin published first, complete arriving)
        // is a superset and must still win.
        assert!(
            !lib_body_strictly_loses_members(&types, thin, complete),
            "heritage completion (growing the member set) must be allowed",
        );
    }
// TSZ_INLINE_TEST_END 5dbecfadca477c4776c7c42e2a703c777a5e73a888bdbef3b7866815e6611abc

// TSZ_INLINE_TEST_BEGIN f57ebc45d23b188a7707b32aa95eeedf5c25d2099c423c45152a3ad53fc2b402 102 growth_via_augmentation_is_allowed
    #[test]
    fn growth_via_augmentation_is_allowed() {
        let types = TypeInterner::new();
        let base = obj(&types, &["a", "b"]);
        let augmented = obj(&types, &["a", "b", "c"]);
        assert!(!lib_body_strictly_loses_members(&types, base, augmented));
    }
// TSZ_INLINE_TEST_END f57ebc45d23b188a7707b32aa95eeedf5c25d2099c423c45152a3ad53fc2b402

// TSZ_INLINE_TEST_BEGIN 979081ded54b37e1dd557a3a92abb5680595524683bb8e95f2b4d64654c970ef 110 equal_member_set_is_not_a_loss
    #[test]
    fn equal_member_set_is_not_a_loss() {
        let types = TypeInterner::new();
        let a = obj(&types, &["a", "b"]);
        let b = obj(&types, &["a", "b"]);
        // Structurally identical objects intern to the same `TypeId`, so this
        // also exercises the `current == candidate` short-circuit.
        assert!(!lib_body_strictly_loses_members(&types, a, b));
    }
// TSZ_INLINE_TEST_END 979081ded54b37e1dd557a3a92abb5680595524683bb8e95f2b4d64654c970ef

// TSZ_INLINE_TEST_BEGIN 2f965e3b060f214bf0962aef6e3260772b8ce583834b12c353af4c3eb0a0ffca 120 added_and_dropped_member_same_size_is_not_a_loss
    #[test]
    fn added_and_dropped_member_same_size_is_not_a_loss() {
        let types = TypeInterner::new();
        let current = obj(&types, &["a", "b"]);
        // Same size, but adds `c` and drops `b`: not a strict subset, so it is
        // not a pure membership loss and replacement proceeds.
        let candidate = obj(&types, &["a", "c"]);
        assert!(!lib_body_strictly_loses_members(&types, current, candidate));
    }
// TSZ_INLINE_TEST_END 2f965e3b060f214bf0962aef6e3260772b8ce583834b12c353af4c3eb0a0ffca

// TSZ_INLINE_TEST_BEGIN af04eed12c523e25cf4a9ec6f46da0a3ab7717121b4f23738d40ba08d73b0758 130 non_object_bodies_allow_replacement
    #[test]
    fn non_object_bodies_allow_replacement() {
        let types = TypeInterner::new();
        let o = obj(&types, &["a"]);
        assert!(!lib_body_strictly_loses_members(&types, TypeId::NUMBER, o));
        assert!(!lib_body_strictly_loses_members(&types, o, TypeId::STRING));
    }
// TSZ_INLINE_TEST_END af04eed12c523e25cf4a9ec6f46da0a3ab7717121b4f23738d40ba08d73b0758
