//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/error_reporter/display_budget.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 6f15208fe4efc800088191fe8d6a0f0440b8589c3f7f9ca5f20e6b0f8553007c 240 inert_without_scope
    #[test]
    fn inert_without_scope() {
        for _ in 0..(DISPLAY_VISIT_BUDGET + DISPLAY_EVAL_FUEL) {
            assert!(try_consume_visit());
            assert!(try_consume_eval_fuel());
        }
        assert_eq!(cached_eval(TypeId::STRING), None);
        record_eval(TypeId::STRING, TypeId::NUMBER);
        assert_eq!(cached_eval(TypeId::STRING), None);
    }
// TSZ_INLINE_TEST_END 6f15208fe4efc800088191fe8d6a0f0440b8589c3f7f9ca5f20e6b0f8553007c

// TSZ_INLINE_TEST_BEGIN 53d41cd571aec048d6a306c6a5f8a6cf74f86b7e385467cd475673e27f60354c 251 visit_budget_exhausts_and_resets_per_scope
    #[test]
    fn visit_budget_exhausts_and_resets_per_scope() {
        {
            let _scope = DisplayBudgetScope::enter();
            for _ in 0..DISPLAY_VISIT_BUDGET {
                assert!(try_consume_visit());
            }
            assert!(!try_consume_visit());
            assert!(!try_consume_visit());
        }
        let _scope = DisplayBudgetScope::enter();
        assert!(try_consume_visit());
    }
// TSZ_INLINE_TEST_END 53d41cd571aec048d6a306c6a5f8a6cf74f86b7e385467cd475673e27f60354c

// TSZ_INLINE_TEST_BEGIN 124e0e2b9415752aa3e86dbf7c80ec2bf3080c096fa1e77c9b985c7bdc9a456b 265 eval_fuel_exhausts_and_resets_per_scope
    #[test]
    fn eval_fuel_exhausts_and_resets_per_scope() {
        {
            let _scope = DisplayBudgetScope::enter();
            for _ in 0..DISPLAY_EVAL_FUEL {
                assert!(try_consume_eval_fuel());
            }
            assert!(!try_consume_eval_fuel());
        }
        let _scope = DisplayBudgetScope::enter();
        assert!(try_consume_eval_fuel());
    }
// TSZ_INLINE_TEST_END 124e0e2b9415752aa3e86dbf7c80ec2bf3080c096fa1e77c9b985c7bdc9a456b

// TSZ_INLINE_TEST_BEGIN 124a412b3a4a1c93ccf3fadc75fadc840a6af7c3cc23c88d4e07835e23fe0896 278 nested_scopes_share_one_budget
    #[test]
    fn nested_scopes_share_one_budget() {
        let _outer = DisplayBudgetScope::enter();
        assert!(try_consume_visit());
        {
            let _inner = DisplayBudgetScope::enter();
            for _ in 0..(DISPLAY_VISIT_BUDGET - 1) {
                assert!(try_consume_visit());
            }
            assert!(!try_consume_visit());
        }
        // Inner scope exit must not reset the outer scope's budget.
        assert!(!try_consume_visit());
    }
// TSZ_INLINE_TEST_END 124a412b3a4a1c93ccf3fadc75fadc840a6af7c3cc23c88d4e07835e23fe0896

// TSZ_INLINE_TEST_BEGIN fa1cbce2b1f3710f7d0beca92b6f7f4de09b7b8ee05270047ce984ad6308656e 293 eval_memo_is_scoped
    #[test]
    fn eval_memo_is_scoped() {
        {
            let _scope = DisplayBudgetScope::enter();
            assert_eq!(cached_eval(TypeId::STRING), None);
            record_eval(TypeId::STRING, TypeId::NUMBER);
            assert_eq!(cached_eval(TypeId::STRING), Some(TypeId::NUMBER));
        }
        let _scope = DisplayBudgetScope::enter();
        assert_eq!(cached_eval(TypeId::STRING), None);
    }
// TSZ_INLINE_TEST_END fa1cbce2b1f3710f7d0beca92b6f7f4de09b7b8ee05270047ce984ad6308656e

// TSZ_INLINE_TEST_BEGIN 4c3ddf0e7a686b5509de4216df5f2ca3c289d2268ea5571079befc13c4012660 305 eval_memo_statistics_report_entries_and_size
    #[test]
    fn eval_memo_statistics_report_entries_and_size() {
        let mut eval_memo = FxHashMap::default();
        assert_eq!(eval_memo.len(), 0);
        assert_eq!(eval_memo_estimated_size_bytes(&eval_memo), 0);

        eval_memo.insert(TypeId::STRING, TypeId::NUMBER);

        assert_eq!(eval_memo.len(), 1);
        assert!(eval_memo_estimated_size_bytes(&eval_memo) > 0);
    }
// TSZ_INLINE_TEST_END 4c3ddf0e7a686b5509de4216df5f2ca3c289d2268ea5571079befc13c4012660

// TSZ_INLINE_TEST_BEGIN 7bed9574d394cfaa9e664f2222dbe1e1b882dbbc3414702429fed3c8dfa1aed7 317 exhausted_budget_does_not_record_eval_memo
    #[test]
    fn exhausted_budget_does_not_record_eval_memo() {
        let _scope = DisplayBudgetScope::enter();
        for _ in 0..DISPLAY_EVAL_FUEL {
            assert!(try_consume_eval_fuel());
        }
        assert!(!try_consume_eval_fuel());
        assert!(is_exhausted());

        record_eval(TypeId::STRING, TypeId::NUMBER);
        assert_eq!(cached_eval(TypeId::STRING), None);
    }
// TSZ_INLINE_TEST_END 7bed9574d394cfaa9e664f2222dbe1e1b882dbbc3414702429fed3c8dfa1aed7

// TSZ_INLINE_TEST_BEGIN 869900e0e120659ad4b8f398aabb5505f9cf86bd179f1f4164e10f742061832d 330 exhaustion_state_resets_per_scope
    #[test]
    fn exhaustion_state_resets_per_scope() {
        {
            let _scope = DisplayBudgetScope::enter();
            for _ in 0..DISPLAY_EVAL_FUEL {
                assert!(try_consume_eval_fuel());
            }
            assert!(!try_consume_eval_fuel());
            assert!(is_exhausted());
        }
        let _scope = DisplayBudgetScope::enter();
        assert!(!is_exhausted());
    }
// TSZ_INLINE_TEST_END 869900e0e120659ad4b8f398aabb5505f9cf86bd179f1f4164e10f742061832d
