//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/types/utilities/element_indexable.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN af78cb928b7158275e1a831add15a70a454a97b72d904b3ce57dba43acf52de2 241 element_indexable_memo_records_entries_and_size
    #[test]
    fn element_indexable_memo_records_entries_and_size() {
        let mut memo = ElementIndexableMemo::default();

        assert_eq!(
            memo.stats(),
            ElementIndexableMemoStats {
                entries: 0,
                estimated_size_bytes: 0,
            }
        );

        assert_eq!(memo.get(TypeId::STRING), None);
        memo.insert(TypeId::STRING, true);
        assert_eq!(memo.get(TypeId::STRING), Some(true));

        let stats = memo.stats();
        assert_eq!(stats.entries, 1);
        assert!(
            stats.estimated_size_bytes >= mem::size_of::<TypeId>() + mem::size_of::<bool>(),
            "estimated size should account for stored key/value bytes: {stats:?}"
        );
    }
// TSZ_INLINE_TEST_END af78cb928b7158275e1a831add15a70a454a97b72d904b3ce57dba43acf52de2

// TSZ_INLINE_TEST_BEGIN 6e4b7433e1a0f44a3c1375d6460cae1ce4de53ce23e651c07cfb7485fdb46740 265 element_indexable_memo_overwrites_finished_result_without_entry_growth
    #[test]
    fn element_indexable_memo_overwrites_finished_result_without_entry_growth() {
        let mut memo = ElementIndexableMemo::default();

        memo.insert(TypeId::NUMBER, false);
        memo.insert(TypeId::NUMBER, true);

        assert_eq!(memo.get(TypeId::NUMBER), Some(true));
        let stats = memo.stats();
        assert_eq!(stats.entries, 1);
    }
// TSZ_INLINE_TEST_END 6e4b7433e1a0f44a3c1375d6460cae1ce4de53ce23e651c07cfb7485fdb46740
