//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-core/src/module_resolver/request_types.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 0fd0f47db5026c804aa6661fdef274aeb8bd8690d310f7fb46f11791d74a137d 523 collapses_curdir_join_artifacts
    #[test]
    fn collapses_curdir_join_artifacts() {
        let p = Path::new("a/./b/./c.js");
        assert_eq!(normalize_display_path(p).to_string_lossy(), "a/b/c.js");
    }
// TSZ_INLINE_TEST_END 0fd0f47db5026c804aa6661fdef274aeb8bd8690d310f7fb46f11791d74a137d

// TSZ_INLINE_TEST_BEGIN 1a8a632289f66796d2d05cd2db729da0a29af33c4323cb52bce94506505af03d 529 collapses_parentdir_against_real_directory
    #[test]
    fn collapses_parentdir_against_real_directory() {
        let p = Path::new("a/b/../c.js");
        assert_eq!(normalize_display_path(p).to_string_lossy(), "a/c.js");
    }
// TSZ_INLINE_TEST_END 1a8a632289f66796d2d05cd2db729da0a29af33c4323cb52bce94506505af03d

// TSZ_INLINE_TEST_BEGIN 504a5f96e94cb1385dedd37ed3a3a8b1396739743e2419a168c9a351c75e2ce3 535 preserves_leading_double_dotdot
    #[test]
    fn preserves_leading_double_dotdot() {
        let p = Path::new("../../foo.js");
        assert_eq!(normalize_display_path(p).to_string_lossy(), "../../foo.js");
    }
// TSZ_INLINE_TEST_END 504a5f96e94cb1385dedd37ed3a3a8b1396739743e2419a168c9a351c75e2ce3

// TSZ_INLINE_TEST_BEGIN d0642171836d1df7e01fa8c167f0515d2c39591ce38d9149634e9de361725922 541 preserves_leading_single_dotdot
    #[test]
    fn preserves_leading_single_dotdot() {
        let p = Path::new("../foo.js");
        assert_eq!(normalize_display_path(p).to_string_lossy(), "../foo.js");
    }
// TSZ_INLINE_TEST_END d0642171836d1df7e01fa8c167f0515d2c39591ce38d9149634e9de361725922

// TSZ_INLINE_TEST_BEGIN ce7c89a0a69a16b455a5b28d853e972585e7178960275ace9f2d8ea8a9ebc250 547 does_not_pop_leading_parentdir_when_followed_by_more
    #[test]
    fn does_not_pop_leading_parentdir_when_followed_by_more() {
        let p = Path::new("../../../x");
        assert_eq!(normalize_display_path(p).to_string_lossy(), "../../../x");
    }
// TSZ_INLINE_TEST_END ce7c89a0a69a16b455a5b28d853e972585e7178960275ace9f2d8ea8a9ebc250

// TSZ_INLINE_TEST_BEGIN bdf43e24cba7b613ed55e70321aed94f1f0b4604ee5864160ffac88a63ac9061 553 clamps_excess_parent_segments_at_root
    #[test]
    fn clamps_excess_parent_segments_at_root() {
        // Canonical `normalize_segments` semantics: tsc/Node clamp `..` at
        // the filesystem root. The historical local loop spelled this
        // `/../b`; resolved module paths cannot underflow the root, so no
        // TS7016 fingerprint can observe the difference.
        let p = Path::new("/a/../../b");
        assert_eq!(normalize_display_path(p).to_string_lossy(), "/b");
    }
// TSZ_INLINE_TEST_END bdf43e24cba7b613ed55e70321aed94f1f0b4604ee5864160ffac88a63ac9061
