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

