use super::*;

// ===========================================================================
// strip_ts_extension
// ===========================================================================

#[test]
fn test_strip_ts_extension() {
    assert_eq!(strip_ts_extension("foo.ts"), "foo");
    assert_eq!(strip_ts_extension("foo.tsx"), "foo");
    assert_eq!(strip_ts_extension("foo.js"), "foo");
    assert_eq!(strip_ts_extension("foo.jsx"), "foo");
    assert_eq!(strip_ts_extension("foo.d.ts"), "foo");
    assert_eq!(strip_ts_extension("foo.d.tsx"), "foo");
    assert_eq!(strip_ts_extension("foo.mts"), "foo");
    assert_eq!(strip_ts_extension("foo.cts"), "foo");
    assert_eq!(strip_ts_extension("foo.mjs"), "foo");
    assert_eq!(strip_ts_extension("foo.cjs"), "foo");
    assert_eq!(strip_ts_extension("foo.d.mts"), "foo");
    assert_eq!(strip_ts_extension("foo.d.cts"), "foo");
}

#[test]
fn test_strip_ts_extension_with_path() {
    assert_eq!(strip_ts_extension("/tmp/test/foo.ts"), "/tmp/test/foo");
    assert_eq!(strip_ts_extension("lib/utils.d.ts"), "lib/utils");
    assert_eq!(strip_ts_extension("src/index.tsx"), "src/index");
}

#[test]
fn test_strip_ts_extension_no_match() {
    assert_eq!(strip_ts_extension("foo"), "foo");
    assert_eq!(strip_ts_extension("foo.txt"), "foo.txt");
    assert_eq!(strip_ts_extension("foo.css"), "foo.css");
    assert_eq!(strip_ts_extension(""), "");
}

#[test]
fn test_strip_dts_before_ts() {
    // .d.ts must be stripped as a whole, not just .ts leaving ".d".
    assert_eq!(strip_ts_extension("types.d.ts"), "types");
    assert_eq!(strip_ts_extension("globals.d.tsx"), "globals");
}

// ===========================================================================
// relative_specifier_for_file
// ===========================================================================

#[test]
fn test_relative_specifier_same_dir() {
    let from = Path::new("/tmp/test");
    let to = Path::new("/tmp/test/types.ts");
    assert_eq!(
        relative_specifier_for_file(from, to),
        Some("./types".to_string())
    );
}

#[test]
fn test_relative_specifier_nested() {
    let from = Path::new("/tmp/test");
    let to = Path::new("/tmp/test/lib/utils.ts");
    assert_eq!(
        relative_specifier_for_file(from, to),
        Some("./lib/utils".to_string())
    );
}

#[test]
fn test_relative_specifier_parent_dir() {
    let from = Path::new("/tmp/test/src");
    let to = Path::new("/tmp/test/lib/utils.ts");
    assert_eq!(
        relative_specifier_for_file(from, to),
        Some("../lib/utils".to_string())
    );
}

#[test]
fn test_relative_specifier_two_levels_up() {
    let from = Path::new("/tmp/test/src/deep");
    let to = Path::new("/tmp/test/lib/utils.ts");
    assert_eq!(
        relative_specifier_for_file(from, to),
        Some("../../lib/utils".to_string())
    );
}

#[test]
fn test_relative_specifier_dts_extension() {
    let from = Path::new("/tmp/test");
    let to = Path::new("/tmp/test/types.d.ts");
    assert_eq!(
        relative_specifier_for_file(from, to),
        Some("./types".to_string())
    );
}

// ===========================================================================
// normalize_import_specifier
// ===========================================================================

#[test]
fn test_normalize_strips_whitespace_and_matching_quotes() {
    // Double quotes.
    let got = normalize_import_specifier("  \"./foo\"  ").expect("some");
    assert_eq!(got.text, "./foo");
    assert_eq!(got.kind, SpecifierKind::Relative);
    assert!(!got.is_directory_hint);

    // Single quotes.
    let got = normalize_import_specifier("'./bar'").expect("some");
    assert_eq!(got.text, "./bar");

    // Lopsided quotes are NOT stripped (they are not a matching pair).
    let got = normalize_import_specifier("\"./foo").expect("some");
    assert_eq!(got.text, "\"./foo");
}

#[test]
fn test_normalize_returns_none_for_empty_or_quotes_only() {
    assert!(normalize_import_specifier("").is_none());
    assert!(normalize_import_specifier("   ").is_none());
    assert!(normalize_import_specifier("\"\"").is_none());
    assert!(normalize_import_specifier("''").is_none());
}

#[test]
fn test_normalize_converts_backslashes() {
    let got = normalize_import_specifier(".\\foo\\bar").expect("some");
    assert_eq!(got.text, "./foo/bar");
    assert_eq!(got.kind, SpecifierKind::Relative);
}

#[test]
fn test_normalize_classifies_kinds() {
    let kinds = |s: &str| normalize_import_specifier(s).map(|c| c.kind);
    assert_eq!(kinds("./foo"), Some(SpecifierKind::Relative));
    assert_eq!(kinds("."), Some(SpecifierKind::Relative));
    assert_eq!(kinds("./"), Some(SpecifierKind::Relative));
    assert_eq!(kinds("../foo"), Some(SpecifierKind::Parent));
    assert_eq!(kinds(".."), Some(SpecifierKind::Parent));
    assert_eq!(kinds("../"), Some(SpecifierKind::Parent));
    assert_eq!(kinds("/abs/path"), Some(SpecifierKind::Absolute));
    assert_eq!(kinds("lodash"), Some(SpecifierKind::Bare));
    assert_eq!(kinds("@scope/pkg"), Some(SpecifierKind::Bare));
}

#[test]
fn test_normalize_directory_hint() {
    let hint = |s: &str| normalize_import_specifier(s).map(|c| c.is_directory_hint);
    // Dot chains are always directory hints, both with and without trailing slash.
    assert_eq!(hint("."), Some(true));
    assert_eq!(hint("./"), Some(true));
    assert_eq!(hint(".."), Some(true));
    assert_eq!(hint("../"), Some(true));
    assert_eq!(hint("../.."), Some(true));
    assert_eq!(hint("../../"), Some(true));
    // Trailing slash on non-dot-chain is a directory hint.
    assert_eq!(hint("./foo/"), Some(true));
    assert_eq!(hint("./a/b/"), Some(true));
    // No trailing slash, not a dot chain → not a directory hint.
    assert_eq!(hint("./foo"), Some(false));
    assert_eq!(hint("./a/b"), Some(false));
    assert_eq!(hint("lodash"), Some(false));
}

#[test]
fn test_normalize_strips_trailing_slash_on_non_dot_chains() {
    // `./foo/` canonicalizes to `./foo` (the trailing slash is a directory
    // hint but not part of the canonical key).
    assert_eq!(
        normalize_import_specifier("./foo/").map(|c| c.text),
        Some("./foo".to_string())
    );
    // Pure dot chains keep their form.
    assert_eq!(
        normalize_import_specifier("./").map(|c| c.text),
        Some("./".to_string())
    );
    assert_eq!(
        normalize_import_specifier(".").map(|c| c.text),
        Some(".".to_string())
    );
}

// ===========================================================================
// module_specifier_candidates (compatibility shim)
// ===========================================================================

#[test]
fn test_candidates_returns_single_entry_for_already_canonical_input() {
    // When the raw input already equals the canonical form and has no TS
    // extension to strip, there is no fan-out at all.
    assert_eq!(
        module_specifier_candidates("./foo"),
        vec!["./foo".to_string()]
    );
    assert_eq!(module_specifier_candidates("."), vec![".".to_string()]);
    assert_eq!(module_specifier_candidates("./"), vec!["./".to_string()]);
}

#[test]
fn test_candidates_returns_canonical_plus_raw_when_different() {
    let candidates = module_specifier_candidates("  \"./foo\"  ");
    // At most three entries — no quoted/chain variants, only canonical + raw
    // (+ a stem fallback when the extension is a recognized TS/JS one).
    assert!(candidates.len() <= 3);
    assert!(candidates.contains(&"./foo".to_string()));
    assert!(candidates.contains(&"  \"./foo\"  ".to_string()));
}

#[test]
fn test_candidates_normalizes_backslashes_without_fan_out() {
    let candidates = module_specifier_candidates(".\\foo\\bar");
    assert!(candidates.len() <= 3);
    assert!(candidates.contains(&"./foo/bar".to_string()));
    assert!(candidates.contains(&".\\foo\\bar".to_string()));
}

#[test]
fn test_candidates_adds_extension_stripped_stem_for_ts_extensions() {
    // TS allows `./foo.js` to resolve to a `./foo.d.ts` target. The stripped
    // stem is included as a single narrow fallback.
    let candidates = module_specifier_candidates("./foo.js");
    assert!(candidates.contains(&"./foo.js".to_string()));
    assert!(candidates.contains(&"./foo".to_string()));
    assert!(candidates.len() <= 3);
}

#[test]
fn test_candidates_has_no_stem_fallback_for_plain_identifiers() {
    // Plain specifiers without a recognized extension — no stem fallback.
    assert_eq!(
        module_specifier_candidates("./foo"),
        vec!["./foo".to_string()]
    );
    assert_eq!(
        module_specifier_candidates("lodash"),
        vec!["lodash".to_string()]
    );
}

/// Prove the new resolver does NOT rely on trying many textual aliases for
/// the same target. Every legitimate spelling that a caller might pass in
/// canonicalizes to a single (canonical) lookup key that is present in the
/// map. This test exercises a broad mix of spellings and asserts that each
/// canonicalizes to something the map already contains directly.
#[test]
fn test_resolution_does_not_depend_on_alias_explosion() {
    let files = vec![
        "/proj/src/main.ts".to_string(),
        "/proj/src/types.ts".to_string(),
        "/proj/src/lib/index.ts".to_string(),
    ];
    let (paths, _modules) = build_module_resolution_maps(&files);

    // Varying surface forms of "types" from main.ts.
    for raw in &[
        "./types",
        "  ./types  ",
        "\"./types\"",
        "'./types'",
        ".\\types",
        "./types/",
        "types",
    ] {
        let canon = normalize_import_specifier(raw).expect("canonical").text;
        assert!(
            paths.contains_key(&(0, canon.clone())),
            "map missing canonical lookup key {canon:?} (from raw {raw:?})",
        );
        // And exactly one canonical key covers the lookup — no fallback list.
        let candidates = module_specifier_candidates(raw);
        assert!(
            candidates.iter().any(|c| c == &canon),
            "candidates {candidates:?} should include canonical {canon:?}",
        );
    }

    // Varying surface forms of the `lib` directory index from main.ts.
    for raw in &["./lib", "./lib/", "'./lib'", ".\\lib", ".\\lib\\", "lib"] {
        let canon = normalize_import_specifier(raw).expect("canonical").text;
        assert!(
            paths.contains_key(&(0, canon.clone())),
            "map missing canonical lookup key {canon:?} (from raw {raw:?})",
        );
    }
}

// ===========================================================================
// TargetIndex / build_target_index
// ===========================================================================

#[test]
fn test_target_index_len_matches_non_skipped_files() {
    let files = vec![
        "/proj/a.ts".to_string(),
        "/proj/b.ts".to_string(),
        "/proj/c.d.ts".to_string(),
    ];
    let index = build_target_index(&files);
    assert_eq!(index.len(), 3);
    assert!(!index.is_empty());
}

#[test]
fn test_target_index_skips_arbitrary_extension_declaration_files() {
    // `foo.d.ts.ts` is an arbitrary-extension declaration file — it must be
    // dropped from the target index so its stripped form never shadows a real
    // declaration import.
    let files = vec!["/proj/a.ts".to_string(), "/proj/foo.d.ts.ts".to_string()];
    let index = build_target_index(&files);
    assert_eq!(index.len(), 1);
}

#[test]
fn test_empty_file_list_builds_empty_index() {
    let files: Vec<String> = vec![];
    let index = build_target_index(&files);
    assert!(index.is_empty());
}

// ===========================================================================
// resolve_from_source (index-based direct resolution)
// ===========================================================================

#[test]
fn test_resolve_from_source_relative_file() {
    let files = vec![
        "/proj/src/main.ts".to_string(),
        "/proj/src/types.ts".to_string(),
    ];
    let index = build_target_index(&files);
    let spec = normalize_import_specifier("./types").unwrap();
    assert_eq!(
        resolve_from_source("/proj/src/main.ts", &spec, &index),
        Some(1)
    );
}

#[test]
fn test_resolve_from_source_directory_index() {
    let files = vec![
        "/proj/src/main.ts".to_string(),
        "/proj/src/lib/index.ts".to_string(),
    ];
    let index = build_target_index(&files);
    for raw in &["./lib", "./lib/"] {
        let spec = normalize_import_specifier(raw).unwrap();
        assert_eq!(
            resolve_from_source("/proj/src/main.ts", &spec, &index),
            Some(1),
            "failed for {raw}",
        );
    }
}

#[test]
fn test_resolve_from_source_dot_and_dot_slash_both_work() {
    let files = vec![
        "/proj/a/index.ts".to_string(),
        "/proj/a/file.ts".to_string(),
    ];
    let index = build_target_index(&files);
    for raw in &[".", "./"] {
        let spec = normalize_import_specifier(raw).unwrap();
        assert_eq!(
            resolve_from_source("/proj/a/file.ts", &spec, &index),
            Some(0),
            "failed for {raw}",
        );
    }
}

#[test]
fn test_resolve_from_source_returns_none_for_bare_specifiers() {
    let files = vec!["/proj/src/main.ts".to_string()];
    let index = build_target_index(&files);
    let spec = normalize_import_specifier("lodash").unwrap();
    assert_eq!(
        resolve_from_source("/proj/src/main.ts", &spec, &index),
        None
    );
}

// ===========================================================================
// build_module_resolution_maps — intentional behavior
// ===========================================================================

#[test]
fn test_simple_relative_import() {
    let files = vec![
        "/tmp/test/main.ts".to_string(),
        "/tmp/test/types.ts".to_string(),
    ];

    let (paths, modules) = build_module_resolution_maps(&files);

    assert_eq!(paths.get(&(0, "./types".to_string())), Some(&1));
    assert!(modules.contains("./types"));
    // Same-directory bare alias (intentional compatibility shim).
    assert_eq!(paths.get(&(0, "types".to_string())), Some(&1));
}

#[test]
fn test_bidirectional_resolution() {
    let files = vec!["/tmp/test/a.ts".to_string(), "/tmp/test/b.ts".to_string()];

    let (paths, _) = build_module_resolution_maps(&files);

    assert_eq!(paths.get(&(0, "./b".to_string())), Some(&1));
    assert_eq!(paths.get(&(1, "./a".to_string())), Some(&0));
}

#[test]
fn test_nested_import() {
    let files = vec![
        "/tmp/test/main.ts".to_string(),
        "/tmp/test/lib/utils.ts".to_string(),
    ];

    let (paths, modules) = build_module_resolution_maps(&files);

    assert_eq!(paths.get(&(0, "./lib/utils".to_string())), Some(&1));
    assert!(modules.contains("./lib/utils"));
}

#[test]
fn test_deeply_nested_import() {
    let files = vec![
        "/tmp/test/main.ts".to_string(),
        "/tmp/test/src/lib/deep/module.ts".to_string(),
    ];

    let (paths, modules) = build_module_resolution_maps(&files);

    assert_eq!(
        paths.get(&(0, "./src/lib/deep/module".to_string())),
        Some(&1)
    );
    assert!(modules.contains("./src/lib/deep/module"));
}

#[test]
fn test_nested_bare_alias_is_not_registered() {
    // Intentional removal: `lib/utils` (bare nested) would semantically
    // collide with package sub-path imports; not registered anymore.
    let files = vec![
        "/tmp/test/main.ts".to_string(),
        "/tmp/test/lib/utils.ts".to_string(),
    ];
    let (paths, _) = build_module_resolution_maps(&files);
    assert!(!paths.contains_key(&(0, "lib/utils".to_string())));
}

#[test]
fn test_node_modules_sibling_package_bare_alias_is_registered() {
    let files = vec![
        "/tmp/test/node_modules/baz/index.d.ts".to_string(),
        "/tmp/test/node_modules/foo/index.d.ts".to_string(),
    ];
    let (paths, modules) = build_module_resolution_maps(&files);

    assert_eq!(paths.get(&(0, "foo".to_string())), Some(&1));
    assert!(modules.contains("foo"));
}

#[test]
fn test_scoped_node_modules_package_bare_alias_is_registered() {
    let files = vec![
        "/tmp/test/src/app.ts".to_string(),
        "/tmp/test/node_modules/@scope/pkg/index.d.ts".to_string(),
    ];
    let (paths, modules) = build_module_resolution_maps(&files);

    assert_eq!(paths.get(&(0, "@scope/pkg".to_string())), Some(&1));
    assert!(modules.contains("@scope/pkg"));
}

#[test]
fn test_parent_directory_import() {
    let files = vec![
        "/tmp/test/src/app.ts".to_string(),
        "/tmp/test/lib/utils.ts".to_string(),
    ];

    let (paths, modules) = build_module_resolution_maps(&files);

    assert_eq!(paths.get(&(0, "../lib/utils".to_string())), Some(&1));
    assert!(modules.contains("../lib/utils"));
}

#[test]
fn test_sibling_directory_import() {
    let files = vec![
        "/tmp/test/src/components/Button.tsx".to_string(),
        "/tmp/test/src/utils/helpers.ts".to_string(),
    ];

    let (paths, _) = build_module_resolution_maps(&files);

    assert_eq!(paths.get(&(0, "../utils/helpers".to_string())), Some(&1));
}

#[test]
fn test_tsx_extension() {
    let files = vec![
        "/tmp/test/app.ts".to_string(),
        "/tmp/test/Button.tsx".to_string(),
    ];
    let (paths, _) = build_module_resolution_maps(&files);
    assert_eq!(paths.get(&(0, "./Button".to_string())), Some(&1));
}

#[test]
fn test_dts_extension() {
    let files = vec![
        "/tmp/test/app.ts".to_string(),
        "/tmp/test/types.d.ts".to_string(),
    ];
    let (paths, _) = build_module_resolution_maps(&files);
    assert_eq!(paths.get(&(0, "./types".to_string())), Some(&1));
}

#[test]
fn test_mts_extension() {
    let files = vec![
        "/tmp/test/app.mts".to_string(),
        "/tmp/test/utils.mts".to_string(),
    ];
    let (paths, _) = build_module_resolution_maps(&files);
    assert_eq!(paths.get(&(0, "./utils".to_string())), Some(&1));
}

#[test]
fn test_js_extension() {
    let files = vec![
        "/tmp/test/app.ts".to_string(),
        "/tmp/test/legacy.js".to_string(),
    ];
    let (paths, _) = build_module_resolution_maps(&files);
    assert_eq!(paths.get(&(0, "./legacy".to_string())), Some(&1));
}

#[test]
fn test_cjs_extension() {
    let files = vec![
        "/tmp/test/app.ts".to_string(),
        "/tmp/test/config.cjs".to_string(),
    ];
    let (paths, _) = build_module_resolution_maps(&files);
    assert_eq!(paths.get(&(0, "./config".to_string())), Some(&1));
}

#[test]
fn test_declaration_mts() {
    let files = vec![
        "/tmp/test/app.ts".to_string(),
        "/tmp/test/types.d.mts".to_string(),
    ];
    let (paths, _) = build_module_resolution_maps(&files);
    assert_eq!(paths.get(&(0, "./types".to_string())), Some(&1));
}

#[test]
fn test_index_file_resolution() {
    let files = vec![
        "/tmp/test/main.ts".to_string(),
        "/tmp/test/lib/index.ts".to_string(),
    ];

    let (paths, modules) = build_module_resolution_maps(&files);

    assert_eq!(paths.get(&(0, "./lib".to_string())), Some(&1));
    assert!(modules.contains("./lib"));
    assert_eq!(paths.get(&(0, "./lib/index".to_string())), Some(&1));
}

#[test]
fn test_index_tsx_resolution() {
    let files = vec![
        "/tmp/test/main.ts".to_string(),
        "/tmp/test/components/index.tsx".to_string(),
    ];
    let (paths, _) = build_module_resolution_maps(&files);
    assert_eq!(paths.get(&(0, "./components".to_string())), Some(&1));
}

#[test]
fn test_index_dts_resolution() {
    let files = vec![
        "/tmp/test/main.ts".to_string(),
        "/tmp/test/types/index.d.ts".to_string(),
    ];
    let (paths, _) = build_module_resolution_maps(&files);
    assert_eq!(paths.get(&(0, "./types".to_string())), Some(&1));
}

#[test]
fn test_nested_index_resolution() {
    let files = vec![
        "/tmp/test/main.ts".to_string(),
        "/tmp/test/src/lib/index.ts".to_string(),
    ];
    let (paths, _) = build_module_resolution_maps(&files);
    assert_eq!(paths.get(&(0, "./src/lib".to_string())), Some(&1));
}

#[test]
fn test_same_dir_index_registers_dot_and_slash_forms() {
    // For `a/index.ts` imported from `a/file.ts`, both `.` and `./` are valid
    // spellings and are both registered directly in the map.
    let files = vec![
        "/proj/a/index.ts".to_string(),
        "/proj/a/file.ts".to_string(),
    ];
    let (paths, modules) = build_module_resolution_maps(&files);

    assert_eq!(paths.get(&(1, ".".to_string())), Some(&0));
    assert_eq!(paths.get(&(1, "./".to_string())), Some(&0));
    assert!(modules.contains("."));
    assert!(modules.contains("./"));
}

#[test]
fn test_parent_dir_index_registers_dot_dot_and_slash_forms() {
    let files = vec![
        "/proj/a/index.ts".to_string(),
        "/proj/a/b/file.ts".to_string(),
    ];
    let (paths, modules) = build_module_resolution_maps(&files);

    // From `a/b/file.ts`, `..` and `../` both point at `a/index.ts`.
    assert_eq!(paths.get(&(1, "..".to_string())), Some(&0));
    assert_eq!(paths.get(&(1, "../".to_string())), Some(&0));
    assert!(modules.contains(".."));
    assert!(modules.contains("../"));
}

#[test]
fn test_grandparent_dir_index_registers_chain_and_slash_forms() {
    let files = vec![
        "/proj/a/index.ts".to_string(),
        "/proj/a/b/c/file.ts".to_string(),
    ];
    let (paths, _) = build_module_resolution_maps(&files);

    // Both `../..` and `../../` refer to `a/index.ts` from `a/b/c/file.ts`.
    assert_eq!(paths.get(&(1, "../..".to_string())), Some(&0));
    assert_eq!(paths.get(&(1, "../../".to_string())), Some(&0));
}

#[test]
fn test_multiple_targets() {
    let files = vec![
        "/tmp/test/main.ts".to_string(),
        "/tmp/test/utils.ts".to_string(),
        "/tmp/test/types.ts".to_string(),
        "/tmp/test/config.ts".to_string(),
    ];
    let (paths, _) = build_module_resolution_maps(&files);
    assert_eq!(paths.get(&(0, "./utils".to_string())), Some(&1));
    assert_eq!(paths.get(&(0, "./types".to_string())), Some(&2));
    assert_eq!(paths.get(&(0, "./config".to_string())), Some(&3));
}

#[test]
fn test_cross_imports_between_nested() {
    let files = vec![
        "/tmp/test/src/a.ts".to_string(),
        "/tmp/test/src/b.ts".to_string(),
        "/tmp/test/lib/c.ts".to_string(),
    ];
    let (paths, _) = build_module_resolution_maps(&files);

    assert_eq!(paths.get(&(0, "./b".to_string())), Some(&1));
    assert_eq!(paths.get(&(0, "../lib/c".to_string())), Some(&2));
    assert_eq!(paths.get(&(2, "../src/a".to_string())), Some(&0));
}

#[test]
fn test_self_import_resolves_to_same_file() {
    let files = vec!["/tmp/test/main.ts".to_string()];
    let (paths, _) = build_module_resolution_maps(&files);
    assert_eq!(paths.get(&(0, "./main".to_string())), Some(&0));
    assert_eq!(paths.get(&(0, "main".to_string())), Some(&0));
}

#[test]
fn test_empty_file_list() {
    let files: Vec<String> = vec![];
    let (paths, modules) = build_module_resolution_maps(&files);
    assert!(paths.is_empty());
    assert!(modules.is_empty());
}

#[test]
fn test_typical_project_layout() {
    let files = vec![
        "/project/src/index.ts".to_string(),
        "/project/src/app.ts".to_string(),
        "/project/src/components/Button.tsx".to_string(),
        "/project/src/components/index.ts".to_string(),
        "/project/src/utils/helpers.ts".to_string(),
        "/project/src/types/api.d.ts".to_string(),
    ];

    let (paths, _) = build_module_resolution_maps(&files);

    assert_eq!(paths.get(&(0, "./app".to_string())), Some(&1));
    assert_eq!(paths.get(&(0, "./components".to_string())), Some(&3));
    assert_eq!(paths.get(&(0, "./components/Button".to_string())), Some(&2));
    assert_eq!(paths.get(&(0, "./utils/helpers".to_string())), Some(&4));
    assert_eq!(paths.get(&(0, "./types/api".to_string())), Some(&5));
    assert_eq!(paths.get(&(2, "../utils/helpers".to_string())), Some(&4));
    assert_eq!(paths.get(&(2, "../types/api".to_string())), Some(&5));
}

#[test]
fn test_arbitrary_extension_declaration_file_is_not_mapped() {
    // A `foo.d.ts.ts` file is skipped by the target index, so `./foo.d.ts`
    // and `./foo` both remain unmapped (there is no other file providing them).
    let files = vec!["/proj/app.ts".to_string(), "/proj/foo.d.ts.ts".to_string()];
    let (paths, modules) = build_module_resolution_maps(&files);
    assert!(!paths.contains_key(&(0, "./foo.d.ts".to_string())));
    assert!(!paths.contains_key(&(0, "./foo".to_string())));
    assert!(!modules.contains("./foo.d.ts"));
    assert!(!modules.contains("./foo"));
}

#[test]
fn test_extension_bearing_specifier_resolves_to_same_target() {
    // Users legitimately write the extension in `require()`, triple-slash
    // references, and JS sources. Both `./a` and `./a.js` must resolve to
    // the same target file.
    let files = vec!["/proj/main.ts".to_string(), "/proj/a.js".to_string()];
    let (paths, modules) = build_module_resolution_maps(&files);

    assert_eq!(paths.get(&(0, "./a".to_string())), Some(&1));
    assert_eq!(paths.get(&(0, "./a.js".to_string())), Some(&1));
    assert!(modules.contains("./a"));
    assert!(modules.contains("./a.js"));
}

#[test]
fn test_extension_bearing_dts_specifier() {
    let files = vec!["/proj/main.ts".to_string(), "/proj/types.d.ts".to_string()];
    let (paths, _) = build_module_resolution_maps(&files);
    assert_eq!(paths.get(&(0, "./types".to_string())), Some(&1));
    assert_eq!(paths.get(&(0, "./types.d.ts".to_string())), Some(&1));
}

// ---------------------------------------------------------------------------
// resolve_specifier_via_file_index — matches every canonical form
// `build_module_resolution_maps` registers, without the O(N²) cross-product.
// ---------------------------------------------------------------------------

fn file_index_from(files: &[&str]) -> FxHashMap<String, usize> {
    files
        .iter()
        .enumerate()
        .map(|(i, s)| ((*s).to_string(), i))
        .collect()
}

#[test]
fn test_fast_resolver_same_dir_relative() {
    let files = ["/proj/a.ts", "/proj/b.ts"];
    let idx = file_index_from(&files);
    assert_eq!(
        resolve_specifier_via_file_index("/proj/a.ts", "./b", &idx),
        Some(1),
    );
    assert_eq!(
        resolve_specifier_via_file_index("/proj/a.ts", "./b.ts", &idx),
        Some(1),
    );
    // Same-directory bare alias.
    assert_eq!(
        resolve_specifier_via_file_index("/proj/a.ts", "b", &idx),
        Some(1),
    );
}

#[test]
fn test_fast_resolver_parent_and_sibling() {
    let files = ["/proj/src/a.ts", "/proj/lib/b.ts"];
    let idx = file_index_from(&files);
    assert_eq!(
        resolve_specifier_via_file_index("/proj/src/a.ts", "../lib/b", &idx),
        Some(1),
    );
    assert_eq!(
        resolve_specifier_via_file_index("/proj/src/a.ts", "../lib/b.ts", &idx),
        Some(1),
    );
}

#[test]
fn test_fast_resolver_directory_index() {
    let files = ["/proj/main.ts", "/proj/lib/index.ts"];
    let idx = file_index_from(&files);
    assert_eq!(
        resolve_specifier_via_file_index("/proj/main.ts", "./lib", &idx),
        Some(1),
    );
    assert_eq!(
        resolve_specifier_via_file_index("/proj/main.ts", "./lib/", &idx),
        Some(1),
    );
}

#[test]
fn test_fast_resolver_dot_chain_to_index() {
    let files = ["/proj/sub/main.ts", "/proj/sub/index.ts"];
    let idx = file_index_from(&files);
    assert_eq!(
        resolve_specifier_via_file_index("/proj/sub/main.ts", ".", &idx),
        Some(1),
    );
    assert_eq!(
        resolve_specifier_via_file_index("/proj/sub/main.ts", "./", &idx),
        Some(1),
    );
}

#[test]
fn test_fast_resolver_parent_dot_chain_to_index() {
    let files = ["/proj/sub/main.ts", "/proj/index.ts"];
    let idx = file_index_from(&files);
    assert_eq!(
        resolve_specifier_via_file_index("/proj/sub/main.ts", "..", &idx),
        Some(1),
    );
    assert_eq!(
        resolve_specifier_via_file_index("/proj/sub/main.ts", "../", &idx),
        Some(1),
    );
}

#[test]
fn test_fast_resolver_handles_bare_source_file_name() {
    // Regression: earlier the resolver returned None when the source file
    // had no directory component, breaking test harnesses that use bare
    // file names like `other.js`. Treat a missing src_dir as "current
    // directory" so relative specifiers still resolve against siblings.
    let files = ["types.ts", "other.js"];
    let idx = file_index_from(&files);
    assert_eq!(
        resolve_specifier_via_file_index("other.js", "./types", &idx),
        Some(0),
    );
}

#[test]
fn test_fast_resolver_miss_for_unknown_specifier() {
    let files = ["/proj/main.ts", "/proj/types.d.ts"];
    let idx = file_index_from(&files);
    assert_eq!(
        resolve_specifier_via_file_index("/proj/main.ts", "./does-not-exist", &idx),
        None,
    );
    // Bare package-style specifier that isn't a same-dir file is not a match.
    assert_eq!(
        resolve_specifier_via_file_index("/proj/main.ts", "react", &idx),
        None,
    );
}

#[test]
fn test_fast_resolver_rejects_nested_bare_package_subpaths() {
    let files = [
        "/proj/main.ts",
        "/proj/lib/utils.ts",
        "/proj/react/jsx-runtime/index.ts",
        "/proj/lib/index.ts",
    ];
    let idx = file_index_from(&files);

    // Only same-directory single-segment bare aliases are supported. Nested
    // bare specifiers are package subpaths, so the fallback must not reinterpret
    // them as project-relative files after the primary resolver misses.
    assert_eq!(
        resolve_specifier_via_file_index("/proj/main.ts", "lib/utils", &idx),
        None,
    );
    assert_eq!(
        resolve_specifier_via_file_index("/proj/main.ts", "react/jsx-runtime", &idx),
        None,
    );
    assert_eq!(
        resolve_specifier_via_file_index("/proj/main.ts", "lib", &idx),
        Some(3),
    );
}

#[test]
fn test_fast_resolver_tsx_and_dts_fanout() {
    let files = ["/proj/a.tsx", "/proj/b/main.ts"];
    let idx = file_index_from(&files);
    assert_eq!(
        resolve_specifier_via_file_index("/proj/b/main.ts", "../a", &idx),
        Some(0),
    );
    // Extension-bearing form resolves directly.
    assert_eq!(
        resolve_specifier_via_file_index("/proj/b/main.ts", "../a.tsx", &idx),
        Some(0),
    );
}

#[test]
fn test_fast_resolver_matches_legacy_map_entries() {
    // For a realistic cross-directory project, every (src, specifier) entry
    // `build_module_resolution_maps` registers must be resolvable by the
    // fast resolver. That's the invariant that makes the hot-path fallback
    // behavior-preserving.
    let files = vec![
        "/proj/pkg/src/a.ts".to_string(),
        "/proj/pkg/src/b.ts".to_string(),
        "/proj/pkg/src/nested/c.ts".to_string(),
        "/proj/pkg/src/nested/index.ts".to_string(),
        "/proj/pkg/lib/util.ts".to_string(),
        "/proj/pkg/lib/index.tsx".to_string(),
        "/proj/pkg/types.d.ts".to_string(),
    ];
    let (legacy, _) = build_module_resolution_maps(&files);
    let idx = file_index_from(&files.iter().map(String::as_str).collect::<Vec<_>>());

    for ((src_idx, specifier), &tgt) in legacy.iter() {
        let got = resolve_specifier_via_file_index(&files[*src_idx], specifier, &idx);
        assert_eq!(
            got,
            Some(tgt),
            "fast resolver disagreed for src={} spec={}",
            files[*src_idx],
            specifier,
        );
    }
}
