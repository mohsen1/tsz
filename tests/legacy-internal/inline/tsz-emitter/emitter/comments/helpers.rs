//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-emitter/src/emitter/comments/helpers.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN dc525bdeb766c98c7c09943e2e3d73ce3ac7e3841ea3c86b85a9196ac377bc4c 1405 test_strip_leading_whitespace_basic
    #[test]
    fn test_strip_leading_whitespace_basic() {
        assert_eq!(strip_leading_whitespace("   * @type", 2), " * @type");
        assert_eq!(strip_leading_whitespace("   * @type", 3), "* @type");
        assert_eq!(strip_leading_whitespace("   * @type", 0), "   * @type");
    }
// TSZ_INLINE_TEST_END dc525bdeb766c98c7c09943e2e3d73ce3ac7e3841ea3c86b85a9196ac377bc4c

// TSZ_INLINE_TEST_BEGIN 8b98a9c208049a9d59dca0127dc164f2ef33cef7f2d04bb9507ff1ae4632c2e9 1412 test_strip_leading_whitespace_strips_up_to_count
    #[test]
    fn test_strip_leading_whitespace_strips_up_to_count() {
        // When count exceeds available whitespace, only strip actual whitespace
        assert_eq!(strip_leading_whitespace(" * foo", 4), "* foo");
        assert_eq!(strip_leading_whitespace("* foo", 4), "* foo");
    }
// TSZ_INLINE_TEST_END 8b98a9c208049a9d59dca0127dc164f2ef33cef7f2d04bb9507ff1ae4632c2e9

// TSZ_INLINE_TEST_BEGIN 98bf322bb438d607926d30f893c86f7c76f94ed6d515ce448e5e3469e0eb91b4 1419 test_strip_leading_whitespace_stops_at_non_whitespace
    #[test]
    fn test_strip_leading_whitespace_stops_at_non_whitespace() {
        // Non-whitespace characters stop the stripping even within count
        assert_eq!(strip_leading_whitespace("abc", 3), "abc");
        assert_eq!(strip_leading_whitespace("  abc", 4), "abc");
    }
// TSZ_INLINE_TEST_END 98bf322bb438d607926d30f893c86f7c76f94ed6d515ce448e5e3469e0eb91b4

// TSZ_INLINE_TEST_BEGIN 9bb75cc7ac9530d23f3191f217fbe60585126b5abd4285683e3e0b9074021f3e 1426 test_strip_leading_whitespace_tabs
    #[test]
    fn test_strip_leading_whitespace_tabs() {
        assert_eq!(strip_leading_whitespace("\t\t* foo", 2), "* foo");
        assert_eq!(strip_leading_whitespace("\t * foo", 1), " * foo");
    }
// TSZ_INLINE_TEST_END 9bb75cc7ac9530d23f3191f217fbe60585126b5abd4285683e3e0b9074021f3e

// TSZ_INLINE_TEST_BEGIN 1689e0b2b01578ea699909838549da815da42476339abdbfa231ef4a597ce3a9 1432 test_strip_leading_whitespace_empty
    #[test]
    fn test_strip_leading_whitespace_empty() {
        assert_eq!(strip_leading_whitespace("", 3), "");
        assert_eq!(strip_leading_whitespace("   ", 2), " ");
    }
// TSZ_INLINE_TEST_END 1689e0b2b01578ea699909838549da815da42476339abdbfa231ef4a597ce3a9
