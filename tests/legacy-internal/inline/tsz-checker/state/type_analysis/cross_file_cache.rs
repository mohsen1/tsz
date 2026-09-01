//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/state/type_analysis/cross_file_cache.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 2d016d22c0b7b86d180bd38ff14b51469420158b2ca527f3aed487d8aaa7ee0f 91 user_ts_files_are_classified_as_user_source
    #[test]
    fn user_ts_files_are_classified_as_user_source() {
        assert_eq!(
            classify_declaration_file_for_cache("src/main.ts", false),
            DeclarationFileCacheClass::UserSource,
        );
        assert_eq!(
            classify_declaration_file_for_cache("packages/foo/index.tsx", false),
            DeclarationFileCacheClass::UserSource,
        );
    }
// TSZ_INLINE_TEST_END 2d016d22c0b7b86d180bd38ff14b51469420158b2ca527f3aed487d8aaa7ee0f

// TSZ_INLINE_TEST_BEGIN cb9e25dcb8b341f89b622f7d84d0a57a3f43c49060003fc1a83c0fa4941c2286 103 dom_like_lib_files_are_cacheable_declaration_files
    #[test]
    fn dom_like_lib_files_are_cacheable_declaration_files() {
        for file_name in [
            "lib.dom.d.ts",
            "lib.dom.iterable.d.ts",
            "lib.dom.asynciterable.d.ts",
            "lib.webworker.d.ts",
            "lib.webworker.iterable.d.ts",
            "dom.generated.d.ts",
            "webworker.asynciterable.d.ts",
        ] {
            assert_eq!(
                classify_declaration_file_for_cache(file_name, true),
                DeclarationFileCacheClass::DomOrExternalPackage,
                "{file_name}",
            );
        }
    }
// TSZ_INLINE_TEST_END cb9e25dcb8b341f89b622f7d84d0a57a3f43c49060003fc1a83c0fa4941c2286

// TSZ_INLINE_TEST_BEGIN 97b1e1c75f2952ab234a753a9ac3e739cb804206f28eeb72c4d514dc040ab0a7 122 external_package_paths_with_separator_variants_route_through_cache
    #[test]
    fn external_package_paths_with_separator_variants_route_through_cache() {
        for file_name in [
            "node_modules/.pnpm/react@18.2.0/node_modules/react/index.d.ts",
            "/repo/node_modules/.pnpm/lodash@4.17.21/node_modules/lodash/index.d.ts",
            r"C:\repo\node_modules\@scope\pkg\sub\types.d.ts",
        ] {
            assert_eq!(
                classify_declaration_file_for_cache(file_name, true),
                DeclarationFileCacheClass::DomOrExternalPackage,
                "{file_name}",
            );
        }
    }
// TSZ_INLINE_TEST_END 97b1e1c75f2952ab234a753a9ac3e739cb804206f28eeb72c4d514dc040ab0a7

// TSZ_INLINE_TEST_BEGIN e0f7935ace327f7e6b6ccd7e9c3a905cb25d36445d5fe06ff3f682af0d4f2ad1 137 non_dom_builtin_lib_keeps_existing_shared_name_path
    #[test]
    fn non_dom_builtin_lib_keeps_existing_shared_name_path() {
        for file_name in [
            "lib.es5.d.ts",
            "lib.es2015.d.ts",
            "lib.es2015.core.d.ts",
            "lib.es2020.symbol.wellknown.d.ts",
            "lib.esnext.d.ts",
            "lib.decorators.d.ts",
            "lib.scripthost.d.ts",
            "/repo/node_modules/typescript/lib/lib.es5.d.ts",
            r"C:\repo\node_modules\typescript\lib\lib.es2020.symbol.wellknown.d.ts",
        ] {
            assert_eq!(
                classify_declaration_file_for_cache(file_name, true),
                DeclarationFileCacheClass::NonDomBuiltinLib,
                "{file_name}",
            );
        }
    }
// TSZ_INLINE_TEST_END e0f7935ace327f7e6b6ccd7e9c3a905cb25d36445d5fe06ff3f682af0d4f2ad1

// TSZ_INLINE_TEST_BEGIN ada144d75696d3c01e0e6e872ccacebcdc747fb6d7f36e8c4fb4aa590a42eb76 158 local_declaration_files_outside_node_modules_stay_on_legacy_path
    #[test]
    fn local_declaration_files_outside_node_modules_stay_on_legacy_path() {
        for file_name in [
            "packages/foo/src/types.d.ts",
            "/repo/fixtures/node-modules-like/types.d.ts",
        ] {
            assert_eq!(
                classify_declaration_file_for_cache(file_name, true),
                DeclarationFileCacheClass::NonDomBuiltinLib,
                "{file_name}",
            );
        }
    }
// TSZ_INLINE_TEST_END ada144d75696d3c01e0e6e872ccacebcdc747fb6d7f36e8c4fb4aa590a42eb76

// TSZ_INLINE_TEST_BEGIN dc06422f54a66e7226d9a4ae01b8421f09d66c9922ded78e67acafb143ffecc0 172 declaration_flag_drives_classification_even_for_lib_named_user_files
    #[test]
    fn declaration_flag_drives_classification_even_for_lib_named_user_files() {
        // `is_declaration_file` is the source of truth; the file-name check
        // is only a refinement *within* declaration files, so a user `.ts`
        // whose name collides with a lib stem must not be reclassified.
        assert_eq!(
            classify_declaration_file_for_cache("lib.dom.d.ts", false),
            DeclarationFileCacheClass::UserSource,
        );
        assert_eq!(
            classify_declaration_file_for_cache("node_modules/react/x.ts", false),
            DeclarationFileCacheClass::UserSource,
        );
    }
// TSZ_INLINE_TEST_END dc06422f54a66e7226d9a4ae01b8421f09d66c9922ded78e67acafb143ffecc0
