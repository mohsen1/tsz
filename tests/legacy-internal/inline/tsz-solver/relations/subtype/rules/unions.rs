//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/relations/subtype/rules/unions.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 9272672eff9ed93baa93fff080f22e30bb28e40e92e7e9faa020306b73cbf405 1026 discriminated_object_size_state_names_exact_cap_and_overflow
    #[test]
    fn discriminated_object_size_state_names_exact_cap_and_overflow() {
        assert_eq!(
            DiscriminatedObjectSizeState::for_property_count(MAX_PROPERTIES_FOR_DISCRIMINATED),
            DiscriminatedObjectSizeState::Continue
        );
        assert_eq!(
            DiscriminatedObjectSizeState::for_property_count(MAX_PROPERTIES_FOR_DISCRIMINATED + 1),
            DiscriminatedObjectSizeState::TooManyProperties
        );
    }
// TSZ_INLINE_TEST_END 9272672eff9ed93baa93fff080f22e30bb28e40e92e7e9faa020306b73cbf405

// TSZ_INLINE_TEST_BEGIN f27373b77a85255b21a883fdefe223bed61390821ad57a99eaa0d49ec20a8817 1038 discriminant_combination_state_names_exact_cap_and_overflow
    #[test]
    fn discriminant_combination_state_names_exact_cap_and_overflow() {
        assert_eq!(
            DiscriminantCombinationState::for_count(MAX_DISCRIMINANT_COMBINATIONS),
            DiscriminantCombinationState::Continue
        );
        assert_eq!(
            DiscriminantCombinationState::for_count(MAX_DISCRIMINANT_COMBINATIONS + 1),
            DiscriminantCombinationState::LimitExceeded
        );
    }
// TSZ_INLINE_TEST_END f27373b77a85255b21a883fdefe223bed61390821ad57a99eaa0d49ec20a8817
