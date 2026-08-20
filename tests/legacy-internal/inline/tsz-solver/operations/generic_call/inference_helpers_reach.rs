//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/operations/generic_call/inference_helpers_reach.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 7310bdbc1213a3a1af3a567f5024de23df51b35278eafa0230695b78047dc6d7 246 round1_reach_visit_state_enters_new_pair
    #[test]
    fn round1_reach_visit_state_enters_new_pair() {
        let mut visited = FxHashSet::default();

        let state = Round1ReachVisitState::record(TypeId::STRING, TypeId::NUMBER, &mut visited);

        assert_eq!(state, Round1ReachVisitState::Entered);
        assert!(visited.contains(&(TypeId::STRING, TypeId::NUMBER)));
    }
// TSZ_INLINE_TEST_END 7310bdbc1213a3a1af3a567f5024de23df51b35278eafa0230695b78047dc6d7

// TSZ_INLINE_TEST_BEGIN ce724322dac1b6587b0d6a4a85e2f60b95a87dc3f9bc7edf1d4c40d99b40508b 256 round1_reach_visit_state_detects_reentry
    #[test]
    fn round1_reach_visit_state_detects_reentry() {
        let mut visited = FxHashSet::default();

        assert_eq!(
            Round1ReachVisitState::record(TypeId::STRING, TypeId::NUMBER, &mut visited),
            Round1ReachVisitState::Entered
        );
        assert_eq!(
            Round1ReachVisitState::record(TypeId::STRING, TypeId::NUMBER, &mut visited),
            Round1ReachVisitState::AlreadyVisited
        );
    }
// TSZ_INLINE_TEST_END ce724322dac1b6587b0d6a4a85e2f60b95a87dc3f9bc7edf1d4c40d99b40508b

// TSZ_INLINE_TEST_BEGIN b30b779bd901e01ba05269b18971eb3470783432ca1861dff5d937362114692e 270 round1_reach_visit_state_distinguishes_target_pairs
    #[test]
    fn round1_reach_visit_state_distinguishes_target_pairs() {
        let mut visited = FxHashSet::default();

        assert_eq!(
            Round1ReachVisitState::record(TypeId::STRING, TypeId::NUMBER, &mut visited),
            Round1ReachVisitState::Entered
        );
        assert_eq!(
            Round1ReachVisitState::record(TypeId::STRING, TypeId::BOOLEAN, &mut visited),
            Round1ReachVisitState::Entered
        );
    }
// TSZ_INLINE_TEST_END b30b779bd901e01ba05269b18971eb3470783432ca1861dff5d937362114692e
