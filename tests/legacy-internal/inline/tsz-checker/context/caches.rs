//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/context/caches.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 0f231783b722ced99f5c69b6b810279e5a58be1a0a8d1fc2eda2429876a81341 992 generic_constraint_proof_memo_rolls_on_stamp_change
    #[test]
    fn generic_constraint_proof_memo_rolls_on_stamp_change() {
        let key = GenericConstraintProofKey::new(TypeId::STRING, TypeId::NUMBER, 0, false);
        let stamp_a = (1, 2, 3, 4);
        let stamp_b = (1, 2, 3, 5);
        let mut memo = GenericConstraintProofMemo::default();

        memo.insert(stamp_a, key, true);
        assert_eq!(memo.get(stamp_a, key), Some(true));
        assert_eq!(
            memo.get(stamp_b, key),
            None,
            "a new resolver/env stamp must drop stale branch proofs"
        );
        memo.insert(stamp_b, key, false);
        assert_eq!(memo.get(stamp_b, key), Some(false));
        assert_eq!(
            memo.get(stamp_a, key),
            None,
            "rolling back to an old numeric stamp starts a fresh memo epoch"
        );
    }
// TSZ_INLINE_TEST_END 0f231783b722ced99f5c69b6b810279e5a58be1a0a8d1fc2eda2429876a81341

// TSZ_INLINE_TEST_BEGIN 5411b88344f8195311fcb02d783aa555a1daf842aba510e79584e2c47336480e 1015 cow_cache_clone_is_shared_until_first_write
    #[test]
    fn cow_cache_clone_is_shared_until_first_write() {
        let mut live: CowCache<FxHashMap<u32, u32>> = CowCache::default();
        live.insert(1, 10);
        let snapshot = live.clone();
        assert!(live.ptr_eq(&snapshot));

        // Reads through either holder never detach.
        assert_eq!(live.get(&1), Some(&10));
        assert_eq!(snapshot.get(&1), Some(&10));
        assert!(live.ptr_eq(&snapshot));

        // First write detaches the writer; the snapshot is isolated.
        live.insert(2, 20);
        assert!(!live.ptr_eq(&snapshot));
        assert_eq!(live.get(&2), Some(&20));
        assert_eq!(snapshot.get(&2), None);
        assert_eq!(snapshot.get(&1), Some(&10));
    }
// TSZ_INLINE_TEST_END 5411b88344f8195311fcb02d783aa555a1daf842aba510e79584e2c47336480e

// TSZ_INLINE_TEST_BEGIN 8e2d4969fe1488f24678ab19f4f7daecf0fd7234e3e4cdf481851eb5a770fa57 1035 cow_cache_clone_from_restores_sharing_with_snapshot
    #[test]
    fn cow_cache_clone_from_restores_sharing_with_snapshot() {
        let mut live: CowCache<FxHashMap<u32, u32>> = CowCache::default();
        live.insert(1, 10);
        let snapshot = live.clone();
        live.insert(2, 20);

        // Rollback: O(1) Arc swap back to the snapshot state.
        live.clone_from(&snapshot);
        assert!(live.ptr_eq(&snapshot));
        assert_eq!(live.get(&2), None);
        assert_eq!(live.get(&1), Some(&10));

        // Rolling back twice is a no-op that keeps sharing.
        live.clone_from(&snapshot);
        assert!(live.ptr_eq(&snapshot));
    }
// TSZ_INLINE_TEST_END 8e2d4969fe1488f24678ab19f4f7daecf0fd7234e3e4cdf481851eb5a770fa57

// TSZ_INLINE_TEST_BEGIN 38034454370cfa40c19cd47e7dbedac5d7b382527d96d2c7d2f3c409d25b187a 1053 cow_cache_parent_writes_after_child_snapshot_stay_isolated
    #[test]
    fn cow_cache_parent_writes_after_child_snapshot_stay_isolated() {
        // `with_parent_cache` ordering: the child snapshots first, the parent
        // keeps mutating afterwards. Parent writes must not leak into the
        // child (and vice versa), exactly as with a deep clone.
        let mut parent: CowCache<FxHashMap<u32, u32>> = CowCache::default();
        parent.insert(1, 10);
        let mut child = parent.clone();

        parent.insert(2, 20);
        assert_eq!(child.get(&2), None);

        child.insert(3, 30);
        assert_eq!(parent.get(&3), None);
        assert_eq!(parent.get(&2), Some(&20));
        assert_eq!(child.get(&1), Some(&10));
    }
// TSZ_INLINE_TEST_END 38034454370cfa40c19cd47e7dbedac5d7b382527d96d2c7d2f3c409d25b187a

// TSZ_INLINE_TEST_BEGIN d91bb622422ba5ee7bc3e0c502e2a3c8be760d6c950e62dc5f910b33323c4b32 1071 cow_cache_into_inner_clones_only_when_shared
    #[test]
    fn cow_cache_into_inner_clones_only_when_shared() {
        let mut live: CowCache<FxHashMap<u32, u32>> = CowCache::default();
        live.insert(1, 10);
        let snapshot = live.clone();
        let inner = live.into_inner();
        assert_eq!(inner.get(&1), Some(&10));
        // The outstanding snapshot still sees its state.
        assert_eq!(snapshot.get(&1), Some(&10));
    }
// TSZ_INLINE_TEST_END d91bb622422ba5ee7bc3e0c502e2a3c8be760d6c950e62dc5f910b33323c4b32

// TSZ_INLINE_TEST_BEGIN 844ad917c38fa2b05a4aa93a21573a2c132c6baccbab6135760f2d68ed02715d 1082 node_type_cache_absent_remove_does_not_detach_shared_snapshot
    #[test]
    fn node_type_cache_absent_remove_does_not_detach_shared_snapshot() {
        let mut parent = NodeTypeCache::new();
        parent.insert(1, TypeId::STRING);
        let mut child = parent.clone();

        assert!(child.remove(&2).is_none());
        assert!(Arc::ptr_eq(&parent.data, &child.data));

        assert_eq!(child.remove(&1), Some(TypeId::STRING));
        assert!(!Arc::ptr_eq(&parent.data, &child.data));
        assert_eq!(parent.get(&1), Some(&TypeId::STRING));
        assert_eq!(child.get(&1), None);
    }
// TSZ_INLINE_TEST_END 844ad917c38fa2b05a4aa93a21573a2c132c6baccbab6135760f2d68ed02715d

// TSZ_INLINE_TEST_BEGIN 6ed9383d7bae1a94bc52bcec57ac5c9befac5d59dbca04ac8245b55c3159ee2f 1097 symbol_type_cache_absent_remove_does_not_detach_shared_snapshot
    #[test]
    fn symbol_type_cache_absent_remove_does_not_detach_shared_snapshot() {
        let sym = SymbolId(1);
        let parent = SymbolTypeCache::new();
        parent.insert(sym, TypeId::STRING);
        let child = parent.clone();

        assert!(child.remove(&SymbolId(2)).is_none());
        assert!(Arc::ptr_eq(&parent.data.borrow(), &child.data.borrow()));

        assert_eq!(child.remove(&sym), Some(TypeId::STRING));
        assert!(!Arc::ptr_eq(&parent.data.borrow(), &child.data.borrow()));
        assert_eq!(parent.get(&sym), Some(TypeId::STRING));
        assert_eq!(child.get(&sym), None);
    }
// TSZ_INLINE_TEST_END 6ed9383d7bae1a94bc52bcec57ac5c9befac5d59dbca04ac8245b55c3159ee2f

// TSZ_INLINE_TEST_BEGIN 171fbd10c7b31caba394f33a5cde262a7a3ca1f4def2e111d6f51dfb9f78820c 1113 node_type_cache_overlay_reads_through_base_and_isolates_writes
    #[test]
    fn node_type_cache_overlay_reads_through_base_and_isolates_writes() {
        let mut caller = NodeTypeCache::new();
        caller.insert(1, TypeId::STRING);

        let mut overlay = caller.overlay();
        // Base entries are visible through the overlay...
        assert_eq!(overlay.get(&1), Some(&TypeId::STRING));
        assert!(overlay.contains_key(&1));

        // ...but overlay writes stay in the overlay's own layer.
        overlay.insert(2, TypeId::NUMBER);
        assert_eq!(overlay.get(&2), Some(&TypeId::NUMBER));
        assert_eq!(caller.get(&2), None);

        // Harvest (`iter`) yields only the overlay's own writes, so the
        // overload-resolution "restore caller, merge winner" choreography
        // never re-merges base entries.
        let harvested: Vec<_> = overlay.iter().collect();
        assert_eq!(harvested, vec![(2, TypeId::NUMBER)]);
    }
// TSZ_INLINE_TEST_END 171fbd10c7b31caba394f33a5cde262a7a3ca1f4def2e111d6f51dfb9f78820c

// TSZ_INLINE_TEST_BEGIN c4334f208be83fc92faebf339e8e7278aa60f41fc214b744a81ce4e877d0b4a1 1135 node_type_cache_overlay_tombstone_masks_base_entry
    #[test]
    fn node_type_cache_overlay_tombstone_masks_base_entry() {
        let mut caller = NodeTypeCache::new();
        caller.insert(1, TypeId::STRING);

        let mut overlay = caller.overlay();
        assert_eq!(overlay.remove(&1), Some(TypeId::STRING));
        // The base entry stays masked rather than resurfacing.
        assert_eq!(overlay.get(&1), None);
        assert!(!overlay.contains_key(&1));
        // Removing again reports the entry as already gone.
        assert_eq!(overlay.remove(&1), None);
        // Tombstones never escape through harvest or materialization.
        assert_eq!(overlay.iter().count(), 0);
        assert!(overlay.to_hash_map().is_empty());
        // A later write through the overlay overrides the tombstone.
        overlay.insert(1, TypeId::NUMBER);
        assert_eq!(overlay.get(&1), Some(&TypeId::NUMBER));
        // The caller's map is untouched throughout.
        assert_eq!(caller.get(&1), Some(&TypeId::STRING));
    }
// TSZ_INLINE_TEST_END c4334f208be83fc92faebf339e8e7278aa60f41fc214b744a81ce4e877d0b4a1

// TSZ_INLINE_TEST_BEGIN afb24fbedf35c3c1d230c96a6f9d2c5c8a6ba990af3e7d17c585ddd4e718d634 1157 node_type_cache_nested_overlay_flattens_to_visible_view
    #[test]
    fn node_type_cache_nested_overlay_flattens_to_visible_view() {
        let mut caller = NodeTypeCache::new();
        caller.insert(1, TypeId::STRING);

        let mut inner = caller.overlay();
        inner.insert(2, TypeId::NUMBER);
        inner.remove(&1);

        let nested = inner.overlay();
        // The nested overlay sees exactly the inner overlay's visible view:
        // the tombstoned base entry stays hidden, the inner write shows.
        assert_eq!(nested.get(&1), None);
        assert_eq!(nested.get(&2), Some(&TypeId::NUMBER));
        assert_eq!(nested.to_hash_map().len(), 1);
    }
// TSZ_INLINE_TEST_END afb24fbedf35c3c1d230c96a6f9d2c5c8a6ba990af3e7d17c585ddd4e718d634
