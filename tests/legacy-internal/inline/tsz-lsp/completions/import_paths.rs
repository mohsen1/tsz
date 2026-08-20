//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-lsp/src/completions/import_paths.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN dc4dc2da9461968df36e137c9582e73a3f33480d37e45f8b1ee5e41fbdf5b076 164 get_import_path_completions_returns_empty_for_empty_partial
    #[test]
    fn get_import_path_completions_returns_empty_for_empty_partial() {
        let entries = vec![entry("./utils", false), entry("./types", false)];
        assert!(get_import_path_completions("", &entries).is_empty());
    }
// TSZ_INLINE_TEST_END dc4dc2da9461968df36e137c9582e73a3f33480d37e45f8b1ee5e41fbdf5b076

// TSZ_INLINE_TEST_BEGIN 98d0c961532fcaaa6fcaab904a24d3993799f7b66f28ca1fa0f15718227fcbcb 170 get_import_path_completions_filters_by_starts_with
    #[test]
    fn get_import_path_completions_filters_by_starts_with() {
        let entries = vec![
            entry("./utils", false),
            entry("./util-helpers", false),
            entry("./types", false),
        ];
        let completions = get_import_path_completions("./util", &entries);
        assert_eq!(completions.len(), 2);
        // Both `./utils` and `./util-helpers` match the prefix.
    }
// TSZ_INLINE_TEST_END 98d0c961532fcaaa6fcaab904a24d3993799f7b66f28ca1fa0f15718227fcbcb

// TSZ_INLINE_TEST_BEGIN 63c6abcf434032d27220500ac79fe1598562304fe925360fbf006011d53f2fb2 182 get_import_path_completions_returns_empty_when_no_match
    #[test]
    fn get_import_path_completions_returns_empty_when_no_match() {
        let entries = vec![entry("./utils", false)];
        let completions = get_import_path_completions("./other", &entries);
        assert!(completions.is_empty());
    }
// TSZ_INLINE_TEST_END 63c6abcf434032d27220500ac79fe1598562304fe925360fbf006011d53f2fb2

// TSZ_INLINE_TEST_BEGIN ce0a9b9707e1898407ed9f655e6123f893f4e6fb1722a184fb523c3c6a4755ea 191 build_import_paths_skips_self_reference
    #[test]
    fn build_import_paths_skips_self_reference() {
        let current = "src/main.ts";
        let files = vec!["src/main.ts".to_string(), "src/utils.ts".to_string()];
        let entries = build_import_paths(current, &files);
        assert!(entries.iter().all(|e| !e.specifier.contains("main")));
    }
// TSZ_INLINE_TEST_END ce0a9b9707e1898407ed9f655e6123f893f4e6fb1722a184fb523c3c6a4755ea

// TSZ_INLINE_TEST_BEGIN 4751ea4ca78dd2e3ebd7d31d1d9e3daa22952d1de6290b3ef483d988fdec5d1f 199 build_import_paths_strips_ts_extension
    #[test]
    fn build_import_paths_strips_ts_extension() {
        let current = "src/a.ts";
        let files = vec!["src/b.ts".to_string()];
        let entries = build_import_paths(current, &files);
        // Specifier should not contain `.ts`.
        let specifier_entry = entries.iter().find(|e| !e.is_directory).unwrap();
        assert!(!specifier_entry.specifier.ends_with(".ts"));
        assert!(specifier_entry.specifier.ends_with("/b"));
    }
// TSZ_INLINE_TEST_END 4751ea4ca78dd2e3ebd7d31d1d9e3daa22952d1de6290b3ef483d988fdec5d1f

// TSZ_INLINE_TEST_BEGIN 8dc546bdb2478bd3aaab02714957943b16ed3546ec6365c066e81e75c2bdff7f 210 build_import_paths_filters_non_importable_extensions
    #[test]
    fn build_import_paths_filters_non_importable_extensions() {
        let current = "src/a.ts";
        let files = vec![
            "src/b.ts".to_string(),
            "src/img.png".to_string(),
            "src/data.csv".to_string(),
            "src/types.d.ts".to_string(),
        ];
        let entries = build_import_paths(current, &files);
        let specifiers: Vec<&str> = entries.iter().map(|e| e.specifier.as_str()).collect();
        assert!(!specifiers.iter().any(|s| s.contains("img")));
        assert!(!specifiers.iter().any(|s| s.contains("csv")));
    }
// TSZ_INLINE_TEST_END 8dc546bdb2478bd3aaab02714957943b16ed3546ec6365c066e81e75c2bdff7f

// TSZ_INLINE_TEST_BEGIN 59cdc9ac009b9038900ab4e2b76022244aca355394ff669e66207b81e410bee5 225 build_import_paths_includes_parent_directories
    #[test]
    fn build_import_paths_includes_parent_directories() {
        let current = "src/a.ts";
        let files = vec!["src/lib/b.ts".to_string(), "src/lib/c.ts".to_string()];
        let entries = build_import_paths(current, &files);
        // Should include both files AND the parent dir entry "lib"
        let dir_entries: Vec<_> = entries.iter().filter(|e| e.is_directory).collect();
        assert!(!dir_entries.is_empty());
        assert!(dir_entries.iter().any(|e| e.specifier.contains("lib")));
    }
// TSZ_INLINE_TEST_END 59cdc9ac009b9038900ab4e2b76022244aca355394ff669e66207b81e410bee5

// TSZ_INLINE_TEST_BEGIN 406ba459c7d95f6dfa3f46b57e8ac8ca4b1273d5502942352c3c667833ff8de8 236 build_import_paths_no_parent_dir_added_twice
    #[test]
    fn build_import_paths_no_parent_dir_added_twice() {
        // Two files in the same directory — only one directory entry.
        let current = "src/a.ts";
        let files = vec!["src/sub/x.ts".to_string(), "src/sub/y.ts".to_string()];
        let entries = build_import_paths(current, &files);
        let sub_dir_entries: Vec<_> = entries
            .iter()
            .filter(|e| e.is_directory && e.specifier.contains("sub"))
            .collect();
        assert_eq!(sub_dir_entries.len(), 1, "duplicate parent dir entries");
    }
// TSZ_INLINE_TEST_END 406ba459c7d95f6dfa3f46b57e8ac8ca4b1273d5502942352c3c667833ff8de8

// TSZ_INLINE_TEST_BEGIN a82d0e9e6fb7c925132e9fd4e91e07514c5ad360cbcab0acc0cdceb6e9cfa0a4 251 is_importable_file_accepts_ts_jsx_and_json
    #[test]
    fn is_importable_file_accepts_ts_jsx_and_json() {
        for ext in &[
            "a.ts", "a.tsx", "a.js", "a.jsx", "a.mts", "a.mjs", "a.cts", "a.cjs", "a.json",
        ] {
            assert!(is_importable_file(ext), "expected importable: {ext}");
        }
    }
// TSZ_INLINE_TEST_END a82d0e9e6fb7c925132e9fd4e91e07514c5ad360cbcab0acc0cdceb6e9cfa0a4

// TSZ_INLINE_TEST_BEGIN f5752c4e725d3550eab3e1a60e36d578e15b73ca43da2b43d340774e36dce724 260 is_importable_file_rejects_non_source_extensions
    #[test]
    fn is_importable_file_rejects_non_source_extensions() {
        for ext in &["a.png", "a.css", "a.md", "a.html", "a"] {
            assert!(!is_importable_file(ext), "expected non-importable: {ext}");
        }
    }
// TSZ_INLINE_TEST_END f5752c4e725d3550eab3e1a60e36d578e15b73ca43da2b43d340774e36dce724

// TSZ_INLINE_TEST_BEGIN ca06acbbd0d50fbb4bf5b6f8a2707fbafe19bb25fd8eb984c2624244b24b51d1 269 strip_ts_extension_removes_ts_jsx_variants
    #[test]
    fn strip_ts_extension_removes_ts_jsx_variants() {
        assert_eq!(strip_ts_extension("a.ts"), "a");
        assert_eq!(strip_ts_extension("a.tsx"), "a");
        assert_eq!(strip_ts_extension("a.mjs"), "a");
        assert_eq!(strip_ts_extension("a.cts"), "a");
    }
// TSZ_INLINE_TEST_END ca06acbbd0d50fbb4bf5b6f8a2707fbafe19bb25fd8eb984c2624244b24b51d1

// TSZ_INLINE_TEST_BEGIN 6cd9c646a0def449cc7340e62acf6eae251538cbafa93617abc9a6c4c29dc024 277 strip_ts_extension_strips_declaration_extensions
    #[test]
    fn strip_ts_extension_strips_declaration_extensions() {
        // Declaration extensions (`.d.ts`, `.d.mts`, `.d.cts`) are stripped as
        // a unit so `"types.d.ts"` → `"types"`, not `"types.d"`.
        assert_eq!(strip_ts_extension("types.d.ts"), "types");
        assert_eq!(strip_ts_extension("types.d.mts"), "types");
        assert_eq!(strip_ts_extension("types.d.cts"), "types");
    }
// TSZ_INLINE_TEST_END 6cd9c646a0def449cc7340e62acf6eae251538cbafa93617abc9a6c4c29dc024

// TSZ_INLINE_TEST_BEGIN 0160355888a6306e24e23d598615abcf915ae9fbb27f1713175d85d507b18f6b 286 strip_ts_extension_preserves_d_part_for_source_tsx
    #[test]
    fn strip_ts_extension_preserves_d_part_for_source_tsx() {
        // `.d.tsx` is a source file (not a declaration), so only `.tsx` is
        // stripped — the `.d` part belongs to the module name.
        assert_eq!(strip_ts_extension("types.d.tsx"), "types.d");
    }
// TSZ_INLINE_TEST_END 0160355888a6306e24e23d598615abcf915ae9fbb27f1713175d85d507b18f6b

// TSZ_INLINE_TEST_BEGIN 2ef8637a201d6af147299d4c0b4dac4bbe6a4c6753849fd23f8f7b4dea8bc6a8 293 strip_ts_extension_leaves_unknown_extensions_untouched
    #[test]
    fn strip_ts_extension_leaves_unknown_extensions_untouched() {
        assert_eq!(strip_ts_extension("readme.md"), "readme.md");
        assert_eq!(strip_ts_extension("file"), "file");
    }
// TSZ_INLINE_TEST_END 2ef8637a201d6af147299d4c0b4dac4bbe6a4c6753849fd23f8f7b4dea8bc6a8

// TSZ_INLINE_TEST_BEGIN bd6c750ea7da9da2ee81d3f091a11d7ec03dd811821f788b3123fe276383d599 301 compute_relative_path_same_directory
    #[test]
    fn compute_relative_path_same_directory() {
        // src/ → src/b.ts means b is a sibling: `./b.ts`.
        assert_eq!(compute_relative_path("src", "src/b.ts"), "./b.ts");
    }
// TSZ_INLINE_TEST_END bd6c750ea7da9da2ee81d3f091a11d7ec03dd811821f788b3123fe276383d599

// TSZ_INLINE_TEST_BEGIN 72d268f61f6e8d4978075324d6d759fbeda6ee8d5292888749b1715f51283a75 307 compute_relative_path_subdirectory
    #[test]
    fn compute_relative_path_subdirectory() {
        // src/ → src/lib/b.ts: `./lib/b.ts`.
        assert_eq!(compute_relative_path("src", "src/lib/b.ts"), "./lib/b.ts");
    }
// TSZ_INLINE_TEST_END 72d268f61f6e8d4978075324d6d759fbeda6ee8d5292888749b1715f51283a75

// TSZ_INLINE_TEST_BEGIN ad5956004bb64289be38652b5876f44e193d889a9dc95c4155984341e1a815c7 313 compute_relative_path_parent_directory
    #[test]
    fn compute_relative_path_parent_directory() {
        // src/sub → src/b.ts: `../b.ts`.
        assert_eq!(compute_relative_path("src/sub", "src/b.ts"), "../b.ts");
    }
// TSZ_INLINE_TEST_END ad5956004bb64289be38652b5876f44e193d889a9dc95c4155984341e1a815c7

// TSZ_INLINE_TEST_BEGIN 3e5319e3966c6840210ec6d5dfb308ee18340d1776db4c228931ba4c352240f0 319 compute_relative_path_disjoint_branches
    #[test]
    fn compute_relative_path_disjoint_branches() {
        // src/a → other/b.ts: `../../other/b.ts`.
        assert_eq!(
            compute_relative_path("src/a", "other/b.ts"),
            "../../other/b.ts"
        );
    }
// TSZ_INLINE_TEST_END 3e5319e3966c6840210ec6d5dfb308ee18340d1776db4c228931ba4c352240f0

// TSZ_INLINE_TEST_BEGIN 8bdf783bfad444f97767b824f46424b73251f12ee6153a015e5711ebe895fbb6 328 compute_relative_path_handles_leading_slashes
    #[test]
    fn compute_relative_path_handles_leading_slashes() {
        // Leading slashes treated as empty parts and skipped.
        assert_eq!(compute_relative_path("/src", "/src/a.ts"), "./a.ts");
    }
// TSZ_INLINE_TEST_END 8bdf783bfad444f97767b824f46424b73251f12ee6153a015e5711ebe895fbb6
