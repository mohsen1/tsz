//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/caches/instantiation_cache.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN b06b8d657bdc29ad02f078384b8dae797fc33d4820a1ae6476e002a094bb6646 274 test_cache_default_returns_none
    #[test]
    fn test_cache_default_returns_none() {
        // An empty cache must miss on every lookup.
        let cache = InstantiationCache::new();
        let key = InstantiationCacheKey::new(type_id(10), canonical(&[(1, 100)]), 0, None);
        assert_eq!(cache.lookup(&key), None);
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
    }
// TSZ_INLINE_TEST_END b06b8d657bdc29ad02f078384b8dae797fc33d4820a1ae6476e002a094bb6646

// TSZ_INLINE_TEST_BEGIN 8295e4fc60ad6effc6c6a26778267904ec925880a99f9ddbb9831e311c37c4e9 284 test_cache_insert_lookup_roundtrip
    #[test]
    fn test_cache_insert_lookup_roundtrip() {
        // Insert then lookup must return the inserted TypeId.
        let cache = InstantiationCache::new();
        let key = InstantiationCacheKey::new(type_id(10), canonical(&[(1, 100)]), 0, None);
        let result = type_id(200);
        cache.insert(key.clone(), result);
        assert_eq!(cache.lookup(&key), Some(result));
        assert_eq!(cache.len(), 1);
    }
// TSZ_INLINE_TEST_END 8295e4fc60ad6effc6c6a26778267904ec925880a99f9ddbb9831e311c37c4e9

// TSZ_INLINE_TEST_BEGIN 2234c261ab0e05ba4aebd0c813be89e4b5fc9f52d1fb269daca16e8a28b54d14 295 test_cache_distinct_keys_disjoint
    #[test]
    fn test_cache_distinct_keys_disjoint() {
        // Different mode_bits, different this_type, and different
        // CanonicalSubst values must each produce distinct cache entries.
        let cache = InstantiationCache::new();

        let base_subst = canonical(&[(1, 100)]);
        let other_subst = canonical(&[(2, 100)]);
        let same_subst_diff_type = canonical(&[(1, 101)]);

        // Distinct mode_bits.
        let k_mode_a = InstantiationCacheKey::new(type_id(10), base_subst.clone(), 0b000, None);
        let k_mode_b = InstantiationCacheKey::new(type_id(10), base_subst.clone(), 0b001, None);
        // Distinct this_type.
        let k_this_none = InstantiationCacheKey::new(type_id(10), base_subst.clone(), 0b000, None);
        let k_this_some =
            InstantiationCacheKey::new(type_id(10), base_subst.clone(), 0b000, Some(type_id(42)));
        // Distinct CanonicalSubst (different atom and different type_id).
        let k_subst_a = InstantiationCacheKey::new(type_id(10), base_subst, 0b000, None);
        let k_subst_b = InstantiationCacheKey::new(type_id(10), other_subst, 0b000, None);
        let k_subst_c = InstantiationCacheKey::new(type_id(10), same_subst_diff_type, 0b000, None);

        cache.insert(k_mode_a.clone(), type_id(1));
        cache.insert(k_mode_b.clone(), type_id(2));
        cache.insert(k_this_some.clone(), type_id(3));
        cache.insert(k_subst_b.clone(), type_id(4));
        cache.insert(k_subst_c.clone(), type_id(5));

        // k_mode_a == k_this_none == k_subst_a; that's the same slot, so the
        // insert above for k_mode_a populates all three.
        assert_eq!(cache.lookup(&k_mode_a), Some(type_id(1)));
        assert_eq!(cache.lookup(&k_this_none), Some(type_id(1)));
        assert_eq!(cache.lookup(&k_subst_a), Some(type_id(1)));

        // The other distinct keys must hold their own values.
        assert_eq!(cache.lookup(&k_mode_b), Some(type_id(2)));
        assert_eq!(cache.lookup(&k_this_some), Some(type_id(3)));
        assert_eq!(cache.lookup(&k_subst_b), Some(type_id(4)));
        assert_eq!(cache.lookup(&k_subst_c), Some(type_id(5)));

        // 5 distinct keys (the three k_*_a aliases collapse into one entry).
        assert_eq!(cache.len(), 5);
    }
// TSZ_INLINE_TEST_END 2234c261ab0e05ba4aebd0c813be89e4b5fc9f52d1fb269daca16e8a28b54d14

// TSZ_INLINE_TEST_BEGIN c1cd13b1d10a3c4306549b8f7d211ff9029178043ad3fc8d68134f7cf6d29b8f 339 test_cache_clear_empties
    #[test]
    fn test_cache_clear_empties() {
        let cache = InstantiationCache::new();
        let key = InstantiationCacheKey::new(type_id(10), canonical(&[(1, 100)]), 0, None);
        cache.insert(key.clone(), type_id(200));
        assert_eq!(cache.len(), 1);
        cache.clear();
        assert!(cache.is_empty());
        assert_eq!(cache.lookup(&key), None);
    }
// TSZ_INLINE_TEST_END c1cd13b1d10a3c4306549b8f7d211ff9029178043ad3fc8d68134f7cf6d29b8f

// TSZ_INLINE_TEST_BEGIN 33ca8189ed860a75b1ecddee5a1246b095cf0aaf1976bfd85730fc7b23683836 350 test_canonical_subst_equal_for_same_pairs
    #[test]
    fn test_canonical_subst_equal_for_same_pairs() {
        // CanonicalSubst constructed from the same sorted pairs must compare
        // equal and hash equal.
        let a = canonical(&[(1, 100), (2, 200)]);
        let b = canonical(&[(2, 200), (1, 100)]); // canonical() sorts internally
        assert_eq!(a, b);

        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut ha = DefaultHasher::new();
        a.hash(&mut ha);
        let mut hb = DefaultHasher::new();
        b.hash(&mut hb);
        assert_eq!(ha.finish(), hb.finish());
    }
// TSZ_INLINE_TEST_END 33ca8189ed860a75b1ecddee5a1246b095cf0aaf1976bfd85730fc7b23683836

// TSZ_INLINE_TEST_BEGIN ca5a909bbaf566232bf575fd9531e7d89159db1e71e6d1dc5ff490804a192781 367 test_canonical_subst_empty_helpers
    #[test]
    fn test_canonical_subst_empty_helpers() {
        let empty = CanonicalSubst::empty();
        assert!(empty.is_empty());
        assert_eq!(empty.len(), 0);
        assert!(empty.as_slice().is_empty());
    }
// TSZ_INLINE_TEST_END ca5a909bbaf566232bf575fd9531e7d89159db1e71e6d1dc5ff490804a192781

// TSZ_INLINE_TEST_BEGIN 8856bbdf94ee56265f1a530ee7dcef6f78c605cafbe50a77f64717c61df66a89 375 shared_identity_domain_heap_is_counted_once_by_arc_pointer
    #[test]
    fn shared_identity_domain_heap_is_counted_once_by_arc_pointer() {
        let parameter = TypeParamInfo {
            origin: TypeParamOrigin::DeclScoped {
                file: atom(99),
                node: 7,
            },
            ..TypeParamInfo::simple(atom(1))
        };
        let substitution = TypeSubstitution::for_signature_domain(&[parameter]);
        let domain = substitution
            .identity_domain_for_cache()
            .expect("declaration-scoped parameter creates an exact domain");
        let first = InstantiationCacheKey::new(type_id(10), canonical(&[(1, 100)]), 0, None)
            .with_identity_domain(Some(Arc::clone(&domain)));
        let second = InstantiationCacheKey::new(type_id(11), canonical(&[(1, 100)]), 0, None)
            .with_identity_domain(Some(domain));

        let mut seen = FxHashSet::default();
        let first_heap = first.estimated_heap_bytes(&mut seen, 64);
        let second_heap = second.estimated_heap_bytes(&mut seen, 64);
        assert!(first_heap >= std::mem::size_of::<IdentitySubstitutionDomain>());
        assert_eq!(second_heap, 0, "the shared `Arc` target is counted once");
    }
// TSZ_INLINE_TEST_END 8856bbdf94ee56265f1a530ee7dcef6f78c605cafbe50a77f64717c61df66a89
