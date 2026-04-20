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

