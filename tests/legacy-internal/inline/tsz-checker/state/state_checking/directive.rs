//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/state/state_checking/directive.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 7502d960818097d4a5edc8253c26aeef9c289e8a5daeefba38c89f2e559cd9f8 313 sibling_reference_is_relative_to_cwd
    #[test]
    fn sibling_reference_is_relative_to_cwd() {
        let resolved = Path::new("/proj/app/missing-a.d.ts");
        assert_eq!(
            display_reference_path(resolved, Some("/proj/app")),
            "missing-a.d.ts"
        );
    }
// TSZ_INLINE_TEST_END 7502d960818097d4a5edc8253c26aeef9c289e8a5daeefba38c89f2e559cd9f8

// TSZ_INLINE_TEST_BEGIN 2a618f00a58e002753181935b7fff052a10b1735c4f6cc68af149dd1bda0328c 322 parent_escaping_reference_keeps_single_dotdot
    #[test]
    fn parent_escaping_reference_keeps_single_dotdot() {
        let resolved = Path::new("/proj/up-missing.d.ts");
        assert_eq!(
            display_reference_path(resolved, Some("/proj/app")),
            "../up-missing.d.ts"
        );
    }
// TSZ_INLINE_TEST_END 2a618f00a58e002753181935b7fff052a10b1735c4f6cc68af149dd1bda0328c

// TSZ_INLINE_TEST_BEGIN 26ed6588228414e6326d4bd497238c2d496cc8fb9d09a2eb5b1237ec86c3eb9b 331 subdirectory_reference_keeps_prefix
    #[test]
    fn subdirectory_reference_keeps_prefix() {
        let resolved = Path::new("/proj/app/sub/deep-missing.d.ts");
        assert_eq!(
            display_reference_path(resolved, Some("/proj/app")),
            "sub/deep-missing.d.ts"
        );
    }
// TSZ_INLINE_TEST_END 26ed6588228414e6326d4bd497238c2d496cc8fb9d09a2eb5b1237ec86c3eb9b

// TSZ_INLINE_TEST_BEGIN accd37e42d817822a7f3489a4c06029bc4c5139e0559a676130313e92dd60361 340 dot_components_are_collapsed_before_relativizing
    #[test]
    fn dot_components_are_collapsed_before_relativizing() {
        let resolved = Path::new("/proj/app/sub/../x.d.ts");
        assert_eq!(
            display_reference_path(resolved, Some("/proj/app")),
            "x.d.ts"
        );
    }
// TSZ_INLINE_TEST_END accd37e42d817822a7f3489a4c06029bc4c5139e0559a676130313e92dd60361

// TSZ_INLINE_TEST_BEGIN 2da058d27546560405dc5f44e9ac40219cadc919577faeff525853ebd5d24565 349 without_current_directory_path_stays_absolute
    #[test]
    fn without_current_directory_path_stays_absolute() {
        let resolved = Path::new("/proj/app/missing-a.d.ts");
        assert_eq!(
            display_reference_path(resolved, None),
            "/proj/app/missing-a.d.ts"
        );
        assert_eq!(
            display_reference_path(resolved, Some("")),
            "/proj/app/missing-a.d.ts"
        );
    }
// TSZ_INLINE_TEST_END 2da058d27546560405dc5f44e9ac40219cadc919577faeff525853ebd5d24565

// TSZ_INLINE_TEST_BEGIN 829e85f718039e1c8bc5b2928bcc7c9e898eaf26e6702647df4506efc976b5e7 362 already_relative_path_is_left_untouched
    #[test]
    fn already_relative_path_is_left_untouched() {
        // A relative resolved path (source file stored relative) is already in
        // the form tsc keeps; do not attempt to relativize it further.
        let resolved = Path::new("app/missing-a.d.ts");
        assert_eq!(
            display_reference_path(resolved, Some("/proj/app")),
            "app/missing-a.d.ts"
        );
    }
// TSZ_INLINE_TEST_END 829e85f718039e1c8bc5b2928bcc7c9e898eaf26e6702647df4506efc976b5e7

// TSZ_INLINE_TEST_BEGIN 8ca567d8e27b40f47b8b1f0d7dc7d8febc628978b19f14ff4ea38c93176160db 373 absolute_reference_literal_is_detected_portably
    #[test]
    fn absolute_reference_literal_is_detected_portably() {
        assert!(super::reference_path_is_absolute("/tmp/missing.d.ts"));
        assert!(super::reference_path_is_absolute("C:/tmp/missing.d.ts"));
        assert!(super::reference_path_is_absolute("C:\\tmp\\missing.d.ts"));
        assert!(!super::reference_path_is_absolute("../missing.d.ts"));
        assert!(!super::reference_path_is_absolute("nested/missing.d.ts"));
    }
// TSZ_INLINE_TEST_END 8ca567d8e27b40f47b8b1f0d7dc7d8febc628978b19f14ff4ea38c93176160db
