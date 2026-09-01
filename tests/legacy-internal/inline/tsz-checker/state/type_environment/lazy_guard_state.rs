//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/state/type_environment/lazy_guard_state.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN b45912c2865a3dce58856d1db3940241eb8a874fef5b42d145ad7a96e94ad2f9 108 application_entry_state_names_every_top_level_cutoff
    #[test]
    fn application_entry_state_names_every_top_level_cutoff() {
        assert_eq!(
            application_resolution_entry_state(true, true, 0, 5, 0, 1),
            ApplicationResolutionEntryState::AlreadyResolved
        );
        assert_eq!(
            application_resolution_entry_state(false, false, 0, 5, 0, 1),
            ApplicationResolutionEntryState::AlreadyVisiting
        );
        assert_eq!(
            application_resolution_entry_state(false, true, 5, 5, 0, 1),
            ApplicationResolutionEntryState::FuelExhausted
        );
        assert_eq!(
            application_resolution_entry_state(false, true, 0, 5, 1, 1),
            ApplicationResolutionEntryState::DepthExceeded
        );
        assert_eq!(
            application_resolution_entry_state(false, true, 0, 5, 0, 1),
            ApplicationResolutionEntryState::Entered { outermost: true }
        );
        assert_eq!(
            application_resolution_entry_state(false, true, 0, 5, 1, 2),
            ApplicationResolutionEntryState::Entered { outermost: false }
        );
    }
// TSZ_INLINE_TEST_END b45912c2865a3dce58856d1db3940241eb8a874fef5b42d145ad7a96e94ad2f9

// TSZ_INLINE_TEST_BEGIN ceb7a00785b3b4c9ee099ceb4e0239aa0d86dbcd02c9c31b63f385303011c283 136 application_work_state_names_local_and_global_fuel_cutoffs
    #[test]
    fn application_work_state_names_local_and_global_fuel_cutoffs() {
        assert_eq!(
            application_resolution_local_fuel_state(4, 5),
            ApplicationResolutionWorkState::Continue
        );
        assert_eq!(
            application_resolution_local_fuel_state(5, 5),
            ApplicationResolutionWorkState::LocalFuelExhausted
        );
        assert_eq!(
            application_resolution_post_consume_state(false),
            ApplicationResolutionWorkState::Continue
        );
        assert_eq!(
            application_resolution_post_consume_state(true),
            ApplicationResolutionWorkState::GlobalFuelExhausted
        );
    }
// TSZ_INLINE_TEST_END ceb7a00785b3b4c9ee099ceb4e0239aa0d86dbcd02c9c31b63f385303011c283

// TSZ_INLINE_TEST_BEGIN 60fde9118e8cc86c0902c7f9a1b2e903e3ecb9e15711488ec63ba75fba676e52 156 refs_work_state_names_prewalk_cutoffs
    #[test]
    fn refs_work_state_names_prewalk_cutoffs() {
        assert_eq!(
            refs_resolution_work_state(false, false),
            RefsResolutionWorkState::Continue
        );
        assert_eq!(
            refs_resolution_work_state(true, false),
            RefsResolutionWorkState::RefsFuelExhausted
        );
        assert_eq!(
            refs_resolution_work_state(false, true),
            RefsResolutionWorkState::GlobalFuelExhausted
        );
    }
// TSZ_INLINE_TEST_END 60fde9118e8cc86c0902c7f9a1b2e903e3ecb9e15711488ec63ba75fba676e52

// TSZ_INLINE_TEST_BEGIN 6b4e2a9e5115aa7b7360e9cdaaa895f6d4cf9aae26dd613c5ce0e5fc33aac203 172 eval_env_entry_state_names_depth_cutoff
    #[test]
    fn eval_env_entry_state_names_depth_cutoff() {
        assert_eq!(
            eval_env_entry_state(4, 5),
            EvalEnvEntryState::Entered { depth: 5 }
        );
        assert_eq!(eval_env_entry_state(5, 5), EvalEnvEntryState::DepthExceeded);
    }
// TSZ_INLINE_TEST_END 6b4e2a9e5115aa7b7360e9cdaaa895f6d4cf9aae26dd613c5ce0e5fc33aac203
