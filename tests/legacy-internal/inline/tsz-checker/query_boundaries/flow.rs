//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/query_boundaries/flow.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 9137da2c9187193b0fa1d535436b17c561dbed2cb1aad656f7996acded55b69c 373 catch_variable_type_returns_unknown_when_flag_set
    #[test]
    fn catch_variable_type_returns_unknown_when_flag_set() {
        assert_eq!(resolve_catch_variable_type(true), TypeId::UNKNOWN);
    }
// TSZ_INLINE_TEST_END 9137da2c9187193b0fa1d535436b17c561dbed2cb1aad656f7996acded55b69c

// TSZ_INLINE_TEST_BEGIN a2073c6157c664b4b85cf81f983de9b381a97a09a3a6ec5b443191a38b9d8d40 378 catch_variable_type_returns_any_when_flag_unset
    #[test]
    fn catch_variable_type_returns_any_when_flag_unset() {
        assert_eq!(resolve_catch_variable_type(false), TypeId::ANY);
    }
// TSZ_INLINE_TEST_END a2073c6157c664b4b85cf81f983de9b381a97a09a3a6ec5b443191a38b9d8d40

// TSZ_INLINE_TEST_BEGIN 80c29d7bab950c93a15e85bff81ce8da6be3e653acab0cdcd6ba5221488f756a 383 catch_variable_type_with_annotation_preserves_annotation
    #[test]
    fn catch_variable_type_with_annotation_preserves_annotation() {
        let annotated = TypeId::STRING;
        assert_eq!(catch_variable_type(Some(annotated), true), TypeId::STRING);
    }
// TSZ_INLINE_TEST_END 80c29d7bab950c93a15e85bff81ce8da6be3e653acab0cdcd6ba5221488f756a

// TSZ_INLINE_TEST_BEGIN beb945b4197bc04785409fe1de8dfa47ef75d70b47b8fa78f8fe3c91f344b379 389 catch_variable_type_without_annotation_uses_flag
    #[test]
    fn catch_variable_type_without_annotation_uses_flag() {
        assert_eq!(catch_variable_type(None, true), TypeId::UNKNOWN);
        assert_eq!(catch_variable_type(None, false), TypeId::ANY);
    }
// TSZ_INLINE_TEST_END beb945b4197bc04785409fe1de8dfa47ef75d70b47b8fa78f8fe3c91f344b379

// TSZ_INLINE_TEST_BEGIN d8727c4c62d0c7c4f4f6bd3fb0e608623b81eb4c17c91d4e79be1c411c1990dd 395 catch_variable_typeof_base_resets_for_catch_var_unknown
    #[test]
    fn catch_variable_typeof_base_resets_for_catch_var_unknown() {
        let result = catch_variable_typeof_base(TypeId::STRING, true, true);
        assert_eq!(result, TypeId::UNKNOWN);
    }
// TSZ_INLINE_TEST_END d8727c4c62d0c7c4f4f6bd3fb0e608623b81eb4c17c91d4e79be1c411c1990dd

// TSZ_INLINE_TEST_BEGIN 30347efdf4bbc4459a5f2da5786910de91d885efc7cdb1fa4d45489c647b663a 401 catch_variable_typeof_base_resets_for_catch_var_any
    #[test]
    fn catch_variable_typeof_base_resets_for_catch_var_any() {
        let result = catch_variable_typeof_base(TypeId::STRING, true, false);
        assert_eq!(result, TypeId::ANY);
    }
// TSZ_INLINE_TEST_END 30347efdf4bbc4459a5f2da5786910de91d885efc7cdb1fa4d45489c647b663a

// TSZ_INLINE_TEST_BEGIN d97dae308014374d9347f7489815a778b083212ca4c0f3def9b131155e189149 407 catch_variable_typeof_base_preserves_non_catch
    #[test]
    fn catch_variable_typeof_base_preserves_non_catch() {
        let result = catch_variable_typeof_base(TypeId::STRING, false, true);
        assert_eq!(result, TypeId::STRING);
    }
// TSZ_INLINE_TEST_END d97dae308014374d9347f7489815a778b083212ca4c0f3def9b131155e189149

// TSZ_INLINE_TEST_BEGIN 7dda12b322c08360dd45d2d98286078580594dcd2ddd171af14385e20bf4a0bb 413 non_nullish_observation_removes_null_and_undefined
    #[test]
    fn non_nullish_observation_removes_null_and_undefined() {
        let db = TypeInterner::new();
        let nullable = db.union(vec![TypeId::STRING, TypeId::NULL, TypeId::UNDEFINED]);

        let observed = apply_flow_observation(&db, nullable, &FlowObservation::NonNullish);
        let helper = narrow_non_nullish(&db, nullable);

        assert_eq!(observed, TypeId::STRING);
        assert_eq!(helper, TypeId::STRING);
    }
// TSZ_INLINE_TEST_END 7dda12b322c08360dd45d2d98286078580594dcd2ddd171af14385e20bf4a0bb
