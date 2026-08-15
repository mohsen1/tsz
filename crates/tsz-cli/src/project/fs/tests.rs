//! Tests for project file discovery (`super`), split out to keep
//! `fs.rs` under the 2000-line CLI source ceiling (#16733).

use super::*;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_temp_dir(label: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    dir.push(format!(
        "tsz_fs_unit_{label}_{}_{}",
        std::process::id(),
        nanos
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn test_build_include_patterns_defaults_only_when_files_are_not_explicit() {
    let implicit_options = FileDiscoveryOptions {
        base_dir: PathBuf::from("."),
        files: Vec::new(),
        files_explicitly_set: false,
        include: None,
        exclude: None,
        out_dir: None,
        follow_links: false,
        allow_js: false,
        resolve_json_module: false,
    };
    let implicit_patterns = build_include_patterns(&implicit_options);
    let pattern_strings: Vec<&str> = implicit_patterns
        .iter()
        .map(|(_, pattern)| pattern.as_str())
        .collect();
    assert_eq!(
        pattern_strings,
        vec![
            "*.ts", "*.tsx", "*.mts", "*.cts", "**/*.ts", "**/*.tsx", "**/*.mts", "**/*.cts"
        ]
    );
    // The zero-config default is ONE spec (tsc's real default is the
    // single pattern `**/*`), even though it synthesizes several
    // per-extension glob strings — every one of them must share spec
    // index 0 so they bucket together instead of ordering `.ts` ahead
    // of `.js` the way an explicit multi-pattern `include` would.
    assert!(
        implicit_patterns
            .iter()
            .all(|(spec_index, _)| *spec_index == 0),
        "zero-config default patterns must all share spec index 0"
    );

    let explicit_options = FileDiscoveryOptions {
        files_explicitly_set: true,
        ..implicit_options
    };
    assert!(build_include_patterns(&explicit_options).is_empty());
}

#[test]
fn test_build_include_patterns_include_json_when_enabled() {
    let options = FileDiscoveryOptions {
        base_dir: PathBuf::from("."),
        files: Vec::new(),
        files_explicitly_set: false,
        include: None,
        exclude: None,
        out_dir: None,
        follow_links: false,
        allow_js: false,
        resolve_json_module: true,
    };

    let pattern_strings: Vec<String> = build_include_patterns(&options)
        .into_iter()
        .map(|(_, pattern)| pattern)
        .collect();
    assert_eq!(
        pattern_strings,
        vec![
            "*.ts", "*.tsx", "*.mts", "*.cts", "**/*.ts", "**/*.tsx", "**/*.mts", "**/*.cts",
        ]
    );
}

#[test]
fn test_normalize_patterns_trims_drops_empty_and_normalizes_prefixes() {
    let normalized = normalize_patterns(&[
        "  ./src\\nested  ".to_string(),
        "".to_string(),
        "   ".to_string(),
        ".\\tests\\case.ts".to_string(),
    ]);

    assert_eq!(normalized, vec!["src/nested", "tests/case.ts"]);
}

#[test]
fn test_expand_include_patterns_preserves_explicit_files_and_expands_directories() {
    let expanded: Vec<String> = expand_include_patterns(&[
        "src".to_string(),
        "tests/".to_string(),
        "src/*".to_string(),
        "already/**/*".to_string(),
        "index.ts".to_string(),
        "subdir/*.tsx".to_string(),
    ])
    .into_iter()
    .map(|(_, pattern)| pattern)
    .collect();

    assert_eq!(
        expanded,
        vec![
            "src/**/*".to_string(),
            "tests/**/*".to_string(),
            "src/*".to_string(),
            "src/*/**/*".to_string(),
            "already/**/*".to_string(),
            "index.ts".to_string(),
            "subdir/*.tsx".to_string(),
        ]
    );
}

#[test]
fn test_expand_include_patterns_keeps_one_spec_index_per_directory_entry() {
    // "src/*" is one user-written spec that expands to two glob patterns
    // ("src/*" itself and "src/*/**/*"); both must carry the same spec
    // index so discover_ts_files buckets them together instead of
    // letting the expansion silently create a second bucket.
    let expanded = expand_include_patterns(&["src/*".to_string(), "*.ts".to_string()]);
    let spec_indices: Vec<usize> = expanded.iter().map(|(index, _)| *index).collect();
    assert_eq!(spec_indices, vec![0, 0, 1]);
}

#[test]
fn test_expand_include_current_directory_is_root_recursive() {
    // tsc expands a directory spec to `<dir>/**/*`; the current-directory
    // spellings `"."` and `"./"` (the latter normalized to "") must become a
    // root-relative `**/*`, not `./**/*` or `/**/*` (which globset cannot
    // match against discovery-relative paths). See `directory_recursive_glob`.
    let patterns_only = |patterns: Vec<(usize, String)>| -> Vec<String> {
        patterns.into_iter().map(|(_, pattern)| pattern).collect()
    };
    assert_eq!(
        patterns_only(expand_include_patterns(&normalize_patterns(&[
            ".".to_string()
        ]))),
        vec!["**/*".to_string()]
    );
    assert_eq!(
        patterns_only(expand_include_patterns(&normalize_patterns(&[
            "./".to_string()
        ]))),
        vec!["**/*".to_string()]
    );
    assert_eq!(
        patterns_only(expand_include_patterns(&normalize_patterns(&[
            "./src".to_string()
        ]))),
        vec!["src/**/*".to_string()]
    );
}

#[test]
fn test_discover_current_directory_include_recurses() {
    // Regression: `include: ["."]` must discover every nested source file
    // (matching tsc), not resolve to zero inputs / TS18003.
    let dir = unique_temp_dir("current_dir_include");
    fs::create_dir_all(dir.join("src/nested")).unwrap();
    fs::write(dir.join("top.ts"), "export const top = 1;").unwrap();
    fs::write(dir.join("src/a.ts"), "export const a = 1;").unwrap();
    fs::write(dir.join("src/nested/b.ts"), "export const b = 1;").unwrap();

    for spec in ["./", "."] {
        let options = FileDiscoveryOptions {
            base_dir: dir.clone(),
            files: Vec::new(),
            files_explicitly_set: false,
            include: Some(vec![spec.to_string()]),
            exclude: None,
            out_dir: None,
            follow_links: false,
            allow_js: false,
            resolve_json_module: false,
        };

        let result = discover_ts_files(&options).unwrap();
        for expected in ["top.ts", "src/a.ts", "src/nested/b.ts"] {
            assert!(
                result.iter().any(|path| path.ends_with(expected)),
                "include [{spec:?}] should discover {expected}, got: {result:?}"
            );
        }
    }

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_discover_terminal_include_star_matches_direct_files() {
    let dir = unique_temp_dir("terminal_include_star");
    fs::create_dir_all(dir.join("src/nested")).unwrap();
    fs::write(dir.join("src/a.js"), "const direct = 1;").unwrap();
    fs::write(dir.join("src/nested/b.js"), "const nested = 1;").unwrap();

    let options = FileDiscoveryOptions {
        base_dir: dir.clone(),
        files: Vec::new(),
        files_explicitly_set: false,
        include: Some(vec!["src/*".to_string()]),
        exclude: None,
        out_dir: None,
        follow_links: false,
        allow_js: true,
        resolve_json_module: false,
    };

    let result = discover_ts_files(&options).unwrap();
    assert!(
        result.iter().any(|path| path.ends_with("src/a.js")),
        "terminal include star should match direct files, got: {result:?}"
    );
    assert!(
        result.iter().any(|path| path.ends_with("src/nested/b.js")),
        "terminal include star should also recurse through matched directories, got: {result:?}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_build_exclude_patterns_adds_defaults_and_relative_out_dir() {
    let base_dir = PathBuf::from("/repo");
    let options = FileDiscoveryOptions {
        base_dir: base_dir.clone(),
        files: Vec::new(),
        files_explicitly_set: false,
        include: None,
        exclude: None,
        out_dir: Some(base_dir.join("dist")),
        follow_links: false,
        allow_js: false,
        resolve_json_module: false,
    };

    let patterns = build_exclude_patterns(&options);

    assert!(patterns.contains(&"node_modules".to_string()));
    assert!(patterns.contains(&"**/node_modules/**".to_string()));
    assert!(patterns.contains(&"dist".to_string()));
    assert!(patterns.contains(&"dist/**".to_string()));
}

#[test]
fn test_leading_globstar_exclude_matches_include_root_relative_path() {
    let dir = unique_temp_dir("leading_globstar_exclude");
    fs::create_dir_all(dir.join("src/dialect/mssql")).unwrap();
    fs::create_dir_all(dir.join("src/dialect/mysql")).unwrap();
    fs::write(
        dir.join("src/dialect/mssql/skip.ts"),
        "export const skip = 1;",
    )
    .unwrap();
    fs::write(
        dir.join("src/dialect/mysql/keep.ts"),
        "export const keep = 1;",
    )
    .unwrap();

    let options = FileDiscoveryOptions {
        base_dir: dir.clone(),
        files: Vec::new(),
        files_explicitly_set: false,
        include: Some(vec!["src/**/*.ts".to_string()]),
        exclude: Some(vec!["**/dialect/mssql/**".to_string()]),
        out_dir: None,
        follow_links: false,
        allow_js: false,
        resolve_json_module: false,
    };

    let result = discover_ts_files(&options).unwrap();
    assert!(
        result
            .iter()
            .any(|path| path.ends_with("src/dialect/mysql/keep.ts")),
        "expected non-excluded mysql file, got: {result:?}"
    );
    assert!(
        !result
            .iter()
            .any(|path| path.ends_with("src/dialect/mssql/skip.ts")),
        "leading globstar exclude should match paths relative to the include root, got: {result:?}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_allow_entry_handles_paths_outside_base_dir() {
    let base_dir = unique_temp_dir("base");
    let outside_dir = unique_temp_dir("outside");
    let outside_file = outside_dir.join("skip.ts");
    fs::write(&outside_file, "export const skip = 1;").unwrap();

    let exclude = build_globset(&[outside_file.to_string_lossy().to_string()]).unwrap();
    let entry = walkdir::WalkDir::new(&outside_file)
        .max_depth(0)
        .into_iter()
        .next()
        .unwrap()
        .unwrap();

    assert!(!allow_entry(&entry, &base_dir, Some(&exclude)));

    let _ = fs::remove_dir_all(&base_dir);
    let _ = fs::remove_dir_all(&outside_dir);
}

#[test]
fn test_module_file_predicates_distinguish_ts_js_and_json() {
    assert!(is_ts_file(Path::new("types.d.ts")));
    assert!(is_ts_file(Path::new("types.d.mts")));
    assert!(is_valid_module_file(Path::new("config.json")));
    assert!(!is_valid_module_file(Path::new("script.js")));
    assert!(is_valid_module_or_js_file(Path::new("script.js")));
    assert!(!is_valid_module_or_js_file(Path::new("README.md")));
}

#[test]
fn test_path_to_pattern_handles_absolute_relative_and_empty_paths() {
    let base_dir = Path::new("/repo");
    assert_eq!(
        path_to_pattern(base_dir, Path::new("src\\nested")),
        Some("src/nested".to_string())
    );
    assert_eq!(
        path_to_pattern(base_dir, Path::new("/repo/dist")),
        Some("dist".to_string())
    );
    assert_eq!(path_to_pattern(base_dir, Path::new("")), None);
    assert_eq!(path_to_pattern(base_dir, Path::new("/other/place")), None);
}

#[test]
fn test_path_has_node_modules_component_matches_whole_component() {
    assert!(path_has_node_modules_component(Path::new(
        "project/node_modules/pkg/index.d.ts"
    )));
    assert!(path_has_node_modules_component(Path::new(
        "/repo/node_modules"
    )));
    assert!(!path_has_node_modules_component(Path::new(
        "project/not_node_modules/pkg/index.d.ts"
    )));
    assert!(!path_has_node_modules_component(Path::new(
        "project/node_modules_cache/pkg/index.d.ts"
    )));
}

#[test]
fn test_ensure_file_exists_rejects_directory_paths() {
    let dir = unique_temp_dir("directory");
    let err = ensure_file_exists(&dir, Path::new("directory")).unwrap_err();
    let msg = err.to_string();
    assert_eq!(msg, "TS6231: directory");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_ensure_file_exists_normalizes_current_dir_to_empty() {
    let dir = unique_temp_dir("dot");
    let err = ensure_file_exists(&dir, Path::new(".")).unwrap_err();
    let msg = err.to_string();
    assert_eq!(msg, "TS6231: ");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_build_globset_reports_invalid_pattern() {
    let err = build_globset(&["[".to_string()]).unwrap_err();
    assert!(err.to_string().contains("invalid glob pattern"));
}

#[test]
fn test_discover_explicitly_listed_js_file_without_allow_js() {
    // Explicitly listed .js files should be included even when allow_js is false.
    // This matches tsc behavior where CLI positional args and tsconfig "files"
    // entries are always compiled regardless of the allowJs setting.
    let dir = std::env::temp_dir().join("tsz_fs_test_explicit_js");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("app.ts"), "const x = 1;").unwrap();
    fs::write(dir.join("lib.js"), "var y = 2;").unwrap();

    let options = FileDiscoveryOptions {
        base_dir: dir.clone(),
        files: vec![PathBuf::from("app.ts"), PathBuf::from("lib.js")],
        files_explicitly_set: true,
        include: None,
        exclude: None,
        out_dir: None,
        follow_links: false,
        allow_js: false, // NOT set, but .js should still be included
        resolve_json_module: false,
    };

    let result = discover_ts_files(&options).unwrap();
    assert!(
        result.iter().any(|p| p.ends_with("app.ts")),
        "explicitly listed .ts file should be included"
    );
    assert!(
        result.iter().any(|p| p.ends_with("lib.js")),
        "explicitly listed .js file should be included even without allowJs"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_discover_explicit_files_preserves_list_order() {
    let dir = std::env::temp_dir().join("tsz_fs_test_explicit_order");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("b.js"), "let a = 10;").unwrap();
    fs::write(dir.join("a.ts"), "let b = 30;").unwrap();

    let options = FileDiscoveryOptions {
        base_dir: dir.clone(),
        files: vec![PathBuf::from("b.js"), PathBuf::from("a.ts")],
        files_explicitly_set: true,
        include: None,
        exclude: None,
        out_dir: None,
        follow_links: false,
        allow_js: false,
        resolve_json_module: false,
    };

    let result = discover_ts_files(&options).unwrap();
    let names: Vec<_> = result
        .iter()
        .map(|path| path.file_name().unwrap().to_string_lossy().to_string())
        .collect();
    assert_eq!(names, vec!["b.js", "a.ts"]);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_discover_wildcard_matched_ts_precedes_alphabetically_earlier_js() {
    // tsc's `matchFiles` (compiler/utilities.ts) buckets each discovered
    // file by the FIRST include pattern that matches it and flattens the
    // buckets in include-list order; it does not merge every match into
    // one alphabetically sorted list. Because `*.ts`-family patterns are
    // listed ahead of `*.js`-family ones (the exact list used by tsc's own
    // test harness, and by tsz's default discovery), every `.ts` file in
    // a project must sort ahead of every `.js` file, even when the `.js`
    // file's name is alphabetically earlier. This determines which
    // cross-file `var` declaration a mixed `.ts`/`.js` project treats as
    // primary for TS2403 declaration-merge checks.
    let dir = std::env::temp_dir().join("tsz_fs_test_ts_before_js_wildcard_order");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("a.js"), "var x = function(){};").unwrap();
    fs::write(dir.join("b.ts"), "var x = 1;").unwrap();

    let options = FileDiscoveryOptions {
        base_dir: dir.clone(),
        files: vec![],
        files_explicitly_set: false,
        include: Some(
            [
                "*.ts", "*.tsx", "*.js", "*.jsx", "**/*.ts", "**/*.tsx", "**/*.js", "**/*.jsx",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
        ),
        exclude: None,
        out_dir: None,
        follow_links: false,
        allow_js: true,
        resolve_json_module: false,
    };

    let result = discover_ts_files(&options).unwrap();
    let names: Vec<_> = result
        .iter()
        .map(|path| path.file_name().unwrap().to_string_lossy().to_string())
        .collect();
    assert_eq!(
        names,
        vec!["b.ts".to_string(), "a.js".to_string()],
        "a `.ts` file must sort ahead of an alphabetically-earlier `.js` file when \
         discovered through a multi-extension include list, matching tsc's per-pattern \
         bucketing instead of a global alphabetical merge"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Build a project under `dir`, discover it with a single recursive
/// include spec, and return the discovered paths relative to `dir`.
fn discover_relative(dir: &Path, files: &[&str], include: &[&str]) -> Vec<String> {
    let _ = fs::remove_dir_all(dir);
    for file in files {
        let path = dir.join(file);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "var dup = 1;").unwrap();
    }

    let options = FileDiscoveryOptions {
        base_dir: dir.to_path_buf(),
        files: vec![],
        files_explicitly_set: false,
        include: Some(include.iter().copied().map(String::from).collect()),
        exclude: None,
        out_dir: None,
        follow_links: false,
        allow_js: true,
        resolve_json_module: false,
    };

    let discovered = discover_ts_files(&options).unwrap();
    let canonical_dir = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    discovered
        .iter()
        .map(|path| {
            path.strip_prefix(&canonical_dir)
                .or_else(|_| path.strip_prefix(dir))
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect()
}

#[test]
fn test_discover_emits_a_directorys_own_files_before_its_subdirectories() {
    // tsc's `visitDirectory` emits the files of the directory it is
    // visiting before recursing into that directory's subdirectories, so a
    // root file whose name sorts *between* two subdirectory names still
    // comes first. Sorting whole paths lexicographically would instead
    // yield `aaa/x.ts, mmm.ts, zzz/y.ts`. Root order decides which
    // declaration a cross-file merge treats as primary, which is
    // observable as the anchor and reported types of TS2403.
    let dir = std::env::temp_dir().join("tsz_fs_test_walk_files_before_subdirs");
    let discovered = discover_relative(&dir, &["mmm.ts", "aaa/x.ts", "zzz/y.ts"], &["**/*.ts"]);

    assert_eq!(
        discovered,
        vec!["mmm.ts", "aaa/x.ts", "zzz/y.ts"],
        "a directory's own files must precede files from its subdirectories, even when a \
         subdirectory name sorts earlier than the file name"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_discover_orders_a_flat_directory_alphabetically() {
    // Fallback case: with no subdirectories the walk order and a plain
    // lexicographic sort agree, which is why every flat-project probe
    // missed the divergence above. This pins that the fix does not
    // perturb the far more common flat case.
    let dir = std::env::temp_dir().join("tsz_fs_test_walk_flat_alphabetical");
    let discovered = discover_relative(&dir, &["c.ts", "a.ts", "b.ts"], &["**/*.ts"]);

    assert_eq!(
        discovered,
        vec!["a.ts", "b.ts", "c.ts"],
        "a flat directory must stay in plain alphabetical order"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_discover_applies_files_before_subdirectories_at_every_depth() {
    // The rule is recursive, not just a root-level special case: within
    // `pkg`, `pkg/mmm.ts` precedes `pkg/aaa/deep.ts` for the same reason
    // `top.ts` precedes all of `pkg`.
    let dir = std::env::temp_dir().join("tsz_fs_test_walk_recursive_depth");
    let discovered = discover_relative(
        &dir,
        &["top.ts", "pkg/mmm.ts", "pkg/aaa/deep.ts", "pkg/zzz/deep.ts"],
        &["**/*.ts"],
    );

    assert_eq!(
        discovered,
        vec!["top.ts", "pkg/mmm.ts", "pkg/aaa/deep.ts", "pkg/zzz/deep.ts"],
        "files-before-subdirectories must hold at every level of the walk"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_discover_include_spec_order_outranks_walk_order() {
    // Walk order sequences files *within* one include spec; it never
    // reorders across specs. Here the second spec's file would win on walk
    // order alone (a root file before a subdirectory file), so this
    // separates the two layers instead of conflating them.
    let dir = std::env::temp_dir().join("tsz_fs_test_walk_under_spec_buckets");
    let discovered = discover_relative(&dir, &["root.ts", "sub/nested.ts"], &["sub/*", "*.ts"]);

    assert_eq!(
        discovered,
        vec!["sub/nested.ts", "root.ts"],
        "include-spec bucketing must still dominate: walk order only sequences files inside \
         a single spec's bucket"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_compare_discovery_order_ranks_files_ahead_of_sibling_subdirectories() {
    // Unit-level pinning of the comparator itself, independent of any
    // filesystem walk.
    assert_eq!(
        compare_discovery_order(Path::new("/p/mmm.ts"), Path::new("/p/aaa/x.ts")),
        Ordering::Less,
        "a file must precede a subdirectory whose name sorts earlier"
    );
    assert_eq!(
        compare_discovery_order(Path::new("/p/aaa/x.ts"), Path::new("/p/mmm.ts")),
        Ordering::Greater,
        "the comparator must be antisymmetric"
    );
    assert_eq!(
        compare_discovery_order(Path::new("/p/aaa/x.ts"), Path::new("/p/zzz/y.ts")),
        Ordering::Less,
        "sibling subdirectories compare by directory name"
    );
    assert_eq!(
        compare_discovery_order(Path::new("/p/a.ts"), Path::new("/p/b.ts")),
        Ordering::Less,
        "sibling files compare by file name"
    );
    assert_eq!(
        compare_discovery_order(Path::new("/p/a.ts"), Path::new("/p/a.ts")),
        Ordering::Equal,
        "a path equals itself"
    );
}

#[test]
fn test_discover_default_include_stays_alphabetical_across_extensions() {
    // Counterpart to `test_discover_wildcard_matched_ts_precedes_alphabetically_earlier_js`:
    // tsc's REAL zero-config default is the single pattern `**/*` (files
    // matched, then filtered by extension) — one spec, one bucket, so the
    // result is alphabetical, extension family notwithstanding. Bucketing
    // by the *expanded* per-extension pattern list (rather than by
    // originating spec) would put every `.ts` file ahead of every `.js`
    // file here too, which is exactly the regression #17423 landed and
    // #17428 reverted (see docs/specs/TSC_ROOT_FILE_ORDER.md). With no
    // explicit `include`, `default_include_patterns` still synthesizes a
    // multi-pattern per-extension list for discovery, but every pattern
    // in it must collapse to spec index 0.
    let dir = std::env::temp_dir().join("tsz_fs_test_default_include_alphabetical");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("a.js"), "var x = 1;").unwrap();
    fs::write(dir.join("b.ts"), "var x = \"s\";").unwrap();

    let options = FileDiscoveryOptions {
        base_dir: dir.clone(),
        files: vec![],
        files_explicitly_set: false,
        include: None,
        exclude: None,
        out_dir: None,
        follow_links: false,
        allow_js: true,
        resolve_json_module: false,
    };

    let result = discover_ts_files(&options).unwrap();
    let names: Vec<_> = result
        .iter()
        .map(|path| path.file_name().unwrap().to_string_lossy().to_string())
        .collect();
    assert_eq!(
        names,
        vec!["a.js".to_string(), "b.ts".to_string()],
        "with no explicit `include`, discovery must stay alphabetical across \
         extensions (tsc's zero-config default is the single pattern `**/*`, not a \
         per-extension bucket list)"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_discover_follow_links_preserves_symlink_ancestor_identity() {
    use std::os::unix::fs::symlink;

    let dir = std::env::temp_dir().join("tsz_fs_test_symlink_ancestor");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("core/node_modules/package-a")).unwrap();
    fs::write(
        dir.join("core/node_modules/package-a/index.d.ts"),
        "export interface Box {}",
    )
    .unwrap();
    symlink(
        dir.join("core/node_modules/package-a"),
        dir.join("package-a"),
    )
    .unwrap();

    let options = FileDiscoveryOptions {
        base_dir: dir.clone(),
        files: vec![],
        files_explicitly_set: false,
        include: None,
        exclude: None,
        out_dir: None,
        follow_links: true,
        allow_js: false,
        resolve_json_module: false,
    };

    let result = discover_ts_files(&options).unwrap();
    assert!(
        result.iter().any(|p| p.ends_with("package-a/index.d.ts")),
        "symlinked package root should stay in its original path"
    );
    assert!(
        !result.iter().any(|p| p
            .to_string_lossy()
            .contains("core/node_modules/package-a/index.d.ts")),
        "canonical target path should not replace the symlink path"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_discover_pattern_matched_js_file_requires_allow_js() {
    // Pattern-matched .js files (from include/exclude) should NOT be included
    // when allow_js is false. This is the correct tsc behavior.
    let dir = std::env::temp_dir().join("tsz_fs_test_pattern_js");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("src/app.ts"), "const x = 1;").unwrap();
    fs::write(dir.join("src/lib.js"), "var y = 2;").unwrap();

    // Without allowJs, pattern-matched .js files are excluded
    let options = FileDiscoveryOptions {
        base_dir: dir.clone(),
        files: vec![],
        files_explicitly_set: false,
        include: Some(vec!["src".to_string()]),
        exclude: None,
        out_dir: None,
        follow_links: false,
        allow_js: false,
        resolve_json_module: false,
    };

    let result = discover_ts_files(&options).unwrap();
    assert!(
        result.iter().any(|p| p.ends_with("app.ts")),
        ".ts file should be included from pattern"
    );
    assert!(
        !result.iter().any(|p| p.ends_with("lib.js")),
        ".js file should NOT be included from pattern without allowJs"
    );

    // With allowJs, pattern-matched .js files are included
    let options_with_js = FileDiscoveryOptions {
        base_dir: dir.clone(),
        files: vec![],
        files_explicitly_set: false,
        include: Some(vec!["src".to_string()]),
        exclude: None,
        out_dir: None,
        follow_links: false,
        allow_js: true,
        resolve_json_module: false,
    };

    let result_with_js = discover_ts_files(&options_with_js).unwrap();
    assert!(
        result_with_js.iter().any(|p| p.ends_with("lib.js")),
        ".js file should be included from pattern with allowJs"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_discover_absolute_include_walks_pattern_prefix() {
    let dir = std::env::temp_dir().join("tsz_fs_test_absolute_include");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("base/src")).unwrap();
    fs::create_dir_all(dir.join("app")).unwrap();
    fs::write(dir.join("base/src/a.ts"), "export const x = 1;").unwrap();

    let options = FileDiscoveryOptions {
        base_dir: dir.join("app"),
        files: vec![],
        files_explicitly_set: false,
        include: Some(vec![
            dir.join("base/src/**/*.ts").to_string_lossy().into_owned(),
        ]),
        exclude: None,
        out_dir: None,
        follow_links: false,
        allow_js: false,
        resolve_json_module: false,
    };

    let result = discover_ts_files(&options).unwrap();
    assert_eq!(result, vec![dir.join("base/src/a.ts")]);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_discover_pattern_matched_json_file_is_not_a_root() {
    let dir = std::env::temp_dir().join("tsz_fs_test_pattern_json");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("src/app.ts"), "const x = 1;").unwrap();
    fs::write(dir.join("src/data.json"), "{ \"a\": 1 }").unwrap();

    let options = FileDiscoveryOptions {
        base_dir: dir.clone(),
        files: vec![],
        files_explicitly_set: false,
        include: Some(vec!["src".to_string()]),
        exclude: None,
        out_dir: None,
        follow_links: false,
        allow_js: false,
        resolve_json_module: false,
    };

    let result = discover_ts_files(&options).unwrap();
    assert!(
        !result.iter().any(|p| p.ends_with("data.json")),
        ".json file should not be included from patterns"
    );

    let options_with_json = FileDiscoveryOptions {
        resolve_json_module: true,
        ..options
    };
    let result_with_json = discover_ts_files(&options_with_json).unwrap();
    assert!(
        !result_with_json.iter().any(|p| p.ends_with("data.json")),
        "resolveJsonModule should not make pattern-matched JSON files roots"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_discover_excludes_json_from_default_include_even_with_resolve_json_module() {
    let dir = std::env::temp_dir().join("tsz_fs_test_config_json_excluded");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("tsconfig.json"), r#"{ "compilerOptions": {} }"#).unwrap();
    fs::write(dir.join("jsconfig.json"), r#"{ "compilerOptions": {} }"#).unwrap();
    fs::write(dir.join("data.json"), r#"{ "key": "value" }"#).unwrap();
    fs::write(dir.join("app.ts"), "const x = 1;").unwrap();

    let options = FileDiscoveryOptions {
        base_dir: dir.clone(),
        files: vec![],
        files_explicitly_set: false,
        include: None, // defaults to **/*
        exclude: None,
        out_dir: None,
        follow_links: false,
        allow_js: false,
        resolve_json_module: true,
    };

    let result = discover_ts_files(&options).unwrap();
    assert!(
        result.iter().any(|p| p.ends_with("app.ts")),
        "should discover .ts files"
    );
    assert!(!result.iter().any(|p| p.ends_with("data.json")));
    assert!(
        !result.iter().any(|p| p.ends_with("tsconfig.json")),
        "tsconfig.json must not be included as program input"
    );
    assert!(
        !result.iter().any(|p| p.ends_with("jsconfig.json")),
        "jsconfig.json must not be included as program input"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_discover_excludes_json_for_explicit_json_include() {
    let dir = std::env::temp_dir().join("tsz_fs_test_explicit_config_json_excluded");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("tsconfig.json"), r#"{ "compilerOptions": {} }"#).unwrap();
    fs::write(dir.join("jsconfig.json"), r#"{ "compilerOptions": {} }"#).unwrap();
    fs::write(dir.join("data.json"), r#"{ "key": "value" }"#).unwrap();

    let options = FileDiscoveryOptions {
        base_dir: dir.clone(),
        files: vec![],
        files_explicitly_set: false,
        include: Some(vec!["*.json".to_string()]),
        exclude: None,
        out_dir: None,
        follow_links: false,
        allow_js: false,
        resolve_json_module: true,
    };

    let result = discover_ts_files(&options).unwrap();
    assert!(
        !result.iter().any(|p| p.ends_with("data.json")),
        "explicit JSON include should not make JSON files roots"
    );
    assert!(
        !result.iter().any(|p| p.ends_with("tsconfig.json")),
        "tsconfig.json must not be included as program input"
    );
    assert!(
        !result.iter().any(|p| p.ends_with("jsconfig.json")),
        "jsconfig.json must not be included as program input"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_discover_treats_d_tsx_as_tsx_source_not_shadowed_declaration() {
    let dir = std::env::temp_dir().join("tsz_fs_test_d_tsx_source");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("index.tsx"), "export const x = <div />;").unwrap();
    fs::write(dir.join("index.d.tsx"), "export const y = <div />;").unwrap();

    let options = FileDiscoveryOptions {
        base_dir: dir.clone(),
        files: vec![],
        files_explicitly_set: false,
        include: None,
        exclude: None,
        out_dir: None,
        follow_links: false,
        allow_js: false,
        resolve_json_module: false,
    };

    let result = discover_ts_files(&options).unwrap();
    assert!(
        result.iter().any(|p| p.ends_with("index.tsx")),
        "regular .tsx source should be discovered"
    );
    assert!(
        result.iter().any(|p| p.ends_with("index.d.tsx")),
        ".d.tsx should be discovered as a .tsx source, not dropped as a declaration"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_default_discovery_includes_mts_cts_and_module_js_variants() {
    // Distinct stems so none of these shadow each other (a same-stem
    // `.mts`/`.mjs` or `.cts`/`.cjs` pair is covered by the dedicated
    // `exclude_shadowed_js_files` shadowing tests below).
    let dir = std::env::temp_dir().join("tsz_fs_test_default_include_extensions");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("mfile.mts"), "export const x = 1;").unwrap();
    fs::write(dir.join("cfile.cts"), "export = 1;").unwrap();
    fs::write(dir.join("mjsfile.mjs"), "export const x = 1;").unwrap();
    fs::write(dir.join("cjsfile.cjs"), "module.exports = 1;").unwrap();

    // With allow_js: true, all module extensions should be discovered
    let options = FileDiscoveryOptions {
        base_dir: dir.clone(),
        files: vec![],
        files_explicitly_set: false,
        include: None,
        exclude: None,
        out_dir: None,
        follow_links: false,
        allow_js: true,
        resolve_json_module: false,
    };

    let result = discover_ts_files(&options).unwrap();
    assert_eq!(
        result.len(),
        4,
        "default include discovery should find .mts/.cts/.mjs/.cjs files, got: {result:?}"
    );

    // Without allow_js, only .mts/.cts should be found (not .mjs/.cjs)
    let options_no_js = FileDiscoveryOptions {
        base_dir: dir.clone(),
        files: vec![],
        files_explicitly_set: false,
        include: None,
        exclude: None,
        out_dir: None,
        follow_links: false,
        allow_js: false,
        resolve_json_module: false,
    };

    let result_no_js = discover_ts_files(&options_no_js).unwrap();
    assert_eq!(
        result_no_js.len(),
        2,
        "default include without allowJs should find .mts/.cts but not .mjs/.cjs, got: {result_no_js:?}"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Oracle-verified against pinned `tsc` 7.0.2: a wildcard-discovered
/// `.js`-family file is dropped from the program when a higher-priority
/// same-stem source file is also discovered, so a same-named `a.ts` +
/// `a.js` pair resolves to a single module (`a.ts`), matching how tsc's
/// own project-mode file discovery treats it. Regression coverage for
/// the `salsa/inferingFromAny.ts` conformance false-positive: the
/// conformance harness compiles multi-`@fileName` fixtures via a
/// synthetic tsconfig `include` glob, so this same shadowing must apply
/// there too, not just to hand-authored real projects.
fn discover_names(dir: &Path, allow_js: bool) -> Vec<String> {
    let options = FileDiscoveryOptions {
        base_dir: dir.to_path_buf(),
        files: vec![],
        files_explicitly_set: false,
        include: None,
        exclude: None,
        out_dir: None,
        follow_links: false,
        allow_js,
        resolve_json_module: false,
    };
    let mut names: Vec<String> = discover_ts_files(&options)
        .unwrap()
        .into_iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
        .collect();
    names.sort();
    names
}

#[test]
fn test_discover_ts_shadows_same_stem_js_wildcard_match() {
    let dir = std::env::temp_dir().join("tsz_fs_test_ts_shadows_js");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("a.ts"), "export const x = 1;").unwrap();
    fs::write(dir.join("a.js"), "module.exports.x = 1;").unwrap();

    assert_eq!(
        discover_names(&dir, true),
        vec!["a.ts".to_string()],
        "a same-stem wildcard-matched .js should be shadowed by .ts"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_discover_tsx_shadows_same_stem_jsx_cross_extension() {
    // Renamed-binder / cross-extension adjacent case: .tsx shadows a
    // same-stem .jsx even though the pair isn't the "matching" tsx/jsx
    // pair by naming convention — tsc's rule is family-wide priority,
    // not paired-extension matching (oracle-verified).
    let dir = std::env::temp_dir().join("tsz_fs_test_tsx_shadows_jsx");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("widget.tsx"), "export const x = 1;").unwrap();
    fs::write(dir.join("widget.jsx"), "module.exports.x = 1;").unwrap();

    assert_eq!(
        discover_names(&dir, true),
        vec!["widget.tsx".to_string()],
        ".tsx should shadow a same-stem .jsx"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_discover_js_shadows_same_stem_jsx_without_ts() {
    // Within the js-only tier, .js outranks .jsx for the same stem even
    // when no ts-family file is present at all (oracle-verified).
    let dir = std::env::temp_dir().join("tsz_fs_test_js_shadows_jsx");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("widget.js"), "module.exports.x = 1;").unwrap();
    fs::write(dir.join("widget.jsx"), "module.exports.x = 1;").unwrap();

    assert_eq!(
        discover_names(&dir, true),
        vec!["widget.js".to_string()],
        ".js should shadow a same-stem .jsx even without a ts sibling"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_discover_mts_shadows_only_same_stem_mjs_not_cross_family() {
    // .mts/.mjs is an independent family from .ts/.js: a same-stem .ts
    // does NOT shadow .mjs, and .mts does NOT shadow a same-stem .js
    // (oracle-verified — cross-family pairs coexist).
    let dir = std::env::temp_dir().join("tsz_fs_test_mts_family_independent");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("a.mts"), "export const x = 1;").unwrap();
    fs::write(dir.join("a.mjs"), "module.exports.x = 1;").unwrap();
    fs::write(dir.join("b.ts"), "export const y = 1;").unwrap();
    fs::write(dir.join("b.mjs"), "module.exports.y = 1;").unwrap();
    fs::write(dir.join("c.mts"), "export const z = 1;").unwrap();
    fs::write(dir.join("c.js"), "module.exports.z = 1;").unwrap();

    assert_eq!(
        discover_names(&dir, true),
        vec![
            "a.mts".to_string(),
            "b.mjs".to_string(),
            "b.ts".to_string(),
            "c.js".to_string(),
            "c.mts".to_string(),
        ],
        "only same-stem .mts/.mjs pairs shadow; cross-family pairs coexist"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_discover_cts_shadows_same_stem_cjs() {
    let dir = std::env::temp_dir().join("tsz_fs_test_cts_shadows_cjs");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("a.cts"), "export = 1;").unwrap();
    fs::write(dir.join("a.cjs"), "module.exports = 1;").unwrap();

    assert_eq!(
        discover_names(&dir, true),
        vec!["a.cts".to_string()],
        "a same-stem .cjs should be shadowed by .cts"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_discover_explicit_js_file_not_shadowed_by_same_stem_ts() {
    // Explicitly listed files (CLI positional args / tsconfig `files`)
    // are never shadowed, even when a same-stem higher-priority file
    // also exists (oracle-verified: `tsc --project` with an explicit
    // `files: ["a.ts", "a.js"]` keeps both).
    let dir = std::env::temp_dir().join("tsz_fs_test_explicit_js_not_shadowed");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("a.ts"), "export const x = 1;").unwrap();
    fs::write(dir.join("a.js"), "module.exports.x = 1;").unwrap();

    let options = FileDiscoveryOptions {
        base_dir: dir.clone(),
        files: vec![PathBuf::from("a.ts"), PathBuf::from("a.js")],
        files_explicitly_set: true,
        include: None,
        exclude: None,
        out_dir: None,
        follow_links: false,
        allow_js: false,
        resolve_json_module: false,
    };

    let mut names: Vec<String> = discover_ts_files(&options)
        .unwrap()
        .into_iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
        .collect();
    names.sort();
    assert_eq!(
        names,
        vec!["a.js".to_string(), "a.ts".to_string()],
        "explicitly listed files must not be shadowed"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_discover_ts_shadows_js_with_renamed_binder() {
    // Same rule, different stem name — proves this is structural
    // (extension-priority), not keyed off a specific file/identifier name.
    let dir = std::env::temp_dir().join("tsz_fs_test_ts_shadows_js_renamed");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("zorbaflux.ts"), "export const q = 1;").unwrap();
    fs::write(dir.join("zorbaflux.js"), "module.exports.q = 1;").unwrap();

    assert_eq!(
        discover_names(&dir, true),
        vec!["zorbaflux.ts".to_string()],
        "shadowing must not depend on the specific stem name"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_explicit_include_without_mts_excludes_mts_root() {
    let dir = std::env::temp_dir().join("tsz_fs_test_explicit_default_include_mts");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("index.mts"), "export const x = 1;").unwrap();

    // Explicit include patterns that do NOT include .mts should not discover .mts files
    let options = FileDiscoveryOptions {
        base_dir: dir.clone(),
        files: vec![],
        files_explicitly_set: false,
        include: Some(vec![
            "*.ts".to_string(),
            "*.tsx".to_string(),
            "*.js".to_string(),
            "*.jsx".to_string(),
            "**/*.ts".to_string(),
            "**/*.tsx".to_string(),
            "**/*.js".to_string(),
            "**/*.jsx".to_string(),
        ]),
        exclude: Some(vec!["node_modules".to_string()]),
        out_dir: None,
        follow_links: false,
        allow_js: true,
        resolve_json_module: false,
    };

    let result = discover_ts_files(&options).unwrap();
    assert!(
        result.is_empty(),
        "explicit include without .mts patterns should ignore .mts files, got: {result:?}"
    );

    let _ = fs::remove_dir_all(&dir);
}
