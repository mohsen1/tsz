//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/evaluation/recursive_growth.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN b72e498862dca5b0ff4d8009ea99b417c689399c447ecd9e0679626b3b65b967 222 recursive_growth_verdict_continues_within_limits
    #[test]
    fn recursive_growth_verdict_continues_within_limits() {
        let db = TypeInterner::new();
        let mut evaluator = TypeEvaluator::new(&db);

        assert_eq!(
            evaluator.detect_recursive_growth_verdict(DEF_ID, &[TypeId::STRING]),
            RecursiveGrowthVerdict::Continue
        );
    }
// TSZ_INLINE_TEST_END b72e498862dca5b0ff4d8009ea99b417c689399c447ecd9e0679626b3b65b967

// TSZ_INLINE_TEST_BEGIN 0f2b86518f9d8815e2f445a73c92d5466ab551cfca32579572af7481357200b4 233 recursive_growth_verdict_names_step_weight_limit
    #[test]
    fn recursive_growth_verdict_names_step_weight_limit() {
        let db = TypeInterner::new();
        let mut evaluator = TypeEvaluator::new(&db);
        let long_literal = db.literal_string(
            &"x".repeat(TypeEvaluator::<NoopResolver>::MAX_RECURSIVE_GROWTH_STEP as usize),
        );

        assert_eq!(
            evaluator.detect_recursive_growth_verdict(DEF_ID, &[long_literal]),
            RecursiveGrowthVerdict::Divergent(RecursiveGrowthLimit::StepWeight)
        );
    }
// TSZ_INLINE_TEST_END 0f2b86518f9d8815e2f445a73c92d5466ab551cfca32579572af7481357200b4

// TSZ_INLINE_TEST_BEGIN 075543260b0a161fbcf0078b3dca0f2af4c990dd42ad48676fdec25c55e89df4 247 recursive_growth_verdict_names_detection_run_limit
    #[test]
    fn recursive_growth_verdict_names_detection_run_limit() {
        let db = TypeInterner::new();
        let mut evaluator = TypeEvaluator::new(&db).with_flag_depth_on_app_cycle();
        evaluator.detection_growth_runs.insert(
            DEF_ID,
            (
                0,
                TypeEvaluator::<NoopResolver>::MAX_DETECTION_GROWTH_STEPS - 1,
            ),
        );

        assert_eq!(
            evaluator.detect_recursive_growth_verdict(DEF_ID, &[TypeId::STRING]),
            RecursiveGrowthVerdict::Divergent(RecursiveGrowthLimit::DetectionRun)
        );
    }
// TSZ_INLINE_TEST_END 075543260b0a161fbcf0078b3dca0f2af4c990dd42ad48676fdec25c55e89df4

// TSZ_INLINE_TEST_BEGIN 8303c053a8c91198ca11c7bb6ffa4beabd23dd7dead29c65ba75806be543d2c0 265 recursive_growth_legacy_bool_collapses_divergent_verdicts
    #[test]
    fn recursive_growth_legacy_bool_collapses_divergent_verdicts() {
        let db = TypeInterner::new();
        let mut evaluator = TypeEvaluator::new(&db).with_flag_depth_on_app_cycle();
        evaluator.detection_growth_runs.insert(
            DEF_ID,
            (
                0,
                TypeEvaluator::<NoopResolver>::MAX_DETECTION_GROWTH_STEPS - 1,
            ),
        );

        assert!(evaluator.detect_recursive_growth(DEF_ID, &[TypeId::STRING]));
    }
// TSZ_INLINE_TEST_END 8303c053a8c91198ca11c7bb6ffa4beabd23dd7dead29c65ba75806be543d2c0
