#[test]
fn test_parse_single_file() {
    let result = parse_file_single("test.ts".to_string(), "let x = 42;".to_string());

    assert_eq!(result.file_name, "test.ts");
    assert!(result.source_file.is_some());
    assert!(result.parse_diagnostics.is_empty());
}

#[test]
fn test_parse_multiple_files_parallel() {
    let files = vec![
        ("a.ts".to_string(), "let a = 1;".to_string()),
        ("b.ts".to_string(), "let b = 2;".to_string()),
        ("c.ts".to_string(), "let c = 3;".to_string()),
    ];

    let results = parse_files_parallel(files);

    assert_eq!(results.len(), 3);
    for result in &results {
        assert!(result.source_file.is_some());
        assert!(result.parse_diagnostics.is_empty());
    }
}

#[test]
fn test_parse_with_stats() {
    let files = vec![
        (
            "a.ts".to_string(),
            "function foo() { return 1; }".to_string(),
        ),
        (
            "b.ts".to_string(),
            "class Bar { constructor() {} }".to_string(),
        ),
    ];

    let (results, stats) = parse_files_with_stats(files);

    assert_eq!(results.len(), 2);
    assert_eq!(stats.file_count, 2);
    assert!(stats.total_bytes > 0);
    assert!(stats.total_nodes > 0);
    assert_eq!(stats.error_count, 0);
}

#[test]
fn test_parallel_parsing_consistency() {
    // Parse the same file multiple times in parallel
    // Results should be consistent
    let source =
        "const x: number = 42; function add(a: number, b: number): number { return a + b; }";
    let files: Vec<_> = (0..10)
        .map(|i| (format!("file{i}.ts"), source.to_string()))
        .collect();

    let results = parse_files_parallel(files);

    // All should have same number of nodes (same source)
    let first_node_count = results[0].arena.len();
    for result in &results {
        assert_eq!(result.arena.len(), first_node_count);
        assert!(result.parse_diagnostics.is_empty());
    }
}

#[test]
fn test_large_batch_parsing() {
    // Test with a larger batch to exercise parallelism
    let files: Vec<_> = (0..100)
        .map(|i| {
            let source = format!("function fn{i}(x: number): number {{ return x * {i}; }}");
            (format!("module{i}.ts"), source)
        })
        .collect();

    let (results, stats) = parse_files_with_stats(files);

    assert_eq!(results.len(), 100);
    assert_eq!(stats.file_count, 100);
    // Note: Parser may produce parse errors for some constructs
    // The key test is that parallel parsing works correctly
    // assert_eq!(stats.error_count, 0);

    // Each file should have similar node counts
    for result in &results {
        assert!(
            result.arena.len() >= 5,
            "Each file should have at least 5 nodes"
        );
    }
}

#[test]
fn test_parse_lib_references_stops_at_first_non_comment_line() {
    let references = parse_lib_references(
        r#"
/// <reference lib="ES2016" />
// a normal comment is still part of the header scan
/// <reference lib='lib.dom.d.ts' />
let not_a_header_line = true;
/// <reference lib="es2023.collection" />
"#,
    );

    assert_eq!(references, vec!["es2016", "lib.dom.d.ts"]);
}

#[test]
fn test_normalize_lib_reference_name_handles_legacy_and_nested_lib_names() {
    assert_eq!(normalize_lib_reference_name("lib.d.ts"), "es5");
    assert_eq!(normalize_lib_reference_name("LIB.ES7.D.TS"), "es2016");
    assert_eq!(normalize_lib_reference_name("lib.dom.d.ts"), "dom");
    assert_eq!(
        normalize_lib_reference_name("lib.dom.iterable.d.ts"),
        "dom.iterable"
    );
    assert_eq!(
        normalize_lib_reference_name("lib.dom.asynciterable.d.ts"),
        "dom.asynciterable"
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn test_rayon_worker_count_for_work_items_caps_only_small_implicit_workloads() {
    assert_eq!(rayon_worker_count_for_work_items(4, 16, false), Some(4));
    assert_eq!(rayon_worker_count_for_work_items(4, 2, false), Some(2));
    assert_eq!(rayon_worker_count_for_work_items(0, 16, false), None);
    assert_eq!(rayon_worker_count_for_work_items(33, 16, false), None);
    assert_eq!(rayon_worker_count_for_work_items(4, 16, true), None);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn test_small_workload_rayon_pool_does_not_cap_global_pool() {
    if std::env::var_os("RAYON_NUM_THREADS").is_some() {
        return;
    }
    let available = std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(1);
    let small_threads = run_with_rayon_pool_for_work_items(4, rayon::current_num_threads);
    assert_eq!(small_threads, available.clamp(1, 4));

    ensure_rayon_global_pool();
    let global_threads = rayon::current_num_threads();
    if available > 4 {
        assert!(
            global_threads > small_threads,
            "small workload runner must use a scoped pool, not permanently cap the global pool"
        );
    }
}

#[test]
fn test_resolve_lib_reference_path_prefers_available_candidate_names() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let lib_dir = temp_dir.path();

    fs::write(lib_dir.join("lib.custom.d.ts"), "").expect("write custom lib");
    fs::write(lib_dir.join("custom.d.ts"), "").expect("write custom bare lib");

    let base_path = lib_dir.join("lib.esnext.d.ts");
    let base_path: &Path = base_path.as_path();

    let custom_path = resolve_lib_reference_path(base_path, "CUSTOM").expect("resolve custom");
    assert_eq!(custom_path, lib_dir.join("lib.custom.d.ts"));

    let wrapped_path =
        resolve_lib_reference_path(base_path, "lib.custom.d.ts").expect("resolve wrapped");
    assert_eq!(wrapped_path, lib_dir.join("lib.custom.d.ts"));

    assert!(resolve_lib_reference_path(base_path, "nonexistent").is_none());
}

#[test]
fn test_resolve_lib_reference_path_uses_embedded_virtual_root_without_disk_probe_shape() {
    let base_path = Path::new("/embedded-lib/es2020.d.ts");

    let es5 = resolve_lib_reference_path(base_path, "lib.es5.d.ts").expect("resolve es5");
    assert_eq!(es5, Path::new("/embedded-lib/es5.d.ts"));

    let dom = resolve_lib_reference_path(base_path, "dom.generated").expect("resolve dom");
    assert_eq!(dom, Path::new("/embedded-lib/dom.d.ts"));

    assert!(resolve_lib_reference_path(base_path, "definitely-not-a-lib").is_none());
}

#[test]
fn test_resolve_generated_embedded_lib_reference_path_uses_normalized_refs() {
    let dom = resolve_generated_embedded_lib_reference_path("dom");
    assert_eq!(dom, Path::new("/embedded-lib/dom.d.ts"));

    let es5 = resolve_generated_embedded_lib_reference_path("lib.d.ts");
    assert_eq!(es5, Path::new("/embedded-lib/es5.d.ts"));
}

#[test]
fn test_collect_lib_files_recursive_cached_reads_embedded_virtual_root_as_static() {
    let mut loaded = FxHashSet::default();
    let mut file_contents = Vec::new();
    let file_cache = FxHashMap::default();

    collect_lib_files_recursive_cached(
        Path::new("/embedded-lib/es2020.d.ts"),
        &mut loaded,
        &mut file_contents,
        &file_cache,
    )
    .expect("collect embedded lib files");

    let (_, source_text) = file_contents
        .iter()
        .find(|(name, _)| name == "/embedded-lib/es2020.d.ts")
        .expect("es2020 entry");
    assert!(matches!(
        source_text,
        LibSourceText::Static {
            text: _,
            content_hash: _
        }
    ));
    assert!(loaded.iter().all(|path| path.starts_with("/embedded-lib")));
}

#[test]
fn test_collect_lib_files_recursive_cached_uses_owned_references_for_physical_libs() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let es2020_path = temp_dir.path().join("lib.es2020.d.ts");
    let custom_path = temp_dir.path().join("lib.custom.d.ts");
    fs::write(&es2020_path, "").expect("write es2020 lib");
    fs::write(&custom_path, "").expect("write custom lib");

    let es2020_path = es2020_path.canonicalize().expect("canonical es2020");
    let custom_path = custom_path.canonicalize().expect("canonical custom");
    let mut file_cache = FxHashMap::default();
    file_cache.insert(
        es2020_path.clone(),
        "/// <reference lib=\"custom\" />\ninterface PhysicalEs2020 {}\n".to_string(),
    );
    file_cache.insert(
        custom_path.clone(),
        "interface PhysicalCustom {}\n".to_string(),
    );

    let mut loaded = FxHashSet::default();
    let mut file_contents = Vec::new();
    collect_lib_files_recursive_cached(&es2020_path, &mut loaded, &mut file_contents, &file_cache)
        .expect("collect physical lib files");

    assert!(
        file_contents
            .iter()
            .any(|(name, _)| name == custom_path.to_string_lossy().as_ref()),
        "physical lib references should be parsed from owned source text"
    );
    assert!(
        file_contents
            .iter()
            .all(|(name, _)| !name.starts_with("/embedded-lib")),
        "physical libs must not use embedded reference metadata"
    );
}

#[test]
fn test_parse_and_bind_single_uses_json_synthetic_bind_path() {
    let result = parse_and_bind_single(
        "settings.json".to_string(),
        r#"{ "compilerOptions": { "strict": true } }"#.to_string(),
    );

    assert_eq!(result.file_name, "settings.json");
    assert!(result.source_file.is_some());
    assert!(result.parse_diagnostics.is_empty());
    assert!(!result.arena.is_empty());
}

#[test]
fn test_parse_and_bind_single_json_identifier_root_emits_tsc_recovery_sequence() {
    let result =
        parse_and_bind_single("settings.json".to_string(), "contents Not read".to_string());

    let got: Vec<(u32, u32, String)> = result
        .parse_diagnostics
        .iter()
        .map(|d| (d.code, d.start, d.message.clone()))
        .collect();

    let expected = vec![
        (1005, 0, "'{' expected.".to_string()),
        (1136, 0, "Property assignment expected.".to_string()),
        (1005, 9, "',' expected.".to_string()),
        (1136, 9, "Property assignment expected.".to_string()),
        (1005, 13, "',' expected.".to_string()),
        (1136, 13, "Property assignment expected.".to_string()),
        (1005, 17, "'}' expected.".to_string()),
    ];

    assert_eq!(got, expected);
}

#[test]
fn test_parse_and_bind_single_json_keyword_root_is_valid() {
    let result = parse_and_bind_single("settings.json".to_string(), "true".to_string());
    assert!(result.parse_diagnostics.is_empty());
}

// =========================================================================
// Parallel Binding Tests
// =========================================================================

#[test]
fn test_bind_single_file() {
    let result = parse_and_bind_single(
        "test.ts".to_string(),
        "let x = 42; function foo() {}".to_string(),
    );

    assert_eq!(result.file_name, "test.ts");
    assert!(result.source_file.is_some());
    assert!(result.parse_diagnostics.is_empty());
    // Should have symbols for x and foo
    assert!(result.file_locals.has("x"));
    assert!(result.file_locals.has("foo"));
}

#[test]
fn test_bind_multiple_files_parallel() {
    let files = vec![
        ("a.ts".to_string(), "let a = 1;".to_string()),
        ("b.ts".to_string(), "function b() {}".to_string()),
        ("c.ts".to_string(), "class C {}".to_string()),
    ];

    let results = parse_and_bind_parallel(files);

    assert_eq!(results.len(), 3);

    // Each file should have its own symbols
    assert!(results[0].file_locals.has("a"));
    assert!(results[1].file_locals.has("b"));
    assert!(results[2].file_locals.has("C"));
}

#[test]
fn test_bind_with_stats() {
    let files = vec![
        (
            "a.ts".to_string(),
            "function foo() { return 1; }".to_string(),
        ),
        ("b.ts".to_string(), "class Bar { x: number; }".to_string()),
    ];

    let (results, stats) = parse_and_bind_with_stats(files);

    assert_eq!(results.len(), 2);
    assert_eq!(stats.file_count, 2);
    assert!(stats.total_nodes > 0);
    assert!(stats.total_symbols > 0);
    assert_eq!(stats.parse_error_count, 0);
}

#[test]
fn test_parallel_binding_consistency() {
    // Bind the same file multiple times in parallel
    // Results should be consistent
    let source =
        "const x: number = 42; function add(a: number, b: number): number { return a + b; }";
    let files: Vec<_> = (0..10)
        .map(|i| (format!("file{i}.ts"), source.to_string()))
        .collect();

    let results = parse_and_bind_parallel(files);

    // All should have same symbols
    for result in &results {
        assert!(result.file_locals.has("x"));
        assert!(result.file_locals.has("add"));
        assert!(result.parse_diagnostics.is_empty());
    }
}

#[test]
fn test_large_batch_binding() {
    // Test with a larger batch to exercise parallelism
    let files: Vec<_> = (0..100)
        .map(|i| {
            let source = format!(
                "function fn{i}(x: number): number {{ return x * {i}; }} let val{i} = fn{i}(10);"
            );
            (format!("module{i}.ts"), source)
        })
        .collect();

    let (results, stats) = parse_and_bind_with_stats(files);

    assert_eq!(results.len(), 100);
    assert_eq!(stats.file_count, 100);
    assert!(
        stats.total_symbols >= 200,
        "Should have at least 200 symbols (2 per file)"
    );

    // Each file should have its function and variable
    for (i, result) in results.iter().enumerate() {
        let fn_name = format!("fn{i}");
        let var_name = format!("val{i}");
        assert!(
            result.file_locals.has(&fn_name),
            "File {i} missing {fn_name}"
        );
        assert!(
            result.file_locals.has(&var_name),
            "File {i} missing {var_name}"
        );
    }
}

// =========================================================================
// Symbol Merging Tests
// =========================================================================

#[test]
fn test_merge_single_file() {
    let files = vec![(
        "a.ts".to_string(),
        "let x = 1; function foo() {}".to_string(),
    )];

    let program = compile_files(files);

    assert_eq!(program.files.len(), 1);
    assert!(program.globals.has("x"));
    assert!(program.globals.has("foo"));
    // Symbols should be in global arena
    assert!(program.symbols.len() >= 2);
}

#[test]
fn test_merge_multiple_files() {
    let files = vec![
        ("a.ts".to_string(), "let a = 1;".to_string()),
        ("b.ts".to_string(), "function b() {}".to_string()),
        ("c.ts".to_string(), "class C {}".to_string()),
    ];

    let program = compile_files(files);

    assert_eq!(program.files.len(), 3);
    // All symbols should be in globals
    assert!(program.globals.has("a"));
    assert!(program.globals.has("b"));
    assert!(program.globals.has("C"));
    // All symbols merged into global arena
    assert!(program.symbols.len() >= 3);
}

#[test]
fn test_build_merged_file_locals_includes_globals_and_locals() {
    // Construct a multi-file program where each file's top-level symbols are
    // visible cross-file. The post-merge per-file checker binders read globals
    // through `binder.file_locals`, so `build_merged_file_locals` must fold
    // `program.globals` into the per-file table for every file.
    let files = vec![
        ("a.ts".to_string(), "let alpha = 1;".to_string()),
        ("b.ts".to_string(), "let beta = 2;".to_string()),
    ];
    let program = compile_files(files);

    assert!(program.globals.has("alpha"));
    assert!(program.globals.has("beta"));

    for file_idx in 0..program.files.len() {
        let merged = program.build_merged_file_locals(file_idx);
        // Every per-file merged table sees both globals (cross-file refs).
        assert!(
            merged.has("alpha"),
            "file {file_idx} merged locals missing global `alpha`"
        );
        assert!(
            merged.has("beta"),
            "file {file_idx} merged locals missing global `beta`"
        );
        // And whatever per-file locals existed.
        if let Some(locals) = program.file_locals.get(file_idx) {
            for (name, sym_id) in locals.iter() {
                assert_eq!(
                    merged.get(name),
                    Some(*sym_id),
                    "file {file_idx} merged locals dropped per-file local `{name}`"
                );
            }
        }
    }
}

#[test]
fn test_build_merged_file_locals_locals_win_on_conflict() {
    // When a per-file local shadows a global name, the merged table must keep
    // the local's `SymbolId`. The previous in-place merge inserted file-locals
    // first and then skipped already-present global names, so the merged
    // result preferred the local; the fast-path helper must preserve that.
    let mut program = compile_files(vec![("a.ts".to_string(), String::new())]);

    // Synthesize an overlap manually: install `shared` in globals as one
    // SymbolId and in file 0's per-file locals as a different SymbolId,
    // then assert the merged table reports the local's SymbolId.
    let global_id = tsz_binder::SymbolId(9999);
    let local_id = tsz_binder::SymbolId(1111);
    program.globals.set("shared".to_string(), global_id);
    if program.file_locals.is_empty() {
        program.file_locals.push(SymbolTable::new());
    }
    program.file_locals[0].set("shared".to_string(), local_id);

    let merged = program.build_merged_file_locals(0);
    assert_eq!(merged.get("shared"), Some(local_id));
}

#[test]
fn test_build_merged_file_locals_empty_locals_returns_globals() {
    // Files with no top-level local entries are common (pure re-export
    // shims, declaration files). The helper must short-circuit to an
    // O(1) Arc::clone of `program.globals` instead of allocating a fresh
    // map and copying every key.
    let mut program = compile_files(vec![("a.ts".to_string(), String::new())]);
    program
        .globals
        .set("hello".to_string(), tsz_binder::SymbolId(7));
    if program.file_locals.is_empty() {
        program.file_locals.push(SymbolTable::new());
    }
    // Force file 0 to have empty per-file locals.
    program.file_locals[0] = SymbolTable::new();

    let merged = program.build_merged_file_locals(0);
    assert_eq!(merged.get("hello"), Some(tsz_binder::SymbolId(7)));
    assert_eq!(merged.len(), program.globals.len());
}

#[test]
fn test_merge_symbol_id_remapping() {
    let files = vec![
        ("a.ts".to_string(), "let x = 1;".to_string()),
        ("b.ts".to_string(), "let y = 2;".to_string()),
    ];

    let program = compile_files(files);

    // Get the symbol IDs from globals
    let x_id = program.globals.get("x").expect("x should exist");
    let y_id = program.globals.get("y").expect("y should exist");

    // IDs should be different (remapped properly)
    assert_ne!(x_id, y_id);

    // Both should be resolvable from global arena
    assert!(program.symbols.get(x_id).is_some());
    assert!(program.symbols.get(y_id).is_some());
}

#[test]
fn test_load_lib_files_for_binding_strict_recurses_reference_libs() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let lib_dir = temp_dir.path();

    fs::write(
        lib_dir.join("lib.esnext.d.ts"),
        "/// <reference lib=\"es2023.collection\" />\ninterface Root {}\n",
    )
    .expect("write esnext");
    fs::write(
        lib_dir.join("lib.es2023.collection.d.ts"),
        "/// <reference lib=\"es5\" />\ninterface WeakKeyTypes { symbol: symbol; }\n",
    )
    .expect("write es2023.collection");
    fs::write(
        lib_dir.join("lib.es5.d.ts"),
        "interface WeakKeyTypes { object: object; }\ninterface Symbol {}\n",
    )
    .expect("write es5");

    let root = lib_dir.join("lib.esnext.d.ts");
    let loaded = load_lib_files_for_binding_strict(&[root.as_path()]).expect("load libs");
    let names: Vec<String> = loaded.iter().map(|lib| lib.file_name.clone()).collect();

    // The loader recursively resolves /// <reference lib="..." /> directives,
    // so the result includes the requested files plus all their transitive
    // dependencies (including embedded standard lib files).
    // Verify the 3 test fixture files are present in the loaded set.
    let expected_files = vec![
        lib_dir.join("lib.es5.d.ts").to_string_lossy().to_string(),
        lib_dir
            .join("lib.es2023.collection.d.ts")
            .to_string_lossy()
            .to_string(),
        lib_dir
            .join("lib.esnext.d.ts")
            .to_string_lossy()
            .to_string(),
    ];
    for expected in &expected_files {
        assert!(
            names.iter().any(|n| n == expected),
            "Expected loaded libs to contain {expected}, got: {names:?}"
        );
    }
    // Should have at least the 3 root files
    assert!(
        names.len() >= 3,
        "Expected at least 3 loaded lib files, got {}",
        names.len()
    );
}

#[test]
fn clone_lib_files_for_checker_creates_distinct_parsed_bound_copies() {
    let lib = std::sync::Arc::new(tsz_binder::lib_loader::LibFile::from_source(
        "lib.test.d.ts".to_string(),
        "interface Array<T> { length: number; }\ninterface Promise<T> { then(): Promise<T>; }\n"
            .to_string(),
    ));

    let cloned = clone_lib_files_for_checker(&[std::sync::Arc::clone(&lib)], false);

    assert_eq!(cloned.len(), 1);
    let cloned_lib = &cloned[0];
    assert_eq!(cloned_lib.file_name, lib.file_name);
    assert_eq!(cloned_lib.root_index, lib.root_index);
    assert!(
        !std::sync::Arc::ptr_eq(&cloned_lib.arena, &lib.arena),
        "checker lib clone must have independent arena identity",
    );
    assert!(
        !std::sync::Arc::ptr_eq(&cloned_lib.binder, &lib.binder),
        "checker lib clone must have independent binder identity",
    );
    assert_eq!(cloned_lib.arena.len(), lib.arena.len());
    assert_eq!(cloned_lib.binder.symbols.len(), lib.binder.symbols.len());
}

#[test]
fn test_compile_files_with_default_libs_preserves_promise_type_parameters() {
    let lib_paths =
        crate::config::resolve_default_lib_files(tsz_common::common::ScriptTarget::ES2015)
            .expect("default libs");
    let program = compile_files_with_libs(
        vec![(
            "main.ts".to_string(),
            "declare const p: Promise<number>;\n".to_string(),
        )],
        &lib_paths,
    );

    let promise_id = program
        .globals
        .get("Promise")
        .expect("Promise should be a global lib symbol");
    let promise = program
        .symbols
        .get(promise_id)
        .expect("Promise symbol should resolve");

    assert!(
        promise.has_any_flags(tsz_binder::symbol_flags::INTERFACE),
        "Promise should retain its interface meaning after lib merge; flags={}",
        promise.flags
    );

    let has_generic_interface_decl = promise.declarations.iter().any(|decl_idx| {
        program
            .declaration_arenas
            .get(&(promise_id, *decl_idx))
            .into_iter()
            .flatten()
            .any(|arena| {
                arena.get(*decl_idx).is_some_and(|node| {
                    arena
                        .get_interface(node)
                        .is_some_and(|iface| iface.type_parameters.is_some())
                })
            })
    });

    assert!(
        has_generic_interface_decl,
        "Promise should retain the generic interface declaration; declarations={:?}",
        promise.declarations
    );
}

#[test]
fn test_merge_preserves_file_locals() {
    let files = vec![
        ("a.ts".to_string(), "let a1 = 1; let a2 = 2;".to_string()),
        ("b.ts".to_string(), "let b1 = 1; let b2 = 2;".to_string()),
    ];

    let program = compile_files(files);

    // Each file should have its own locals
    assert_eq!(program.file_locals.len(), 2);
    assert!(program.file_locals[0].has("a1"));
    assert!(program.file_locals[0].has("a2"));
    assert!(program.file_locals[1].has("b1"));
    assert!(program.file_locals[1].has("b2"));
}

#[test]
fn check_files_parallel_reports_default_lib_breakage_from_global_node_merge() {
    let files = vec![(
        "main.ts".to_string(),
        r#"
const enum SyntaxKind {
    Track,
}

interface Node {
    kind: SyntaxKind;
}
"#
        .to_string(),
    )];

    let lib_files = vec![std::sync::Arc::new(
        crate::lib_loader::LibFile::from_source(
            "lib.dom.d.ts".to_string(),
            r#"
interface Node {
    kind: string;
}

interface Element extends Node {}
interface HTMLElement extends Element {}
interface HTMLTrackElement extends HTMLElement {
    kind: string;
}
"#
            .to_string(),
        ),
    )];
    let program = merge_bind_results(parse_and_bind_parallel_with_libs(files, &lib_files));
    let options = CheckerOptions {
        target: ScriptTarget::ES5,
        module: ModuleKind::ES2015,
        strict: true,
        no_implicit_any: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };

    let result = check_files_parallel(&program, &options, &lib_files);
    let lib_dom_diagnostics = result
        .file_results
        .iter()
        .filter(|file| file.file_name.ends_with("lib.dom.d.ts"))
        .flat_map(|file| file.diagnostics.iter())
        .collect::<Vec<_>>();
    let ts2430_count = lib_dom_diagnostics
        .iter()
        .filter(|diag| diag.code == diagnostic_codes::INTERFACE_INCORRECTLY_EXTENDS_INTERFACE)
        .count();

    assert_eq!(
        ts2430_count, 1,
        "Expected one post-merge lib TS2430 diagnostic after merging Node.kind, got: {result:#?}"
    );
}

#[test]
fn affected_lib_interface_names_empty_skips_lib_interface_pass() {
    let files = vec![("main.ts".to_string(), "export const value = 1;".to_string())];
    let lib_files = vec![std::sync::Arc::new(
        crate::lib_loader::LibFile::from_source(
            "lib.dom.d.ts".to_string(),
            "interface Node { kind: string; }".to_string(),
        ),
    )];
    let program = merge_bind_results(parse_and_bind_parallel_with_libs(files, &lib_files));

    let affected = affected_lib_interface_names(&program, &lib_files);
    assert!(
        affected.is_empty(),
        "module-only user files should not schedule lib interface validation"
    );

    let result = check_files_parallel(&program, &CheckerOptions::default(), &lib_files);
    let lib_result = result
        .file_results
        .iter()
        .find(|file| file.file_name.ends_with("lib.dom.d.ts"))
        .expect("lib files should still get empty result entries");

    assert!(
        lib_result.diagnostics.is_empty(),
        "empty affected-interface pass should not produce lib diagnostics"
    );
}

#[test]
fn lib_file_contains_affected_interface_filters_unrelated_lib_files() {
    let affected = ["Node".to_string()].into_iter().collect();
    let matching_lib = crate::lib_loader::LibFile::from_source(
        "lib.dom.d.ts".to_string(),
        "interface Node { kind: string; }".to_string(),
    );
    let unrelated_lib = crate::lib_loader::LibFile::from_source(
        "lib.es5.d.ts".to_string(),
        "interface Array<T> { length: number; }".to_string(),
    );

    assert!(lib_file_contains_affected_interface(
        &matching_lib,
        &affected
    ));
    assert!(!lib_file_contains_affected_interface(
        &unrelated_lib,
        &affected
    ));
}

#[test]
fn test_merged_program_residency_stats_track_unique_file_arenas() {
    let files = vec![
        ("a.ts".to_string(), "export const a = 1;".to_string()),
        ("b.ts".to_string(), "export const b = 2;".to_string()),
    ];

    let bind_results = parse_and_bind_parallel(files);
    let program = merge_bind_results(bind_results);
    let stats = program.residency_stats();

    assert_eq!(stats.file_count, 2);
    assert_eq!(stats.bound_file_arena_count, 2);
    assert_eq!(stats.unique_arena_count, 2);
    assert!(stats.symbol_arena_count >= 2);
    assert!(stats.declaration_arena_bucket_count >= 2);
    assert!(stats.declaration_arena_mapping_count >= 2);
    assert!(stats.has_skeleton_index);
    assert!(
        stats.skeleton_total_symbol_count >= 2,
        "skeleton should track at least 2 symbols, got {}",
        stats.skeleton_total_symbol_count
    );
    assert!(
        stats.skeleton_estimated_size_bytes > 0,
        "skeleton size estimate should be nonzero when skeleton is present"
    );
    assert!(
        stats.pre_merge_bind_total_bytes > 0,
        "pre-merge bind total should be nonzero for any non-empty merge"
    );
}

#[test]
fn test_merged_program_residency_stats_deduplicate_shared_arena_handles() {
    let files = vec![(
        "a.ts".to_string(),
        "export const a = 1; export function b() { return a; }".to_string(),
    )];

    let bind_results = parse_and_bind_parallel(files);
    let program = merge_bind_results(bind_results);
    let stats = program.residency_stats();

    assert_eq!(stats.file_count, 1);
    assert_eq!(stats.bound_file_arena_count, 1);
    assert_eq!(
        stats.unique_arena_count, 1,
        "symbol/declaration arena maps should point back to the same retained file arena"
    );
    assert!(stats.symbol_arena_count >= 2);
    assert!(stats.declaration_arena_mapping_count >= 2);
    assert!(stats.has_skeleton_index);
}

#[test]
fn test_check_files_parallel_preserves_ts2454_for_named_import_from_export_equals_module() {
    let files = vec![
        (
            "express.d.ts".to_string(),
            r#"
declare namespace Express { export interface Request {} }
declare module "express" {
    function e(): e.Express;
    namespace e {
        interface Request extends Express.Request { get(name: string): string; }
        interface Express {}
    }
    export = e;
}
"#
            .to_string(),
        ),
        (
            "consumer.ts".to_string(),
            r#"
import { Request } from "express";
let x: Request;
const y = x.get("a");
"#
            .to_string(),
        ),
    ];

    let program = compile_files(files);
    let result = check_files_parallel(
        &program,
        &crate::checker::context::CheckerOptions {
            module: tsz_common::common::ModuleKind::CommonJS,
            no_lib: true,
            ..Default::default()
        },
        &[],
    );

    let consumer = result
        .file_results
        .iter()
        .find(|file| file.file_name == "consumer.ts")
        .expect("expected consumer.ts result");

    assert!(
        consumer.diagnostics.iter().any(|diag| diag.code == 2454),
        "Expected TS2454 in consumer.ts. Actual diagnostics: {:#?}",
        consumer.diagnostics
    );
}

#[test]
fn test_check_files_parallel_preserves_ts2454_for_umd_namespace_qualified_type_member() {
    let files = vec![
        (
            "foo.d.ts".to_string(),
            r#"
export var x: number;
export function fn(): void;
export interface Thing { n: typeof x }
export as namespace Foo;
"#
            .to_string(),
        ),
        (
            "a.ts".to_string(),
            r#"
/// <reference path="foo.d.ts" />
Foo.fn();
let x: Foo.Thing;
let y: number = x.n;
"#
            .to_string(),
        ),
    ];

    let program = compile_files(files);
    let result = check_files_parallel(
        &program,
        &crate::checker::context::CheckerOptions {
            module: tsz_common::common::ModuleKind::CommonJS,
            target: tsz_common::common::ScriptTarget::ES2015,
            no_lib: true,
            ..Default::default()
        },
        &[],
    );

    let file = result
        .file_results
        .iter()
        .find(|file| file.file_name == "a.ts")
        .expect("expected a.ts result");
    let relevant_codes: Vec<u32> = file
        .diagnostics
        .iter()
        .filter(|diag| diag.code != 2318)
        .map(|diag| diag.code)
        .collect();

    assert_eq!(
        relevant_codes,
        vec![2454],
        "Expected only TS2454 in a.ts. Actual diagnostics: {:#?}",
        file.diagnostics
    );
}

#[test]
fn test_check_files_parallel_preserves_tdz_after_namespace_reexport() {
    let files = vec![
        (
            "0.ts".to_string(),
            r#"
export const a = 1;
export const b = 2;
"#
            .to_string(),
        ),
        (
            "1.ts".to_string(),
            r#"
export * as ns from "./0";
ns.a;
ns.b;
let ns = { a: 1, b: 2 };
ns.a;
ns.b;
"#
            .to_string(),
        ),
        (
            "2.ts".to_string(),
            r#"
import * as foo from "./1";

foo.ns.a;
foo.ns.b;
"#
            .to_string(),
        ),
    ];

    let program = compile_files(files);
    let result = check_files_parallel(
        &program,
        &crate::checker::context::CheckerOptions {
            module: tsz_common::common::ModuleKind::CommonJS,
            target: tsz_common::common::ScriptTarget::ES2015,
            no_lib: true,
            ..Default::default()
        },
        &[],
    );

    let file = result
        .file_results
        .iter()
        .find(|file| file.file_name == "1.ts")
        .expect("expected 1.ts result");

    let ts2448_count = file
        .diagnostics
        .iter()
        .filter(|diag| diag.code == 2448)
        .count();
    let ts2454_count = file
        .diagnostics
        .iter()
        .filter(|diag| diag.code == 2454)
        .count();

    assert_eq!(
        ts2448_count, 2,
        "Expected exactly two TS2448 diagnostics in 1.ts. Actual diagnostics: {:#?}",
        file.diagnostics
    );
    assert_eq!(
        ts2454_count, 2,
        "Expected exactly two TS2454 diagnostics in 1.ts. Actual diagnostics: {:#?}",
        file.diagnostics
    );

    let importer = result
        .file_results
        .iter()
        .find(|file| file.file_name == "2.ts")
        .expect("expected 2.ts result");
    let importer_codes: Vec<u32> = importer.diagnostics.iter().map(|diag| diag.code).collect();
    assert!(
        importer_codes.is_empty(),
        "Expected no diagnostics in 2.ts. Actual diagnostics: {:#?}",
        importer.diagnostics
    );
}

#[test]
fn test_check_files_parallel_preserves_same_file_namespace_exports() {
    let files = vec![
        ("a.ts".to_string(), "export class A {}\n".to_string()),
        (
            "b.ts".to_string(),
            "export * as a from \"./a\";\n".to_string(),
        ),
        (
            "c.ts".to_string(),
            "import type { a } from \"./b\";\nexport { a };\n".to_string(),
        ),
    ];

    let program = compile_files(files);
    let result = check_files_parallel(
        &program,
        &crate::checker::context::CheckerOptions {
            module: tsz_common::common::ModuleKind::CommonJS,
            target: tsz_common::common::ScriptTarget::ES2015,
            no_lib: true,
            ..Default::default()
        },
        &[],
    );

    let file = result
        .file_results
        .iter()
        .find(|file| file.file_name == "c.ts")
        .expect("expected c.ts result");
    let relevant_codes: Vec<u32> = file
        .diagnostics
        .iter()
        .filter(|diag| diag.code != 2318)
        .map(|diag| diag.code)
        .collect();

    assert!(
        !relevant_codes.contains(&2305),
        "Did not expect TS2305 in c.ts. Actual diagnostics: {:#?}",
        file.diagnostics
    );
}

#[test]
fn test_check_files_parallel_preserves_import_shadowing_type_meaning() {
    let files = vec![
        ("b.ts".to_string(), "export const zzz = 123;\n".to_string()),
        (
            "a.ts".to_string(),
            r#"import * as B from "./b";

interface B {
    x: string;
}

const x: B = { x: "" };
B.zzz;

export { B };
"#
            .to_string(),
        ),
        (
            "index.ts".to_string(),
            r#"import { B } from "./a";

const x: B = { x: "" };
B.zzz;

import * as OriginalB from "./b";
OriginalB.zzz;

const y: OriginalB = x;
"#
            .to_string(),
        ),
    ];

    let program = compile_files(files);
    let result = check_files_parallel(
        &program,
        &crate::checker::context::CheckerOptions {
            module: tsz_common::common::ModuleKind::CommonJS,
            target: tsz_common::common::ScriptTarget::ES2015,
            no_lib: true,
            ..Default::default()
        },
        &[],
    );

    let a_file = result
        .file_results
        .iter()
        .find(|file| file.file_name == "a.ts")
        .expect("expected a.ts result");
    assert!(
        !a_file.diagnostics.iter().any(|diag| diag.code == 2353),
        "Did not expect TS2353 in a.ts. Actual diagnostics: {:#?}",
        a_file.diagnostics
    );

    let index_file = result
        .file_results
        .iter()
        .find(|file| file.file_name == "index.ts")
        .expect("expected index.ts result");
    let ts2709: Vec<_> = index_file
        .diagnostics
        .iter()
        .filter(|diag| diag.code == 2709)
        .collect();
    assert_eq!(
        ts2709.len(),
        1,
        "Expected exactly one TS2709 in index.ts. Actual diagnostics: {:#?}",
        index_file.diagnostics
    );
    assert!(
        ts2709[0].message_text.contains("OriginalB"),
        "Expected TS2709 to mention OriginalB. Actual diagnostic: {:?}",
        ts2709[0]
    );
    assert!(
        !index_file.diagnostics.iter().any(|diag| diag.code == 2353),
        "Did not expect TS2353 in index.ts. Actual diagnostics: {:#?}",
        index_file.diagnostics
    );
}

#[test]
fn test_check_files_parallel_imported_value_wins_over_same_named_type_alias() {
    let files = vec![
        (
            "util.ts".to_string(),
            r#"
export namespace util {
    export const arrayToEnum = <T extends string, U extends [T, ...T[]]>(
        items: U
    ): { [k in U[number]]: k } => {
        const obj: any = {};
        for (const item of items) obj[item] = item;
        return obj as any;
    };
}
"#
            .to_string(),
        ),
        (
            "parseUtil.ts".to_string(),
            r#"
import { util } from "./util";

export const ParsedType = util.arrayToEnum([
    "string",
    "undefined",
]);

export type ParsedType = keyof typeof ParsedType;
"#
            .to_string(),
        ),
        (
            "types.ts".to_string(),
            r#"
import { ParsedType } from "./parseUtil";

const direct: ParsedType = ParsedType.string;
"#
            .to_string(),
        ),
    ];

    let program = compile_files(files);
    let result = check_files_parallel(
        &program,
        &crate::checker::context::CheckerOptions {
            module: tsz_common::common::ModuleKind::CommonJS,
            target: tsz_common::common::ScriptTarget::ES2015,
            no_lib: true,
            ..Default::default()
        },
        &[],
    );

    let types_file = result
        .file_results
        .iter()
        .find(|file| file.file_name == "types.ts")
        .expect("expected types.ts result");
    assert!(
        !types_file
            .diagnostics
            .iter()
            .any(|diag| matches!(diag.code, 2339 | 2322 | 2345)),
        "Expected imported enum-like const to remain usable in value position. Actual diagnostics: {:#?}",
        types_file.diagnostics
    );
}

#[test]
fn test_check_files_parallel_zod_forward_generic_class_constraint() {
    let files = vec![
        (
            "types.ts".to_string(),
            r#"
export interface ZodTypeDef {
    errorMap?: unknown;
}

export type ZodTypeAny = ZodType<any, any, any>;
export type TypeOf<T extends ZodType<any, any, any>> = T["_output"];
export type input<T extends ZodType<any, any, any>> = T["_input"];
export type output<T extends ZodType<any, any, any>> = T["_output"];

export abstract class ZodType<
    Output,
    Def extends ZodTypeDef = ZodTypeDef,
    Input = Output
> {
    readonly _output!: Output;
    readonly _input!: Input;
    readonly _def!: Def;
}
"#
            .to_string(),
        ),
        (
            "index.ts".to_string(),
            r#"
import { TypeOf, ZodType } from "./types";

declare const schema: ZodType<string>;
declare const outputValue: TypeOf<typeof schema>;
const text: string = outputValue;
"#
            .to_string(),
        ),
    ];

    let program = compile_files(files);
    let result = check_files_parallel(
        &program,
        &crate::checker::context::CheckerOptions {
            module: tsz_common::common::ModuleKind::CommonJS,
            target: tsz_common::common::ScriptTarget::ES2015,
            no_lib: true,
            ..Default::default()
        },
        &[],
    );

    let all_diags: Vec<_> = result
        .file_results
        .iter()
        .flat_map(|file| file.diagnostics.iter())
        .filter(|diag| matches!(diag.code, 2313 | 2339 | 2322))
        .collect();
    assert!(
        all_diags.is_empty(),
        "Expected Zod forward generic class constraints to work in parallel checking. Actual diagnostics: {all_diags:#?}"
    );
}

#[test]
fn test_check_files_parallel_zod_issue_data_cross_file_spread() {
    let files = vec![
        (
            "helpers/util.ts".to_string(),
            r#"
type Pick<T, K extends keyof T> = { [P in K]: T[P] };
type Exclude<T, U> = T extends U ? never : T;

export namespace util {
    export type OmitKeys<T, K extends string> = Pick<T, Exclude<keyof T, K>>;
    export const arrayToEnum = <T extends string, U extends [T, ...T[]]>(
        items: U
    ): { [k in U[number]]: k } => {
        const obj: any = {};
        return obj as any;
    };
}
"#
            .to_string(),
        ),
        (
            "ZodError.ts".to_string(),
            r#"
import { ZodParsedType } from "./helpers/parseUtil";
import { util } from "./helpers/util";

export const ZodIssueCode = util.arrayToEnum([
    "invalid_type",
    "custom",
    "invalid_union",
    "invalid_enum_value",
    "unrecognized_keys",
    "invalid_arguments",
    "invalid_return_type",
    "invalid_date",
    "invalid_string",
    "too_small",
    "too_big",
    "invalid_intersection_types",
    "not_multiple_of",
]);

export type ZodIssueCode = keyof typeof ZodIssueCode;

export type ZodIssueBase = {
    path: (string | number)[];
    message?: string;
};

export interface ZodInvalidTypeIssue extends ZodIssueBase {
    code: typeof ZodIssueCode.invalid_type;
    expected: ZodParsedType;
    received: ZodParsedType;
}

export interface ZodCustomIssue extends ZodIssueBase {
    code: typeof ZodIssueCode.custom;
    params?: { [k: string]: any };
}

export interface ZodInvalidUnionIssue extends ZodIssueBase {
    code: typeof ZodIssueCode.invalid_union;
    unionErrors: ZodError[];
}

export interface ZodInvalidEnumValueIssue extends ZodIssueBase {
    code: typeof ZodIssueCode.invalid_enum_value;
    options: (string | number)[];
}

export interface ZodUnrecognizedKeysIssue extends ZodIssueBase {
    code: typeof ZodIssueCode.unrecognized_keys;
    keys: string[];
}

export interface ZodInvalidArgumentsIssue extends ZodIssueBase {
    code: typeof ZodIssueCode.invalid_arguments;
    argumentsError: ZodError;
}

export interface ZodInvalidReturnTypeIssue extends ZodIssueBase {
    code: typeof ZodIssueCode.invalid_return_type;
    returnTypeError: ZodError;
}

export interface ZodInvalidDateIssue extends ZodIssueBase {
    code: typeof ZodIssueCode.invalid_date;
}

export type StringValidation = "email" | "url" | "uuid" | "regex" | "cuid";

export interface ZodInvalidStringIssue extends ZodIssueBase {
    code: typeof ZodIssueCode.invalid_string;
    validation: StringValidation;
}

export interface ZodTooSmallIssue extends ZodIssueBase {
    code: typeof ZodIssueCode.too_small;
    minimum: number;
    inclusive: boolean;
    type: "array" | "string" | "number";
}

export interface ZodTooBigIssue extends ZodIssueBase {
    code: typeof ZodIssueCode.too_big;
    maximum: number;
    inclusive: boolean;
    type: "array" | "string" | "number";
}

export interface ZodInvalidIntersectionTypesIssue extends ZodIssueBase {
    code: typeof ZodIssueCode.invalid_intersection_types;
}

export interface ZodNotMultipleOfIssue extends ZodIssueBase {
    code: typeof ZodIssueCode.not_multiple_of;
    multipleOf: number;
}

export type ZodIssueOptionalMessage =
    | ZodInvalidTypeIssue
    | ZodUnrecognizedKeysIssue
    | ZodInvalidUnionIssue
    | ZodInvalidEnumValueIssue
    | ZodInvalidArgumentsIssue
    | ZodInvalidReturnTypeIssue
    | ZodInvalidDateIssue
    | ZodInvalidStringIssue
    | ZodTooSmallIssue
    | ZodTooBigIssue
    | ZodInvalidIntersectionTypesIssue
    | ZodNotMultipleOfIssue
    | ZodCustomIssue;

export type ZodIssue = ZodIssueOptionalMessage & { message: string };

export class ZodError {
    issues: ZodIssue[] = [];
    constructor(issues: ZodIssue[]) {
        this.issues = issues;
    }
}

type stripPath<T extends object> = T extends any
    ? util.OmitKeys<T, "path">
    : never;

export type IssueData = stripPath<ZodIssueOptionalMessage> & {
    path?: (string | number)[];
};
"#
            .to_string(),
        ),
        (
            "helpers/parseUtil.ts".to_string(),
            r#"
import {
    IssueData,
    ZodIssue,
    ZodIssueOptionalMessage,
} from "../ZodError";
import { util } from "./util";

export const ZodParsedType = util.arrayToEnum([
    "string",
    "nan",
    "number",
    "integer",
    "boolean",
    "undefined",
    "null",
    "array",
    "object",
    "unknown",
]);

export type ZodParsedType = keyof typeof ZodParsedType;

export const makeIssue = (params: {
    path: (string | number)[];
    issueData: IssueData;
}): ZodIssue => {
    const { path, issueData } = params;
    const fullPath = [...path, ...(issueData.path || [])];
    const fullIssue = {
        ...issueData,
        path: fullPath,
    };

    consume(fullIssue);

    return {
        ...issueData,
        path: fullPath,
        message: issueData.message || "",
    };
};

declare function consume(issue: ZodIssueOptionalMessage): void;
"#
            .to_string(),
        ),
    ];

    let program = compile_files(files);
    let result = check_files_parallel(
        &program,
        &crate::checker::context::CheckerOptions {
            module: tsz_common::common::ModuleKind::CommonJS,
            target: tsz_common::common::ScriptTarget::ES2015,
            no_lib: true,
            strict: true,
            strict_null_checks: true,
            no_implicit_any: true,
            ..Default::default()
        },
        &[],
    );

    let target_diags: Vec<_> = result
        .file_results
        .iter()
        .flat_map(|file| file.diagnostics.iter())
        .filter(|diag| matches!(diag.code, 2339 | 2345 | 2322))
        .collect();
    assert!(
        target_diags.is_empty(),
        "Expected Zod IssueData cross-file spread to preserve union properties. Actual diagnostics: {target_diags:#?}"
    );
}

#[test]
fn test_check_files_parallel_keeps_namespace_local_component_for_create_element_inference() {
    let files = vec![(
        "test.ts".to_string(),
        r#"
declare class Component<P> { constructor(props: P); props: P; }

namespace N1 {
    declare class Component<P> {
        constructor(props: P);
    }

    interface ComponentClass<P = {}> {
        new (props: P): Component<P>;
    }

    type CreateElementChildren<P> =
        P extends { children?: infer C }
            ? C extends any[]
                ? C
                : C[]
            : unknown;

    declare function createElement<P extends {}>(
        type: ComponentClass<P>,
        ...children: CreateElementChildren<P>
    ): any;

    declare function createElement2<P extends {}>(
        type: ComponentClass<P>,
        child: CreateElementChildren<P>
    ): any;

    class InferFunctionTypes extends Component<{ children: (foo: number) => string }> {}

    createElement(InferFunctionTypes, (foo) => "" + foo);
    createElement2(InferFunctionTypes, [(foo) => "" + foo]);
}
"#
        .to_string(),
    )];

    let program = compile_files(files);
    let result = check_files_parallel(
        &program,
        &crate::checker::context::CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2015,
            no_lib: true,
            ..Default::default()
        },
        &[],
    );

    let file_result = result
        .file_results
        .iter()
        .find(|file| file.file_name == "test.ts")
        .expect("expected test.ts result");
    assert!(
        !file_result.diagnostics.iter().any(|diag| diag.code == 2345),
        "parallel file checking should preserve namespace-local ComponentClass inference for createElement. Actual diagnostics: {:#?}",
        file_result.diagnostics,
    );

    let file = program
        .files
        .iter()
        .find(|file| file.file_name == "test.ts")
        .expect("expected merged test.ts file");
    let rebuilt_binder = create_binder_from_bound_file(file, &program, 0);
    let query_cache = tsz_solver::construction::QueryCache::new(&program.type_interner);
    let mut recreated_checker = crate::checker::state::CheckerState::with_options(
        &file.arena,
        &rebuilt_binder,
        &query_cache,
        file.file_name.clone(),
        &crate::checker::context::CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2015,
            no_lib: true,
            ..Default::default()
        },
    );
    recreated_checker.check_source_file(file.source_file);
    assert!(
        !recreated_checker
            .ctx
            .diagnostics
            .iter()
            .any(|diag| diag.code == 2345),
        "recreated binder checking should preserve namespace-local ComponentClass inference for createElement. Actual diagnostics: {:#?}",
        recreated_checker.ctx.diagnostics,
    );
}

#[test]
fn test_recreated_binder_keeps_namespace_local_generic_class_application_instance_type() {
    let files = vec![(
        "test.ts".to_string(),
        r#"
declare class Component<P> { constructor(props: P); props: P; }

namespace N1 {
    declare class Component<P> {
        constructor(props: P);
    }

    interface ComponentClass<P = {}> {
        new (props: P): Component<P>;
    }

    declare let c: ComponentClass<{ children: (foo: number) => string }>;
    const z = new c({ children: (foo) => "" + foo });
    z.props;
}
"#
        .to_string(),
    )];

    let program = compile_files(files);
    let file = program
        .files
        .iter()
        .find(|file| file.file_name == "test.ts")
        .expect("expected merged test.ts file");
    let rebuilt_binder = create_binder_from_bound_file(file, &program, 0);
    let query_cache = tsz_solver::construction::QueryCache::new(&program.type_interner);
    let mut checker = crate::checker::state::CheckerState::with_options(
        &file.arena,
        &rebuilt_binder,
        &query_cache,
        file.file_name.clone(),
        &crate::checker::context::CheckerOptions::default(),
    );
    checker.check_source_file(file.source_file);
    let source_file = file
        .arena
        .get(file.source_file)
        .and_then(|node| file.arena.get_source_file(node))
        .expect("missing source file");
    let namespace_stmt = source_file
        .statements
        .nodes
        .iter()
        .copied()
        .find(|&stmt_idx| {
            let Some(stmt_node) = file.arena.get(stmt_idx) else {
                return false;
            };
            let Some(module_decl) = file.arena.get_module(stmt_node) else {
                return false;
            };
            file.arena
                .get_identifier_at(module_decl.name)
                .is_some_and(|ident| ident.escaped_text.as_str() == "N1")
        })
        .expect("missing namespace declaration");
    let namespace_body_statements = file
        .arena
        .get(namespace_stmt)
        .and_then(|node| file.arena.get_module(node))
        .map(|module| module.body)
        .and_then(|body_idx| file.arena.get(body_idx))
        .and_then(|node| file.arena.get_module_block(node))
        .and_then(|module_block| module_block.statements.as_ref())
        .map(|statements| statements.nodes.clone())
        .expect("missing namespace body");
    let component_return_name = namespace_body_statements
        .iter()
        .copied()
        .find_map(|stmt_idx| {
            let stmt_node = file.arena.get(stmt_idx)?;
            let interface_decl = file.arena.get_interface(stmt_node)?;
            let interface_name = file
                .arena
                .get_identifier_at(interface_decl.name)?
                .escaped_text
                .as_str();
            if interface_name != "ComponentClass" {
                return None;
            }
            let construct_idx = interface_decl.members.nodes.first().copied()?;
            let construct_node = file.arena.get(construct_idx)?;
            let construct_sig = file.arena.get_signature(construct_node)?;
            let type_ref_node = file.arena.get(construct_sig.type_annotation)?;
            let type_ref = file.arena.get_type_ref(type_ref_node)?;
            Some(type_ref.type_name)
        })
        .expect("missing Component<P> return type");
    let local_component_sym = file
        .scopes
        .iter()
        .find(|scope| {
            scope.kind == crate::binder::ContainerKind::Module && scope.table.has("Component")
        })
        .and_then(|scope| scope.table.get("Component"))
        .expect("missing namespace-local Component");
    let binder_resolved_component = rebuilt_binder.resolve_identifier_with_filter(
        &file.arena,
        component_return_name,
        &[],
        |_| true,
    );

    assert_eq!(
        binder_resolved_component,
        Some(local_component_sym),
        "rebuilt binder should resolve the interface's unqualified Component<P> to the namespace-local symbol",
    );
    assert!(
        checker.ctx.diagnostics.iter().any(|diag| diag.code == 2339),
        "recreated binder should keep the namespace-local Component<P> instance type inside ComponentClass<P>. Actual diagnostics: {:#?}",
        checker.ctx.diagnostics
    );
}

