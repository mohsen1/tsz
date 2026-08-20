//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-common/src/module_resolution/path_identity.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 9b62df4dc4b9201b99c3f3b381604689992a07e8d9f87982704866f13779f3a4 161 clamps_excess_parent_segments_at_root
    #[test]
    fn clamps_excess_parent_segments_at_root() {
        // The historical divergence: a naive pop loop produced `/../b` here,
        // while the driver clamped to `/b`. Both layers now agree on `/b`.
        assert_eq!(
            normalize_segments(Path::new("/a/../../b")),
            PathBuf::from("/b")
        );
        assert_eq!(
            normalize_segments(Path::new("/a/b/../../../c")),
            PathBuf::from("/c")
        );
        assert_eq!(
            normalize_segments(Path::new("/root/../x")),
            PathBuf::from("/x")
        );
    }
// TSZ_INLINE_TEST_END 9b62df4dc4b9201b99c3f3b381604689992a07e8d9f87982704866f13779f3a4

// TSZ_INLINE_TEST_BEGIN ec6cd8e66f16d1b0b243a2a1f342c1b47196b3aff34937f1b73d7bd5434d10c0 179 preserves_leading_parent_on_relative_paths
    #[test]
    fn preserves_leading_parent_on_relative_paths() {
        assert_eq!(
            normalize_segments(Path::new("../foo")),
            PathBuf::from("../foo")
        );
        assert_eq!(
            normalize_segments(Path::new("../../foo/bar")),
            PathBuf::from("../../foo/bar")
        );
        assert_eq!(
            normalize_segments(Path::new("a/../../foo")),
            PathBuf::from("../foo")
        );
    }
// TSZ_INLINE_TEST_END ec6cd8e66f16d1b0b243a2a1f342c1b47196b3aff34937f1b73d7bd5434d10c0

// TSZ_INLINE_TEST_BEGIN 30aabc551786943cf8b28074f88883410ffa77da1b3f3a92f5a07898220b9f52 195 collapses_interior_dot_and_parent_segments
    #[test]
    fn collapses_interior_dot_and_parent_segments() {
        assert_eq!(normalize_segments(Path::new("a/./b")), PathBuf::from("a/b"));
        assert_eq!(
            normalize_segments(Path::new("a/b/../c")),
            PathBuf::from("a/c")
        );
        assert_eq!(
            normalize_segments(Path::new("/a/./b/../c")),
            PathBuf::from("/a/c")
        );
    }
// TSZ_INLINE_TEST_END 30aabc551786943cf8b28074f88883410ffa77da1b3f3a92f5a07898220b9f52

// TSZ_INLINE_TEST_BEGIN 8f64560b25c5c3323c31548a82108741ccc64b3fdb6e390685cbf13e321c811c 208 equivalent_spellings_share_one_identity
    #[test]
    fn equivalent_spellings_share_one_identity() {
        // Every spelling of the same absolute file collapses to one key, so the
        // file graph cannot mint duplicate declaration roots for it.
        let canonical = normalize_segments(Path::new("/pkg/lib/index.d.ts"));
        for spelling in [
            "/pkg/lib/index.d.ts",
            "/pkg/./lib/index.d.ts",
            "/pkg/lib/sub/../index.d.ts",
            "/pkg/extra/../../pkg/lib/index.d.ts",
        ] {
            assert_eq!(normalize_segments(Path::new(spelling)), canonical);
        }
    }
// TSZ_INLINE_TEST_END 8f64560b25c5c3323c31548a82108741ccc64b3fdb6e390685cbf13e321c811c

// TSZ_INLINE_TEST_BEGIN 118c955abcd1f4481454540646bb14cd311911848f6cae50792a5468a54bb749 223 resolve_relative_slash_specifier_collapses_against_base
    #[test]
    fn resolve_relative_slash_specifier_collapses_against_base() {
        assert_eq!(
            resolve_relative_slash_specifier("src/lib", "./mod"),
            Some("src/lib/mod".to_string())
        );
        assert_eq!(
            resolve_relative_slash_specifier("src/lib", "../mod"),
            Some("src/mod".to_string())
        );
        assert_eq!(
            resolve_relative_slash_specifier("", "./mod"),
            Some("mod".to_string())
        );
        // Empty segments (doubled slashes) are skipped, matching the
        // historical AMD-resolver loops.
        assert_eq!(
            resolve_relative_slash_specifier("src", ".//.//mod"),
            Some("src/mod".to_string())
        );
        // Segments seeded from `base_dir` pop verbatim: a seeded `..` is
        // itself poppable, so it does not trigger the underflow bail.
        assert_eq!(
            resolve_relative_slash_specifier("../lib", "../mod"),
            Some("../mod".to_string())
        );
    }
// TSZ_INLINE_TEST_END 118c955abcd1f4481454540646bb14cd311911848f6cae50792a5468a54bb749

// TSZ_INLINE_TEST_BEGIN 5a1c6370277ded43375b9a9ebb93b1550ad3bd015181ea6c2041f850b879b49d 251 resolve_relative_slash_specifier_bails_on_underflow_and_empty
    #[test]
    fn resolve_relative_slash_specifier_bails_on_underflow_and_empty() {
        // `..` escaping the virtual root: the caller picks the fallback.
        assert_eq!(resolve_relative_slash_specifier("", "../mod"), None);
        assert_eq!(resolve_relative_slash_specifier("src", "../../mod"), None);
        // Empty results also bail (`define()` dep arrays cannot hold "").
        assert_eq!(resolve_relative_slash_specifier("", "."), None);
        assert_eq!(resolve_relative_slash_specifier("src", ".."), None);
    }
// TSZ_INLINE_TEST_END 5a1c6370277ded43375b9a9ebb93b1550ad3bd015181ea6c2041f850b879b49d

// TSZ_INLINE_TEST_BEGIN 3cae83287880c562ecd9c61b8394479cb6c0cac798176ed1d3ecab89473ac5f3 261 apply_slash_segments_lossy_drops_unmatched_parent
    #[test]
    fn apply_slash_segments_lossy_drops_unmatched_parent() {
        let mut segments = vec!["pkg"];
        apply_slash_segments_lossy(&mut segments, "../../mod");
        // First `..` pops `pkg`; the second has nothing to cancel and is
        // dropped (lossy), matching the historical jsdoc ambient-module loop.
        assert_eq!(segments, vec!["mod"]);

        let mut segments: Vec<&str> = Vec::new();
        apply_slash_segments_lossy(&mut segments, "./a/../b");
        assert_eq!(segments, vec!["b"]);
    }
// TSZ_INLINE_TEST_END 3cae83287880c562ecd9c61b8394479cb6c0cac798176ed1d3ecab89473ac5f3

// TSZ_INLINE_TEST_BEGIN 074b809e1f6c2012bea8a08658fa537b6504376f3770f72eba46da4a5abb7334 274 is_already_normalized_matches_normalize_segments_fast_path
    #[test]
    fn is_already_normalized_matches_normalize_segments_fast_path() {
        // Already-canonical paths take the borrow fast path.
        assert!(is_already_normalized(Path::new("/a/b/c.ts")));
        assert!(is_already_normalized(Path::new("a/b/c.ts")));
        // A leading `.`, an *interior* `.` (which `Path::components` hides but
        // `normalize_segments` still strips), and any `..` must all be reported
        // as not-yet-normalized so the fast path agrees with the rebuild.
        assert!(!is_already_normalized(Path::new("./a/b")));
        assert!(!is_already_normalized(Path::new("a/./b")));
        assert!(!is_already_normalized(Path::new("/pkg/./dist/index.d.ts")));
        assert!(!is_already_normalized(Path::new("a/b/.")));
        assert!(!is_already_normalized(Path::new("a/../b")));
        // Any path the fast path skips must be byte-identical after normalizing,
        // so taking the borrow path never changes the resulting identity.
        for already in ["/a/b/c.ts", "a/b/c.ts", "../keep/me"] {
            let p = Path::new(already);
            if is_already_normalized(p) {
                assert_eq!(normalize_segments(p), PathBuf::from(already));
            }
        }
        // Conversely, every path the fast path declines must be exactly what the
        // owned rebuild produces — the fast path and `normalize_segments` agree
        // on one textual spelling. Interior `.` is the case that regressed.
        for needs_norm in [
            "a/./b",
            "/pkg/./dist/index.d.ts",
            "a/b/.",
            "./a/b",
            "a/../b",
        ] {
            let p = Path::new(needs_norm);
            assert!(!is_already_normalized(p), "{needs_norm} is not canonical");
        }
        assert_eq!(normalize_segments(Path::new("a/./b")), PathBuf::from("a/b"));
        assert_eq!(
            normalize_segments(Path::new("/pkg/./dist/index.d.ts")),
            PathBuf::from("/pkg/dist/index.d.ts")
        );
    }
// TSZ_INLINE_TEST_END 074b809e1f6c2012bea8a08658fa537b6504376f3770f72eba46da4a5abb7334
