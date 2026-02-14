use super::*;

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
    // .d.ts must be stripped as a whole, not just .ts leaving ".d"
    assert_eq!(strip_ts_extension("types.d.ts"), "types");
    assert_eq!(strip_ts_extension("globals.d.tsx"), "globals");
}

#[test]
fn test_relative_specifier_same_dir() {
    let from = Path::new("/tmp/test");
    let to = Path::new("/tmp/test/types.ts");
    assert_eq!(relative_specifier(from, to), Some("./types".to_string()));
}

#[test]
fn test_relative_specifier_nested() {
    let from = Path::new("/tmp/test");
    let to = Path::new("/tmp/test/lib/utils.ts");
    assert_eq!(
        relative_specifier(from, to),
        Some("./lib/utils".to_string())
    );
}

#[test]
fn test_relative_specifier_parent_dir() {
    let from = Path::new("/tmp/test/src");
    let to = Path::new("/tmp/test/lib/utils.ts");
    assert_eq!(
        relative_specifier(from, to),
        Some("../lib/utils".to_string())
    );
}

#[test]
fn test_relative_specifier_two_levels_up() {
    let from = Path::new("/tmp/test/src/deep");
    let to = Path::new("/tmp/test/lib/utils.ts");
    assert_eq!(
        relative_specifier(from, to),
        Some("../../lib/utils".to_string())
    );
}

#[test]
fn test_relative_specifier_dts_extension() {
    let from = Path::new("/tmp/test");
    let to = Path::new("/tmp/test/types.d.ts");
    assert_eq!(relative_specifier(from, to), Some("./types".to_string()));
}

#[test]
fn test_simple_relative_import() {
    let files = vec![
        "/tmp/test/main.ts".to_string(),
        "/tmp/test/types.ts".to_string(),
    ];

    let (paths, modules) = build_module_resolution_maps(&files);

    assert_eq!(paths.get(&(0, "./types".to_string())), Some(&1));
    assert!(modules.contains("./types"));
    // Also available without ./ prefix
    assert_eq!(paths.get(&(0, "types".to_string())), Some(&1));
}

#[test]
fn test_module_specifier_candidates_trims_quotes_and_whitespace() {
    let actual = module_specifier_candidates("  \"./foo\"  ");
    assert!(actual.contains(&"./foo".to_string()));
    assert!(actual.contains(&"\"./foo\"".to_string()));
    assert!(actual.contains(&"'./foo'".to_string()));
}

#[test]
fn test_module_specifier_candidates_normalizes_backslashes() {
    let actual = module_specifier_candidates(".\\foo\\bar");
    assert!(actual.contains(&".\\foo\\bar".to_string()));
    assert!(actual.contains(&"./foo/bar".to_string()));
}

#[test]
fn test_bidirectional_resolution() {
    let files = vec!["/tmp/test/a.ts".to_string(), "/tmp/test/b.ts".to_string()];

    let (paths, _) = build_module_resolution_maps(&files);

    // a can import b
    assert_eq!(paths.get(&(0, "./b".to_string())), Some(&1));
    // b can import a
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
fn test_parent_directory_import() {
    let files = vec![
        "/tmp/test/src/app.ts".to_string(),
        "/tmp/test/lib/utils.ts".to_string(),
    ];

    let (paths, modules) = build_module_resolution_maps(&files);

    // From src/app.ts, ../lib/utils should resolve to lib/utils.ts
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

    // ./lib should resolve to lib/index.ts
    assert_eq!(paths.get(&(0, "./lib".to_string())), Some(&1));
    assert!(modules.contains("./lib"));
    // ./lib/index should also work
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

    // a -> b (same directory)
    assert_eq!(paths.get(&(0, "./b".to_string())), Some(&1));
    // a -> c (different directory)
    assert_eq!(paths.get(&(0, "../lib/c".to_string())), Some(&2));
    // c -> a
    assert_eq!(paths.get(&(2, "../src/a".to_string())), Some(&0));
}

#[test]
fn test_self_import_excluded() {
    let files = vec!["/tmp/test/main.ts".to_string()];

    let (paths, _) = build_module_resolution_maps(&files);

    // A file should not resolve to itself
    assert!(paths.is_empty());
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

    // index.ts -> app
    assert_eq!(paths.get(&(0, "./app".to_string())), Some(&1));
    // index.ts -> components (via index.ts)
    assert_eq!(paths.get(&(0, "./components".to_string())), Some(&3));
    // index.ts -> components/Button
    assert_eq!(paths.get(&(0, "./components/Button".to_string())), Some(&2));
    // index.ts -> utils/helpers
    assert_eq!(paths.get(&(0, "./utils/helpers".to_string())), Some(&4));
    // index.ts -> types/api
    assert_eq!(paths.get(&(0, "./types/api".to_string())), Some(&5));
    // Button -> ../utils/helpers
    assert_eq!(paths.get(&(2, "../utils/helpers".to_string())), Some(&4));
    // Button -> ../types/api
    assert_eq!(paths.get(&(2, "../types/api".to_string())), Some(&5));
}
