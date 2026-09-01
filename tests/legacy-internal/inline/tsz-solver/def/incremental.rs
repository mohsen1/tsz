//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/def/incremental.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN e7d0b8fb5f707e9202732f96a83035d0f22e6287d56eec21481b8a56ba9f9c20 317 test_empty_changeset
    #[test]
    fn test_empty_changeset() {
        let cs = FileChangeSet::new();
        assert!(cs.is_empty());
        assert_eq!(cs.len(), 0);

        let store = DefinitionStore::new();
        let summary = cs.apply_invalidation(&store);
        assert_eq!(summary.total_files(), 0);
        assert_eq!(summary.total_defs_invalidated, 0);
        assert!(!summary.had_invalidations());
    }
// TSZ_INLINE_TEST_END e7d0b8fb5f707e9202732f96a83035d0f22e6287d56eec21481b8a56ba9f9c20

// TSZ_INLINE_TEST_BEGIN 39bba3da2355af5f7103f6eea58cfc9dbef6b836fe7646adb68ef72a3c73ad89 330 test_mark_changed_invalidates_defs
    #[test]
    fn test_mark_changed_invalidates_defs() {
        let (store, ids) = make_store_with_file(5);

        assert!(store.contains(ids[0]));
        assert!(store.contains(ids[1]));

        let mut cs = FileChangeSet::new();
        cs.mark_changed(5, 0xAAAA, 0xBBBB);

        let summary = cs.apply_invalidation(&store);
        assert_eq!(summary.files_modified, 1);
        assert_eq!(summary.total_defs_invalidated, 2);
        assert!(summary.had_invalidations());

        // Definitions should be gone.
        assert!(!store.contains(ids[0]));
        assert!(!store.contains(ids[1]));
    }
// TSZ_INLINE_TEST_END 39bba3da2355af5f7103f6eea58cfc9dbef6b836fe7646adb68ef72a3c73ad89

// TSZ_INLINE_TEST_BEGIN 7885e18540178d49b6c256f8f3b50d6fc914aee8c37537ff3547c4c48ab277ea 350 test_mark_removed_invalidates_defs
    #[test]
    fn test_mark_removed_invalidates_defs() {
        let (store, ids) = make_store_with_file(3);

        let mut cs = FileChangeSet::new();
        cs.mark_removed(3);

        let summary = cs.apply_invalidation(&store);
        assert_eq!(summary.files_removed, 1);
        assert_eq!(summary.total_defs_invalidated, 2);

        assert!(!store.contains(ids[0]));
        assert!(!store.contains(ids[1]));
    }
// TSZ_INLINE_TEST_END 7885e18540178d49b6c256f8f3b50d6fc914aee8c37537ff3547c4c48ab277ea

// TSZ_INLINE_TEST_BEGIN 212cbb02163cd3b21b32d2c3c0d0f305ed019cc4b9e14fed87aaab70622db94f 365 test_mark_added_does_not_invalidate
    #[test]
    fn test_mark_added_does_not_invalidate() {
        let store = DefinitionStore::new();

        let mut cs = FileChangeSet::new();
        cs.mark_added(99);

        let summary = cs.apply_invalidation(&store);
        assert_eq!(summary.files_added, 1);
        assert_eq!(summary.total_defs_invalidated, 0);
        assert!(!summary.had_invalidations());
    }
// TSZ_INLINE_TEST_END 212cbb02163cd3b21b32d2c3c0d0f305ed019cc4b9e14fed87aaab70622db94f

// TSZ_INLINE_TEST_BEGIN c60cd72dd528dbe69f61fc971b94d7c0a888ceb8cdba38181816d61894904222 378 test_files_needing_rebind
    #[test]
    fn test_files_needing_rebind() {
        let mut cs = FileChangeSet::new();
        cs.mark_changed(1, 0, 1);
        cs.mark_removed(2);
        cs.mark_added(3);

        let rebind = cs.files_needing_rebind();
        assert_eq!(rebind.len(), 2);
        assert!(rebind.contains(&1));
        assert!(rebind.contains(&3));
        // Removed file should NOT need rebind.
        assert!(!rebind.contains(&2));
    }
// TSZ_INLINE_TEST_END c60cd72dd528dbe69f61fc971b94d7c0a888ceb8cdba38181816d61894904222

// TSZ_INLINE_TEST_BEGIN 68f11f7e171c8591e2b7a9bf337c58eafeb1f220929ddb145aa4a066e14cefd0 393 test_files_needing_invalidation
    #[test]
    fn test_files_needing_invalidation() {
        let mut cs = FileChangeSet::new();
        cs.mark_changed(1, 0, 1);
        cs.mark_removed(2);
        cs.mark_added(3);

        let invalidate = cs.files_needing_invalidation();
        assert_eq!(invalidate.len(), 2);
        assert!(invalidate.contains(&1));
        assert!(invalidate.contains(&2));
        // Added file should NOT need invalidation.
        assert!(!invalidate.contains(&3));
    }
// TSZ_INLINE_TEST_END 68f11f7e171c8591e2b7a9bf337c58eafeb1f220929ddb145aa4a066e14cefd0

// TSZ_INLINE_TEST_BEGIN a26f4ead52b3cd83d2d683908514378affa2e30326b54b980e53d7bd6e8e09d8 408 test_mixed_changeset
    #[test]
    fn test_mixed_changeset() {
        let store = DefinitionStore::new();

        // File 1: 2 defs
        let mut info1a = DefinitionInfo::type_alias(Atom(100), vec![], TypeId::NUMBER);
        info1a.file_id = Some(1);
        store.register(info1a);

        let mut info1b = DefinitionInfo::type_alias(Atom(101), vec![], TypeId::STRING);
        info1b.file_id = Some(1);
        store.register(info1b);

        // File 2: 1 def
        let mut info2 = DefinitionInfo::type_alias(Atom(200), vec![], TypeId::BOOLEAN);
        info2.file_id = Some(2);
        let id_c = store.register(info2);

        let mut cs = FileChangeSet::new();
        cs.mark_changed(1, 0xAA, 0xBB); // file 1 modified
        cs.mark_added(3); // file 3 added (nothing to invalidate)

        let summary = cs.apply_invalidation(&store);
        assert_eq!(summary.files_modified, 1);
        assert_eq!(summary.files_added, 1);
        assert_eq!(summary.total_defs_invalidated, 2); // only file 1's defs

        // File 2's def should be preserved.
        assert!(store.contains(id_c));
    }
// TSZ_INLINE_TEST_END a26f4ead52b3cd83d2d683908514378affa2e30326b54b980e53d7bd6e8e09d8

// TSZ_INLINE_TEST_BEGIN 0d8deb2600fee866de9cdede0c780d4997f34a5826a79af03d35f8d76c6eda81 439 test_diff_fingerprints_no_change
    #[test]
    fn test_diff_fingerprints_no_change() {
        let old = vec![(1, 0xAA), (2, 0xBB)];
        let new = vec![(1, 0xAA), (2, 0xBB)];

        let cs = diff_fingerprints(&old, &new);
        assert!(cs.is_empty());
    }
// TSZ_INLINE_TEST_END 0d8deb2600fee866de9cdede0c780d4997f34a5826a79af03d35f8d76c6eda81

// TSZ_INLINE_TEST_BEGIN faec85cca9cc8e6a04afa0ee186e85319ab6745029dd49ea6200a167113562b5 448 test_diff_fingerprints_modified
    #[test]
    fn test_diff_fingerprints_modified() {
        let old = vec![(1, 0xAA), (2, 0xBB)];
        let new = vec![(1, 0xAA), (2, 0xCC)]; // file 2 changed

        let cs = diff_fingerprints(&old, &new);
        assert_eq!(cs.len(), 1);

        let (file_id, change) = &cs.changes[0];
        assert_eq!(*file_id, 2);
        assert_eq!(
            *change,
            FileChange::Modified {
                old_fingerprint: 0xBB,
                new_fingerprint: 0xCC,
            }
        );
    }
// TSZ_INLINE_TEST_END faec85cca9cc8e6a04afa0ee186e85319ab6745029dd49ea6200a167113562b5

// TSZ_INLINE_TEST_BEGIN 9d0e810c2409c237855ffcf73f75753cf25618fbea087b88874b9cf15ff74461 467 test_diff_fingerprints_added_and_removed
    #[test]
    fn test_diff_fingerprints_added_and_removed() {
        let old = vec![(1, 0xAA)];
        let new = vec![(2, 0xBB)];

        let cs = diff_fingerprints(&old, &new);
        assert_eq!(cs.len(), 2);

        // Should have one removed (file 1) and one added (file 2).
        let removed: Vec<_> = cs
            .iter()
            .filter(|(_, c)| matches!(c, FileChange::Removed))
            .collect();
        let added: Vec<_> = cs
            .iter()
            .filter(|(_, c)| matches!(c, FileChange::Added))
            .collect();

        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].0, 1);
        assert_eq!(added.len(), 1);
        assert_eq!(added[0].0, 2);
    }
// TSZ_INLINE_TEST_END 9d0e810c2409c237855ffcf73f75753cf25618fbea087b88874b9cf15ff74461

// TSZ_INLINE_TEST_BEGIN 50031480065213adbd01ee054ef2448abeee7f5801426823a1ae9306c922e67c 491 test_diff_fingerprints_mixed
    #[test]
    fn test_diff_fingerprints_mixed() {
        let old = vec![(1, 0xAA), (2, 0xBB), (3, 0xCC)];
        let new = vec![(1, 0xAA), (2, 0xDD), (4, 0xEE)];
        // file 1: unchanged, file 2: modified, file 3: removed, file 4: added

        let cs = diff_fingerprints(&old, &new);

        let modified: Vec<_> = cs
            .iter()
            .filter(|(_, c)| matches!(c, FileChange::Modified { .. }))
            .collect();
        let removed: Vec<_> = cs
            .iter()
            .filter(|(_, c)| matches!(c, FileChange::Removed))
            .collect();
        let added: Vec<_> = cs
            .iter()
            .filter(|(_, c)| matches!(c, FileChange::Added))
            .collect();

        assert_eq!(modified.len(), 1);
        assert_eq!(modified[0].0, 2);
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].0, 3);
        assert_eq!(added.len(), 1);
        assert_eq!(added[0].0, 4);
    }
// TSZ_INLINE_TEST_END 50031480065213adbd01ee054ef2448abeee7f5801426823a1ae9306c922e67c

// TSZ_INLINE_TEST_BEGIN ff9c4490144cfca9b279b6fe78e1275997f22de6f5c061fe1fdfb7adaa096bf1 520 test_changeset_with_capacity
    #[test]
    fn test_changeset_with_capacity() {
        let cs = FileChangeSet::with_capacity(10);
        assert!(cs.is_empty());
        assert_eq!(cs.len(), 0);
    }
// TSZ_INLINE_TEST_END ff9c4490144cfca9b279b6fe78e1275997f22de6f5c061fe1fdfb7adaa096bf1

// TSZ_INLINE_TEST_BEGIN 2ccc3a7a43a297c83c7cad838bdf3bbae2891c150d210ac9d9aa48a80b49a8f5 527 test_summary_files_needing_repopulation
    #[test]
    fn test_summary_files_needing_repopulation() {
        let mut cs = FileChangeSet::new();
        cs.mark_changed(1, 0, 1);
        cs.mark_removed(2);
        cs.mark_added(3);
        cs.mark_added(4);

        let store = DefinitionStore::new();
        let summary = cs.apply_invalidation(&store);

        // 1 modified + 2 added = 3 needing repopulation
        assert_eq!(summary.files_needing_repopulation(), 3);
    }
// TSZ_INLINE_TEST_END 2ccc3a7a43a297c83c7cad838bdf3bbae2891c150d210ac9d9aa48a80b49a8f5
