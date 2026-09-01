//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-common/src/module_resolution/types_versions.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 44744ec74ebb1f3b23f31be921a9c4c711c8f48d40d150192daffe89b31bbe4b 271 parses_partial_and_pre_release_versions
    #[test]
    fn parses_partial_and_pre_release_versions() {
        assert_eq!(
            parse_semver("6"),
            Some(SemVer {
                major: 6,
                minor: 0,
                patch: 0
            })
        );
        assert_eq!(
            parse_semver("5.4"),
            Some(SemVer {
                major: 5,
                minor: 4,
                patch: 0
            })
        );
        assert_eq!(
            parse_semver("6.0.3-beta+abc"),
            Some(SemVer {
                major: 6,
                minor: 0,
                patch: 3
            })
        );
        assert_eq!(parse_semver(""), None);
        assert_eq!(parse_semver("x"), None);
    }
// TSZ_INLINE_TEST_END 44744ec74ebb1f3b23f31be921a9c4c711c8f48d40d150192daffe89b31bbe4b

// TSZ_INLINE_TEST_BEGIN c3aea6a9fdb8014c4b0779765481a127efff6b4aaae05fa5442dea9de9a151b4 301 range_matching_handles_star_disjunction_and_conjunction
    #[test]
    fn range_matching_handles_star_disjunction_and_conjunction() {
        assert!(range_matches("*", V6));
        assert!(range_matches("", V6));
        assert!(range_matches(">=6.0", V6));
        assert!(!range_matches(">=6.0", V5));
        assert!(range_matches(">=5.0 <7.0", V6));
        assert!(!range_matches(">=5.0 <5.5", V6));
        assert!(range_matches(">=7.0 || >=5.0", V6));
        // Malformed disjunction segment must not vacuously match.
        assert!(!range_matches(">=7.0 || ", V6));
        // An unparseable range never matches.
        assert!(!range_matches("garbage", V6));
    }
// TSZ_INLINE_TEST_END c3aea6a9fdb8014c4b0779765481a127efff6b4aaae05fa5442dea9de9a151b4

// TSZ_INLINE_TEST_BEGIN c8372821d40250c92e3f362209054b6827b9edc3e9e3a3081b8394e10d70e479 316 selects_first_matching_range_in_declaration_order
    #[test]
    fn selects_first_matching_range_in_declaration_order() {
        let tv = json!({
            ">=6.0": { "feature/*": ["loose/feature/*"] },
            ">=5.0 <7.0": { "feature/*": ["ranged/feature/*"] },
            "*": { "feature/*": ["fallback/feature/*"] },
        });
        assert_eq!(
            resolve_candidate_targets(&tv, "feature/widget", V6),
            vec!["loose/feature/widget".to_string()]
        );
    }
// TSZ_INLINE_TEST_END c8372821d40250c92e3f362209054b6827b9edc3e9e3a3081b8394e10d70e479

// TSZ_INLINE_TEST_BEGIN dc6ef1fd94780d18071ae25c21c065a42ba1d681a728dd30b16d23f7f0d6e0ea 329 skips_non_matching_ranges_and_falls_through_to_next
    #[test]
    fn skips_non_matching_ranges_and_falls_through_to_next() {
        let tv = json!({
            ">=7.0": { "*": ["next/*"] },
            ">=5.0": { "*": ["current/*"] },
        });
        // 6.0.3 does not satisfy >=7.0, so the first *matching* range is >=5.0.
        assert_eq!(
            resolve_candidate_targets(&tv, "index", V6),
            vec!["current/index".to_string()]
        );
    }
// TSZ_INLINE_TEST_END dc6ef1fd94780d18071ae25c21c065a42ba1d681a728dd30b16d23f7f0d6e0ea

// TSZ_INLINE_TEST_BEGIN e7b2fb855a5824c749a8a443dbb32ef7a7f534cfd5607ca3c75dd98e0a78eab9 342 no_matching_range_yields_no_candidates
    #[test]
    fn no_matching_range_yields_no_candidates() {
        let tv = json!({ ">=7.0": { "*": ["next/*"] } });
        assert!(resolve_candidate_targets(&tv, "index", V6).is_empty());
    }
// TSZ_INLINE_TEST_END e7b2fb855a5824c749a8a443dbb32ef7a7f534cfd5607ca3c75dd98e0a78eab9

// TSZ_INLINE_TEST_BEGIN 1f801b1e1cb6f6b895fcc1125c364c07de87c72c445936c2a3abbfbec4e5c247 348 exact_match_beats_short_wildcard
    #[test]
    fn exact_match_beats_short_wildcard() {
        let tv = json!({
            "*": { "a": ["wild/a"], "*": ["wild/*"] },
        });
        // Exact key "a" must win over the wildcard even though it is shorter.
        assert_eq!(
            resolve_candidate_targets(&tv, "a", V6),
            vec!["wild/a".to_string()]
        );
    }
// TSZ_INLINE_TEST_END 1f801b1e1cb6f6b895fcc1125c364c07de87c72c445936c2a3abbfbec4e5c247

// TSZ_INLINE_TEST_BEGIN 83a64e8ef214310e71669ad1ffa988dc2c9293f983be4a73d8490f1e5bf91e96 360 longest_prefix_wildcard_wins_ties_keep_first
    #[test]
    fn longest_prefix_wildcard_wins_ties_keep_first() {
        let tv = json!({
            "*": {
                "*": ["short/*"],
                "feature/*": ["long/*"],
            },
        });
        assert_eq!(
            resolve_candidate_targets(&tv, "feature/x", V6),
            vec!["long/x".to_string()]
        );
    }
// TSZ_INLINE_TEST_END 83a64e8ef214310e71669ad1ffa988dc2c9293f983be4a73d8490f1e5bf91e96

// TSZ_INLINE_TEST_BEGIN 31d1b58406c12a06810d8aeab174397a2516b8352caa02fa85ccfdbf7134cfb9 374 declaration_order_independent_of_specificity_for_version_keys
    #[test]
    fn declaration_order_independent_of_specificity_for_version_keys() {
        // Version selection must NOT prefer the tighter/more-specific range; it
        // takes the first matching one in declaration order.
        let tv = json!({
            ">=5.0 <7.0": { "*": ["ranged/*"] },
            ">=6.0": { "*": ["loose/*"] },
        });
        assert_eq!(
            resolve_candidate_targets(&tv, "index", V6),
            vec!["ranged/index".to_string()]
        );
    }
// TSZ_INLINE_TEST_END 31d1b58406c12a06810d8aeab174397a2516b8352caa02fa85ccfdbf7134cfb9

// TSZ_INLINE_TEST_BEGIN c510fcd886be2a8de1e3ddfd1fc1458295311dd90a6481fd8bddd51f1725d459 388 multi_star_keys_are_ignored
    #[test]
    fn multi_star_keys_are_ignored() {
        let tv = json!({ "*": { "a/*/*": ["bad/*"], "a/*": ["good/*"] } });
        assert_eq!(
            resolve_candidate_targets(&tv, "a/b", V6),
            vec!["good/b".to_string()]
        );
    }
// TSZ_INLINE_TEST_END c510fcd886be2a8de1e3ddfd1fc1458295311dd90a6481fd8bddd51f1725d459

// TSZ_INLINE_TEST_BEGIN fb4cea3342ab078b377fa78811c561b74346c27ff7560caf3a17674070748b3c 397 array_targets_preserve_order
    #[test]
    fn array_targets_preserve_order() {
        let tv = json!({ "*": { "*": ["first/*", "second/*"] } });
        assert_eq!(
            resolve_candidate_targets(&tv, "x", V6),
            vec!["first/x".to_string(), "second/x".to_string()]
        );
    }
// TSZ_INLINE_TEST_END fb4cea3342ab078b377fa78811c561b74346c27ff7560caf3a17674070748b3c
