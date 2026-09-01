//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-cli/src/driver/resolution/exports_imports.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 8d09977ab958be1452df9dab35cfa33e6c1d33af5af50e4fc0c3a59a1e0c4dfa 460 exports_target_nested_matching_null_blocks_outer_default
    #[test]
    fn exports_target_nested_matching_null_blocks_outer_default() {
        // node -> require:null blocks; neither the inner `default` nor the outer
        // `default` is reached.
        let exports = serde_json::json!({
            "node": { "require": null, "default": "./inner.js" },
            "default": "./outer.js"
        });
        assert_eq!(
            resolve_exports_target(&exports, CJS_CONDS, TEST_VERSION),
            TargetMatch::Blocked,
        );
    }
// TSZ_INLINE_TEST_END 8d09977ab958be1452df9dab35cfa33e6c1d33af5af50e4fc0c3a59a1e0c4dfa

// TSZ_INLINE_TEST_BEGIN f51360bc4e4725f20f931954546b7446c79ae7ccd9148ecf88ae01e1af66a8ae 474 exports_target_top_level_matching_null_blocks_sibling
    #[test]
    fn exports_target_top_level_matching_null_blocks_sibling() {
        let exports = serde_json::json!({ "node": null, "default": "./fallback.js" });
        assert_eq!(
            resolve_exports_target(&exports, CJS_CONDS, TEST_VERSION),
            TargetMatch::Blocked,
        );
    }
// TSZ_INLINE_TEST_END f51360bc4e4725f20f931954546b7446c79ae7ccd9148ecf88ae01e1af66a8ae

// TSZ_INLINE_TEST_BEGIN e49940007c8eb4f992fbd3d00e3c36193fcfd677b87496f009af986e9b1ab6e6 483 exports_target_null_on_unmatched_condition_resolves_default
    #[test]
    fn exports_target_null_on_unmatched_condition_resolves_default() {
        // `import` is not a CommonJS condition, so its null is never reached.
        let exports = serde_json::json!({ "import": null, "default": "./present.js" });
        assert_eq!(
            resolve_exports_target(&exports, CJS_CONDS, TEST_VERSION),
            TargetMatch::Resolved("./present.js".to_string()),
        );
    }
// TSZ_INLINE_TEST_END e49940007c8eb4f992fbd3d00e3c36193fcfd677b87496f009af986e9b1ab6e6

// TSZ_INLINE_TEST_BEGIN 346bee445495a1f5a9d25bfc01baf20189fb5e1dd9bdb7358783f929050f71fe 493 exports_target_null_array_element_blocks_remaining
    #[test]
    fn exports_target_null_array_element_blocks_remaining() {
        let exports = serde_json::json!([null, "./real.js"]);
        assert_eq!(
            resolve_exports_target(&exports, CJS_CONDS, TEST_VERSION),
            TargetMatch::Blocked,
        );
    }
// TSZ_INLINE_TEST_END 346bee445495a1f5a9d25bfc01baf20189fb5e1dd9bdb7358783f929050f71fe

// TSZ_INLINE_TEST_BEGIN 300963d9a3ad19c9a1f10f3c6d776c7d7ba3e30e06273c8693b3a3c582830a9b 502 exports_subpath_exact_null_key_blocks_without_pattern_fallthrough
    #[test]
    fn exports_subpath_exact_null_key_blocks_without_pattern_fallthrough() {
        // `"./blocked": null` blocks even though `"./*"` would otherwise match;
        // a different subpath still resolves through the wildcard.
        let exports = serde_json::json!({ "./blocked": null, "./*": "./impl/*.js" });
        assert_eq!(
            resolve_exports_subpath(&exports, "./blocked", &["default"], TEST_VERSION),
            TargetMatch::Blocked,
        );
        assert_eq!(
            resolve_exports_subpath(&exports, "./allowed", &["default"], TEST_VERSION)
                .into_option()
                .as_deref(),
            Some("./impl/allowed.js"),
        );
    }
// TSZ_INLINE_TEST_END 300963d9a3ad19c9a1f10f3c6d776c7d7ba3e30e06273c8693b3a3c582830a9b

// TSZ_INLINE_TEST_BEGIN 9229e242a056574b5a0201a3a801cdfd2253c254869912dcd8dc3cfd72732041 519 imports_candidates_nested_matching_null_blocks_and_yields_no_candidates
    #[test]
    fn imports_candidates_nested_matching_null_blocks_and_yields_no_candidates() {
        // The `#imports` twin: a nested matching null blocks the collection, so
        // the outer `default` is never accumulated and the import is unresolved.
        let imports = serde_json::json!({
            "#feature": {
                "node": { "require": null, "default": "./inner.js" },
                "default": "./outer.js"
            }
        });
        assert!(
            resolve_imports_subpath_candidates_with_flavor(
                &imports,
                "#feature",
                CJS_CONDS,
                TEST_VERSION,
            )
            .is_empty(),
            "nested matching null must yield no #imports candidates"
        );
    }
// TSZ_INLINE_TEST_END 9229e242a056574b5a0201a3a801cdfd2253c254869912dcd8dc3cfd72732041

// TSZ_INLINE_TEST_BEGIN 3e7bf552592c9c7bef0ffaf6636320f9f1ded1527f10619fed246f119bd0b688 541 imports_candidates_null_on_unmatched_condition_resolves_default
    #[test]
    fn imports_candidates_null_on_unmatched_condition_resolves_default() {
        let imports = serde_json::json!({
            "#feature": { "import": null, "default": "./present.js" }
        });
        assert_eq!(
            resolve_imports_subpath_candidates_with_flavor(
                &imports,
                "#feature",
                CJS_CONDS,
                TEST_VERSION,
            ),
            vec![("./present.js".to_string(), false)],
        );
    }
// TSZ_INLINE_TEST_END 3e7bf552592c9c7bef0ffaf6636320f9f1ded1527f10619fed246f119bd0b688

// TSZ_INLINE_TEST_BEGIN 174cfe5d2acb69980306a864132e8a86bc3473eb7d03abcdc59b542984acf170 562 resolve_exports_subpath_wildcard_beats_directory_key_in_either_order
    #[test]
    fn resolve_exports_subpath_wildcard_beats_directory_key_in_either_order() {
        // Regression: a `*` key is strictly more specific than the trailing-slash
        // directory key it shares a base with, so `tsc` always picks the wildcard
        // target regardless of JSON authoring order. The prior 2-tuple comparator
        // tied them and let JSON order pick the (wrong) directory target.
        for exports in [
            serde_json::json!({ "./lib/": "./d/", "./lib/*": "./s/*" }),
            serde_json::json!({ "./lib/*": "./s/*", "./lib/": "./d/" }),
        ] {
            assert_eq!(
                resolve_exports_subpath(&exports, "./lib/foo.js", &["default"], TEST_VERSION)
                    .into_option()
                    .as_deref(),
                Some("./s/foo.js"),
            );
        }

        // The bare-`"./"` directory sugar vs `"./*"` shows the same fix.
        for exports in [
            serde_json::json!({ "./": "./pub/", "./*": "./src/*" }),
            serde_json::json!({ "./*": "./src/*", "./": "./pub/" }),
        ] {
            assert_eq!(
                resolve_exports_subpath(&exports, "./foo.js", &["default"], TEST_VERSION)
                    .into_option()
                    .as_deref(),
                Some("./src/foo.js"),
            );
        }
    }
// TSZ_INLINE_TEST_END 174cfe5d2acb69980306a864132e8a86bc3473eb7d03abcdc59b542984acf170

// TSZ_INLINE_TEST_BEGIN c3fa620b6d6821662fa3ebf072efb8dc9b75af164dfed6c84188fc7d6513c921 594 resolve_exports_subpath_uses_prefix_specificity_not_total_length
    #[test]
    fn resolve_exports_subpath_uses_prefix_specificity_not_total_length() {
        // Regression: with the old `key.len()` rule, `./abc/*` (length 7) and
        // `./*/abc` (length 7) tied, so the JSON ordering decided who won.
        // With the `(prefix_len, suffix_len)` tuple, `./abc/*` (prefix 6) is
        // always the strict winner for `./abc/abc` regardless of JSON order.
        for exports in [
            serde_json::json!({
                "./*/abc": "./by-star-abc.js",
                "./abc/*": "./by-abc-star.js"
            }),
            serde_json::json!({
                "./abc/*": "./by-abc-star.js",
                "./*/abc": "./by-star-abc.js"
            }),
        ] {
            assert_eq!(
                resolve_exports_subpath(&exports, "./abc/abc", &["default"], TEST_VERSION)
                    .into_option()
                    .as_deref(),
                Some("./by-abc-star.js"),
            );
        }
    }
// TSZ_INLINE_TEST_END c3fa620b6d6821662fa3ebf072efb8dc9b75af164dfed6c84188fc7d6513c921

// TSZ_INLINE_TEST_BEGIN c05f2fb3d08e5f0521c9923fe35ea3bdd8b9ea7ac746024e72aabb3a24f1ca2a 619 resolve_exports_subpath_true_ties_resolve_to_first_in_json_order
    #[test]
    fn resolve_exports_subpath_true_ties_resolve_to_first_in_json_order() {
        // When the `(prefix_len, suffix_len)` tuples are truly equal, the spec
        // says first-in-source-order wins. `serde_json` is built with
        // `preserve_order`, so iteration follows the JSON authoring order.
        let exports = serde_json::json!({
            "./a/*": "./first.js",
            "./b/*": "./second.js"
        });
        assert_eq!(
            resolve_exports_subpath(&exports, "./a/x", &["default"], TEST_VERSION)
                .into_option()
                .as_deref(),
            Some("./first.js"),
        );

        let exports_reversed = serde_json::json!({
            "./b/*": "./second.js",
            "./a/*": "./first.js"
        });
        assert_eq!(
            resolve_exports_subpath(&exports_reversed, "./b/x", &["default"], TEST_VERSION)
                .into_option()
                .as_deref(),
            Some("./second.js"),
        );
    }
// TSZ_INLINE_TEST_END c05f2fb3d08e5f0521c9923fe35ea3bdd8b9ea7ac746024e72aabb3a24f1ca2a

// TSZ_INLINE_TEST_BEGIN d3e8af4062366caf2d9a2adf39777e20db53175931587318ae719c873e171e5e 647 apply_exports_subpath_replaces_every_star_in_target
    #[test]
    fn apply_exports_subpath_replaces_every_star_in_target() {
        // Node `PACKAGE_TARGET_RESOLVE` / tsc `replace(/\*/g, subpath)`: every
        // `*` in the target is substituted with the captured subpath, matching
        // the tsz-core resolver's `apply_wildcard_substitution`. The prior
        // first-`*`-only substitution stranded a literal `*` (→ spurious TS2307).
        assert_eq!(
            apply_exports_subpath("./dist/*/*.js", "button"),
            "./dist/button/button.js"
        );
        assert_eq!(apply_exports_subpath("./*/*/*.d.ts", "a"), "./a/a/a.d.ts");
        // Single-star, no-star, and trailing-slash directory targets are
        // unaffected by the fix.
        assert_eq!(
            apply_exports_subpath("./dist/*.js", "index"),
            "./dist/index.js"
        );
        assert_eq!(
            apply_exports_subpath("./dist/index.js", "x"),
            "./dist/index.js"
        );
        assert_eq!(apply_exports_subpath("./lib/", "sub/mod"), "./lib/sub/mod");
    }
// TSZ_INLINE_TEST_END d3e8af4062366caf2d9a2adf39777e20db53175931587318ae719c873e171e5e

// TSZ_INLINE_TEST_BEGIN 3a9afebacddc7cab34b970eff271c64e0bf6c985aaf4e359f12346fbc07ae5d5 671 resolve_exports_subpath_substitutes_every_star_end_to_end
    #[test]
    fn resolve_exports_subpath_substitutes_every_star_end_to_end() {
        // The replace-all rule must hold through the driver's exports resolver,
        // not just the leaf helper: a single-`*` KEY mapping to a multi-`*`
        // TARGET resolves with no literal `*` left behind.
        let exports = serde_json::json!({ "./*": "./dist/*/*.js" });
        assert_eq!(
            resolve_exports_subpath(&exports, "./button", &["default"], TEST_VERSION)
                .into_option()
                .as_deref(),
            Some("./dist/button/button.js"),
        );
    }
// TSZ_INLINE_TEST_END 3a9afebacddc7cab34b970eff271c64e0bf6c985aaf4e359f12346fbc07ae5d5

// TSZ_INLINE_TEST_BEGIN 8cddda737630c840e4af83c53f82c159f2442e36787afb0e471309f3583db30f 690 types_versions_range_matches_handles_star_empty_and_disjunctions
    // NOTE: `select_paths`/`parse_pattern`/version-range selection are owned and
    // unit-tested by `tsz_common::module_resolution::types_versions`. The CLI
    // keeps the end-to-end `resolve_types_versions` coverage in the driver
    // integration tests; only the re-exported `types_versions_range_matches`
    // (used by versioned export conditions) is smoke-tested here.
    #[test]
    fn types_versions_range_matches_handles_star_empty_and_disjunctions() {
        assert!(types_versions_range_matches("*", TEST_VERSION));
        assert!(types_versions_range_matches("", TEST_VERSION));
        assert!(types_versions_range_matches(">=4 <6", TEST_VERSION));
        assert!(!types_versions_range_matches(">=6", TEST_VERSION));
        assert!(types_versions_range_matches(">=6 || <=5.4", TEST_VERSION));
    }
// TSZ_INLINE_TEST_END 8cddda737630c840e4af83c53f82c159f2442e36787afb0e471309f3583db30f

// TSZ_INLINE_TEST_BEGIN d9d6a5a8d2fc96e54c1611fc5e7f77fa9fcfed194f4e03098f4395b75a8d300b 699 resolve_imports_subpath_uses_prefix_specificity_not_total_length
    #[test]
    fn resolve_imports_subpath_uses_prefix_specificity_not_total_length() {
        // Same regression as exports, on the `#`-prefixed imports field.
        for imports in [
            serde_json::json!({
                "#*/abc": "./by-star-abc.js",
                "#abc/*": "./by-abc-star.js"
            }),
            serde_json::json!({
                "#abc/*": "./by-abc-star.js",
                "#*/abc": "./by-star-abc.js"
            }),
        ] {
            assert_eq!(
                resolve_imports_subpath_candidates_with_flavor(
                    &imports,
                    "#abc/abc",
                    &["default"],
                    TEST_VERSION,
                ),
                vec![("./by-abc-star.js".to_string(), false)],
            );
        }
    }
// TSZ_INLINE_TEST_END d9d6a5a8d2fc96e54c1611fc5e7f77fa9fcfed194f4e03098f4395b75a8d300b
