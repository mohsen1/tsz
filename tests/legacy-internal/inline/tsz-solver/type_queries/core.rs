//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/type_queries/core.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN f27f60d5d7e0c09ba899ab4e4acfd1fe8726086f732bd34def535aef2e2fdf45 1596 valid_spread_depth_state_continues_at_limit
    #[test]
    fn valid_spread_depth_state_continues_at_limit() {
        assert_eq!(
            ValidSpreadDepthState::from_depth(MAX_VALID_SPREAD_DEPTH),
            ValidSpreadDepthState::Continue
        );
    }
// TSZ_INLINE_TEST_END f27f60d5d7e0c09ba899ab4e4acfd1fe8726086f732bd34def535aef2e2fdf45

// TSZ_INLINE_TEST_BEGIN dc33741e0bb73cd732f4ea02ac737e6e6da087dd6e01f7ef66d292323134b735 1604 valid_spread_depth_state_limits_past_limit
    #[test]
    fn valid_spread_depth_state_limits_past_limit() {
        assert_eq!(
            ValidSpreadDepthState::from_depth(MAX_VALID_SPREAD_DEPTH + 1),
            ValidSpreadDepthState::LimitExceeded
        );
    }
// TSZ_INLINE_TEST_END dc33741e0bb73cd732f4ea02ac737e6e6da087dd6e01f7ef66d292323134b735

// TSZ_INLINE_TEST_BEGIN ded6c59005cbd7e1c7518037edb2b5216e8cee9186306740ea5e3bf3a9508a4f 2134 widens_mutable_boolean_literal_array_to_boolean_array
    #[test]
    fn widens_mutable_boolean_literal_array_to_boolean_array() {
        let db = TypeInterner::new();
        let boolean_array = db.array(TypeId::BOOLEAN);

        for value in [true, false] {
            let literal = db.literal_boolean(value);
            let literal_array = db.array(literal);
            assert_eq!(
                boolean_literal_array_display_type(&db, literal_array),
                Some(boolean_array),
                "Array<{value}> should widen to boolean[]"
            );
        }
    }
// TSZ_INLINE_TEST_END ded6c59005cbd7e1c7518037edb2b5216e8cee9186306740ea5e3bf3a9508a4f

// TSZ_INLINE_TEST_BEGIN ca300302bd154bb2a02fe7b289ae8dd3fd617d4738cd2079a9159a91c32a8ee1 2150 leaves_non_boolean_literal_arrays_untouched
    #[test]
    fn leaves_non_boolean_literal_arrays_untouched() {
        let db = TypeInterner::new();
        assert_eq!(
            boolean_literal_array_display_type(&db, db.array(TypeId::BOOLEAN)),
            None
        );
        assert_eq!(
            boolean_literal_array_display_type(&db, db.array(db.literal_number(1.0))),
            None
        );
        assert_eq!(
            boolean_literal_array_display_type(&db, db.array(TypeId::STRING)),
            None
        );
        assert_eq!(
            boolean_literal_array_display_type(&db, TypeId::BOOLEAN),
            None
        );
        assert_eq!(
            boolean_literal_array_display_type(&db, db.literal_boolean(true)),
            None
        );
    }
// TSZ_INLINE_TEST_END ca300302bd154bb2a02fe7b289ae8dd3fd617d4738cd2079a9159a91c32a8ee1
