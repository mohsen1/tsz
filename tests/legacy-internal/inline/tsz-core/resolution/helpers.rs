//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-core/src/resolution/helpers.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN a0df1b1cfc51e8be2fd82efafd73c7d768a1e008c613f2617004329c6c6297f5 670 export_pattern_specificity_mirrors_pattern_key_compare
    #[test]
    fn export_pattern_specificity_mirrors_pattern_key_compare() {
        // Wildcard key: base = indexOf('*') + 1, flagged as a pattern.
        assert_eq!(export_pattern_specificity("./*"), (3, 1, 3));
        assert_eq!(export_pattern_specificity("./lib/*"), (7, 1, 7));
        assert_eq!(export_pattern_specificity("./*.js"), (3, 1, 6));
        // Directory / exact key: base = full length, not a pattern.
        assert_eq!(export_pattern_specificity("./"), (2, 0, 2));
        assert_eq!(export_pattern_specificity("./lib/"), (6, 0, 6));
        assert_eq!(export_pattern_specificity("./foo"), (5, 0, 5));
    }
// TSZ_INLINE_TEST_END a0df1b1cfc51e8be2fd82efafd73c7d768a1e008c613f2617004329c6c6297f5

// TSZ_INLINE_TEST_BEGIN 2fd033861407ee63fb62a9e8f11bb4068c653361eaa754c94f89b3bfde5a8077 682 find_best_export_pattern_prefers_wildcard_over_equal_base_directory_either_order
    #[test]
    fn find_best_export_pattern_prefers_wildcard_over_equal_base_directory_either_order() {
        // `"./*"` (base 3) must beat `"./"` (base 2) for `./foo`, no matter the
        // JSON declaration order. Before the PATTERN_KEY_COMPARE fix these tied
        // on `(prefix_len, suffix_len)` and the winner flipped with key order,
        // resolving the same specifier to different physical files between rows.
        assert_eq!(best_key(&["./", "./*"], "./foo"), Some("./*"));
        assert_eq!(best_key(&["./*", "./"], "./foo"), Some("./*"));
    }
// TSZ_INLINE_TEST_END 2fd033861407ee63fb62a9e8f11bb4068c653361eaa754c94f89b3bfde5a8077

// TSZ_INLINE_TEST_BEGIN 26ef85ac220c3b6745b40381f7ed79959fe289555ac91e13bd283a09df02cc7b 692 find_best_export_pattern_longer_directory_base_beats_short_wildcard
    #[test]
    fn find_best_export_pattern_longer_directory_base_beats_short_wildcard() {
        // A longer anchored directory prefix is more specific than a short
        // wildcard (Node orders by base length first): `"./lib/"` (base 6) wins
        // over `"./*"` (base 3) for `./lib/x`, independent of order.
        assert_eq!(best_key(&["./*", "./lib/"], "./lib/x"), Some("./lib/"));
        assert_eq!(best_key(&["./lib/", "./*"], "./lib/x"), Some("./lib/"));
        // …but a wildcard with an even longer base reclaims the win.
        assert_eq!(best_key(&["./lib/", "./lib/*"], "./lib/x"), Some("./lib/*"));
        assert_eq!(best_key(&["./lib/*", "./lib/"], "./lib/x"), Some("./lib/*"));
    }
// TSZ_INLINE_TEST_END 26ef85ac220c3b6745b40381f7ed79959fe289555ac91e13bd283a09df02cc7b

// TSZ_INLINE_TEST_BEGIN 19fb562529602c1b67decc56b4cf5ec9eb720131862753327235587b729805d2 704 find_best_export_pattern_orders_wildcards_by_base_then_total_length
    #[test]
    fn find_best_export_pattern_orders_wildcards_by_base_then_total_length() {
        // Longer prefix before `*` wins.
        assert_eq!(
            best_key(&["./*", "./feature/*"], "./feature/btn"),
            Some("./feature/*")
        );
        assert_eq!(
            best_key(&["./feature/*", "./*"], "./feature/btn"),
            Some("./feature/*")
        );
        // Equal base length → longer total (longer suffix) wins.
        assert_eq!(best_key(&["./*", "./*.js"], "./a.js"), Some("./*.js"));
        assert_eq!(best_key(&["./*.js", "./*"], "./a.js"), Some("./*.js"));
    }
// TSZ_INLINE_TEST_END 19fb562529602c1b67decc56b4cf5ec9eb720131862753327235587b729805d2

// TSZ_INLINE_TEST_BEGIN 98f888f3134bbb632f1d3d6fb2e46f9637aeb023657aa39a792f82f833e7c4bd 720 apply_wildcard_substitution_replaces_every_star_in_pattern_target
    #[test]
    fn apply_wildcard_substitution_replaces_every_star_in_pattern_target() {
        // Node `PACKAGE_TARGET_RESOLVE` / tsc `replace(/\*/g, subpath)`: a
        // pattern target with two or more `*` substitutes the captured subpath
        // into ALL of them. The prior `replacen(.., 1)` left a literal `*` in
        // the resolved path, which never exists on disk → spurious TS2307.
        assert_eq!(
            apply_wildcard_substitution("./dist/*/*.js", "button", false),
            "./dist/button/button.js"
        );
        assert_eq!(
            apply_wildcard_substitution("./*/*/*.d.ts", "a/b", false),
            "./a/b/a/b/a/b.d.ts"
        );
        // Single-star and no-star targets are unchanged by the fix (the common
        // case): the two replacement strategies coincide for one occurrence.
        assert_eq!(
            apply_wildcard_substitution("./dist/*.js", "index", false),
            "./dist/index.js"
        );
        assert_eq!(
            apply_wildcard_substitution("./dist/index.js", "ignored", false),
            "./dist/index.js"
        );
    }
// TSZ_INLINE_TEST_END 98f888f3134bbb632f1d3d6fb2e46f9637aeb023657aa39a792f82f833e7c4bd

// TSZ_INLINE_TEST_BEGIN e78cd162f5646cadb066190932e9dfd7c2c1721af9d2835cf380d74d46036e24 746 substitute_wildcard_in_exports_replaces_every_star_through_nesting
    #[test]
    fn substitute_wildcard_in_exports_replaces_every_star_through_nesting() {
        // The multi-`*` substitution must hold through conditional/array
        // nesting, since `substitute_wildcard_in_exports` recurses to every
        // string leaf. A barrel like `"./*": { "types": "./types/*/*.d.ts",
        // "default": "./dist/*/*.js" }` must not strand a literal `*`.
        let value = PackageExports::Conditional(vec![
            (
                "types".to_string(),
                PackageExports::String("./types/*/*.d.ts".to_string()),
            ),
            (
                "default".to_string(),
                PackageExports::Array(vec![
                    PackageExports::String("./dist/*/*.mjs".to_string()),
                    PackageExports::String("./dist/*.cjs".to_string()),
                ]),
            ),
        ]);

        let substituted = substitute_wildcard_in_exports(&value, "widget", false);

        let PackageExports::Conditional(entries) = substituted else {
            panic!("expected a conditional value after substitution");
        };
        let leaves: Vec<String> = entries
            .iter()
            .flat_map(|(_, v)| collect_string_leaves(v))
            .collect();
        assert_eq!(
            leaves,
            vec![
                "./types/widget/widget.d.ts".to_string(),
                "./dist/widget/widget.mjs".to_string(),
                "./dist/widget.cjs".to_string(),
            ]
        );
        // No literal `*` survives anywhere in the substituted tree.
        assert!(leaves.iter().all(|leaf| !leaf.contains('*')));
    }
// TSZ_INLINE_TEST_END e78cd162f5646cadb066190932e9dfd7c2c1721af9d2835cf380d74d46036e24

// TSZ_INLINE_TEST_BEGIN 6514be9095836333fb68736338a2043403fbe904c731f1ded83513227fb50c51 803 types_versions_compiler_version_uses_trimmed_value_and_fallback
    #[test]
    fn types_versions_compiler_version_uses_trimmed_value_and_fallback() {
        assert_eq!(
            types_versions_compiler_version(Some(" 5.4 ")),
            SemVer {
                major: 5,
                minor: 4,
                patch: 0,
            }
        );
        assert_eq!(
            types_versions_compiler_version(Some("not-a-version")),
            default_types_versions_compiler_version()
        );
        assert_eq!(
            types_versions_compiler_version(None),
            SemVer {
                major: 7,
                minor: 0,
                patch: 2,
            }
        );
    }
// TSZ_INLINE_TEST_END 6514be9095836333fb68736338a2043403fbe904c731f1ded83513227fb50c51

// TSZ_INLINE_TEST_BEGIN f0541ed0443684c8b4a8d6dd8f9a942efe3c0f46dc50bd6e680fb0b44fa7b3b9 827 parse_semver_ignores_prerelease_and_build_metadata
    #[test]
    fn parse_semver_ignores_prerelease_and_build_metadata() {
        assert_eq!(
            parse_semver("3.1.0-0"),
            Some(SemVer {
                major: 3,
                minor: 1,
                patch: 0,
            })
        );
        assert_eq!(
            parse_semver("5.4.1+dev"),
            Some(SemVer {
                major: 5,
                minor: 4,
                patch: 1,
            })
        );
    }
// TSZ_INLINE_TEST_END f0541ed0443684c8b4a8d6dd8f9a942efe3c0f46dc50bd6e680fb0b44fa7b3b9

// TSZ_INLINE_TEST_BEGIN 9450dedf7d61fd14cedec87931a5159aecd70df8677b7105e04240e58471427b 847 select_types_versions_paths_returns_first_matching_key_in_declaration_order
    #[test]
    fn select_types_versions_paths_returns_first_matching_key_in_declaration_order() {
        // tsc's `getPackageJsonTypesVersionsPaths` is a `for...in` loop that
        // returns the first key whose range satisfies the compiler version.
        // With `"*"` declared first, every later key is unreachable — even a
        // tighter `">=5.4"` range. This pins parity with that behavior.
        let types_versions = json!({
            "*": { "*": ["fallback/index.d.ts"] },
            ">=5.4": { "*": ["modern/index.d.ts"] },
            ">=5.2 <5.4": { "*": ["mid/index.d.ts"] }
        });

        let selected = select_types_versions_paths(
            &types_versions,
            SemVer {
                major: 5,
                minor: 4,
                patch: 1,
            },
        )
        .expect("expected a matching typesVersions entry");

        assert_eq!(selected.get("*"), Some(&json!(["fallback/index.d.ts"])));

        // The natural ordering — fallback last — picks the tighter range.
        let types_versions_natural = json!({
            ">=5.4": { "*": ["modern/index.d.ts"] },
            ">=5.2 <5.4": { "*": ["mid/index.d.ts"] },
            "*": { "*": ["fallback/index.d.ts"] }
        });

        let selected_natural = select_types_versions_paths(
            &types_versions_natural,
            SemVer {
                major: 5,
                minor: 4,
                patch: 1,
            },
        )
        .expect("expected a matching typesVersions entry");

        assert_eq!(
            selected_natural.get("*"),
            Some(&json!(["modern/index.d.ts"]))
        );
    }
// TSZ_INLINE_TEST_END 9450dedf7d61fd14cedec87931a5159aecd70df8677b7105e04240e58471427b

// TSZ_INLINE_TEST_BEGIN 606d560f8b6068fd9469f1a2a620382cdd28fccb956c530c7e9a7929f63541ad 894 select_types_versions_paths_ties_resolve_to_first_in_declaration_order
    #[test]
    fn select_types_versions_paths_ties_resolve_to_first_in_declaration_order() {
        // Two equally-matching keys: tsc picks whichever was declared first,
        // regardless of lex order or constraint count.
        let first_wins = json!({
            "<=6.0": { "*": ["first/index.d.ts"] },
            "<=5.0": { "*": ["second/index.d.ts"] }
        });

        let selected = select_types_versions_paths(
            &first_wins,
            SemVer {
                major: 4,
                minor: 9,
                patch: 0,
            },
        )
        .expect("expected a matching typesVersions entry");

        assert_eq!(selected.get("*"), Some(&json!(["first/index.d.ts"])));

        // Same content, reversed declaration order — the (now-first) `<=5.0`
        // key wins instead.
        let reversed = json!({
            "<=5.0": { "*": ["second/index.d.ts"] },
            "<=6.0": { "*": ["first/index.d.ts"] }
        });

        let selected_reversed = select_types_versions_paths(
            &reversed,
            SemVer {
                major: 4,
                minor: 9,
                patch: 0,
            },
        )
        .expect("expected a matching typesVersions entry");

        assert_eq!(
            selected_reversed.get("*"),
            Some(&json!(["second/index.d.ts"]))
        );
    }
// TSZ_INLINE_TEST_END 606d560f8b6068fd9469f1a2a620382cdd28fccb956c530c7e9a7929f63541ad

// TSZ_INLINE_TEST_BEGIN 262031172e439ab6a2c73ed18b17f343a249ec0ed258f8d024a6a0e7255407f2 938 select_types_versions_paths_skips_unparseable_keys
    #[test]
    fn select_types_versions_paths_skips_unparseable_keys() {
        // An invalid range key parses as `None` and is skipped; iteration
        // continues to the next valid key.
        let types_versions = json!({
            "not-a-range": { "*": ["skipped/index.d.ts"] },
            ">=5.4": { "*": ["modern/index.d.ts"] }
        });

        let selected = select_types_versions_paths(
            &types_versions,
            SemVer {
                major: 5,
                minor: 4,
                patch: 1,
            },
        )
        .expect("expected a matching typesVersions entry");

        assert_eq!(selected.get("*"), Some(&json!(["modern/index.d.ts"])));
    }
// TSZ_INLINE_TEST_END 262031172e439ab6a2c73ed18b17f343a249ec0ed258f8d024a6a0e7255407f2

// TSZ_INLINE_TEST_BEGIN 4930b205a93bd3024cff7a68fda9399ccc5215aebd5ab529481cb7352503fd1e 960 types_versions_range_matches_bare_star_and_empty
    #[test]
    fn types_versions_range_matches_bare_star_and_empty() {
        let v = SemVer {
            major: 6,
            minor: 0,
            patch: 0,
        };
        assert!(types_versions_range_matches("*", v));
        assert!(types_versions_range_matches("", v));
        assert!(types_versions_range_matches(">=4 <7", v));
        assert!(!types_versions_range_matches(">=7", v));
        // Disjunction: any segment may match.
        assert!(types_versions_range_matches(">=7 || <=6", v));
        // Invalid token in one segment fails just that segment.
        assert!(types_versions_range_matches(">=garbage || >=4", v));
    }
// TSZ_INLINE_TEST_END 4930b205a93bd3024cff7a68fda9399ccc5215aebd5ab529481cb7352503fd1e

// TSZ_INLINE_TEST_BEGIN 05c27103c487e01122221525893c804833310107d248a5fdcf1bfa848a5f23f3 983 split_path_extension_prefers_longest_known_declaration_extension
    #[test]
    fn split_path_extension_prefers_longest_known_declaration_extension() {
        let (base, extension) =
            split_path_extension(Path::new("pkg/index.d.mts")).expect("expected known extension");
        assert_eq!(base, PathBuf::from("pkg/index"));
        assert_eq!(extension, "d.mts");

        let (base, extension) =
            split_path_extension(Path::new("pkg/index.d.ts")).expect("expected known extension");
        assert_eq!(base, PathBuf::from("pkg/index"));
        assert_eq!(extension, "d.ts");
    }
// TSZ_INLINE_TEST_END 05c27103c487e01122221525893c804833310107d248a5fdcf1bfa848a5f23f3

// TSZ_INLINE_TEST_BEGIN a99a4ef1266125a25c1a7c7a2e16bf723ecbc485a6a8d9b1a1ca5b685c4a67ad 996 declaration_extension_substitution_probes_sibling_implementations
    #[test]
    fn declaration_extension_substitution_probes_sibling_implementations() {
        let dts = node16_extension_substitution(Path::new("pkg/a.d.ts"), "d.ts")
            .expect("expected declaration extension substitution");
        assert_eq!(
            dts,
            vec![PathBuf::from("pkg/a.ts"), PathBuf::from("pkg/a.tsx")]
        );

        let dmts = node16_extension_substitution(Path::new("pkg/a.d.mts"), "d.mts")
            .expect("expected declaration module substitution");
        assert_eq!(dmts, vec![PathBuf::from("pkg/a.mts")]);

        let dcts = node16_extension_substitution(Path::new("pkg/a.d.cts"), "d.cts")
            .expect("expected declaration commonjs substitution");
        assert_eq!(dcts, vec![PathBuf::from("pkg/a.cts")]);
    }
// TSZ_INLINE_TEST_END a99a4ef1266125a25c1a7c7a2e16bf723ecbc485a6a8d9b1a1ca5b685c4a67ad

// TSZ_INLINE_TEST_BEGIN fdd2e152e98f814b8363c57b25f2190e0d1fb6ac0352d74cabd60505eb1a2767 1014 try_file_with_suffixes_and_extension_returns_first_existing_candidate
    #[test]
    fn try_file_with_suffixes_and_extension_returns_first_existing_candidate() {
        let dir = tempdir().expect("create temp dir");
        let base = dir.path().join("component");
        let preferred = dir.path().join("component.native.ts");
        let fallback = dir.path().join("component.web.ts");

        std::fs::write(&preferred, "").expect("write preferred candidate");
        std::fs::write(&fallback, "").expect("write fallback candidate");

        let resolved = try_file_with_suffixes_and_extension(
            &base,
            "ts",
            &[".native".to_string(), ".web".to_string()],
        )
        .expect("expected one suffix candidate to resolve");

        assert_eq!(resolved, preferred);
    }
// TSZ_INLINE_TEST_END fdd2e152e98f814b8363c57b25f2190e0d1fb6ac0352d74cabd60505eb1a2767

// TSZ_INLINE_TEST_BEGIN 7c39ebf74fad12fc91c75c402d3b82f2fee79892d6bd9796ed59a9b269bbf4f8 1034 resolve_explicit_unknown_extension_accepts_existing_nonstandard_files_only
    #[test]
    fn resolve_explicit_unknown_extension_accepts_existing_nonstandard_files_only() {
        let dir = tempdir().expect("create temp dir");
        let custom = dir.path().join("entry.custom");
        let known = dir.path().join("entry.ts");
        let no_extension = dir.path().join("entry");

        std::fs::write(&custom, "").expect("write custom extension file");
        std::fs::write(&known, "").expect("write known extension file");
        std::fs::write(&no_extension, "").expect("write extensionless file");

        assert_eq!(
            resolve_explicit_unknown_extension(&custom),
            Some(custom.clone())
        );
        assert_eq!(resolve_explicit_unknown_extension(&known), None);
        assert_eq!(resolve_explicit_unknown_extension(&no_extension), None);
    }
// TSZ_INLINE_TEST_END 7c39ebf74fad12fc91c75c402d3b82f2fee79892d6bd9796ed59a9b269bbf4f8

// TSZ_INLINE_TEST_BEGIN c1836534be718fe79ea81e4d297a973102d1f8a9cbc9b59c9f27614a317a86aa 1053 node16_and_main_declaration_substitutions_cover_js_family_extensions
    #[test]
    fn node16_and_main_declaration_substitutions_cover_js_family_extensions() {
        assert_eq!(
            node16_extension_substitution(Path::new("pkg/index.js"), "js"),
            Some(vec![
                PathBuf::from("pkg/index.ts"),
                PathBuf::from("pkg/index.tsx"),
                PathBuf::from("pkg/index.d.ts"),
            ])
        );
        assert_eq!(
            node16_extension_substitution(Path::new("pkg/index.mjs"), "mjs"),
            Some(vec![
                PathBuf::from("pkg/index.mts"),
                PathBuf::from("pkg/index.d.mts"),
            ])
        );
        assert_eq!(
            declaration_substitution_for_main(Path::new("pkg/index.cjs")),
            Some(PathBuf::from("pkg/index.d.cts"))
        );
        assert_eq!(
            declaration_substitution_for_main(Path::new("pkg/index.jsx")),
            Some(PathBuf::from("pkg/index.d.ts"))
        );
        assert_eq!(
            declaration_substitution_for_main(Path::new("pkg/index.ts")),
            None
        );
    }
// TSZ_INLINE_TEST_END c1836534be718fe79ea81e4d297a973102d1f8a9cbc9b59c9f27614a317a86aa

// TSZ_INLINE_TEST_BEGIN fb0d8b4296dd856316aa2dba0ee7c3862bb8e5086ee7520f8a6402f967d4c712 1084 path_existence_caches_are_stable_until_reset_for_files_and_directories
    #[test]
    fn path_existence_caches_are_stable_until_reset_for_files_and_directories() {
        clear_path_existence_caches();
        let root = tempdir().expect("create temp dir");
        let file = root.path().join("index.ts");
        let dir = root.path().join("nested");
        std::fs::write(&file, "").expect("write probed file");
        std::fs::create_dir(&dir).expect("create probed directory");

        // First probes record the file and directory as present.
        assert!(cached_is_file(&file));
        assert!(cached_is_dir(&dir));

        // Remove both underneath the caches. Within a single compilation the
        // filesystem is assumed stable, so the cached answers are reused even
        // though the paths are now gone. This is what collapses the repeated
        // `stat()` syscalls the resolver would otherwise issue for the same
        // files and ancestor directories across every import.
        std::fs::remove_file(&file).expect("remove probed file");
        std::fs::remove_dir(&dir).expect("remove probed directory");
        assert!(cached_is_file(&file));
        assert!(cached_is_dir(&dir));

        // The unified reset clears both caches (not just the file cache), so
        // the next compilation cycle re-reads the real filesystem state.
        clear_path_existence_caches();
        assert!(!cached_is_file(&file));
        assert!(!cached_is_dir(&dir));
    }
// TSZ_INLINE_TEST_END fb0d8b4296dd856316aa2dba0ee7c3862bb8e5086ee7520f8a6402f967d4c712
