//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/inference/infer_bct_guard_state.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN b2cb6434571162acf1b926b92d524d597b9b06db6115e672f7f55f3c357d04c5 52 extends_walk_state_names_continue_and_depth_cutoff
    #[test]
    fn extends_walk_state_names_continue_and_depth_cutoff() {
        assert_eq!(extends_walk_state(0, 20), ExtendsWalkState::Continue);
        assert_eq!(extends_walk_state(20, 20), ExtendsWalkState::DepthExceeded);
    }
// TSZ_INLINE_TEST_END b2cb6434571162acf1b926b92d524d597b9b06db6115e672f7f55f3c357d04c5

// TSZ_INLINE_TEST_BEGIN 79680a5fca65b5d1947c9d3f42d8c223d72c454937cf14a28bffe403ea83c592 58 class_hierarchy_visit_state_names_revisit_cutoff
    #[test]
    fn class_hierarchy_visit_state_names_revisit_cutoff() {
        assert_eq!(
            class_hierarchy_visit_state(true),
            ClassHierarchyVisitState::Entered
        );
        assert_eq!(
            class_hierarchy_visit_state(false),
            ClassHierarchyVisitState::AlreadyVisited
        );
    }
// TSZ_INLINE_TEST_END 79680a5fca65b5d1947c9d3f42d8c223d72c454937cf14a28bffe403ea83c592

// TSZ_INLINE_TEST_BEGIN 68670721bb6ae06cac59bc2524975c95807b54a41d7c6070b7174b618ce7f21e 70 active_subtype_pair_state_names_coinductive_fallback
    #[test]
    fn active_subtype_pair_state_names_coinductive_fallback() {
        assert_eq!(
            active_subtype_pair_state(false),
            ActiveSubtypePairState::Entered
        );
        assert_eq!(
            active_subtype_pair_state(true),
            ActiveSubtypePairState::AlreadyActive { fallback: true }
        );
    }
// TSZ_INLINE_TEST_END 68670721bb6ae06cac59bc2524975c95807b54a41d7c6070b7174b618ce7f21e
