//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/context/env_eval_cache.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN c65ff5620fbe3fb3e18efc639e9ff93d3af979a9a5ed75aa298296523f38326d 729 contextual_signature_normalization_cache_invalidates_by_def_dependency
    #[test]
    fn contextual_signature_normalization_cache_invalidates_by_def_dependency() {
        let mut cache = EnvEvalCache::default();
        cache.insert_contextual_signature_normalization(
            TypeId(10),
            stamp(1),
            TypeId(20),
            deps(&[DefId(7)]),
        );

        assert_eq!(
            cache.get_contextual_signature_normalization(TypeId(10), stamp(1)),
            Some(TypeId(20))
        );
        cache.invalidate_for_def(DefId(8));
        assert_eq!(
            cache.get_contextual_signature_normalization(TypeId(10), stamp(1)),
            Some(TypeId(20))
        );

        cache.invalidate_for_def(DefId(7));
        assert_eq!(
            cache.get_contextual_signature_normalization(TypeId(10), stamp(1)),
            None
        );
    }
// TSZ_INLINE_TEST_END c65ff5620fbe3fb3e18efc639e9ff93d3af979a9a5ed75aa298296523f38326d

// TSZ_INLINE_TEST_BEGIN ebf92c4011a6ef27739f26622d53f9e6521f85e96b78e96efdba2f43e19930f5 756 contextual_signature_normalization_cache_serves_only_matching_stamp
    #[test]
    fn contextual_signature_normalization_cache_serves_only_matching_stamp() {
        let mut cache = EnvEvalCache::default();
        cache.insert_contextual_signature_normalization(
            TypeId(10),
            stamp(1),
            TypeId(20),
            deps(&[]),
        );

        assert_eq!(
            cache.get_contextual_signature_normalization(TypeId(10), stamp(1)),
            Some(TypeId(20))
        );
        assert_eq!(
            cache.get_contextual_signature_normalization(TypeId(10), stamp(2)),
            None
        );
    }
// TSZ_INLINE_TEST_END ebf92c4011a6ef27739f26622d53f9e6521f85e96b78e96efdba2f43e19930f5

// TSZ_INLINE_TEST_BEGIN 26d90661ef51f3a1013495f5f01341ce27438664190d5985f2a96d884ae98b6d 776 contextual_signature_normalization_cache_invalidates_reachable_key_or_result
    #[test]
    fn contextual_signature_normalization_cache_invalidates_reachable_key_or_result() {
        let mut cache = EnvEvalCache::default();
        cache.insert_contextual_signature_normalization(
            TypeId(10),
            stamp(1),
            TypeId(20),
            deps(&[]),
        );
        cache.insert_contextual_signature_normalization(
            TypeId(30),
            stamp(1),
            TypeId(40),
            deps(&[]),
        );

        let removed =
            cache.invalidate_contextual_signature_normalizations_matching(|key, value| {
                key == TypeId(30) || value == TypeId(20)
            });

        assert_eq!(removed, 2);
        assert_eq!(cache.contextual_signature_normalization_len(), 0);
    }
// TSZ_INLINE_TEST_END 26d90661ef51f3a1013495f5f01341ce27438664190d5985f2a96d884ae98b6d

// TSZ_INLINE_TEST_BEGIN bedbe7306fa46c06ddc67e2afbe1c426096c98b9a040db6e9ac274e150017079 801 contextual_signature_normalization_cache_clear_drops_entries
    #[test]
    fn contextual_signature_normalization_cache_clear_drops_entries() {
        let mut cache = EnvEvalCache::default();
        cache.insert_contextual_signature_normalization(
            TypeId(10),
            stamp(1),
            TypeId(20),
            deps(&[DefId(7)]),
        );

        cache.clear();

        assert_eq!(
            cache.get_contextual_signature_normalization(TypeId(10), stamp(1)),
            None
        );
        assert_eq!(cache.contextual_signature_normalization_len(), 0);
    }
// TSZ_INLINE_TEST_END bedbe7306fa46c06ddc67e2afbe1c426096c98b9a040db6e9ac274e150017079
