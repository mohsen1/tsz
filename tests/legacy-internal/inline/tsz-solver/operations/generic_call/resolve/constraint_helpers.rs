//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/operations/generic_call/resolve/constraint_helpers.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 3119ab2ab36ea4329ffa37d94230db1374ec40522fbf38cf7493a075e2ee7d2e 768 fuel_stops_before_the_sixty_fifth_unique_node
    #[test]
    fn fuel_stops_before_the_sixty_fifth_unique_node() {
        let mut active = FxHashSet::default();
        let mut steps = 0;
        for offset in 0..MAX_RAW_CONSTRAINT_MATERIALIZATION_STEPS {
            assert!(enter_raw_constraint_materialization(
                TypeId(1_000 + offset as u32),
                &mut active,
                &mut steps,
            ));
        }
        assert!(!enter_raw_constraint_materialization(
            TypeId(2_000),
            &mut active,
            &mut steps,
        ));
        assert_eq!(steps, MAX_RAW_CONSTRAINT_MATERIALIZATION_STEPS);
    }
// TSZ_INLINE_TEST_END 3119ab2ab36ea4329ffa37d94230db1374ec40522fbf38cf7493a075e2ee7d2e

// TSZ_INLINE_TEST_BEGIN 977781766de603b000675327c7708ab75a08a10454589db9165c82a712a5a877 787 cycle_guard_is_path_scoped_for_shared_nodes
    #[test]
    fn cycle_guard_is_path_scoped_for_shared_nodes() {
        let shared = TypeId(1_000);
        let mut active = FxHashSet::default();
        let mut steps = 0;
        assert!(enter_raw_constraint_materialization(
            shared,
            &mut active,
            &mut steps,
        ));
        assert!(!enter_raw_constraint_materialization(
            shared,
            &mut active,
            &mut steps,
        ));
        active.remove(&shared);
        assert!(enter_raw_constraint_materialization(
            shared,
            &mut active,
            &mut steps,
        ));
        assert_eq!(steps, 2);
    }
// TSZ_INLINE_TEST_END 977781766de603b000675327c7708ab75a08a10454589db9165c82a712a5a877
