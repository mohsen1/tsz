//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/evaluation/evaluate/query_budget.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 016d43929fee637c013d75dcca5345adfae813acb36a386283f464470147e7ac 155 budget_state_names_entry_verdict
    #[test]
    fn budget_state_names_entry_verdict() {
        assert_eq!(
            EvalQueryBudgetState::from_exhausted(false),
            EvalQueryBudgetState::WithinBudget
        );
        assert_eq!(
            EvalQueryBudgetState::from_exhausted(true),
            EvalQueryBudgetState::Exhausted
        );
    }
// TSZ_INLINE_TEST_END 016d43929fee637c013d75dcca5345adfae813acb36a386283f464470147e7ac

// TSZ_INLINE_TEST_BEGIN d90ad828f91115558af41483ecc4173eee1e58c9852260723ad572ff1d17fd3b 170 op_counter_resets_per_top_level_query
    /// The per-query operation counter resets when a fresh top-level query
    /// begins (live frame count returns to zero), so one type position can never
    /// carry its op count into the next.
    #[test]
    fn op_counter_resets_per_top_level_query() {
        {
            let _f1 = EvalQueryFrame::enter(1000, None);
            let _f2 = EvalQueryFrame::enter(1000, None);
            let _f3 = EvalQueryFrame::enter(1000, None);
            assert_eq!(eval_query_ops(), 3);
            assert_eq!(eval_query_active(), 3);
        }
        // All frames dropped -> live count back to zero.
        assert_eq!(eval_query_active(), 0);

        // Second top-level query starts fresh: op counter reset to 1, not 4.
        let _f = EvalQueryFrame::enter(1000, None);
        assert_eq!(eval_query_ops(), 1);
    }
// TSZ_INLINE_TEST_END d90ad828f91115558af41483ecc4173eee1e58c9852260723ad572ff1d17fd3b

// TSZ_INLINE_TEST_BEGIN 83a7af7a9d69e002d6a3e0ad24f58c8180f0aa2af0fb1cafab2bb7beb57b2449 190 budget_exhaustion_is_reported_until_query_unwinds
    /// Once the budget is exceeded within a single query, the frame reports
    /// exhaustion so `evaluate` can bail; nested frames keep reporting it until
    /// the query unwinds.
    #[test]
    fn budget_exhaustion_is_reported_until_query_unwinds() {
        let f1 = EvalQueryFrame::enter(2, None);
        assert_eq!(f1.budget_state, EvalQueryBudgetState::WithinBudget);
        let f2 = EvalQueryFrame::enter(2, None);
        assert_eq!(f2.budget_state, EvalQueryBudgetState::WithinBudget);
        let f3 = EvalQueryFrame::enter(2, None);
        assert_eq!(f3.budget_state, EvalQueryBudgetState::Exhausted);
        let f4 = EvalQueryFrame::enter(2, None);
        assert_eq!(f4.budget_state, EvalQueryBudgetState::Exhausted);
        drop((f1, f2, f3, f4));
        assert_eq!(eval_query_active(), 0);
    }
// TSZ_INLINE_TEST_END 83a7af7a9d69e002d6a3e0ad24f58c8180f0aa2af0fb1cafab2bb7beb57b2449

// TSZ_INLINE_TEST_BEGIN ab96c2bc492e8447fe165f28758afda313168cb8f38a6891dec4d3216c0a2026 205 budget_defaults_when_env_unset
    /// With no override set the resolved budget is the default.
    #[test]
    fn budget_defaults_when_env_unset() {
        assert_eq!(resolved_max_eval_ops(), DEFAULT_MAX_EVAL_OPS_PER_QUERY);
    }
// TSZ_INLINE_TEST_END ab96c2bc492e8447fe165f28758afda313168cb8f38a6891dec4d3216c0a2026
