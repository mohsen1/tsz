//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/operations/constraints/reverse_mapped.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 1b0c6498e057887684f3884180d32fb913ea243159912475db8467fcd6baa880 1669 classifies_new_pair_under_cap_as_entered
    #[test]
    fn classifies_new_pair_under_cap_as_entered() {
        let active_pairs = FxHashSet::default();
        assert_eq!(
            classify_reverse_mapped_recursion(
                &active_pairs,
                (TypeId(100), TypeId(200)),
                REVERSE_MAPPED_DEPTH_CAP - 1,
            ),
            ReverseMappedRecursionState::Entered
        );
    }
// TSZ_INLINE_TEST_END 1b0c6498e057887684f3884180d32fb913ea243159912475db8467fcd6baa880

// TSZ_INLINE_TEST_BEGIN fd09352d094ca69284cb9d653085201a6a802cc14ef87ecf26cb14391d203f28 1682 classifies_active_pair_as_already_active_before_depth_limit
    #[test]
    fn classifies_active_pair_as_already_active_before_depth_limit() {
        let pair = (TypeId(100), TypeId(200));
        let mut active_pairs = FxHashSet::default();
        active_pairs.insert(pair);
        assert_eq!(
            classify_reverse_mapped_recursion(&active_pairs, pair, REVERSE_MAPPED_DEPTH_CAP),
            ReverseMappedRecursionState::AlreadyActive
        );
    }
// TSZ_INLINE_TEST_END fd09352d094ca69284cb9d653085201a6a802cc14ef87ecf26cb14391d203f28

// TSZ_INLINE_TEST_BEGIN cf94cf18f3b7a0f8a2aee047a88cf7ffd6d8f18cff490eddec61e834561d78fa 1693 classifies_new_pair_at_cap_as_depth_limit_exceeded
    #[test]
    fn classifies_new_pair_at_cap_as_depth_limit_exceeded() {
        let active_pairs = FxHashSet::default();
        assert_eq!(
            classify_reverse_mapped_recursion(
                &active_pairs,
                (TypeId(100), TypeId(200)),
                REVERSE_MAPPED_DEPTH_CAP,
            ),
            ReverseMappedRecursionState::DepthLimitExceeded
        );
    }
// TSZ_INLINE_TEST_END cf94cf18f3b7a0f8a2aee047a88cf7ffd6d8f18cff490eddec61e834561d78fa

// TSZ_INLINE_TEST_BEGIN a94aa756299ec124e7114b9ab74e557bd60b17c0c206fee3e469597985492986 1706 records_entered_pair_and_depth
    #[test]
    fn records_entered_pair_and_depth() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);
        let evaluator = CallEvaluator::new(&interner, &mut checker);
        let pair = (TypeId(100), TypeId(200));

        let (state, previous_depth) = evaluator.record_reverse_mapped_recursion_state(pair);

        assert_eq!(state, ReverseMappedRecursionState::Entered);
        assert_eq!(previous_depth, 0);
        assert_eq!(evaluator.reverse_mapped_depth.get(), 1);
        assert!(evaluator.reverse_mapped_visited.borrow().contains(&pair));
    }
// TSZ_INLINE_TEST_END a94aa756299ec124e7114b9ab74e557bd60b17c0c206fee3e469597985492986

// TSZ_INLINE_TEST_BEGIN bd139edcf0376e3e2672e804922ab8f6fcf26e2c654a2b956250c3fe157be64e 1721 already_active_fallback_preserves_current_recursion_state
    #[test]
    fn already_active_fallback_preserves_current_recursion_state() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);
        let evaluator = CallEvaluator::new(&interner, &mut checker);
        let pair = (TypeId(100), TypeId(200));
        evaluator.reverse_mapped_depth.set(7);
        evaluator.reverse_mapped_visited.borrow_mut().insert(pair);

        let (state, previous_depth) = evaluator.record_reverse_mapped_recursion_state(pair);

        assert_eq!(state, ReverseMappedRecursionState::AlreadyActive);
        assert_eq!(previous_depth, 7);
        assert_eq!(evaluator.reverse_mapped_depth.get(), 7);
        assert_eq!(evaluator.reverse_mapped_visited.borrow().len(), 1);
        assert!(evaluator.reverse_mapped_visited.borrow().contains(&pair));
    }
// TSZ_INLINE_TEST_END bd139edcf0376e3e2672e804922ab8f6fcf26e2c654a2b956250c3fe157be64e

// TSZ_INLINE_TEST_BEGIN 058db342b584d3fa4e5a16ac0ecc57e5dea148648806e36fc970a0b62de28c6f 1739 depth_limit_fallback_preserves_current_recursion_state
    #[test]
    fn depth_limit_fallback_preserves_current_recursion_state() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);
        let evaluator = CallEvaluator::new(&interner, &mut checker);
        let pair = (TypeId(100), TypeId(200));
        evaluator.reverse_mapped_depth.set(REVERSE_MAPPED_DEPTH_CAP);

        let (state, previous_depth) = evaluator.record_reverse_mapped_recursion_state(pair);

        assert_eq!(state, ReverseMappedRecursionState::DepthLimitExceeded);
        assert_eq!(previous_depth, REVERSE_MAPPED_DEPTH_CAP);
        assert_eq!(
            evaluator.reverse_mapped_depth.get(),
            REVERSE_MAPPED_DEPTH_CAP
        );
        assert!(!evaluator.reverse_mapped_visited.borrow().contains(&pair));
    }
// TSZ_INLINE_TEST_END 058db342b584d3fa4e5a16ac0ecc57e5dea148648806e36fc970a0b62de28c6f

// TSZ_INLINE_TEST_BEGIN 9ce00a6411a503e3dfeda90ea457d55932705526f48b0d6db9d57f8a3581f4a6 1758 entered_state_descends_finite_mapped_template_and_restores_state
    #[test]
    fn entered_state_descends_finite_mapped_template_and_restores_state() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);
        let mut evaluator = CallEvaluator::new(&interner, &mut checker);
        let (source, mapped_template, target_placeholder, prop_name) =
            reverse_mapped_fixture(&interner);

        let reversed = evaluator
            .reverse_infer_through_template(source, mapped_template, target_placeholder)
            .expect("finite mapped template should reverse through its property");

        assert_object_property(&interner, reversed, prop_name, TypeId::NUMBER);
        assert_eq!(evaluator.reverse_mapped_depth.get(), 0);
        assert!(
            evaluator.reverse_mapped_visited.borrow().is_empty(),
            "entered pair should be removed after finite descent"
        );
    }
// TSZ_INLINE_TEST_END 9ce00a6411a503e3dfeda90ea457d55932705526f48b0d6db9d57f8a3581f4a6

// TSZ_INLINE_TEST_BEGIN 1888f255f54774ccf9092bcee0900c3122e67d5433b3200cc986aab18eebbd15 1778 already_active_state_falls_back_to_source_value
    #[test]
    fn already_active_state_falls_back_to_source_value() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);
        let mut evaluator = CallEvaluator::new(&interner, &mut checker);
        let (source, mapped_template, target_placeholder, _prop_name) =
            reverse_mapped_fixture(&interner);
        let pair = (mapped_template, source);
        evaluator.reverse_mapped_depth.set(5);
        evaluator.reverse_mapped_visited.borrow_mut().insert(pair);

        let reversed = evaluator
            .reverse_infer_through_template(source, mapped_template, target_placeholder)
            .expect("active recursive pair should converge to the source value");

        assert_eq!(reversed, source);
        assert_eq!(evaluator.reverse_mapped_depth.get(), 5);
        assert!(evaluator.reverse_mapped_visited.borrow().contains(&pair));
    }
// TSZ_INLINE_TEST_END 1888f255f54774ccf9092bcee0900c3122e67d5433b3200cc986aab18eebbd15

// TSZ_INLINE_TEST_BEGIN a1a9668f5b1a26d3f49de9d350112b9c8cdd6c8441749ce607ab7b8aee1e20d8 1798 depth_limit_state_falls_back_to_source_value
    #[test]
    fn depth_limit_state_falls_back_to_source_value() {
        let interner = TypeInterner::new();
        let mut checker = CompatChecker::new(&interner);
        let mut evaluator = CallEvaluator::new(&interner, &mut checker);
        let (source, mapped_template, target_placeholder, _prop_name) =
            reverse_mapped_fixture(&interner);
        let pair = (mapped_template, source);
        evaluator.reverse_mapped_depth.set(REVERSE_MAPPED_DEPTH_CAP);

        let reversed = evaluator
            .reverse_infer_through_template(source, mapped_template, target_placeholder)
            .expect("depth-limited recursive pair should converge to the source value");

        assert_eq!(reversed, source);
        assert_eq!(
            evaluator.reverse_mapped_depth.get(),
            REVERSE_MAPPED_DEPTH_CAP
        );
        assert!(!evaluator.reverse_mapped_visited.borrow().contains(&pair));
    }
// TSZ_INLINE_TEST_END a1a9668f5b1a26d3f49de9d350112b9c8cdd6c8441749ce607ab7b8aee1e20d8
