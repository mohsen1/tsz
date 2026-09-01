//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-common/src/module_resolution/package_exports.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 532dc5ecba18c6dfb95a2f847f8f6bab37a7c2fde8f446d05adc0f3da017d702 164 reverse_match_root_export_returns_empty_subpath
    #[test]
    fn reverse_match_root_export_returns_empty_subpath() {
        let exports = json!("./dist/index.js");
        assert_eq!(
            reverse_match_exports_subpath(&exports, "./dist/index.js"),
            Some(String::new())
        );
    }
// TSZ_INLINE_TEST_END 532dc5ecba18c6dfb95a2f847f8f6bab37a7c2fde8f446d05adc0f3da017d702

// TSZ_INLINE_TEST_BEGIN bf81eda74fffc4c9f5941ad3be378137f8ae0448bde58396670af8380ecc52d9 173 reverse_match_subpath_wildcard_applies_captured_middle
    #[test]
    fn reverse_match_subpath_wildcard_applies_captured_middle() {
        let exports = json!({
            "./feature/*": "./dist/feature/*.js"
        });

        assert_eq!(
            reverse_match_exports_subpath(&exports, "./dist/feature/a/b.js"),
            Some("feature/a/b".to_string())
        );
    }
// TSZ_INLINE_TEST_END bf81eda74fffc4c9f5941ad3be378137f8ae0448bde58396670af8380ecc52d9

// TSZ_INLINE_TEST_BEGIN 406b21316a2a5adb0c71ebbc49ea3618a0b6fe2e65e6d1dd3d554f6a330f76e8 185 reverse_match_preserves_condition_order
    #[test]
    fn reverse_match_preserves_condition_order() {
        let exports = json!({
            ".": {
                "types": "./dist/index.d.ts",
                "default": "./dist/index.js"
            }
        });

        assert_eq!(
            reverse_match_exports_subpath(&exports, "./dist/index.d.ts"),
            Some(String::new())
        );
        assert_eq!(
            reverse_match_exports_subpath(&exports, "./dist/index.js"),
            Some(String::new())
        );
    }
// TSZ_INLINE_TEST_END 406b21316a2a5adb0c71ebbc49ea3618a0b6fe2e65e6d1dd3d554f6a330f76e8

// TSZ_INLINE_TEST_BEGIN 738bad70c9659876a308e50b00b5935a7848a9efd0af4d94f6f7669db0f98272 204 match_export_target_supports_folder_entries
    #[test]
    fn match_export_target_supports_folder_entries() {
        assert_eq!(
            match_export_target("./feature/", "./dist/feature/", "./dist/feature/a.js"),
            Some("feature/a.js".to_string())
        );
    }
// TSZ_INLINE_TEST_END 738bad70c9659876a308e50b00b5935a7848a9efd0af4d94f6f7669db0f98272

// TSZ_INLINE_TEST_BEGIN 0eb1d804e2039b1f15546cad890909c2e3677dd9024a846baad9698a2b4cf5fa 212 pattern_key_specificity_mirrors_compare_pattern_keys
    #[test]
    fn pattern_key_specificity_mirrors_compare_pattern_keys() {
        // Non-wildcard (exact / `/`-directory) keys: base == total == full length.
        assert_eq!(pattern_key_specificity("./exact.js"), (10, 0, 10));
        assert_eq!(pattern_key_specificity("./"), (2, 0, 2));
        assert_eq!(pattern_key_specificity("./lib/"), (6, 0, 6));
        // Wildcard keys: base == indexOf('*') + 1, is_pattern == 1.
        assert_eq!(pattern_key_specificity("./*"), (3, 1, 3));
        assert_eq!(pattern_key_specificity("./lib/*"), (7, 1, 7));
        assert_eq!(pattern_key_specificity("./*.ts"), (3, 1, 6));

        // A wildcard strictly outranks the directory key it shares a base with —
        // the order-independence the 2-tuple comparator lost.
        assert!(pattern_key_specificity("./*") > pattern_key_specificity("./"));
        assert!(pattern_key_specificity("./lib/*") > pattern_key_specificity("./lib/"));
        // Longer base still wins first (directory beats shorter wildcard).
        assert!(pattern_key_specificity("./lib/") > pattern_key_specificity("./*"));
        // Equal base, both wildcards: longer total (suffix) wins.
        assert!(pattern_key_specificity("./*.js") > pattern_key_specificity("./*"));
    }
// TSZ_INLINE_TEST_END 0eb1d804e2039b1f15546cad890909c2e3677dd9024a846baad9698a2b4cf5fa
