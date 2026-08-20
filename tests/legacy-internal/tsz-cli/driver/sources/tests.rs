use super::*;
use std::io::Write;
use tempfile::tempdir;

// ---------------- node_modules_depth ----------------

#[test]
fn node_modules_depth_zero_for_paths_without_segment() {
    assert_eq!(node_modules_depth(Path::new("/a/b/c.ts")), 0);
    assert_eq!(node_modules_depth(Path::new("relative/file.js")), 0);
    assert_eq!(node_modules_depth(Path::new("")), 0);
}

#[test]
fn node_modules_depth_counts_each_segment_independently() {
    assert_eq!(
        node_modules_depth(Path::new("/proj/node_modules/foo/index.js")),
        1
    );
    assert_eq!(
        node_modules_depth(Path::new(
            "/proj/node_modules/foo/node_modules/bar/index.js"
        )),
        2
    );
    assert_eq!(
        node_modules_depth(Path::new(
            "/a/node_modules/b/node_modules/c/node_modules/d/x.js"
        )),
        3
    );
}

#[test]
fn node_modules_depth_does_not_match_substring_segments() {
    // A directory whose name merely contains "node_modules" must not count.
    assert_eq!(
        node_modules_depth(Path::new("/proj/my_node_modules_clone/x.js")),
        0
    );
    assert_eq!(
        node_modules_depth(Path::new("/proj/node_modules_extra/x.js")),
        0
    );
}

// ---------------- has_source_file_extension ----------------

#[test]
fn has_source_file_extension_accepts_ts_family() {
    for path in [
        "a.ts", "a.tsx", "a.mts", "a.cts", "a.d.ts", "a.d.mts", "a.d.cts",
    ] {
        assert!(
            has_source_file_extension(Path::new(path)),
            "expected ts-family path to be accepted: {path}"
        );
    }
}

#[test]
fn has_source_file_extension_accepts_js_family() {
    for path in ["a.js", "a.jsx", "a.mjs", "a.cjs"] {
        assert!(
            has_source_file_extension(Path::new(path)),
            "expected js-family path to be accepted: {path}"
        );
    }
}

#[test]
fn has_source_file_extension_accepts_json() {
    assert!(has_source_file_extension(Path::new("pkg/data.json")));
}

#[test]
fn has_source_file_extension_rejects_unrelated_extensions() {
    for path in ["a.css", "a.html", "a.md", "a.wasm", "a.json5", "a.node"] {
        assert!(
            !has_source_file_extension(Path::new(path)),
            "expected non-source path to be rejected: {path}"
        );
    }
}

#[test]
fn has_source_file_extension_rejects_no_extension_or_empty() {
    assert!(!has_source_file_extension(Path::new("README")));
    assert!(!has_source_file_extension(Path::new("")));
}

#[test]
fn collect_type_root_files_wildcard_skips_parent_default_roots() {
    let dir = tempdir().unwrap();
    let app_dir = dir.path().join("src");
    let package = dir.path().join("node_modules/@types/foo");
    std::fs::create_dir_all(&package).unwrap();
    std::fs::create_dir_all(&app_dir).unwrap();
    std::fs::write(package.join("index.d.ts"), "declare module \"xyz\" {}\n").unwrap();

    let options = ResolvedCompilerOptions {
        types: Some(vec!["*".to_string()]),
        ..Default::default()
    };
    let (files, unresolved) = collect_type_root_files(&app_dir, &options);

    assert!(
        files.is_empty(),
        "wildcard should not load parent roots: {files:?}"
    );
    assert!(unresolved.is_empty());
}

#[test]
fn collect_type_root_files_explicit_types_use_parent_default_roots() {
    let dir = tempdir().unwrap();
    let app_dir = dir.path().join("src");
    let package = dir.path().join("node_modules/@types/foo");
    std::fs::create_dir_all(&package).unwrap();
    std::fs::create_dir_all(&app_dir).unwrap();
    let entry = package.join("index.d.ts");
    std::fs::write(&entry, "declare module \"xyz\" {}\n").unwrap();

    let options = ResolvedCompilerOptions {
        types: Some(vec!["foo".to_string()]),
        ..Default::default()
    };
    let (files, unresolved) = collect_type_root_files(&app_dir, &options);

    assert_eq!(unresolved, Vec::<String>::new());
    assert_eq!(files, vec![canonicalize_or_owned(&entry)]);
}

#[test]
fn read_source_files_preserves_reference_discovery_order() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("src/root.ts");
    let foo = dir.path().join("node_modules/foo/index.d.ts");
    let foo_alpha = dir
        .path()
        .join("node_modules/foo/node_modules/alpha/index.d.ts");
    let bar = dir.path().join("node_modules/bar/index.d.ts");
    let bar_alpha = dir
        .path()
        .join("node_modules/bar/node_modules/alpha/index.d.ts");

    for path in [&root, &foo, &foo_alpha, &bar, &bar_alpha] {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    }
    std::fs::write(
        &root,
        "/// <reference types=\"foo\" />\n/// <reference types=\"bar\" />\n",
    )
    .unwrap();
    std::fs::write(&foo, "/// <reference types=\"alpha\" />\n").unwrap();
    std::fs::write(&foo_alpha, "declare var alpha: any;\n").unwrap();
    std::fs::write(&bar, "/// <reference types=\"alpha\" />\n").unwrap();
    std::fs::write(&bar_alpha, "declare var alpha: {};\n").unwrap();

    let result = read_source_files(
        &[root],
        dir.path(),
        &ResolvedCompilerOptions::default(),
        None,
        None,
    )
    .expect("read source files");
    let paths: Vec<_> = result.sources.iter().map(|source| &source.path).collect();

    let foo_alpha_pos = paths
        .iter()
        .position(|path| path.ends_with("node_modules/foo/node_modules/alpha/index.d.ts"))
        .expect("foo alpha loaded");
    let bar_alpha_pos = paths
        .iter()
        .position(|path| path.ends_with("node_modules/bar/node_modules/alpha/index.d.ts"))
        .expect("bar alpha loaded");
    assert!(
        foo_alpha_pos < bar_alpha_pos,
        "reference discovery order should load foo's alpha before bar's alpha; got {paths:?}"
    );
}

/// Read `root` (plus its on-disk deps) from `dir` and return the recorded
/// dependency file names for `root.ts`, in stored order.
fn root_dep_file_names(dir: &Path, root: &Path) -> Vec<String> {
    let result = read_source_files(
        std::slice::from_ref(&root.to_path_buf()),
        dir,
        &ResolvedCompilerOptions::default(),
        None,
        None,
    )
    .expect("read source files");
    let canonical_root = result
        .sources
        .iter()
        .find(|source| source.path.ends_with("root.ts"))
        .expect("root.ts loaded")
        .path
        .clone();
    result
        .dependencies
        .get(&canonical_root)
        .expect("root dependencies recorded")
        .iter()
        .map(|dep| dep.file_name().unwrap().to_string_lossy().into_owned())
        .collect()
}

#[test]
fn read_source_files_records_dependencies_in_source_import_order() {
    // The per-file dependency list seeds BFS discovery on cached rebuilds,
    // and discovery order fixes global `SymbolId` assignment. Storing deps
    // in source-import order (rather than a hashed set) is what keeps a
    // cached rebuild's `SymbolId`s identical to the original fresh build.
    let dir = tempdir().unwrap();
    let root = dir.path().join("src/root.ts");
    // Import order deliberately not alphabetical so a hashed set would be
    // very likely to reorder these entries.
    let dep_names = ["zeta", "alpha", "mid", "beta", "gamma"];
    std::fs::create_dir_all(root.parent().unwrap()).unwrap();
    let imports: String = dep_names
        .iter()
        .map(|name| format!("import './{name}';\n"))
        .collect();
    std::fs::write(&root, imports).unwrap();
    for name in dep_names {
        let dep = dir.path().join(format!("src/{name}.ts"));
        std::fs::write(&dep, "export {};\n").unwrap();
    }

    let expected: Vec<String> = dep_names.iter().map(|n| format!("{n}.ts")).collect();
    assert_eq!(
        root_dep_file_names(dir.path(), &root),
        expected,
        "dependencies must be recorded in source-import order"
    );
}

#[test]
fn read_source_files_dedups_repeated_imports_preserving_first_order() {
    // Importing the same module twice must not duplicate it in the dep list,
    // and the first occurrence's position must be preserved.
    let dir = tempdir().unwrap();
    let root = dir.path().join("src/root.ts");
    std::fs::create_dir_all(root.parent().unwrap()).unwrap();
    std::fs::write(
            &root,
            "import { a } from './first';\nimport { b } from './second';\nimport { c } from './first';\na;b;c;\n",
        )
        .unwrap();
    std::fs::write(
        dir.path().join("src/first.ts"),
        "export const a = 1; export const c = 2;\n",
    )
    .unwrap();
    std::fs::write(dir.path().join("src/second.ts"), "export const b = 1;\n").unwrap();

    assert_eq!(
        root_dep_file_names(dir.path(), &root),
        vec!["first.ts".to_string(), "second.ts".to_string()],
        "repeated imports must be deduped while preserving first-seen order"
    );
}

#[test]
fn read_source_files_records_successful_module_resolutions() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("src/main.ts");
    let dep = dir.path().join("src/dep.ts");
    std::fs::create_dir_all(root.parent().unwrap()).unwrap();
    std::fs::write(&root, "import { value } from './dep';\nvalue;\n").unwrap();
    std::fs::write(&dep, "export const value = 1;\n").unwrap();

    let result = read_source_files(
        &[root],
        dir.path(),
        &ResolvedCompilerOptions::default(),
        None,
        None,
    )
    .expect("read source files");
    let containing_file = result
        .sources
        .iter()
        .find(|source| source.path.ends_with("main.ts"))
        .expect("main.ts loaded")
        .path
        .clone();
    let key = SourceModuleResolutionKey {
        containing_file,
        specifier: "./dep".to_string(),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
    };
    let resolved = result
        .module_resolutions
        .get(&key)
        .expect("successful import resolution recorded");
    assert!(resolved.canonical_path.ends_with("src/dep.ts"));
    assert!(!resolved.resolved_using_ts_extension);
}

#[test]
fn read_source_files_reads_body_of_explicit_root_js_under_node_modules() {
    // A `node_modules`-hosted JS file that is itself an explicit program
    // root (tsc's `rootNames`) is a real program input, not something
    // reached "by descending into node_modules" — `maxNodeModuleJsDepth`
    // (default 0) must not starve its body. See #16928: a root skipped
    // here keeps a registered `SourceFile` with a permanently empty
    // statement list, breaking every downstream CJS `module.exports`
    // surface computation for it.
    let dir = tempdir().unwrap();
    let root = dir.path().join("node_modules/untyped/index.js");
    std::fs::create_dir_all(root.parent().unwrap()).unwrap();
    std::fs::write(&root, "module.exports = { hello: function() {} };\n").unwrap();

    let result = read_source_files(
        std::slice::from_ref(&root),
        dir.path(),
        &ResolvedCompilerOptions::default(),
        None,
        None,
    )
    .expect("read source files");

    let entry = result
        .sources
        .iter()
        .find(|source| source.path.ends_with("node_modules/untyped/index.js"))
        .expect("root js file registered");
    assert!(
        entry.text.is_some(),
        "explicit-root JS file under node_modules must have its body read"
    );
}

#[test]
fn read_source_files_skips_body_of_non_root_js_under_node_modules() {
    // The counterpart to the explicit-root case above: a `node_modules`
    // JS file reached only via `require()`/import (not a program root)
    // stays gated by `maxNodeModuleJsDepth` (default 0) exactly as
    // before this fix — only root status bypasses the gate.
    let dir = tempdir().unwrap();
    let root = dir.path().join("src/main.ts");
    let dep = dir.path().join("node_modules/untyped/index.js");
    std::fs::create_dir_all(root.parent().unwrap()).unwrap();
    std::fs::create_dir_all(dep.parent().unwrap()).unwrap();
    std::fs::write(
        &root,
        "import u = require(\"../node_modules/untyped/index.js\");\nu;\n",
    )
    .unwrap();
    std::fs::write(&dep, "module.exports = { hello: function() {} };\n").unwrap();

    let options = ResolvedCompilerOptions {
        allow_js: true,
        checker: tsz::checker::context::CheckerOptions {
            allow_js: true,
            ..Default::default()
        },
        ..Default::default()
    };

    let result =
        read_source_files(&[root], dir.path(), &options, None, None).expect("read source files");

    let paths: Vec<_> = result.sources.iter().map(|s| s.path.clone()).collect();
    let entry = result
        .sources
        .iter()
        .find(|source| source.path.ends_with("node_modules/untyped/index.js"))
        .unwrap_or_else(|| {
            panic!("non-root js file still registered (skipped, not absent); got: {paths:?}")
        });
    assert!(
        entry.text.is_none(),
        "non-root JS file beyond maxNodeModuleJsDepth must stay skipped"
    );
    assert!(
        result.depth_skipped_js_paths.contains(&entry.path),
        "depth-skipped stub must be recorded so downstream CJS export \
         inference does not treat it as a normal resolved target (#16934)"
    );
}

#[test]
fn read_source_files_root_js_under_node_modules_is_not_depth_skipped() {
    // The explicit-root counterpart: a root JS file under `node_modules`
    // (see `read_source_files_reads_body_of_explicit_root_js_under_node_modules`
    // above) has its body read and must never land in
    // `depth_skipped_js_paths`, even though its path shape would otherwise
    // match `should_skip_js_in_node_modules`.
    let dir = tempdir().unwrap();
    let root_js = dir.path().join("node_modules/untyped/index.js");
    std::fs::create_dir_all(root_js.parent().unwrap()).unwrap();
    std::fs::write(&root_js, "module.exports = { hello: function() {} };\n").unwrap();

    let options = ResolvedCompilerOptions {
        allow_js: true,
        checker: tsz::checker::context::CheckerOptions {
            allow_js: true,
            ..Default::default()
        },
        ..Default::default()
    };

    let result = read_source_files(
        std::slice::from_ref(&root_js),
        dir.path(),
        &options,
        None,
        None,
    )
    .expect("read source files");

    assert!(
        !result.depth_skipped_js_paths.contains(&root_js),
        "an explicit program root must never be treated as depth-skipped"
    );
}

#[test]
fn read_source_files_does_not_record_failed_module_resolutions() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("src/main.ts");
    std::fs::create_dir_all(root.parent().unwrap()).unwrap();
    std::fs::write(&root, "import './missing';\n").unwrap();

    let result = read_source_files(
        &[root],
        dir.path(),
        &ResolvedCompilerOptions::default(),
        None,
        None,
    )
    .expect("read source files");

    assert!(
        result.module_resolutions.is_empty(),
        "failed resolution must fall back to diagnostic lookup"
    );
    let containing_file = result
        .sources
        .iter()
        .find(|source| source.path.ends_with("main.ts"))
        .expect("main.ts loaded")
        .path
        .clone();
    assert!(
        result
            .module_resolution_misses
            .contains(&SourceModuleResolutionKey {
                containing_file,
                specifier: "./missing".to_string(),
                import_kind: ImportKind::EsmImport,
                resolution_mode_override: None,
            })
    );
}

// ---------------- should_skip_js_in_node_modules ----------------

#[test]
fn should_skip_js_in_node_modules_false_for_ts_files() {
    // TS files are never skipped by this gate, regardless of depth.
    assert!(!should_skip_js_in_node_modules(
        Path::new("/p/node_modules/foo/index.ts"),
        0
    ));
    assert!(!should_skip_js_in_node_modules(
        Path::new("/p/node_modules/foo/node_modules/bar/x.tsx"),
        0
    ));
}

#[test]
fn should_skip_js_in_node_modules_false_when_depth_zero() {
    // JS file outside node_modules has depth 0; never skipped.
    assert!(!should_skip_js_in_node_modules(
        Path::new("/proj/src/index.js"),
        0
    ));
    assert!(!should_skip_js_in_node_modules(
        Path::new("/proj/src/index.js"),
        5
    ));
}

#[test]
fn should_skip_js_in_node_modules_threshold_boundary() {
    // depth=1 with max_depth=0 -> skip (1 > 0)
    assert!(should_skip_js_in_node_modules(
        Path::new("/p/node_modules/foo/index.js"),
        0
    ));
    // depth=1 with max_depth=1 -> keep (1 > 1 is false)
    assert!(!should_skip_js_in_node_modules(
        Path::new("/p/node_modules/foo/index.js"),
        1
    ));
    // depth=2 with max_depth=1 -> skip
    assert!(should_skip_js_in_node_modules(
        Path::new("/p/node_modules/foo/node_modules/bar/index.js"),
        1
    ));
    // depth=2 with max_depth=2 -> keep
    assert!(!should_skip_js_in_node_modules(
        Path::new("/p/node_modules/foo/node_modules/bar/index.js"),
        2
    ));
}

#[test]
fn should_skip_js_in_node_modules_jsx_mjs_cjs_branches() {
    for ext in ["js", "jsx", "mjs", "cjs"] {
        let path_str = format!("/p/node_modules/foo/index.{ext}");
        assert!(
            should_skip_js_in_node_modules(Path::new(&path_str), 0),
            "expected js-family `{ext}` inside node_modules to be skipped at max=0"
        );
    }
}

// ---------------- classify_binary_file ----------------

#[test]
fn classify_binary_file_empty_returns_none() {
    assert_eq!(classify_binary_file(b""), None);
}

#[test]
fn classify_binary_file_plain_utf8_returns_none() {
    let text = b"export const x: number = 1;\n// hello\n";
    assert_eq!(classify_binary_file(text), None);
}

#[test]
fn classify_binary_file_many_nulls_returns_some_true() {
    // 11 null bytes scattered in the first 1024 bytes -> binary, suppress.
    let mut bytes = vec![b'a'; 1024];
    for slot in bytes.iter_mut().take(11) {
        *slot = 0;
    }
    assert_eq!(classify_binary_file(&bytes), Some(true));
}

#[test]
fn classify_binary_file_consecutive_nulls_returns_some_true() {
    // 4 consecutive nulls inside the first 512 bytes -> binary.
    // Keep total nulls <= 10 so the many-null branch does not fire first.
    let mut bytes = vec![b'a'; 64];
    bytes[10] = 0;
    bytes[11] = 0;
    bytes[12] = 0;
    bytes[13] = 0;
    assert_eq!(classify_binary_file(&bytes), Some(true));
}

#[test]
fn classify_binary_file_three_consecutive_nulls_not_enough() {
    // 3 consecutive nulls (total nulls = 3) -> not enough, returns None.
    let mut bytes = vec![b'a'; 64];
    bytes[10] = 0;
    bytes[11] = 0;
    bytes[12] = 0;
    assert_eq!(classify_binary_file(&bytes), None);
}

#[test]
fn classify_binary_file_control_bytes_route_through_soft_check() {
    // 4 stray control bytes (non-whitespace, < 0x20) trigger the "control"
    // branch which delegates to soft_control_binary_should_suppress.
    // With no printable trailing payload, suppression should be true.
    let bytes: Vec<u8> = vec![0x01, 0x02, 0x03, 0x04];
    assert_eq!(classify_binary_file(&bytes), Some(true));
}

#[test]
fn classify_binary_file_whitespace_controls_do_not_count() {
    // tab/newline/CR/FF/VT are excluded from the control-byte tally.
    let bytes: Vec<u8> = vec![b'\t', b'\n', b'\r', 0x0C, 0x0B, b'a', b'b'];
    assert_eq!(classify_binary_file(&bytes), None);
}

#[test]
fn classify_binary_file_three_control_bytes_not_enough() {
    // Only 3 control bytes; control-bytes branch needs >= 4. Returns None.
    let bytes: Vec<u8> = vec![0x01, 0x02, 0x03, b'a', b'b', b'c'];
    assert_eq!(classify_binary_file(&bytes), None);
}

// ---------------- soft_control_binary_should_suppress ----------------

#[test]
fn soft_control_binary_suppresses_when_payload_is_short() {
    // No newline at all -> entire input is the payload. Only one printable
    // ASCII byte ('a') -> suppress.
    let bytes: Vec<u8> = vec![0x01, 0x02, b'a'];
    assert!(soft_control_binary_should_suppress(&bytes));
}

#[test]
fn soft_control_binary_keeps_diagnostics_when_payload_has_text() {
    // Payload "abc" has 3 printable ASCII bytes -> do not suppress.
    let bytes: Vec<u8> = vec![0x01, 0x02, b'a', b'b', b'c'];
    assert!(!soft_control_binary_should_suppress(&bytes));
}

#[test]
fn soft_control_binary_uses_payload_after_last_newline() {
    // Last newline at index 5; payload after it = b"hi" (2 printable) ->
    // not suppressed (printable_ascii_count is 2, condition is `< 2`).
    let bytes: Vec<u8> = vec![b'a', b'b', b'c', b'd', b'e', b'\n', b'h', b'i'];
    assert!(!soft_control_binary_should_suppress(&bytes));
}

#[test]
fn soft_control_binary_suppresses_when_post_newline_payload_is_short() {
    // Payload after last newline is just one printable char -> suppress.
    let bytes: Vec<u8> = vec![b'a', b'b', b'c', b'\n', b'q'];
    assert!(soft_control_binary_should_suppress(&bytes));
}

// ---------------- read_source_file ----------------

fn write_temp(dir: &std::path::Path, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.join(name);
    let mut f = std::fs::File::create(&path).expect("create temp file");
    f.write_all(bytes).expect("write temp file");
    path
}

#[test]
fn read_source_file_plain_utf8_returns_text() {
    let dir = tempdir().unwrap();
    let path = write_temp(dir.path(), "ascii.ts", b"export const x = 1;\n");
    match read_source_file(&path) {
        FileReadResult::Text(t) => assert_eq!(t, "export const x = 1;\n"),
        other => panic!("expected Text, got {other:?}"),
    }
}

#[test]
fn read_source_file_utf16_be_bom_decodes_text() {
    let dir = tempdir().unwrap();
    // "Hi" in UTF-16 BE with BOM.
    let bytes: Vec<u8> = vec![0xFE, 0xFF, 0x00, b'H', 0x00, b'i'];
    let path = write_temp(dir.path(), "u16be.ts", &bytes);
    match read_source_file(&path) {
        FileReadResult::Text(t) => assert_eq!(t, "Hi"),
        other => panic!("expected Text from UTF-16 BE BOM, got {other:?}"),
    }
}

#[test]
fn read_source_file_utf16_le_bom_decodes_text() {
    let dir = tempdir().unwrap();
    // "Hi" in UTF-16 LE with BOM.
    let bytes: Vec<u8> = vec![0xFF, 0xFE, b'H', 0x00, b'i', 0x00];
    let path = write_temp(dir.path(), "u16le.ts", &bytes);
    match read_source_file(&path) {
        FileReadResult::Text(t) => assert_eq!(t, "Hi"),
        other => panic!("expected Text from UTF-16 LE BOM, got {other:?}"),
    }
}

#[test]
fn read_source_file_binary_marks_suppression() {
    let dir = tempdir().unwrap();
    // 11 null bytes -> classify_binary_file returns Some(true).
    let mut bytes = vec![b'a'; 64];
    for slot in bytes.iter_mut().take(11) {
        *slot = 0;
    }
    let path = write_temp(dir.path(), "bin.bin", &bytes);
    match read_source_file(&path) {
        FileReadResult::Binary {
            suppress_parser_diagnostics,
            ..
        } => assert!(suppress_parser_diagnostics),
        other => panic!("expected Binary, got {other:?}"),
    }
}

#[test]
fn read_source_file_missing_file_returns_error() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("does_not_exist.ts");
    match read_source_file(&path) {
        FileReadResult::Error(msg) => assert!(!msg.is_empty()),
        other => panic!("expected Error, got {other:?}"),
    }
}

#[test]
fn read_source_file_invalid_utf8_falls_back_to_lossy_binary() {
    let dir = tempdir().unwrap();
    // Stray 0xFF byte not paired with 0xFE makes invalid UTF-8 but does not
    // hit BOM or many-nulls branches: from_utf8 fails -> Binary{ suppress=true }.
    let bytes: Vec<u8> = vec![b'a', b'b', 0xFF, b'c'];
    let path = write_temp(dir.path(), "bad-utf8.ts", &bytes);
    match read_source_file(&path) {
        FileReadResult::Binary {
            suppress_parser_diagnostics,
            ..
        } => assert!(suppress_parser_diagnostics),
        other => panic!("expected Binary fallback, got {other:?}"),
    }
}

// ---------------- has_no_default_lib_directive ----------------

#[test]
fn has_no_default_lib_directive_true_for_canonical_form() {
    let src = "/// <reference no-default-lib=\"true\" />\nexport {};\n";
    assert!(has_no_default_lib_directive(src));
}

#[test]
fn has_no_default_lib_directive_true_for_single_quotes() {
    let src = "/// <reference no-default-lib='true' />\n";
    assert!(has_no_default_lib_directive(src));
}

#[test]
fn has_no_default_lib_directive_false_when_value_false() {
    let src = "/// <reference no-default-lib=\"false\" />\n";
    assert!(!has_no_default_lib_directive(src));
}

#[test]
fn has_no_default_lib_directive_skips_blank_lines_before_first_triple_slash() {
    let src = "\n\n   \n/// <reference no-default-lib=\"true\" />\n";
    assert!(has_no_default_lib_directive(src));
}

#[test]
fn has_no_default_lib_directive_stops_at_first_non_directive_non_blank() {
    // A non-`///` non-blank line breaks the prefix scan, so a later directive
    // is ignored.
    let src = "import x from './a';\n/// <reference no-default-lib=\"true\" />\n";
    assert!(!has_no_default_lib_directive(src));
}

#[test]
fn has_no_default_lib_directive_false_when_absent() {
    assert!(!has_no_default_lib_directive(
        "/// <reference path=\"./other.d.ts\" />\n"
    ));
    assert!(!has_no_default_lib_directive(""));
}

// ---------------- parse_reference_no_default_lib_value ----------------

#[test]
fn parse_reference_no_default_lib_value_true_double_quotes() {
    assert_eq!(
        parse_reference_no_default_lib_value("/// <reference no-default-lib=\"true\" />"),
        Some(true)
    );
}

#[test]
fn parse_reference_no_default_lib_value_true_single_quotes() {
    assert_eq!(
        parse_reference_no_default_lib_value("/// <reference no-default-lib='true' />"),
        Some(true)
    );
}

#[test]
fn parse_reference_no_default_lib_value_false() {
    assert_eq!(
        parse_reference_no_default_lib_value("/// <reference no-default-lib=\"false\" />"),
        Some(false)
    );
}

#[test]
fn parse_reference_no_default_lib_value_case_insensitive_value() {
    assert_eq!(
        parse_reference_no_default_lib_value("/// <reference no-default-lib=\"TRUE\" />"),
        Some(true)
    );
    assert_eq!(
        parse_reference_no_default_lib_value("/// <reference no-default-lib=\"False\" />"),
        Some(false)
    );
}

#[test]
fn parse_reference_no_default_lib_value_unknown_value_is_none() {
    assert_eq!(
        parse_reference_no_default_lib_value("/// <reference no-default-lib=\"yes\" />"),
        None
    );
}

#[test]
fn parse_reference_no_default_lib_value_unquoted_value_is_none() {
    assert_eq!(
        parse_reference_no_default_lib_value("/// <reference no-default-lib=true />"),
        None
    );
}

#[test]
fn parse_reference_no_default_lib_value_missing_equals_is_none() {
    assert_eq!(
        parse_reference_no_default_lib_value("/// <reference no-default-lib \"true\" />"),
        None
    );
}

#[test]
fn parse_reference_no_default_lib_value_needle_absent_is_none() {
    assert_eq!(
        parse_reference_no_default_lib_value("/// <reference path=\"./a.d.ts\" />"),
        None
    );
    assert_eq!(parse_reference_no_default_lib_value(""), None);
}

#[test]
fn parse_reference_no_default_lib_value_tolerates_extra_spaces() {
    assert_eq!(
        parse_reference_no_default_lib_value("/// <reference   no-default-lib   =   \"true\"   />"),
        Some(true)
    );
}

// ---------------- resolve_tsconfig_path ----------------

#[test]
fn resolve_tsconfig_path_missing_file_reports_path_does_not_exist() {
    let dir = tempdir().unwrap();
    let cwd = dir.path();
    let result = resolve_tsconfig_path(cwd, Some(Path::new("missing/tsconfig.json")));
    match result {
        Err(ResolveTsconfigError::PathDoesNotExist(p)) => {
            assert_eq!(p, Path::new("missing/tsconfig.json"));
            assert_eq!(
                format!("{}", ResolveTsconfigError::PathDoesNotExist(p)),
                "The specified path does not exist: 'missing/tsconfig.json'."
            );
        }
        other => panic!("expected PathDoesNotExist, got {other:?}"),
    }
}

#[test]
fn resolve_tsconfig_path_missing_directory_reports_path_does_not_exist() {
    let dir = tempdir().unwrap();
    let cwd = dir.path();
    let result = resolve_tsconfig_path(cwd, Some(Path::new("missing-dir")));
    match result {
        Err(ResolveTsconfigError::PathDoesNotExist(p)) => {
            assert_eq!(p, Path::new("missing-dir"));
        }
        other => panic!("expected PathDoesNotExist, got {other:?}"),
    }
}

#[test]
fn resolve_tsconfig_path_existing_dir_without_config_reports_no_config() {
    let dir = tempdir().unwrap();
    let cwd = dir.path();
    std::fs::create_dir_all(cwd.join("empty-dir")).unwrap();
    let result = resolve_tsconfig_path(cwd, Some(Path::new("empty-dir")));
    match result {
        Err(ResolveTsconfigError::NoConfigInDirectory(p)) => {
            assert_eq!(p, Path::new("empty-dir"));
            assert_eq!(
                format!("{}", ResolveTsconfigError::NoConfigInDirectory(p)),
                "Cannot find a tsconfig.json file at the specified directory: 'empty-dir'."
            );
        }
        other => panic!("expected NoConfigInDirectory, got {other:?}"),
    }
}

#[test]
fn resolve_tsconfig_path_existing_dir_with_config_returns_path() {
    let dir = tempdir().unwrap();
    let cwd = dir.path();
    std::fs::create_dir_all(cwd.join("proj")).unwrap();
    let tsconfig = cwd.join("proj/tsconfig.json");
    std::fs::write(&tsconfig, "{}").unwrap();
    let result = resolve_tsconfig_path(cwd, Some(Path::new("proj")));
    let resolved = result.expect("expected success").expect("expected Some");
    assert_eq!(resolved, normalize_path(&tsconfig));
}

#[test]
fn resolve_tsconfig_path_explicit_file_returns_path() {
    let dir = tempdir().unwrap();
    let cwd = dir.path();
    let tsconfig = cwd.join("custom.json");
    std::fs::write(&tsconfig, "{}").unwrap();
    let result = resolve_tsconfig_path(cwd, Some(Path::new("custom.json")));
    let resolved = result.expect("expected success").expect("expected Some");
    assert_eq!(resolved, normalize_path(&tsconfig));
}

#[test]
fn resolve_tsconfig_path_preserves_user_supplied_path_in_error() {
    // The diagnostic message must use the user-supplied (relative) path,
    // not the absolute resolved path, so that messages match tsc parity.
    let dir = tempdir().unwrap();
    let cwd = dir.path();
    let result = resolve_tsconfig_path(cwd, Some(Path::new("missing/tsconfig.json")));
    let err = result.unwrap_err();
    assert_eq!(
        format!("{err}"),
        "The specified path does not exist: 'missing/tsconfig.json'."
    );
}
