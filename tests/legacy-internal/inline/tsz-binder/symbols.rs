//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-binder/src/symbols.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 50b51fd1387eb2f371afa49d3e962fdb021b80311b82d306e06f782b93ea513c 1289 all_declarations_empty_returns_empty
    #[test]
    fn all_declarations_empty_returns_empty() {
        let s = sym();
        assert!(s.all_declarations().is_empty());
    }
// TSZ_INLINE_TEST_END 50b51fd1387eb2f371afa49d3e962fdb021b80311b82d306e06f782b93ea513c

// TSZ_INLINE_TEST_BEGIN 73f2e9addce4c6bd088dcef8e99f860ec09ca1417c3c667465dd81d1b31f7107 1295 all_declarations_only_declarations
    #[test]
    fn all_declarations_only_declarations() {
        let mut s = sym();
        s.add_declaration(NodeIndex(1), None);
        s.add_declaration(NodeIndex(2), None);
        assert_eq!(s.all_declarations(), vec![NodeIndex(1), NodeIndex(2)]);
    }
// TSZ_INLINE_TEST_END 73f2e9addce4c6bd088dcef8e99f860ec09ca1417c3c667465dd81d1b31f7107

// TSZ_INLINE_TEST_BEGIN ab360dd0371c2707e880bc694f786fa48bcd19d21379614fed8f9fe368fd13dd 1303 all_declarations_only_value_declaration
    #[test]
    fn all_declarations_only_value_declaration() {
        let mut s = sym();
        s.set_value_declaration(NodeIndex(5), None);
        assert_eq!(s.all_declarations(), vec![NodeIndex(5)]);
    }
// TSZ_INLINE_TEST_END ab360dd0371c2707e880bc694f786fa48bcd19d21379614fed8f9fe368fd13dd

// TSZ_INLINE_TEST_BEGIN 131c0b0584f4ee820a925f5c9119e7751ca741d72db0469415a82b68cbaeff69 1310 all_declarations_value_first_then_others_no_duplicate
    #[test]
    fn all_declarations_value_first_then_others_no_duplicate() {
        let mut s = sym();
        s.add_declaration(NodeIndex(1), None);
        s.add_declaration(NodeIndex(2), None);
        s.set_value_declaration(NodeIndex(2), None);
        // value_declaration should appear first, and not be duplicated.
        assert_eq!(s.all_declarations(), vec![NodeIndex(2), NodeIndex(1)]);
    }
// TSZ_INLINE_TEST_END 131c0b0584f4ee820a925f5c9119e7751ca741d72db0469415a82b68cbaeff69

// TSZ_INLINE_TEST_BEGIN 8447d4b7fd0105f8e524ed07dc259a9a7c24dfa7651a3fb97658c5659dd11aea 1320 all_declarations_value_not_in_declarations
    #[test]
    fn all_declarations_value_not_in_declarations() {
        let mut s = sym();
        s.add_declaration(NodeIndex(1), None);
        s.add_declaration(NodeIndex(2), None);
        s.set_value_declaration(NodeIndex(9), None);
        assert_eq!(
            s.all_declarations(),
            vec![NodeIndex(9), NodeIndex(1), NodeIndex(2)]
        );
    }
// TSZ_INLINE_TEST_END 8447d4b7fd0105f8e524ed07dc259a9a7c24dfa7651a3fb97658c5659dd11aea

// TSZ_INLINE_TEST_BEGIN 187cc2b174abdf066409b8ee3785de7bb3b201fd67f554999548df645f56a9c6 1332 primary_declaration_prefers_value_declaration
    #[test]
    fn primary_declaration_prefers_value_declaration() {
        let mut s = sym();
        s.add_declaration(NodeIndex(1), None);
        s.set_value_declaration(NodeIndex(9), None);
        assert_eq!(s.primary_declaration(), Some(NodeIndex(9)));
    }
// TSZ_INLINE_TEST_END 187cc2b174abdf066409b8ee3785de7bb3b201fd67f554999548df645f56a9c6

// TSZ_INLINE_TEST_BEGIN 8061018764ab068979c686e97d645428dad1425ec76ecb269bf9d34fcf200483 1340 primary_declaration_falls_back_to_first
    #[test]
    fn primary_declaration_falls_back_to_first() {
        let mut s = sym();
        s.add_declaration(NodeIndex(3), None);
        s.add_declaration(NodeIndex(4), None);
        assert_eq!(s.primary_declaration(), Some(NodeIndex(3)));
    }
// TSZ_INLINE_TEST_END 8061018764ab068979c686e97d645428dad1425ec76ecb269bf9d34fcf200483

// TSZ_INLINE_TEST_BEGIN 7e37fceab06193935edbf3cc0cc3933147b0425714fb3509422a6df0211a6379 1348 primary_declaration_none_when_empty
    #[test]
    fn primary_declaration_none_when_empty() {
        let s = sym();
        assert_eq!(s.primary_declaration(), None);
    }
// TSZ_INLINE_TEST_END 7e37fceab06193935edbf3cc0cc3933147b0425714fb3509422a6df0211a6379

// TSZ_INLINE_TEST_BEGIN c64f1da7797eef03c8de9cf9a8b32405cd905edf83eb4b02d593112e8b78f04b 1361 symbol_size_is_pinned
    /// Pin the size of `Symbol` so accidental field growth is caught in
    /// review; every interned symbol pays this footprint. Dropping the
    /// redundant span fields (#13072 PR 1) brought this from 200 to 176 bytes;
    /// out-of-lining the import-alias payload behind `Option<Box<ImportAliasData>>`
    /// (#13072 PR 2) brought it from 176 to 136 bytes, since only the
    /// fewer-than-5% import-alias symbols now pay for the module specifier,
    /// renamed name, and resolution-mode fields.
    #[test]
    fn symbol_size_is_pinned() {
        assert_eq!(std::mem::size_of::<Symbol>(), 136);
    }
// TSZ_INLINE_TEST_END c64f1da7797eef03c8de9cf9a8b32405cd905edf83eb4b02d593112e8b78f04b

// TSZ_INLINE_TEST_BEGIN e854286723b64fc574b916cdda5e0290df31c543daf49bf86f5c4d60798b76ad 1370 declaration_span_accessors_empty_symbol
    /// The derived span accessors must reproduce the semantics of the
    /// removed stored fields: first non-`None` span across
    /// `add_declaration`/`set_value_declaration` events, and the span
    /// recorded by the last `set_value_declaration`.
    #[test]
    fn declaration_span_accessors_empty_symbol() {
        let s = sym();
        assert_eq!(s.first_declaration_span(), None);
        assert_eq!(s.value_declaration_span(), None);
    }
// TSZ_INLINE_TEST_END e854286723b64fc574b916cdda5e0290df31c543daf49bf86f5c4d60798b76ad

// TSZ_INLINE_TEST_BEGIN 6f5e5bd1c2ce7a5cf1eb4618920c58cba9cbf62901010f82d6a819ee6ae68f97 1377 declaration_span_accessors_add_then_set_same_span
    #[test]
    fn declaration_span_accessors_add_then_set_same_span() {
        // The dominant binder pattern: add_declaration followed by
        // set_value_declaration with the same node and span.
        let mut s = sym();
        s.add_declaration(NodeIndex(1), Some((10, 20)));
        s.set_value_declaration(NodeIndex(1), Some((10, 20)));
        assert_eq!(s.first_declaration_span(), Some((10, 20)));
        assert_eq!(s.value_declaration_span(), Some((10, 20)));
    }
// TSZ_INLINE_TEST_END 6f5e5bd1c2ce7a5cf1eb4618920c58cba9cbf62901010f82d6a819ee6ae68f97

// TSZ_INLINE_TEST_BEGIN a2d01947b3ee1f4f6847338a5bdbacdf19d66aa547cf894aef0dc5d50ed31e4c 1388 declaration_span_accessors_first_span_sticks_across_merges
    #[test]
    fn declaration_span_accessors_first_span_sticks_across_merges() {
        // Declaration merging: later declarations must not change the
        // first-declaration span.
        let mut s = sym();
        s.add_declaration(NodeIndex(1), Some((10, 20)));
        s.add_declaration(NodeIndex(2), Some((30, 40)));
        s.set_value_declaration(NodeIndex(2), Some((30, 40)));
        assert_eq!(s.first_declaration_span(), Some((10, 20)));
        assert_eq!(s.value_declaration_span(), Some((30, 40)));
    }
// TSZ_INLINE_TEST_END a2d01947b3ee1f4f6847338a5bdbacdf19d66aa547cf894aef0dc5d50ed31e4c

// TSZ_INLINE_TEST_BEGIN c602e125069e59e5847c82a8dcb2acd57139a8b47c7abeb4fbbba5d1ec95430c 1400 declaration_span_accessors_set_before_add_enum_member_pattern
    #[test]
    fn declaration_span_accessors_set_before_add_enum_member_pattern() {
        // Enum members call set_value_declaration before add_declaration
        // with the same node and span (binding/declaration.rs).
        let mut s = sym();
        s.set_value_declaration(NodeIndex(7), Some((5, 9)));
        s.add_declaration(NodeIndex(7), Some((5, 9)));
        assert_eq!(s.first_declaration_span(), Some((5, 9)));
        assert_eq!(s.value_declaration_span(), Some((5, 9)));
    }
// TSZ_INLINE_TEST_END c602e125069e59e5847c82a8dcb2acd57139a8b47c7abeb4fbbba5d1ec95430c

// TSZ_INLINE_TEST_BEGIN 4341280135dbe830fa1236febe7d8c4617fbba1f2da9549661b6971a64ea9854 1411 declaration_span_accessors_value_only_symbol_falls_back
    #[test]
    fn declaration_span_accessors_value_only_symbol_falls_back() {
        // A symbol whose only recorded declaration is a value declaration
        // (no add_declaration) reports the value span as its first span,
        // matching the old stored-field write in set_value_declaration.
        let mut s = sym();
        s.set_value_declaration(NodeIndex(3), Some((42, 50)));
        assert_eq!(s.first_declaration_span(), Some((42, 50)));
        assert_eq!(s.value_declaration_span(), Some((42, 50)));
    }
// TSZ_INLINE_TEST_END 4341280135dbe830fa1236febe7d8c4617fbba1f2da9549661b6971a64ea9854

// TSZ_INLINE_TEST_BEGIN f63beca30c145740fba0bd587242c6f2778ab321ffeba609a7eed3e1189936ab 1422 declaration_span_accessors_skip_unknown_entries
    #[test]
    fn declaration_span_accessors_skip_unknown_entries() {
        // A None-span declaration must not shadow a later known span,
        // matching the old "first non-None event span" semantics.
        let mut s = sym();
        s.add_declaration(NodeIndex(1), None);
        s.add_declaration(NodeIndex(2), Some((30, 40)));
        assert_eq!(s.first_declaration_span(), Some((30, 40)));
        assert_eq!(s.value_declaration_span(), None);
    }
// TSZ_INLINE_TEST_END f63beca30c145740fba0bd587242c6f2778ab321ffeba609a7eed3e1189936ab

// TSZ_INLINE_TEST_BEGIN 6822256c5c01b9e080435d72d2a0f03e532ea13493cab713fec3cc6117831671 1433 declaration_span_accessors_resetting_value_declaration_clears_span
    #[test]
    fn declaration_span_accessors_resetting_value_declaration_clears_span() {
        // The incremental prune path resets the value declaration through
        // set_value_declaration with None; the derived span must follow.
        let mut s = sym();
        s.set_value_declaration(NodeIndex(3), Some((42, 50)));
        s.set_value_declaration(NodeIndex::NONE, None);
        assert_eq!(s.value_declaration_span(), None);
    }
// TSZ_INLINE_TEST_END 6822256c5c01b9e080435d72d2a0f03e532ea13493cab713fec3cc6117831671

// TSZ_INLINE_TEST_BEGIN 6cda3d70c79e323ecf8a0c98d3a81da7d72e9b2e7c684202d95678577c18fa2e 1443 symbol_table_atom_lookup_ignores_foreign_arena_atoms
    #[test]
    fn symbol_table_atom_lookup_ignores_foreign_arena_atoms() {
        let lib_owner = 11;
        let user_owner = 22;
        let mut table = SymbolTable::new();

        table.set_with_atom(
            "captureEvents".to_string(),
            Some((lib_owner, AstAtom(7))),
            SymbolId(1),
        );
        table.set("globalThis".to_string(), SymbolId(2));

        assert_eq!(
            table.get_by_atom_or_name(Some((lib_owner, AstAtom(7))), "missing"),
            Some(SymbolId(1)),
            "same-arena atom lookups may use the side index"
        );
        assert_eq!(
            table.get_by_atom_or_name(Some((user_owner, AstAtom(7))), "globalThis"),
            Some(SymbolId(2)),
            "foreign atoms must not hit the same raw atom id in this table"
        );
    }
// TSZ_INLINE_TEST_END 6cda3d70c79e323ecf8a0c98d3a81da7d72e9b2e7c684202d95678577c18fa2e

// TSZ_INLINE_TEST_BEGIN 44e5a653fe43568c42299472dbf34e08adab5618e31b376e6cc2f25e55d2c67a 1468 reserve_symbol_ids_assigns_zero_based_ids_with_default_arena
    #[test]
    fn reserve_symbol_ids_assigns_zero_based_ids_with_default_arena() {
        let mut arena = SymbolArena::new();
        arena.reserve_symbol_ids(3);
        assert_eq!(arena.len(), 3);
        for i in 0..3u32 {
            let s = arena.get(SymbolId(i)).expect("reserved symbol present");
            assert_eq!(s.id, SymbolId(i));
        }
    }
// TSZ_INLINE_TEST_END 44e5a653fe43568c42299472dbf34e08adab5618e31b376e6cc2f25e55d2c67a

// TSZ_INLINE_TEST_BEGIN f8181658104984ffdca43518d96da7f3c03e26ecd68b2c3f3fa1e244a2f77525 1479 reserve_symbol_ids_shifts_ids_by_base_offset
    #[test]
    fn reserve_symbol_ids_shifts_ids_by_base_offset() {
        let base = SymbolArena::CHECKER_SYMBOL_BASE;
        let mut arena = SymbolArena::new_with_base(base);
        arena.reserve_symbol_ids(4);
        assert_eq!(arena.len(), 4);

        // Each placeholder's stored id must be base_offset + index, matching
        // the contract used by `alloc`/`alloc_from` and `get`/`get_mut`.
        for i in 0..4u32 {
            let id = SymbolId(base + i);
            let s = arena
                .get(id)
                .expect("placeholder reachable via base-shifted id");
            assert_eq!(s.id, id);
        }

        // IDs below base_offset must still be rejected (different arena).
        assert!(arena.get(SymbolId(0)).is_none());
    }
// TSZ_INLINE_TEST_END f8181658104984ffdca43518d96da7f3c03e26ecd68b2c3f3fa1e244a2f77525

// TSZ_INLINE_TEST_BEGIN 33d867efcb209ed3eee214e07a2906131ea01e3418184d516760f9c2bd9d985b 1500 reserve_symbol_ids_then_alloc_continues_id_sequence
    #[test]
    fn reserve_symbol_ids_then_alloc_continues_id_sequence() {
        let base = SymbolArena::CHECKER_SYMBOL_BASE;
        let mut arena = SymbolArena::new_with_base(base);
        arena.reserve_symbol_ids(2);
        let next = arena.alloc(0, String::new());
        // After reserving 2 placeholders, the next alloc must produce
        // base_offset + 2 (i.e. continue past the reserved range).
        assert_eq!(next, SymbolId(base + 2));
        assert_eq!(arena.get(next).map(|s| s.id), Some(next));
    }
// TSZ_INLINE_TEST_END 33d867efcb209ed3eee214e07a2906131ea01e3418184d516760f9c2bd9d985b

// TSZ_INLINE_TEST_BEGIN a1a4a60eaa8bfc7451ee13a9f3987d5890e4e26ce8779f84764e903a9328729d 1512 shared_prefix_name_index_survives_private_append
    #[test]
    fn shared_prefix_name_index_survives_private_append() {
        let mut arena = SymbolArena::new();
        let array_id = arena.alloc(0, "Array".to_owned());
        arena.share_current_symbols_for_append();

        let local_id = arena.alloc(0, "Local".to_owned());

        assert_eq!(arena.find_by_name("Array"), Some(array_id));
        assert_eq!(arena.find_all_by_name("Array"), &[array_id]);
        assert_eq!(arena.find_by_name("Local"), Some(local_id));
        assert_eq!(arena.find_all_by_name("Local"), &[local_id]);
        assert!(arena.shared_name_index.contains_key("Array"));
        assert!(!arena.name_index.contains_key("Array"));
    }
// TSZ_INLINE_TEST_END a1a4a60eaa8bfc7451ee13a9f3987d5890e4e26ce8779f84764e903a9328729d

// TSZ_INLINE_TEST_BEGIN d24906ed4c9f2621ef07986751aebc4064f4ae89d0a5c5ddd82a24ca6f660c8d 1528 shared_prefix_name_index_preserves_duplicate_lookup_order
    #[test]
    fn shared_prefix_name_index_preserves_duplicate_lookup_order() {
        let mut arena = SymbolArena::new();
        let shared_id = arena.alloc(0, "Iterator".to_owned());
        arena.share_current_symbols_for_append();

        let local_id = arena.alloc(0, "Iterator".to_owned());

        assert_eq!(arena.find_by_name("Iterator"), Some(shared_id));
        assert_eq!(arena.find_all_by_name("Iterator"), &[shared_id, local_id]);
        assert_eq!(
            arena.name_index.get("Iterator").map(Vec::as_slice),
            Some([shared_id, local_id].as_slice())
        );
    }
// TSZ_INLINE_TEST_END d24906ed4c9f2621ef07986751aebc4064f4ae89d0a5c5ddd82a24ca6f660c8d

// TSZ_INLINE_TEST_BEGIN 66da3690fcb743a80b1a4a641e4b74f501c186d29298dae4e225cfad87168d57 1544 lib_prefix_is_pristine_tracks_shared_prefix_state
    #[test]
    fn lib_prefix_is_pristine_tracks_shared_prefix_state() {
        let mut arena = SymbolArena::new();
        arena.alloc(0, "Array".to_owned());
        arena.alloc(0, "Promise".to_owned());

        // Before sharing, nothing is in the prefix.
        assert!(!arena.lib_prefix_is_pristine(2));
        assert!(arena.lib_prefix_is_pristine(0));

        arena.share_current_symbols_for_append();
        let local = arena.alloc(0, "Local".to_owned());

        // Two lib symbols sit untouched in the shared prefix; the private
        // append (`Local`) does not affect the prefix.
        assert!(arena.lib_prefix_is_pristine(2));
        // A different reported lib count must not match the prefix.
        assert!(!arena.lib_prefix_is_pristine(3));

        // Mutating the private symbol keeps the prefix pristine.
        arena.get_mut(local).expect("local symbol").flags = 1;
        assert!(arena.lib_prefix_is_pristine(2));
    }
// TSZ_INLINE_TEST_END 66da3690fcb743a80b1a4a641e4b74f501c186d29298dae4e225cfad87168d57

// TSZ_INLINE_TEST_BEGIN 505a9ca0f40768a352948c0389176bbd359464abfc830ed08d97a83b7f22c276 1568 lib_prefix_is_pristine_false_after_lib_symbol_materialized
    #[test]
    fn lib_prefix_is_pristine_false_after_lib_symbol_materialized() {
        let mut arena = SymbolArena::new();
        let array_id = arena.alloc(0, "Array".to_owned());
        arena.alloc(0, "Promise".to_owned());
        arena.share_current_symbols_for_append();
        arena.alloc(0, "Local".to_owned());

        assert!(arena.lib_prefix_is_pristine(2));

        // Mutating a shared-prefix (lib) symbol materializes the prefix back
        // into `symbols`, collapsing the shared prefix to empty. The pristine
        // invariant must then report `false`, routing compaction to its full
        // filtered scan instead of the private-only fast path.
        arena.get_mut(array_id).expect("array symbol").flags = 1;
        assert!(!arena.lib_prefix_is_pristine(2));
    }
// TSZ_INLINE_TEST_END 505a9ca0f40768a352948c0389176bbd359464abfc830ed08d97a83b7f22c276
