//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-lsp/src/project/core.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 49e3e52cf05ffd95081c2dff62b4cbf3c7eefcc3e48877ab3d555fb7b2983af2 1800 collapses_join_artifacts_for_import_target_matching
    #[test]
    fn collapses_join_artifacts_for_import_target_matching() {
        // Pinned before routing through path_identity::normalize_segments:
        // the call-site domain is `importer_dir.join(specifier)` spellings.
        assert_eq!(
            normalize_path_for_compare(Path::new("/src/./a/../b.ts")),
            "/src/b.ts"
        );
        assert_eq!(
            normalize_path_for_compare(Path::new("/src/utils/../types.ts")),
            "/src/types.ts"
        );
    }
// TSZ_INLINE_TEST_END 49e3e52cf05ffd95081c2dff62b4cbf3c7eefcc3e48877ab3d555fb7b2983af2

// TSZ_INLINE_TEST_BEGIN c66685f2accb47a59bfc22b6defb4eb41bd455878da22b15dd076cde3f76ee16 1814 clamps_excess_parent_segments_at_root
    #[test]
    fn clamps_excess_parent_segments_at_root() {
        // Both the historical split('/') loop (via its leading-"" sentinel)
        // and the canonical helper clamp `..` at the filesystem root.
        assert_eq!(
            normalize_path_for_compare(Path::new("/a/../../b.ts")),
            "/b.ts"
        );
    }
// TSZ_INLINE_TEST_END c66685f2accb47a59bfc22b6defb4eb41bd455878da22b15dd076cde3f76ee16

// TSZ_INLINE_TEST_BEGIN 1be05cc7c7c1aa8f5779e2ca2372854efc5dc3214ba2dd1761f45052297f1fd3 1824 keeps_unmatched_parent_on_relative_importer
    #[test]
    fn keeps_unmatched_parent_on_relative_importer() {
        // Canonical semantics (changed from the historical loop, which
        // dropped the unmatched `..` and produced `x.ts`): an importer
        // escaping the project root can no longer alias an in-project
        // target spelling.
        assert_eq!(normalize_path_for_compare(Path::new("../x.ts")), "../x.ts");
    }
// TSZ_INLINE_TEST_END 1be05cc7c7c1aa8f5779e2ca2372854efc5dc3214ba2dd1761f45052297f1fd3
