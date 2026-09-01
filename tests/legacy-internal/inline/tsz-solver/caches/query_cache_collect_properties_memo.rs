//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/caches/query_cache_collect_properties_memo.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 9286d1aa6780006568ee0e054151b91c5f95b59bb1745f26fd91c7f5e36a91b7 164 serves_only_the_matching_generation
    #[test]
    fn serves_only_the_matching_generation() {
        let mut memo = CollectPropertiesMemo::default();
        let t = TypeId::STRING;

        memo.insert(t, 7, result(0));

        assert_eq!(memo.get(t, 7), Some(result(0)));
        // The caller's generation is authoritative: a different stamp misses
        // even though a result for this `TypeId` exists.
        assert_eq!(memo.get(t, 8), None);
        assert_eq!(memo.get(TypeId::NUMBER, 7), None);
    }
// TSZ_INLINE_TEST_END 9286d1aa6780006568ee0e054151b91c5f95b59bb1745f26fd91c7f5e36a91b7

// TSZ_INLINE_TEST_BEGIN f6b6761974b1bdcbe52cbff0d4a23cad0a273f3d5b07bcb5d38b9c95bb3e4f91 178 re_collection_at_same_generation_updates_in_place
    #[test]
    fn re_collection_at_same_generation_updates_in_place() {
        let mut memo = CollectPropertiesMemo::default();
        let t = TypeId::STRING;

        memo.insert(t, 3, result(0));
        memo.insert(t, 3, result(1));

        assert_eq!(memo.generations_for(t), 1);
        assert_eq!(memo.get(t, 3), Some(result(1)));
    }
// TSZ_INLINE_TEST_END f6b6761974b1bdcbe52cbff0d4a23cad0a273f3d5b07bcb5d38b9c95bb3e4f91

// TSZ_INLINE_TEST_BEGIN f0cc7b1ae75700c99fd4852320f040f7ba79ab78d4b153c65a76316f21791641 190 bounds_retained_generations_and_evicts_oldest
    #[test]
    fn bounds_retained_generations_and_evicts_oldest() {
        let mut memo = CollectPropertiesMemo::default();
        let t = TypeId::STRING;

        // Insert more distinct generations than the per-type bound allows.
        for generation in 1..=(MAX_GENERATIONS_PER_TYPE as u64 + 3) {
            memo.insert(t, generation, result(2));
        }

        assert_eq!(memo.generations_for(t), MAX_GENERATIONS_PER_TYPE);

        // The oldest generations were evicted; only the most recent survive,
        // and each still serves the value it was stored with.
        let highest = MAX_GENERATIONS_PER_TYPE as u64 + 3;
        for generation in 1..=3 {
            assert_eq!(memo.get(t, generation), None, "stale generation evicted");
        }
        for generation in (highest - MAX_GENERATIONS_PER_TYPE as u64 + 1)..=highest {
            assert_eq!(memo.get(t, generation), Some(result(2)));
        }
    }
// TSZ_INLINE_TEST_END f0cc7b1ae75700c99fd4852320f040f7ba79ab78d4b153c65a76316f21791641

// TSZ_INLINE_TEST_BEGIN 79d244fa635b7b5a18267e43a3dc38e2a7a6104ae47c8759a4d994b1f74f18a9 213 generations_are_tracked_per_type
    #[test]
    fn generations_are_tracked_per_type() {
        let mut memo = CollectPropertiesMemo::default();

        memo.insert(TypeId::STRING, 1, result(0));
        memo.insert(TypeId::NUMBER, 1, result(1));
        memo.insert(TypeId::STRING, 2, result(2));

        assert_eq!(memo.type_count(), 2);
        assert_eq!(memo.get(TypeId::STRING, 1), Some(result(0)));
        assert_eq!(memo.get(TypeId::STRING, 2), Some(result(2)));
        assert_eq!(memo.get(TypeId::NUMBER, 1), Some(result(1)));
    }
// TSZ_INLINE_TEST_END 79d244fa635b7b5a18267e43a3dc38e2a7a6104ae47c8759a4d994b1f74f18a9

// TSZ_INLINE_TEST_BEGIN 176f8d96f24371f0aab3092817108d04a7e4454f5ac4f95fae49a9f0fc7fb77a 227 clear_drops_all_entries
    #[test]
    fn clear_drops_all_entries() {
        let mut memo = CollectPropertiesMemo::default();
        memo.insert(TypeId::STRING, 1, result(0));
        memo.insert(TypeId::NUMBER, 2, result(1));

        memo.clear();

        assert_eq!(memo.total_entries(), 0);
        assert_eq!(memo.get(TypeId::STRING, 1), None);
    }
// TSZ_INLINE_TEST_END 176f8d96f24371f0aab3092817108d04a7e4454f5ac4f95fae49a9f0fc7fb77a
