//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-wasm/src/wasm_api/source_file.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN a3f69776b524c267d40c0b395a043b2fcdcd2278e723e35be5c81b8c81f99251 300 byte_len_to_u32_saturating_returns_zero_for_empty
    // Regression for issue #4778: previously `end()` was
    // `self.text.len() as u32`, which silently wraps for sources larger
    // than `u32::MAX`. After the fix the conversion saturates at
    // `u32::MAX`, preserving the `end >= pos` invariant.
    #[test]
    fn byte_len_to_u32_saturating_returns_zero_for_empty() {
        assert_eq!(byte_len_to_u32_saturating(0), 0);
    }
// TSZ_INLINE_TEST_END a3f69776b524c267d40c0b395a043b2fcdcd2278e723e35be5c81b8c81f99251

// TSZ_INLINE_TEST_BEGIN cb0eb7c171f556de6dbc7d2fbb6acc233d0573ed1c12c64e7367ac346b0cbf1c 305 byte_len_to_u32_saturating_round_trips_normal_sizes
    #[test]
    fn byte_len_to_u32_saturating_round_trips_normal_sizes() {
        assert_eq!(byte_len_to_u32_saturating(42), 42);
        assert_eq!(byte_len_to_u32_saturating(1_000_000), 1_000_000);
    }
// TSZ_INLINE_TEST_END cb0eb7c171f556de6dbc7d2fbb6acc233d0573ed1c12c64e7367ac346b0cbf1c

// TSZ_INLINE_TEST_BEGIN 214e60372c4590262e04ba872e88849d5c7cb8bcebcdf44dae8b871970273df5 311 byte_len_to_u32_saturating_passes_through_u32_max
    #[test]
    fn byte_len_to_u32_saturating_passes_through_u32_max() {
        assert_eq!(byte_len_to_u32_saturating(u32::MAX as usize), u32::MAX);
    }
// TSZ_INLINE_TEST_END 214e60372c4590262e04ba872e88849d5c7cb8bcebcdf44dae8b871970273df5

// TSZ_INLINE_TEST_BEGIN 47f6a90387e9c33f46211cb084b0e0f5e9401356c5f832d2da9b42aede007963 316 byte_len_to_u32_saturating_does_not_wrap_above_u32_max
    #[test]
    fn byte_len_to_u32_saturating_does_not_wrap_above_u32_max() {
        // The pre-fix `as u32` cast would wrap to 0 here. Saturating
        // returns u32::MAX, preserving the end >= pos invariant.
        let one_past = (u32::MAX as usize)
            .checked_add(1)
            .expect("u32::MAX + 1 must be representable in usize on 64-bit targets");
        assert_eq!(byte_len_to_u32_saturating(one_past), u32::MAX);
        assert_eq!(byte_len_to_u32_saturating(usize::MAX), u32::MAX);
    }
// TSZ_INLINE_TEST_END 47f6a90387e9c33f46211cb084b0e0f5e9401356c5f832d2da9b42aede007963
