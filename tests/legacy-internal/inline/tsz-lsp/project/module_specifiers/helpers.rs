//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-lsp/src/project/module_specifiers/helpers.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 7ad0ce131900346cbb5fb58212cc9fcaa754211d0ae5c787392174b25737f051 1014 normalize_path_collapses_dot_segments_and_clamps_at_root
    #[test]
    fn normalize_path_collapses_dot_segments_and_clamps_at_root() {
        // Pinned before routing through path_identity::normalize_segments.
        // Every caller joins onto a package/config directory, so the inputs
        // are effectively rooted; these shapes must not change.
        assert_eq!(
            normalize_path(Path::new("/pkg/./lib/../x")),
            PathBuf::from("/pkg/x")
        );
        // Excess `..` clamps at the filesystem root (both the historical loop
        // and the canonical helper agree here).
        assert_eq!(normalize_path(Path::new("/a/../../b")), PathBuf::from("/b"));
        assert_eq!(
            normalize_path(Path::new("/cfg/dir/../paths/target")),
            PathBuf::from("/cfg/paths/target")
        );
        // Canonical semantics for a relative input (unreachable from the
        // rooted call sites): an unmatched `..` is kept, where the
        // historical loop silently dropped it (`x`).
        assert_eq!(normalize_path(Path::new("../x")), PathBuf::from("../x"));
    }
// TSZ_INLINE_TEST_END 7ad0ce131900346cbb5fb58212cc9fcaa754211d0ae5c787392174b25737f051

// TSZ_INLINE_TEST_BEGIN 975f3f501dd05c677d8e3f1bf8ebfbdbdaf90ea4ac754843565c16709465998e 1036 strip_ts_path_extension_uses_shared_ts_family_rules
    #[test]
    fn strip_ts_path_extension_uses_shared_ts_family_rules() {
        assert_eq!(
            strip_ts_path_extension(Path::new("src/types.d.cts")),
            PathBuf::from("src/types")
        );
        assert_eq!(
            strip_ts_path_extension(Path::new("src/types.d.tsx")),
            PathBuf::from("src/types.d")
        );
        assert_eq!(
            strip_ts_path_extension(Path::new("src/runtime.mjs")),
            PathBuf::from("src/runtime.mjs")
        );
    }
// TSZ_INLINE_TEST_END 975f3f501dd05c677d8e3f1bf8ebfbdbdaf90ea4ac754843565c16709465998e

// TSZ_INLINE_TEST_BEGIN 12770899a3a27d92fe1936544aeef6bffe2f9505f1697cb0ea731d1ba3540782 1052 strip_js_ts_extension_uses_shared_known_extension_rules
    #[test]
    fn strip_js_ts_extension_uses_shared_known_extension_rules() {
        assert_eq!(
            strip_js_ts_extension(Path::new("src/runtime.mjs")),
            PathBuf::from("src/runtime")
        );
        assert_eq!(
            strip_js_ts_extension(Path::new("src/types.d.tsx")),
            PathBuf::from("src/types.d")
        );
    }
// TSZ_INLINE_TEST_END 12770899a3a27d92fe1936544aeef6bffe2f9505f1697cb0ea731d1ba3540782
