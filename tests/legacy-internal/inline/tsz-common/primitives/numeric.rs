//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-common/src/primitives/numeric.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN a8d14f8425fc5233b16d0f32fc441f0a86c7fbb89b38a649ace48286e9e82f3f 224 test_parse_numeric_literal_value
    #[test]
    fn test_parse_numeric_literal_value() {
        assert_eq!(parse_numeric_literal_value("123"), Some(123.0));
        assert_eq!(parse_numeric_literal_value("123.456"), Some(123.456));
        assert_eq!(parse_numeric_literal_value("1_000"), Some(1000.0));
        assert_eq!(parse_numeric_literal_value("1e3"), Some(1000.0));
        assert_eq!(parse_numeric_literal_value("1E-3"), Some(0.001));
        assert_eq!(parse_numeric_literal_value("0b11"), Some(3.0));
        assert_eq!(parse_numeric_literal_value("0B111"), Some(7.0));
        assert_eq!(parse_numeric_literal_value("0o10"), Some(8.0));
        assert_eq!(parse_numeric_literal_value("0O123"), Some(83.0));
        assert_eq!(parse_numeric_literal_value("0xFF"), Some(255.0));
        assert_eq!(parse_numeric_literal_value("0Xabc"), Some(2748.0));
        assert_eq!(parse_numeric_literal_value("0b1_0"), Some(2.0));

        // Invalid
        assert_eq!(parse_numeric_literal_value("0b2"), None);
        assert_eq!(parse_numeric_literal_value("0o8"), None);
        assert_eq!(parse_numeric_literal_value("0xg"), None);
    }
// TSZ_INLINE_TEST_END a8d14f8425fc5233b16d0f32fc441f0a86c7fbb89b38a649ace48286e9e82f3f

// TSZ_INLINE_TEST_BEGIN 101ff36b6fd1f3252e3c9fb30a9b564a3adfc3111da9db7c33b7ed0e8d6c4093 245 test_parse_numeric_literal_value_rejects_missing_digits_and_empty_input
    #[test]
    fn test_parse_numeric_literal_value_rejects_missing_digits_and_empty_input() {
        assert_eq!(parse_numeric_literal_value(""), None);
        assert_eq!(parse_numeric_literal_value("0x"), None);
        assert_eq!(parse_numeric_literal_value("0b"), None);
        assert_eq!(parse_numeric_literal_value("0o"), None);
    }
// TSZ_INLINE_TEST_END 101ff36b6fd1f3252e3c9fb30a9b564a3adfc3111da9db7c33b7ed0e8d6c4093

// TSZ_INLINE_TEST_BEGIN 551d61dc6b6b4bd0303c6b76823e4aa3a6eb04e4f5e683b9d34a5a96fb695a57 253 test_parse_numeric_literal_value_rejects_separator_only_radix_body
    #[test]
    fn test_parse_numeric_literal_value_rejects_separator_only_radix_body() {
        // A radix body consisting only of separators has zero digits, which is
        // invalid per spec. Regression for the previous behavior where
        // `0x_` / `0b_` / `0o_` silently returned `Some(0.0)`.
        assert_eq!(parse_numeric_literal_value("0x_"), None);
        assert_eq!(parse_numeric_literal_value("0X__"), None);
        assert_eq!(parse_numeric_literal_value("0b_"), None);
        assert_eq!(parse_numeric_literal_value("0B_"), None);
        assert_eq!(parse_numeric_literal_value("0o_"), None);
        assert_eq!(parse_numeric_literal_value("0O___"), None);
    }
// TSZ_INLINE_TEST_END 551d61dc6b6b4bd0303c6b76823e4aa3a6eb04e4f5e683b9d34a5a96fb695a57

// TSZ_INLINE_TEST_BEGIN 92191911e8c13481cc94375896138d026b578143bb2f134c98933c5f746e049c 266 test_parse_numeric_literal_value_handles_signs_and_separators
    #[test]
    fn test_parse_numeric_literal_value_handles_signs_and_separators() {
        assert_eq!(parse_numeric_literal_value("+42"), Some(42.0));
        assert_eq!(parse_numeric_literal_value("-3.5"), Some(-3.5));
        assert_eq!(parse_numeric_literal_value("1_2_3_4"), Some(1234.0));
        assert_eq!(parse_numeric_literal_value("0xDE_AD"), Some(57005.0));
        assert_eq!(parse_numeric_literal_value("0b1010_1111"), Some(175.0));
        assert_eq!(parse_numeric_literal_value("0o7_7"), Some(63.0));
    }
// TSZ_INLINE_TEST_END 92191911e8c13481cc94375896138d026b578143bb2f134c98933c5f746e049c

// TSZ_INLINE_TEST_BEGIN d671b48004446b723bbc5ad2cac9c64807f24a5c34a3df7d6959186cb0ffc13d 276 to_uint32_wraps_modulo_two_pow_32
    #[test]
    fn to_uint32_wraps_modulo_two_pow_32() {
        // Small values are unchanged.
        assert_eq!(to_uint32(0.0), 0);
        assert_eq!(to_uint32(255.0), 255);
        // `2^31` stays in unsigned range; `2^32` wraps to 0.
        assert_eq!(to_uint32(2_147_483_648.0), 2_147_483_648);
        assert_eq!(to_uint32(4_294_967_296.0), 0);
        // Negative and out-of-range values wrap rather than saturate
        // (a plain `as u32` cast would yield 0 and u32::MAX respectively).
        assert_eq!(to_uint32(-1.0), 4_294_967_295);
        assert_eq!(to_uint32(3_000_000_000.0), 3_000_000_000);
        assert_eq!(to_uint32(4_294_967_297.0), 1);
        // Truncation is toward zero before the modulo.
        assert_eq!(to_uint32(5.9), 5);
        assert_eq!(to_uint32(-5.9), 4_294_967_291);
    }
// TSZ_INLINE_TEST_END d671b48004446b723bbc5ad2cac9c64807f24a5c34a3df7d6959186cb0ffc13d

// TSZ_INLINE_TEST_BEGIN c77d78899e22bdcc205c61f4458d83c3558277a9ea92d5416fb37c472684ad80 294 to_uint32_maps_non_finite_to_zero
    #[test]
    fn to_uint32_maps_non_finite_to_zero() {
        assert_eq!(to_uint32(f64::NAN), 0);
        assert_eq!(to_uint32(f64::INFINITY), 0);
        assert_eq!(to_uint32(f64::NEG_INFINITY), 0);
        assert_eq!(to_uint32(-0.0), 0);
    }
// TSZ_INLINE_TEST_END c77d78899e22bdcc205c61f4458d83c3558277a9ea92d5416fb37c472684ad80

// TSZ_INLINE_TEST_BEGIN fd2ab6875a9489e5a944786df825d4a9360a8239a90aa8d4daa00970e1167d10 302 to_int32_wraps_into_signed_range
    #[test]
    fn to_int32_wraps_into_signed_range() {
        assert_eq!(to_int32(0.0), 0);
        assert_eq!(to_int32(255.0), 255);
        // `0x80000000` is the canonical witness: saturating `as i32` would give
        // i32::MAX (2147483647); ECMAScript ToInt32 wraps to -2147483648.
        assert_eq!(to_int32(2_147_483_648.0), -2_147_483_648);
        assert_ne!(to_int32(2_147_483_648.0), i32::MAX);
        assert_eq!(to_int32(4_294_967_295.0), -1);
        assert_eq!(to_int32(-1.0), -1);
        assert_eq!(to_int32(3_000_000_000.0), -1_294_967_296);
        assert_eq!(to_int32(4_294_967_296.0), 0);
        assert_eq!(to_int32(f64::NAN), 0);
    }
// TSZ_INLINE_TEST_END fd2ab6875a9489e5a944786df825d4a9360a8239a90aa8d4daa00970e1167d10

// TSZ_INLINE_TEST_BEGIN aba80f7469cf05c57ee0b31e92357bc992a9566ceefcf4aed8f4104e55815df3 317 test_parse_numeric_literal_value_mixes_rejections_and_separator_normalization
    #[test]
    fn test_parse_numeric_literal_value_mixes_rejections_and_separator_normalization() {
        assert_eq!(parse_numeric_literal_value("1e"), None);
        assert_eq!(parse_numeric_literal_value("0x1p2"), None);
        assert_eq!(parse_numeric_literal_value("abc"), None);
        assert_eq!(parse_numeric_literal_value("1__2"), Some(12.0));
    }
// TSZ_INLINE_TEST_END aba80f7469cf05c57ee0b31e92357bc992a9566ceefcf4aed8f4104e55815df3

// TSZ_INLINE_TEST_BEGIN f8fd18fe446724618a1b1ad499f12f3d0e268e976309fd816a31810b66babb18 325 js_number_to_string_specials
    #[test]
    fn js_number_to_string_specials() {
        assert_eq!(js_number_to_string(f64::NAN), "NaN");
        assert_eq!(js_number_to_string(f64::INFINITY), "Infinity");
        assert_eq!(js_number_to_string(f64::NEG_INFINITY), "-Infinity");
        assert_eq!(js_number_to_string(0.0), "0");
        assert_eq!(js_number_to_string(-0.0), "0");
    }
// TSZ_INLINE_TEST_END f8fd18fe446724618a1b1ad499f12f3d0e268e976309fd816a31810b66babb18

// TSZ_INLINE_TEST_BEGIN 9981084cefd3321c759a93a8ef9b0a598df24cdc6ecf41ab288d99491c04820b 334 js_number_to_string_fixed_point_range
    #[test]
    fn js_number_to_string_fixed_point_range() {
        assert_eq!(js_number_to_string(42.0), "42");
        assert_eq!(js_number_to_string(-1.0), "-1");
        assert_eq!(js_number_to_string(3.15), "3.15");
        assert_eq!(js_number_to_string(-0.5), "-0.5");
        assert_eq!(js_number_to_string(1e-6), "0.000001");
        // 21-digit integers below 1e21 stay fixed-point, as in JS.
        assert_eq!(js_number_to_string(1e20), "100000000000000000000");
        assert_eq!(js_number_to_string(9.99e20), "999000000000000000000");
    }
// TSZ_INLINE_TEST_END 9981084cefd3321c759a93a8ef9b0a598df24cdc6ecf41ab288d99491c04820b

// TSZ_INLINE_TEST_BEGIN 132971f35542c6487ef88b78fd2be6be9ef5634e0787699f27fea3678e02ef4e 346 js_number_to_string_scientific_range
    #[test]
    fn js_number_to_string_scientific_range() {
        assert_eq!(js_number_to_string(1e21), "1e+21");
        assert_eq!(js_number_to_string(-1e21), "-1e+21");
        assert_eq!(js_number_to_string(1e-7), "1e-7");
        assert_eq!(
            js_number_to_string(1.2345678912345678e53),
            "1.2345678912345678e+53"
        );
    }
// TSZ_INLINE_TEST_END 132971f35542c6487ef88b78fd2be6be9ef5634e0787699f27fea3678e02ef4e

// TSZ_INLINE_TEST_BEGIN c39cd4b6cc1455ee5ee162ecbc3965599f4cd4b083e89ddec88cdee7fe055d7a 357 round_trip_js_number_gate
    #[test]
    fn round_trip_js_number_gate() {
        assert_eq!(round_trip_js_number("42"), Some(42.0));
        assert_eq!(round_trip_js_number("-1"), Some(-1.0));
        assert_eq!(round_trip_js_number("1e+21"), Some(1e21));
        assert_eq!(round_trip_js_number("042"), None);
        assert_eq!(round_trip_js_number("1.0"), None);
        assert_eq!(round_trip_js_number("-0"), None);
        assert_eq!(round_trip_js_number("0x2A"), None);
        assert_eq!(round_trip_js_number("Infinity"), None);
        assert_eq!(round_trip_js_number(""), None);
    }
// TSZ_INLINE_TEST_END c39cd4b6cc1455ee5ee162ecbc3965599f4cd4b083e89ddec88cdee7fe055d7a

// TSZ_INLINE_TEST_BEGIN ad46f0ff0e9a3cd9bdb228f0dc891b3536beef1ba7dce45c4a8f4ad4f6fba05c 370 parse_numeric_literal_value_multibyte_utf8_is_safe
    #[test]
    fn parse_numeric_literal_value_multibyte_utf8_is_safe() {
        // Property names are arbitrary text; a multi-byte first character
        // must not panic the byte-indexed radix-prefix check.
        assert_eq!(parse_numeric_literal_value("日本語"), None);
        assert_eq!(parse_numeric_literal_value("0あ"), None);
        assert_eq!(round_trip_js_number("日本語"), None);
    }
// TSZ_INLINE_TEST_END ad46f0ff0e9a3cd9bdb228f0dc891b3536beef1ba7dce45c4a8f4ad4f6fba05c

// TSZ_INLINE_TEST_BEGIN 1626521f258c952a2febebf16be87e360b1590bcc806a91b57e55887bbd195a6 379 round_trip_js_bigint_gate
    #[test]
    fn round_trip_js_bigint_gate() {
        assert_eq!(round_trip_js_bigint("42"), Some((false, "42")));
        assert_eq!(round_trip_js_bigint("-42"), Some((true, "42")));
        assert_eq!(round_trip_js_bigint("0"), Some((false, "0")));
        assert_eq!(round_trip_js_bigint("042"), None);
        assert_eq!(round_trip_js_bigint("-0"), None);
        assert_eq!(round_trip_js_bigint("4.2"), None);
        assert_eq!(round_trip_js_bigint(""), None);
    }
// TSZ_INLINE_TEST_END 1626521f258c952a2febebf16be87e360b1590bcc806a91b57e55887bbd195a6
