//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-core/src/config/resolved_options.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 42fe441b06dc29e8a12e5073939b4e16d044db184b09277f0397557f20c12fe5 991 exact_key_beats_equal_prefix_wildcard_regardless_of_order
    #[test]
    fn exact_key_beats_equal_prefix_wildcard_regardless_of_order() {
        // `matchPatternOrExact` returns an exact wildcard-free key before any
        // wildcard. `"alias"` and `"alias*"` tie on prefix length for the
        // specifier `"alias"`; the literal key must win in either ordering.
        for order in [
            vec![
                mapping("alias", &["./exact"]),
                mapping("alias*", &["./wild"]),
            ],
            vec![
                mapping("alias*", &["./wild"]),
                mapping("alias", &["./exact"]),
            ],
        ] {
            let (idx, star) =
                PathMapping::select_best(&order, "alias").expect("a mapping must be selected");
            assert_eq!(order[idx].pattern, "alias", "exact key must win the tie");
            assert_eq!(star, "");
        }
    }
// TSZ_INLINE_TEST_END 42fe441b06dc29e8a12e5073939b4e16d044db184b09277f0397557f20c12fe5

// TSZ_INLINE_TEST_BEGIN cedbe3b8f069de592abcc38f32e5b5ce1e4a4f0282315426422330d1b10d8ff7 1013 longest_prefix_wildcard_wins_independent_of_order
    #[test]
    fn longest_prefix_wildcard_wins_independent_of_order() {
        // The longest-prefix wildcard is chosen even when the input is not
        // pre-sorted by specificity, proving the selection does not depend on
        // `build_path_mappings`' ordering.
        let unsorted = vec![
            mapping("*", &["./external.d.ts"]),
            mapping("next/dist/*", &["./src/*"]),
            mapping("next/dist/compiled/*", &["./compiled/*"]),
        ];
        let (idx, star) = PathMapping::select_best(&unsorted, "next/dist/compiled/react")
            .expect("a wildcard must be selected");
        assert_eq!(unsorted[idx].pattern, "next/dist/compiled/*");
        assert_eq!(star, "react");
    }
// TSZ_INLINE_TEST_END cedbe3b8f069de592abcc38f32e5b5ce1e4a4f0282315426422330d1b10d8ff7

// TSZ_INLINE_TEST_BEGIN a433ae0d8591058edcb26bb5670518b3c78f34abdb97cfdd8dd017f1e56b4b86 1029 no_match_returns_none
    #[test]
    fn no_match_returns_none() {
        let mappings = vec![mapping("@app/*", &["./src/*"])];
        assert!(PathMapping::select_best(&mappings, "unrelated/thing").is_none());
    }
// TSZ_INLINE_TEST_END a433ae0d8591058edcb26bb5670518b3c78f34abdb97cfdd8dd017f1e56b4b86

// TSZ_INLINE_TEST_BEGIN 06c7a8a0e7658f9c1ea0fadd25c5629a97f5073913133a8fc90b9a0802be0be6 1035 multi_star_key_is_dropped_at_build_like_tsc_try_parse_pattern
    #[test]
    fn multi_star_key_is_dropped_at_build_like_tsc_try_parse_pattern() {
        use super::build_path_mappings;
        use rustc_hash::FxHashMap;

        // tsc's `tryParsePattern` returns `undefined` for a key with two `*`, so
        // `tryParsePatterns` never builds a mapping for it. `build_path_mappings`
        // must drop it at the parser so it can never match a specifier (which it
        // would, on its mis-derived first-`*` `prefix`/`suffix`).
        let mut paths: FxHashMap<String, Vec<String>> = FxHashMap::default();
        paths.insert("a/*/*".to_string(), vec!["./wrong/*".to_string()]);
        paths.insert("*".to_string(), vec!["./types/*".to_string()]);

        let mappings = build_path_mappings(&paths);
        assert!(
            mappings.iter().all(|m| m.pattern != "a/*/*"),
            "multi-`*` key must be dropped at build time"
        );

        // With the malformed key gone, only the valid catch-all can match.
        let (idx, star) =
            PathMapping::select_best(&mappings, "a/foo/bar").expect("catch-all must match");
        assert_eq!(mappings[idx].pattern, "*");
        assert_eq!(star, "a/foo/bar");
    }
// TSZ_INLINE_TEST_END 06c7a8a0e7658f9c1ea0fadd25c5629a97f5073913133a8fc90b9a0802be0be6

// TSZ_INLINE_TEST_BEGIN 31aa5ce975aeb3d96cbe3d3ec053af57ac8a23ae12ca0351c795d5cec979285e 1061 single_star_key_still_matches
    #[test]
    fn single_star_key_still_matches() {
        // The multi-`*` guard must not regress an ordinary single-`*` wildcard.
        let one_star = mapping("@app/*", &["./src/*"]);
        assert_eq!(
            one_star.match_specifier("@app/util"),
            Some("util".to_string())
        );
    }
// TSZ_INLINE_TEST_END 31aa5ce975aeb3d96cbe3d3ec053af57ac8a23ae12ca0351c795d5cec979285e

// TSZ_INLINE_TEST_BEGIN 014087bbf00d66f0532fc890b028815189dea15c406f752d7b4f2feeb6376477 1071 wildcard_target_substitutes_only_first_star
    #[test]
    fn wildcard_target_substitutes_only_first_star() {
        // tsc uses `subst.replace("*", matchedStar)`, which replaces only the
        // first `*`. A target with a second `*` keeps it verbatim.
        let m = mapping("@gen/*", &["./gen/*/*.js"]);
        let star = m.match_specifier("@gen/foo").expect("wildcard matches");
        assert_eq!(m.substitute_target(&m.targets[0], &star), "./gen/foo/*.js");
    }
// TSZ_INLINE_TEST_END 014087bbf00d66f0532fc890b028815189dea15c406f752d7b4f2feeb6376477

// TSZ_INLINE_TEST_BEGIN 09a12715b2aaeb86e69fb012a178b06c3a1bf078d0ecc0738bb55d324e487ba0 1080 wildcard_target_without_star_is_used_verbatim
    #[test]
    fn wildcard_target_without_star_is_used_verbatim() {
        let m = mapping("@fallback/*", &["./shim.d.ts"]);
        let star = m
            .match_specifier("@fallback/anything")
            .expect("wildcard matches");
        assert_eq!(m.substitute_target(&m.targets[0], &star), "./shim.d.ts");
    }
// TSZ_INLINE_TEST_END 09a12715b2aaeb86e69fb012a178b06c3a1bf078d0ecc0738bb55d324e487ba0

// TSZ_INLINE_TEST_BEGIN c802b79a5923da5bf2d6bff3ad2341306b1ffeef608f487ebc5d0b2dbcbb7008 1089 wildcard_target_with_empty_capture_substitutes_empty
    #[test]
    fn wildcard_target_with_empty_capture_substitutes_empty() {
        // Specifier equal to prefix+suffix captures an empty `*`; tsc still
        // substitutes (the empty string), unlike an exact key.
        let m = mapping("@app/*", &["./src/*"]);
        let star = m.match_specifier("@app/").expect("empty capture matches");
        assert_eq!(star, "");
        assert_eq!(m.substitute_target(&m.targets[0], &star), "./src/");
    }
// TSZ_INLINE_TEST_END c802b79a5923da5bf2d6bff3ad2341306b1ffeef608f487ebc5d0b2dbcbb7008

// TSZ_INLINE_TEST_BEGIN 61526ff030305a824bb324de200f8ca4f47abcbdcb29affec8e2499839d52022 1099 exact_key_target_is_used_verbatim_keeping_literal_star
    #[test]
    fn exact_key_target_is_used_verbatim_keeping_literal_star() {
        // For an exact, wildcard-free key tsc leaves `matchedStar` undefined and
        // uses the target verbatim (`path = subst`), so a literal `*` in the
        // target is preserved rather than stripped.
        let m = mapping("foo", &["./bar/*"]);
        let star = m.match_specifier("foo").expect("exact key matches");
        assert_eq!(star, "");
        assert_eq!(m.substitute_target(&m.targets[0], &star), "./bar/*");
    }
// TSZ_INLINE_TEST_END 61526ff030305a824bb324de200f8ca4f47abcbdcb29affec8e2499839d52022
