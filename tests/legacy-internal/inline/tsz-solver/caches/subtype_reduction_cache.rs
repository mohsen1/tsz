//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/caches/subtype_reduction_cache.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 8501dc15067f7b3917a42c68f3dce5e649e0e4ab23214c4e867abaa0e4fbf990 277 empty_cache_misses
    #[test]
    fn empty_cache_misses() {
        let cache = SubtypeReductionCache::new();
        let key = cache_key_for_nominal_hierarchy(&[1, 2], false);
        assert!(cache.lookup(&key).is_none());
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
    }
// TSZ_INLINE_TEST_END 8501dc15067f7b3917a42c68f3dce5e649e0e4ab23214c4e867abaa0e4fbf990

// TSZ_INLINE_TEST_BEGIN 74203bae78bbcefc340226d3642f1d12730bd4a563ad29229c420198b917d124 286 insert_then_lookup_roundtrip
    #[test]
    fn insert_then_lookup_roundtrip() {
        let cache = SubtypeReductionCache::new();
        let key = cache_key_for_nominal_hierarchy(&[1, 2], false);
        let value = arc_slice(&[1, 2]);
        cache.insert(key.clone(), value.clone());
        let got = cache.lookup(&key).expect("hit");
        assert_eq!(&got[..], &value[..]);
        assert_eq!(cache.len(), 1);
    }
// TSZ_INLINE_TEST_END 74203bae78bbcefc340226d3642f1d12730bd4a563ad29229c420198b917d124

// TSZ_INLINE_TEST_BEGIN 7d207f0360ba14dad2e4398c85fdbb3c7b8797dbbcf56d1e3536cbecc1829aa2 297 order_independence_of_input_slice
    #[test]
    fn order_independence_of_input_slice() {
        // Two slices with the same set of TypeIds in different orders must
        // hash to the same cache slot — that's the whole point of the
        // sorted-key form (mirrors tsc's getTypeListId).
        let cache = SubtypeReductionCache::new();
        let k_ab = cache_key_for_nominal_hierarchy(&[3, 1, 2], false);
        let k_ba = cache_key_for_nominal_hierarchy(&[1, 2, 3], false);
        assert_eq!(k_ab, k_ba);
        cache.insert(k_ab, arc_slice(&[1, 2, 3]));
        assert!(cache.lookup(&k_ba).is_some());
        assert_eq!(cache.len(), 1);
    }
// TSZ_INLINE_TEST_END 7d207f0360ba14dad2e4398c85fdbb3c7b8797dbbcf56d1e3536cbecc1829aa2

// TSZ_INLINE_TEST_BEGIN d7fe4c38b9b7fa68292fa55780cc597fb0532b5e03d932feabd2a5c0d78fe063 311 distinct_lists_do_not_alias
    #[test]
    fn distinct_lists_do_not_alias() {
        // {1, 2} and {1, 3} must produce distinct cache entries even
        // though they share an element.
        let cache = SubtypeReductionCache::new();
        let k_12 = cache_key_for_nominal_hierarchy(&[1, 2], false);
        let k_13 = cache_key_for_nominal_hierarchy(&[1, 3], false);
        assert_ne!(k_12, k_13);
        cache.insert(k_12.clone(), arc_slice(&[1, 2]));
        cache.insert(k_13.clone(), arc_slice(&[1, 3]));
        let v_12 = cache.lookup(&k_12).expect("hit");
        let v_13 = cache.lookup(&k_13).expect("hit");
        assert_eq!(&v_12[..], &[type_id(1), type_id(2)]);
        assert_eq!(&v_13[..], &[type_id(1), type_id(3)]);
        assert_eq!(cache.len(), 2);
    }
// TSZ_INLINE_TEST_END d7fe4c38b9b7fa68292fa55780cc597fb0532b5e03d932feabd2a5c0d78fe063

// TSZ_INLINE_TEST_BEGIN 59ca2e9781d6db7d9900bd7212f1b4208e239a0bd0a08e3cecbd976fe7ca3f65 328 mode_bits_isolate_nominal_hierarchy_resolution_from_default
    #[test]
    fn mode_bits_isolate_nominal_hierarchy_resolution_from_default() {
        // Same TypeIds, different nominal-hierarchy-resolution flag →
        // distinct entries. This guards against caching a structural-only
        // result and serving it when class-hierarchy resolution is enabled
        // (which can change the outcome).
        let cache = SubtypeReductionCache::new();
        let default = cache_key_for_nominal_hierarchy(&[1, 2], false);
        let nominal = cache_key_for_nominal_hierarchy(&[1, 2], true);
        assert_ne!(default, nominal);
        assert_eq!(default.mode_bits, 0);
        assert_eq!(nominal.mode_bits, MODE_NOMINAL_HIERARCHY_RESOLUTION);
        cache.insert(default.clone(), arc_slice(&[1, 2]));
        cache.insert(nominal.clone(), arc_slice(&[1]));
        assert_eq!(
            &cache.lookup(&default).expect("default entry was inserted")[..],
            &[type_id(1), type_id(2)]
        );
        assert_eq!(
            &cache.lookup(&nominal).expect("nominal entry was inserted")[..],
            &[type_id(1)]
        );
        assert_eq!(cache.len(), 2);
    }
// TSZ_INLINE_TEST_END 59ca2e9781d6db7d9900bd7212f1b4208e239a0bd0a08e3cecbd976fe7ca3f65

// TSZ_INLINE_TEST_BEGIN c0ba0530558a828df137d855f20254c279388c1ce17404d68f3696e4a1d21757 353 clear_empties_cache
    #[test]
    fn clear_empties_cache() {
        let cache = SubtypeReductionCache::new();
        let key = cache_key_for_nominal_hierarchy(&[7], false);
        cache.insert(key.clone(), arc_slice(&[7]));
        assert_eq!(cache.len(), 1);
        cache.clear();
        assert!(cache.is_empty());
        assert!(cache.lookup(&key).is_none());
    }
// TSZ_INLINE_TEST_END c0ba0530558a828df137d855f20254c279388c1ce17404d68f3696e4a1d21757

// TSZ_INLINE_TEST_BEGIN ed881724b6d9531d48668886747a65b1ed6e4bbe710f2b67ccf04226dba47001 364 sorted_type_ids_helpers
    #[test]
    fn sorted_type_ids_helpers() {
        let s = SortedTypeIds::from_slice(&[type_id(3), type_id(1), type_id(2)]);
        assert_eq!(s.len(), 3);
        assert!(!s.is_empty());
        assert_eq!(s.as_slice(), &[type_id(1), type_id(2), type_id(3)]);
    }
// TSZ_INLINE_TEST_END ed881724b6d9531d48668886747a65b1ed6e4bbe710f2b67ccf04226dba47001

// TSZ_INLINE_TEST_BEGIN 685fe9d99da2d65a1e85d56699c1f51333b8b026c6a4d3a77c69c5f0a7411c47 372 request_cache_key_owns_option_packing
    #[test]
    fn request_cache_key_owns_option_packing() {
        let types = [type_id(9), type_id(4), type_id(7)];
        let request = SubtypeReductionRequest::new(&types).with_nominal_hierarchy_resolution(true);
        let key = request.cache_key();

        assert_eq!(request.types(), &types);
        assert_eq!(
            key.sorted_type_ids.as_slice(),
            &[type_id(4), type_id(7), type_id(9)]
        );
        assert_eq!(key.mode_bits, MODE_NOMINAL_HIERARCHY_RESOLUTION);
    }
// TSZ_INLINE_TEST_END 685fe9d99da2d65a1e85d56699c1f51333b8b026c6a4d3a77c69c5f0a7411c47

// TSZ_INLINE_TEST_BEGIN dfbf2be838a3b7bd041b9b63a107c43a0e2b19a506135141c7f3a2033225af17 386 default_options_use_zero_mode_bits
    #[test]
    fn default_options_use_zero_mode_bits() {
        assert_eq!(SubtypeReductionOptions::new().mode_bits(), 0);
        assert_eq!(
            SubtypeReductionOptions::new()
                .with_nominal_hierarchy_resolution(true)
                .mode_bits(),
            MODE_NOMINAL_HIERARCHY_RESOLUTION
        );
    }
// TSZ_INLINE_TEST_END dfbf2be838a3b7bd041b9b63a107c43a0e2b19a506135141c7f3a2033225af17
