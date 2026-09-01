//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-lowering/src/lower/advanced.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 1bfd4ef6819560e6568164297a11929da7369b2f391b6729e134460ac85cefc9 1248 strip_separators_returns_borrowed_when_no_underscores
    #[test]
    fn strip_separators_returns_borrowed_when_no_underscores() {
        let result = TypeLowering::strip_numeric_separators("123");
        assert!(matches!(result, Cow::Borrowed(_)));
        assert_eq!(result, "123");
    }
// TSZ_INLINE_TEST_END 1bfd4ef6819560e6568164297a11929da7369b2f391b6729e134460ac85cefc9

// TSZ_INLINE_TEST_BEGIN a782b7e219b758098c4e26a7a7b9b5b7ef56fbddd8c40b7ffde0db2a653f8410 1255 strip_separators_empty_string_is_borrowed
    #[test]
    fn strip_separators_empty_string_is_borrowed() {
        let result = TypeLowering::strip_numeric_separators("");
        assert!(matches!(result, Cow::Borrowed(_)));
        assert_eq!(result, "");
    }
// TSZ_INLINE_TEST_END a782b7e219b758098c4e26a7a7b9b5b7ef56fbddd8c40b7ffde0db2a653f8410

// TSZ_INLINE_TEST_BEGIN 9086114eb5799474a7d03ee4815fae61cc38387abd8935625ad834463b491a5b 1262 strip_separators_removes_single_underscore
    #[test]
    fn strip_separators_removes_single_underscore() {
        let result = TypeLowering::strip_numeric_separators("1_000");
        assert!(matches!(result, Cow::Owned(_)));
        assert_eq!(result, "1000");
    }
// TSZ_INLINE_TEST_END 9086114eb5799474a7d03ee4815fae61cc38387abd8935625ad834463b491a5b

// TSZ_INLINE_TEST_BEGIN 50e0e717509302e909b030f13a2f64c5808c62ca4ad65c05fcd064256a93ef84 1269 strip_separators_removes_multiple_underscores
    #[test]
    fn strip_separators_removes_multiple_underscores() {
        let result = TypeLowering::strip_numeric_separators("1_000_000");
        assert!(matches!(result, Cow::Owned(_)));
        assert_eq!(result, "1000000");
    }
// TSZ_INLINE_TEST_END 50e0e717509302e909b030f13a2f64c5808c62ca4ad65c05fcd064256a93ef84

// TSZ_INLINE_TEST_BEGIN 61802273cf7dd222678dc4bd25479bb0f978aec33f6cafeeda0bf9b82d592141 1276 strip_separators_handles_leading_underscore
    #[test]
    fn strip_separators_handles_leading_underscore() {
        // The helper just removes underscores, validity is the parser's concern.
        let result = TypeLowering::strip_numeric_separators("_123");
        assert_eq!(result, "123");
    }
// TSZ_INLINE_TEST_END 61802273cf7dd222678dc4bd25479bb0f978aec33f6cafeeda0bf9b82d592141

// TSZ_INLINE_TEST_BEGIN a2ad63960571dcdfc1b9d73fc67a5721e0c39197f89d1e86f45887c8de11e8eb 1283 strip_separators_handles_trailing_underscore
    #[test]
    fn strip_separators_handles_trailing_underscore() {
        let result = TypeLowering::strip_numeric_separators("123_");
        assert_eq!(result, "123");
    }
// TSZ_INLINE_TEST_END a2ad63960571dcdfc1b9d73fc67a5721e0c39197f89d1e86f45887c8de11e8eb

// TSZ_INLINE_TEST_BEGIN 71db63d2328864b827ccebb5448dd8a539e13614e4565cc9802f990f0bf86af0 1289 strip_separators_handles_only_underscores
    #[test]
    fn strip_separators_handles_only_underscores() {
        let result = TypeLowering::strip_numeric_separators("___");
        assert!(matches!(result, Cow::Owned(_)));
        assert_eq!(result, "");
    }
// TSZ_INLINE_TEST_END 71db63d2328864b827ccebb5448dd8a539e13614e4565cc9802f990f0bf86af0

// TSZ_INLINE_TEST_BEGIN f2911d411780f6b11f54fe60a102078429bd1358fda3bb9509c0945ee4079524 1296 strip_separators_handles_hex_digits
    #[test]
    fn strip_separators_handles_hex_digits() {
        // The helper is base-agnostic — it preserves all non-underscore bytes.
        let result = TypeLowering::strip_numeric_separators("F_F_AB");
        assert_eq!(result, "FFAB");
    }
// TSZ_INLINE_TEST_END f2911d411780f6b11f54fe60a102078429bd1358fda3bb9509c0945ee4079524

// TSZ_INLINE_TEST_BEGIN 56b4070f1b270c63775efd655390b93f39e56cf8cb53a265310982e0f492b177 1305 bigint_base_empty_returns_none
    #[test]
    fn bigint_base_empty_returns_none() {
        assert_eq!(TypeLowering::bigint_base_to_decimal("", 16), None);
        assert_eq!(TypeLowering::bigint_base_to_decimal("", 2), None);
        assert_eq!(TypeLowering::bigint_base_to_decimal("", 8), None);
    }
// TSZ_INLINE_TEST_END 56b4070f1b270c63775efd655390b93f39e56cf8cb53a265310982e0f492b177

// TSZ_INLINE_TEST_BEGIN 29ebc185e49c8e1e4c9be7f9a26de442534706569a5c620059e9719a6646f876 1312 bigint_base_only_separators_returns_none
    #[test]
    fn bigint_base_only_separators_returns_none() {
        // No actual digits seen — saw_digit stays false → None.
        assert_eq!(TypeLowering::bigint_base_to_decimal("_", 16), None);
        assert_eq!(TypeLowering::bigint_base_to_decimal("__", 10), None);
    }
// TSZ_INLINE_TEST_END 29ebc185e49c8e1e4c9be7f9a26de442534706569a5c620059e9719a6646f876

// TSZ_INLINE_TEST_BEGIN 926269ba0e8b635324d09a9ad57cfa8c355807b89f69049931f8f9d08fe37042 1319 bigint_base_zero_returns_zero
    #[test]
    fn bigint_base_zero_returns_zero() {
        assert_eq!(
            TypeLowering::bigint_base_to_decimal("0", 16).as_deref(),
            Some("0"),
        );
        assert_eq!(
            TypeLowering::bigint_base_to_decimal("0", 2).as_deref(),
            Some("0"),
        );
        assert_eq!(
            TypeLowering::bigint_base_to_decimal("0", 10).as_deref(),
            Some("0"),
        );
    }
// TSZ_INLINE_TEST_END 926269ba0e8b635324d09a9ad57cfa8c355807b89f69049931f8f9d08fe37042

// TSZ_INLINE_TEST_BEGIN 0a89f0565caf371ca0c3b0eb9855aee80fcd9fda689390c9d7016d869cc1ac69 1335 bigint_base_hex_basic_values
    #[test]
    fn bigint_base_hex_basic_values() {
        assert_eq!(
            TypeLowering::bigint_base_to_decimal("FF", 16).as_deref(),
            Some("255"),
        );
        assert_eq!(
            TypeLowering::bigint_base_to_decimal("ff", 16).as_deref(),
            Some("255"),
        );
        assert_eq!(
            TypeLowering::bigint_base_to_decimal("100", 16).as_deref(),
            Some("256"),
        );
    }
// TSZ_INLINE_TEST_END 0a89f0565caf371ca0c3b0eb9855aee80fcd9fda689390c9d7016d869cc1ac69

// TSZ_INLINE_TEST_BEGIN 8c2c8d86cb793b71cfcc54a331e38019f9edc8d4b032de685b0377a418be8ff3 1351 bigint_base_binary_basic_values
    #[test]
    fn bigint_base_binary_basic_values() {
        assert_eq!(
            TypeLowering::bigint_base_to_decimal("1010", 2).as_deref(),
            Some("10"),
        );
        assert_eq!(
            TypeLowering::bigint_base_to_decimal("11111111", 2).as_deref(),
            Some("255"),
        );
    }
// TSZ_INLINE_TEST_END 8c2c8d86cb793b71cfcc54a331e38019f9edc8d4b032de685b0377a418be8ff3

// TSZ_INLINE_TEST_BEGIN dd78f9873d2b273da87a50b71cb6bd3a4e8f820636ac52d378a8424da96ee8bc 1363 bigint_base_octal_basic_values
    #[test]
    fn bigint_base_octal_basic_values() {
        assert_eq!(
            TypeLowering::bigint_base_to_decimal("77", 8).as_deref(),
            Some("63"),
        );
        assert_eq!(
            TypeLowering::bigint_base_to_decimal("10", 8).as_deref(),
            Some("8"),
        );
    }
// TSZ_INLINE_TEST_END dd78f9873d2b273da87a50b71cb6bd3a4e8f820636ac52d378a8424da96ee8bc

// TSZ_INLINE_TEST_BEGIN 70cb70e2a73b7601f93d54885ead8724c418f29b48a20776e8ac04c799251fc9 1375 bigint_base_strips_leading_zeros
    #[test]
    fn bigint_base_strips_leading_zeros() {
        assert_eq!(
            TypeLowering::bigint_base_to_decimal("00FF", 16).as_deref(),
            Some("255"),
        );
        assert_eq!(
            TypeLowering::bigint_base_to_decimal("0001010", 2).as_deref(),
            Some("10"),
        );
    }
// TSZ_INLINE_TEST_END 70cb70e2a73b7601f93d54885ead8724c418f29b48a20776e8ac04c799251fc9

// TSZ_INLINE_TEST_BEGIN f4cc4aaa86e287610e6b00faefaed9163ce1e8f4402c1f8ef4a21652a52ff75b 1387 bigint_base_accepts_underscore_separators
    #[test]
    fn bigint_base_accepts_underscore_separators() {
        assert_eq!(
            TypeLowering::bigint_base_to_decimal("F_F", 16).as_deref(),
            Some("255"),
        );
        assert_eq!(
            TypeLowering::bigint_base_to_decimal("1010_1010", 2).as_deref(),
            Some("170"),
        );
    }
// TSZ_INLINE_TEST_END f4cc4aaa86e287610e6b00faefaed9163ce1e8f4402c1f8ef4a21652a52ff75b

// TSZ_INLINE_TEST_BEGIN b997a2f6236a3cf6d2d237fd550ef9617019992281cde0f802e478c934124725 1399 bigint_base_rejects_invalid_digit_for_base
    #[test]
    fn bigint_base_rejects_invalid_digit_for_base() {
        // 8 is not a valid octal digit.
        assert_eq!(TypeLowering::bigint_base_to_decimal("8", 8), None);
        // 2 is not a valid binary digit.
        assert_eq!(TypeLowering::bigint_base_to_decimal("2", 2), None);
        // G is not a valid hex digit.
        assert_eq!(TypeLowering::bigint_base_to_decimal("G", 16), None);
    }
// TSZ_INLINE_TEST_END b997a2f6236a3cf6d2d237fd550ef9617019992281cde0f802e478c934124725

// TSZ_INLINE_TEST_BEGIN 96bce317f0e7a62aa51b190ad561bf9bb30cdd0fcbfdc0ed1a914215eb8ce794 1409 bigint_base_rejects_non_digit_byte
    #[test]
    fn bigint_base_rejects_non_digit_byte() {
        // Non-alphanumeric bytes (other than '_') are rejected outright.
        assert_eq!(TypeLowering::bigint_base_to_decimal("1.5", 10), None);
        assert_eq!(TypeLowering::bigint_base_to_decimal("1+1", 10), None);
        assert_eq!(TypeLowering::bigint_base_to_decimal("a!", 16), None);
    }
// TSZ_INLINE_TEST_END 96bce317f0e7a62aa51b190ad561bf9bb30cdd0fcbfdc0ed1a914215eb8ce794

// TSZ_INLINE_TEST_BEGIN 7369455eef32ffa6858a44c5ca90904db2fd9c0e88250eb464c159baa987c8c2 1417 bigint_base_handles_max_u64_in_hex
    #[test]
    fn bigint_base_handles_max_u64_in_hex() {
        // u64::MAX = 18446744073709551615; this must not lose precision.
        assert_eq!(
            TypeLowering::bigint_base_to_decimal("FFFFFFFFFFFFFFFF", 16).as_deref(),
            Some("18446744073709551615"),
        );
    }
// TSZ_INLINE_TEST_END 7369455eef32ffa6858a44c5ca90904db2fd9c0e88250eb464c159baa987c8c2

// TSZ_INLINE_TEST_BEGIN 71d4d294b871c291c08e6afbb2e46e822342a67c46e25555334523972b9843ee 1426 bigint_base_handles_value_beyond_u64
    #[test]
    fn bigint_base_handles_value_beyond_u64() {
        // 2^64 = 18446744073709551616 — beyond u64::MAX, still must be exact.
        assert_eq!(
            TypeLowering::bigint_base_to_decimal("10000000000000000", 16).as_deref(),
            Some("18446744073709551616"),
        );
        // 2^128 — well past u64::MAX.
        let two_to_128 = "100000000000000000000000000000000";
        assert_eq!(
            TypeLowering::bigint_base_to_decimal(two_to_128, 16).as_deref(),
            Some("340282366920938463463374607431768211456"),
        );
    }
// TSZ_INLINE_TEST_END 71d4d294b871c291c08e6afbb2e46e822342a67c46e25555334523972b9843ee

// TSZ_INLINE_TEST_BEGIN 5e39d711d6c49e9272b2ffb549f6cc734ef1cc489f719a5ee83788357bfbe695 1441 bigint_base_decimal_uses_base_10
    #[test]
    fn bigint_base_decimal_uses_base_10() {
        // Base 10 with leading zero is also handled.
        assert_eq!(
            TypeLowering::bigint_base_to_decimal("0123", 10).as_deref(),
            Some("123"),
        );
        // 9 is valid in base 10 but not in base 8.
        assert_eq!(
            TypeLowering::bigint_base_to_decimal("9", 10).as_deref(),
            Some("9"),
        );
        assert_eq!(TypeLowering::bigint_base_to_decimal("9", 8), None);
    }
// TSZ_INLINE_TEST_END 5e39d711d6c49e9272b2ffb549f6cc734ef1cc489f719a5ee83788357bfbe695

// TSZ_INLINE_TEST_BEGIN 2a9d5b5917ae34ef4d4ccd262d254ea1ebf9c156b71ae771b3a4828c6352f363 1462 normalize_bigint_decimal_no_separators
    #[test]
    fn normalize_bigint_decimal_no_separators() {
        let arena = NodeArena::new();
        let interner = TypeInterner::new();
        let lowering = make_lowering(&arena, &interner);

        let result = lowering.normalize_bigint_literal("1234");
        assert!(matches!(result, Some(Cow::Borrowed("1234"))));
    }
// TSZ_INLINE_TEST_END 2a9d5b5917ae34ef4d4ccd262d254ea1ebf9c156b71ae771b3a4828c6352f363

// TSZ_INLINE_TEST_BEGIN c587b71a8051c62b64b16fd02a711c84d9ff90da518abcffd5136704c0eb9037 1472 normalize_bigint_decimal_with_separators
    #[test]
    fn normalize_bigint_decimal_with_separators() {
        let arena = NodeArena::new();
        let interner = TypeInterner::new();
        let lowering = make_lowering(&arena, &interner);

        let result = lowering.normalize_bigint_literal("1_000_000");
        // Underscores cause owned allocation, then no leading zeros to trim.
        assert!(matches!(result.as_deref(), Some("1000000")));
    }
// TSZ_INLINE_TEST_END c587b71a8051c62b64b16fd02a711c84d9ff90da518abcffd5136704c0eb9037

// TSZ_INLINE_TEST_BEGIN f4d1eadac23f3c68015c5e4c62d01afa38aba56a6b812dc8ea38fedf3a747c9b 1483 normalize_bigint_zero_decimal
    #[test]
    fn normalize_bigint_zero_decimal() {
        let arena = NodeArena::new();
        let interner = TypeInterner::new();
        let lowering = make_lowering(&arena, &interner);

        // Several variants of "zero" all normalize to "0".
        assert_eq!(lowering.normalize_bigint_literal("0").as_deref(), Some("0"));
        assert_eq!(
            lowering.normalize_bigint_literal("000").as_deref(),
            Some("0"),
        );
        assert_eq!(
            lowering.normalize_bigint_literal("0_0_0").as_deref(),
            Some("0"),
        );
    }
// TSZ_INLINE_TEST_END f4d1eadac23f3c68015c5e4c62d01afa38aba56a6b812dc8ea38fedf3a747c9b

// TSZ_INLINE_TEST_BEGIN bacabe2b9c49793570f5034e38a988c1a45b63d8a9385eda7afda7a2c07cbd58 1501 normalize_bigint_strips_leading_zeros_decimal
    #[test]
    fn normalize_bigint_strips_leading_zeros_decimal() {
        let arena = NodeArena::new();
        let interner = TypeInterner::new();
        let lowering = make_lowering(&arena, &interner);

        assert_eq!(
            lowering.normalize_bigint_literal("0001").as_deref(),
            Some("1"),
        );
        assert_eq!(
            lowering.normalize_bigint_literal("0_001").as_deref(),
            Some("1"),
        );
    }
// TSZ_INLINE_TEST_END bacabe2b9c49793570f5034e38a988c1a45b63d8a9385eda7afda7a2c07cbd58

// TSZ_INLINE_TEST_BEGIN 0d621e156b505a3a385d2cb3a54c1fc1347b9698f0ce2a5c7af7ba3e214a5bee 1517 normalize_bigint_hex_lowercase_prefix
    #[test]
    fn normalize_bigint_hex_lowercase_prefix() {
        let arena = NodeArena::new();
        let interner = TypeInterner::new();
        let lowering = make_lowering(&arena, &interner);

        assert_eq!(
            lowering.normalize_bigint_literal("0xFF").as_deref(),
            Some("255"),
        );
        assert_eq!(
            lowering.normalize_bigint_literal("0xff").as_deref(),
            Some("255"),
        );
    }
// TSZ_INLINE_TEST_END 0d621e156b505a3a385d2cb3a54c1fc1347b9698f0ce2a5c7af7ba3e214a5bee

// TSZ_INLINE_TEST_BEGIN 1634003cd2494e0abd255e05e297eb4c841641d740c0eb763b60c20b81644bce 1533 normalize_bigint_hex_uppercase_prefix
    #[test]
    fn normalize_bigint_hex_uppercase_prefix() {
        let arena = NodeArena::new();
        let interner = TypeInterner::new();
        let lowering = make_lowering(&arena, &interner);

        assert_eq!(
            lowering.normalize_bigint_literal("0XFF").as_deref(),
            Some("255"),
        );
    }
// TSZ_INLINE_TEST_END 1634003cd2494e0abd255e05e297eb4c841641d740c0eb763b60c20b81644bce

// TSZ_INLINE_TEST_BEGIN c54baa076eea84ee93ae1ee33d4e68a22a38b66ef3a5971953142d1fd6ffc866 1545 normalize_bigint_binary_prefix
    #[test]
    fn normalize_bigint_binary_prefix() {
        let arena = NodeArena::new();
        let interner = TypeInterner::new();
        let lowering = make_lowering(&arena, &interner);

        assert_eq!(
            lowering.normalize_bigint_literal("0b1010").as_deref(),
            Some("10"),
        );
        assert_eq!(
            lowering.normalize_bigint_literal("0B1010").as_deref(),
            Some("10"),
        );
    }
// TSZ_INLINE_TEST_END c54baa076eea84ee93ae1ee33d4e68a22a38b66ef3a5971953142d1fd6ffc866

// TSZ_INLINE_TEST_BEGIN 5a68803cc72cc52c4b84ba599652b214790ab26ef7d2348741fb15d07d32b49d 1561 normalize_bigint_octal_prefix
    #[test]
    fn normalize_bigint_octal_prefix() {
        let arena = NodeArena::new();
        let interner = TypeInterner::new();
        let lowering = make_lowering(&arena, &interner);

        assert_eq!(
            lowering.normalize_bigint_literal("0o77").as_deref(),
            Some("63"),
        );
        assert_eq!(
            lowering.normalize_bigint_literal("0O77").as_deref(),
            Some("63"),
        );
    }
// TSZ_INLINE_TEST_END 5a68803cc72cc52c4b84ba599652b214790ab26ef7d2348741fb15d07d32b49d

// TSZ_INLINE_TEST_BEGIN 1a8bd783bf2d01879084cec192ddfd9e02dbec58a61808ba8154c4343ef10679 1577 normalize_bigint_prefixed_with_separators
    #[test]
    fn normalize_bigint_prefixed_with_separators() {
        let arena = NodeArena::new();
        let interner = TypeInterner::new();
        let lowering = make_lowering(&arena, &interner);

        assert_eq!(
            lowering.normalize_bigint_literal("0xFF_FF").as_deref(),
            Some("65535"),
        );
        assert_eq!(
            lowering.normalize_bigint_literal("0b1010_1010").as_deref(),
            Some("170"),
        );
        assert_eq!(
            lowering.normalize_bigint_literal("0o7_7").as_deref(),
            Some("63"),
        );
    }
// TSZ_INLINE_TEST_END 1a8bd783bf2d01879084cec192ddfd9e02dbec58a61808ba8154c4343ef10679

// TSZ_INLINE_TEST_BEGIN 14923614f7ffea268fc772e7805acd6883256d6021b3747792570698ff2c7161 1597 normalize_bigint_empty_after_prefix_returns_none
    #[test]
    fn normalize_bigint_empty_after_prefix_returns_none() {
        let arena = NodeArena::new();
        let interner = TypeInterner::new();
        let lowering = make_lowering(&arena, &interner);

        assert!(lowering.normalize_bigint_literal("0x").is_none());
        assert!(lowering.normalize_bigint_literal("0b").is_none());
        assert!(lowering.normalize_bigint_literal("0o").is_none());
    }
// TSZ_INLINE_TEST_END 14923614f7ffea268fc772e7805acd6883256d6021b3747792570698ff2c7161

// TSZ_INLINE_TEST_BEGIN 03bee425ecfb05669207170a61ff874a7314dedd5639e5962d1c3cc934a1daa6 1608 normalize_bigint_invalid_digit_after_prefix_returns_none
    #[test]
    fn normalize_bigint_invalid_digit_after_prefix_returns_none() {
        let arena = NodeArena::new();
        let interner = TypeInterner::new();
        let lowering = make_lowering(&arena, &interner);

        // 'g' is not a valid hex digit.
        assert!(lowering.normalize_bigint_literal("0xG").is_none());
        // '2' is not a valid binary digit.
        assert!(lowering.normalize_bigint_literal("0b2").is_none());
        // '8' is not a valid octal digit.
        assert!(lowering.normalize_bigint_literal("0o8").is_none());
    }
// TSZ_INLINE_TEST_END 03bee425ecfb05669207170a61ff874a7314dedd5639e5962d1c3cc934a1daa6

// TSZ_INLINE_TEST_BEGIN 7f46f8591653f81bccbf0aa9063d85f9def978786daa835a648b4c8da06ef16f 1622 normalize_bigint_borrowed_decimal_when_no_change_needed
    #[test]
    fn normalize_bigint_borrowed_decimal_when_no_change_needed() {
        // No prefix, no separators, no leading zeros → can stay borrowed.
        let arena = NodeArena::new();
        let interner = TypeInterner::new();
        let lowering = make_lowering(&arena, &interner);

        let result = lowering.normalize_bigint_literal("42");
        assert!(matches!(result, Some(Cow::Borrowed("42"))));
    }
// TSZ_INLINE_TEST_END 7f46f8591653f81bccbf0aa9063d85f9def978786daa835a648b4c8da06ef16f

// TSZ_INLINE_TEST_BEGIN c1803b678616adc99d3b756d987d700b981f5ed65fbc0cdfccbc4c3ee1376763 1633 normalize_bigint_handles_very_large_hex
    #[test]
    fn normalize_bigint_handles_very_large_hex() {
        let arena = NodeArena::new();
        let interner = TypeInterner::new();
        let lowering = make_lowering(&arena, &interner);

        // u128::MAX = 340282366920938463463374607431768211455
        assert_eq!(
            lowering
                .normalize_bigint_literal("0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF")
                .as_deref(),
            Some("340282366920938463463374607431768211455"),
        );
    }
// TSZ_INLINE_TEST_END c1803b678616adc99d3b756d987d700b981f5ed65fbc0cdfccbc4c3ee1376763
