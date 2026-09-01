//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/operations/constraints/walker_guard_state.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 6e5bbf9d8adbe725dda89ee50970326aabe42aa3f7fff5ee7441a2f733627274 141 constraint_step_state_continues_below_limit_and_stops_at_limit
    #[test]
    fn constraint_step_state_continues_below_limit_and_stops_at_limit() {
        assert_eq!(
            constraint_step_state(3, 4),
            ConstraintStepState::Continue { next_steps: 4 }
        );
        assert_eq!(
            constraint_step_state(4, 4),
            ConstraintStepState::LimitExceeded
        );
    }
// TSZ_INLINE_TEST_END 6e5bbf9d8adbe725dda89ee50970326aabe42aa3f7fff5ee7441a2f733627274

// TSZ_INLINE_TEST_BEGIN 215f8dfed57246a2c3f366e1bc4728e9ee05a218321db5b568f9950af13b5e8b 153 constraint_pair_visit_state_enters_once_then_reports_revisit
    #[test]
    fn constraint_pair_visit_state_enters_once_then_reports_revisit() {
        let mut pairs = FxHashSet::default();
        assert_eq!(
            constraint_pair_visit_state(&mut pairs, TypeId::STRING, TypeId::NUMBER, 0),
            ConstraintPairVisitState::Entered
        );
        assert_eq!(
            constraint_pair_visit_state(&mut pairs, TypeId::STRING, TypeId::NUMBER, 0),
            ConstraintPairVisitState::AlreadyVisited
        );
    }
// TSZ_INLINE_TEST_END 215f8dfed57246a2c3f366e1bc4728e9ee05a218321db5b568f9950af13b5e8b

// TSZ_INLINE_TEST_BEGIN 6fcd346be733bbf0e8b3a824d6fb5e84504ebad66bf1cc0f63d3b4b4a0569616 166 constraint_pair_guard_distinguishes_modes_in_either_order
    #[test]
    fn constraint_pair_guard_distinguishes_modes_in_either_order() {
        for modes in [[0, 0b010], [0b010, 0]] {
            let mut pairs = FxHashSet::default();
            assert_eq!(
                constraint_pair_visit_state(&mut pairs, TypeId::STRING, TypeId::NUMBER, modes[0],),
                ConstraintPairVisitState::Entered
            );
            assert_eq!(
                constraint_pair_visit_state(&mut pairs, TypeId::STRING, TypeId::NUMBER, modes[1],),
                ConstraintPairVisitState::Entered
            );
            assert_eq!(
                constraint_pair_visit_state(&mut pairs, TypeId::STRING, TypeId::NUMBER, modes[1],),
                ConstraintPairVisitState::AlreadyVisited
            );
        }
    }
// TSZ_INLINE_TEST_END 6fcd346be733bbf0e8b3a824d6fb5e84504ebad66bf1cc0f63d3b4b4a0569616

// TSZ_INLINE_TEST_BEGIN 5000d97d5c1ded81fb1ac2dab6a1d8659aa0bb6fec2b98071b6ae9f8c12bcae1 185 constraint_depth_state_continues_below_limit_and_stops_at_limit
    #[test]
    fn constraint_depth_state_continues_below_limit_and_stops_at_limit() {
        assert_eq!(
            constraint_depth_state(2, 3),
            ConstraintDepthState::Continue { next_depth: 3 }
        );
        assert_eq!(
            constraint_depth_state(3, 3),
            ConstraintDepthState::LimitExceeded
        );
    }
// TSZ_INLINE_TEST_END 5000d97d5c1ded81fb1ac2dab6a1d8659aa0bb6fec2b98071b6ae9f8c12bcae1
