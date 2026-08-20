//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/types/utilities/const_enum_eval.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 43801691a74284274cbcc32ef92e0581776642b092480180a9820c643e0c791c 504 bitwise_or_wraps_like_ecmascript_toint32
    #[test]
    fn bitwise_or_wraps_like_ecmascript_toint32() {
        // `0x80000000` is 2_147_483_648. ECMAScript `ToInt32` wraps it to
        // -2_147_483_648 before `| 0`; the old saturating `as i32` cast produced
        // i32::MAX (2_147_483_647), diverging from tsc.
        assert_eq!(
            first_member_value("const enum E { A = 0x80000000 | 0 }"),
            Some(-2_147_483_648.0)
        );
        // Renamed binder: behavior follows the value, not the enum/member name.
        assert_eq!(
            first_member_value("const enum Flags { Mask = 0x80000000 | 0 }"),
            Some(-2_147_483_648.0)
        );
    }
// TSZ_INLINE_TEST_END 43801691a74284274cbcc32ef92e0581776642b092480180a9820c643e0c791c

// TSZ_INLINE_TEST_BEGIN eeee033951d340391d4a296c317eb3fc2f8a9b13ef424f5a7c2acdf670099937 520 bitwise_and_xor_not_wrap
    #[test]
    fn bitwise_and_xor_not_wrap() {
        // 0xFFFFFFFF -> ToInt32 -> -1.
        assert_eq!(
            first_member_value("const enum E { A = 0xFFFFFFFF & 0xFFFFFFFF }"),
            Some(-1.0)
        );
        assert_eq!(
            first_member_value("const enum E { A = 0xFFFFFFFF ^ 0 }"),
            Some(-1.0)
        );
        // ~0 == -1, and ~0x7FFFFFFF == -2147483648.
        assert_eq!(first_member_value("const enum E { A = ~0 }"), Some(-1.0));
        assert_eq!(
            first_member_value("const enum E { A = ~0x7FFFFFFF }"),
            Some(-2_147_483_648.0)
        );
    }
// TSZ_INLINE_TEST_END eeee033951d340391d4a296c317eb3fc2f8a9b13ef424f5a7c2acdf670099937

// TSZ_INLINE_TEST_BEGIN a2cb72a4f7a04271c82253f791345a374a22d6c22208b10af1477c836ff25928 539 shifts_use_toint32_touint32_operands
    #[test]
    fn shifts_use_toint32_touint32_operands() {
        // 1 << 31 -> i32::MIN (signed left shift), matching JS.
        assert_eq!(
            first_member_value("const enum E { A = 1 << 31 }"),
            Some(-2_147_483_648.0)
        );
        // Signed `>>` sign-extends: -2147483648 >> 31 == -1.
        assert_eq!(
            first_member_value("const enum E { A = (0x80000000 | 0) >> 31 }"),
            Some(-1.0)
        );
        // Unsigned `>>>` zero-fills the ToUint32 operand: 0x80000000 >>> 31 == 1.
        assert_eq!(
            first_member_value("const enum E { A = 0x80000000 >>> 31 }"),
            Some(1.0)
        );
    }
// TSZ_INLINE_TEST_END a2cb72a4f7a04271c82253f791345a374a22d6c22208b10af1477c836ff25928

// TSZ_INLINE_TEST_BEGIN 29ff09bb1f1c839598a6567bd279eeaf6bd0a4a2af471a9ccdb0d9ad912c069a 558 small_values_are_unaffected
    #[test]
    fn small_values_are_unaffected() {
        // Regression: ordinary in-range constants keep their value.
        assert_eq!(
            first_member_value("const enum E { A = 1 << 4 }"),
            Some(16.0)
        );
        assert_eq!(first_member_value("const enum E { A = 6 & 3 }"), Some(2.0));
        assert_eq!(first_member_value("const enum E { A = 5 | 2 }"), Some(7.0));
        assert_eq!(first_member_value("const enum E { A = 255 }"), Some(255.0));
    }
// TSZ_INLINE_TEST_END 29ff09bb1f1c839598a6567bd279eeaf6bd0a4a2af471a9ccdb0d9ad912c069a
