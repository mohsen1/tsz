//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/context/eval_memo_purity.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 5365f634c84c2f479c63a18e18c2474c74e9fa9460027c44016b82446049e060 115 first_insert_is_not_a_violation
    #[test]
    fn first_insert_is_not_a_violation() {
        assert!(!is_divergent_overwrite(None, &TypeId::STRING));
    }
// TSZ_INLINE_TEST_END 5365f634c84c2f479c63a18e18c2474c74e9fa9460027c44016b82446049e060

// TSZ_INLINE_TEST_BEGIN 1c699135981c5e167ec490b5e6dbd933751f74e6c6547793295477b89751d29b 120 identical_reinsert_is_not_a_violation
    #[test]
    fn identical_reinsert_is_not_a_violation() {
        assert!(!is_divergent_overwrite(
            Some(&TypeId::STRING),
            &TypeId::STRING
        ));
    }
// TSZ_INLINE_TEST_END 1c699135981c5e167ec490b5e6dbd933751f74e6c6547793295477b89751d29b

// TSZ_INLINE_TEST_BEGIN a6d8f83354f9c54d90356b963358c94df1e62e154e7a65fd137fec49885f4bd5 128 differing_reinsert_under_same_stamp_is_a_violation
    #[test]
    fn differing_reinsert_under_same_stamp_is_a_violation() {
        assert!(is_divergent_overwrite(
            Some(&TypeId::STRING),
            &TypeId::NUMBER
        ));
    }
// TSZ_INLINE_TEST_END a6d8f83354f9c54d90356b963358c94df1e62e154e7a65fd137fec49885f4bd5

// TSZ_INLINE_TEST_BEGIN 38ed377c6224112a1c92b0a00d98dd3167be77ca0d8c2bdcc8e59629472c5178 136 record_insert_reports_only_on_divergence
    #[test]
    fn record_insert_reports_only_on_divergence() {
        // First write: nothing displaced, no violation.
        assert!(!record_insert(
            ASSIGNABILITY_EVAL_MEMO,
            TypeId::STRING,
            None,
            &TypeId::NUMBER
        ));
        // Stable replay of the same result: still fine.
        assert!(!record_insert(
            ASSIGNABILITY_EVAL_MEMO,
            TypeId::STRING,
            Some(TypeId::NUMBER),
            &TypeId::NUMBER
        ));
        // A different result for the same key under the same stamp is the leak.
        let before = divergence_count();
        assert!(record_insert(
            AWAITED_ASSIGNABILITY_EVAL_MEMO,
            TypeId::STRING,
            Some(TypeId::NUMBER),
            &TypeId::BOOLEAN
        ));
        assert!(
            divergence_count() > before,
            "a detected divergence must advance the process-wide counter"
        );
    }
// TSZ_INLINE_TEST_END 38ed377c6224112a1c92b0a00d98dd3167be77ca0d8c2bdcc8e59629472c5178

// TSZ_INLINE_TEST_BEGIN be6e3229442e5cc9e774075c6509ead9fe83188d64e35e3c74dcc78f57aa3955 166 record_insert_generalizes_to_struct_valued_memos
    #[test]
    fn record_insert_generalizes_to_struct_valued_memos() {
        // The failure memo stores a struct, not a `TypeId`; the same contract
        // and detector apply to a divergent `related` verdict for one key.
        let related = CachedAssignabilityAnalysis {
            related: true,
            depth_exceeded: false,
            iteration_exceeded: false,
            weak_union_violation: false,
            failure_reason: None,
        };
        let mut unrelated = related.clone();
        unrelated.related = false;
        let key = (TypeId::STRING, TypeId::NUMBER, 0u16, false);

        assert!(!record_insert(
            ASSIGNABILITY_FAILURE_MEMO,
            key,
            Some(related.clone()),
            &related
        ));
        assert!(record_insert(
            ASSIGNABILITY_FAILURE_MEMO,
            key,
            Some(related),
            &unrelated
        ));
    }
// TSZ_INLINE_TEST_END be6e3229442e5cc9e774075c6509ead9fe83188d64e35e3c74dcc78f57aa3955
