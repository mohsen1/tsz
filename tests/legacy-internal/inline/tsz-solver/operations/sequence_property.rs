//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/operations/sequence_property.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN e02d518a1a65de99c1c8df347681be872b5a32fde7e947a518fc041d116a40fe 208 tuple_spread_depth_state_continues_at_limit
    #[test]
    fn tuple_spread_depth_state_continues_at_limit() {
        assert_eq!(
            TupleSpreadDepthState::from_depth(MAX_TUPLE_SPREAD_DEPTH),
            TupleSpreadDepthState::Continue
        );
    }
// TSZ_INLINE_TEST_END e02d518a1a65de99c1c8df347681be872b5a32fde7e947a518fc041d116a40fe

// TSZ_INLINE_TEST_BEGIN 4f3a4f4ac3e08467ea64d79e0750ff35b3047e97eb2e43659f0b9305fdd19a2b 216 tuple_spread_depth_state_limits_past_limit
    #[test]
    fn tuple_spread_depth_state_limits_past_limit() {
        assert_eq!(
            TupleSpreadDepthState::from_depth(MAX_TUPLE_SPREAD_DEPTH + 1),
            TupleSpreadDepthState::LimitExceeded
        );
    }
// TSZ_INLINE_TEST_END 4f3a4f4ac3e08467ea64d79e0750ff35b3047e97eb2e43659f0b9305fdd19a2b
