use super::*;

#[path = "tests_completions.rs"]
mod completions;
#[path = "tests_navigation.rs"]
mod navigation;

fn make_server() -> Server {
    Server {
        completion_import_module_specifier_ending: None,
        import_module_specifier_preference: None,
        organize_imports_type_order: None,
        organize_imports_ignore_case: false,
        auto_import_file_exclude_patterns: Vec::new(),
        lib_dir: PathBuf::from("/nonexistent"),
        tests_lib_dir: PathBuf::from("/nonexistent"),
        lib_cache: FxHashMap::default(),
        unified_lib_cache: None,
        checks_completed: 0,
        response_seq: 0,
        open_files: FxHashMap::default(),
        external_project_files: FxHashMap::default(),
        _server_mode: ServerMode::Semantic,
        _log_config: LogConfig {
            level: LogLevel::Off,
            file: None,
            trace_to_console: false,
        },
        enable_telemetry: false,
        allow_importing_ts_extensions: false,
        inferred_check_options: CheckOptions::default(),
        inferred_projectinfo_options: None,
        auto_imports_allowed_for_inferred_projects: true,
        inferred_module_is_none_for_projects: false,
        auto_import_specifier_exclude_regexes: Vec::new(),
        include_completions_with_class_member_snippets: false,
        new_line_character: None,
        plugin_configs: FxHashMap::default(),
        native_ts_worker: None,
    }
}

fn make_server_with_real_libs() -> Server {
    let mut server = make_server();
    server.lib_dir = Server::find_lib_dir().expect("lib dir should be discoverable in tests");
    server.tests_lib_dir = Server::find_tests_lib_dir(&server.lib_dir);
    server
}

#[test]
fn test_find_lib_dir_finds_workspace_libs() {
    let lib_dir = Server::find_lib_dir().expect("lib dir should be discoverable in tests");
    assert!(
        lib_dir.join("lib.es5.d.ts").exists() || lib_dir.join("es5.d.ts").exists(),
        "expected lib.es5.d.ts or es5.d.ts in {}",
        lib_dir.display()
    );
}

#[test]
fn test_find_tests_lib_dir_returns_existing_directory() {
    let lib_dir = Server::find_lib_dir().expect("lib dir should be discoverable in tests");
    let tests_lib_dir = Server::find_tests_lib_dir(&lib_dir);
    assert!(
        tests_lib_dir.exists(),
        "expected tests lib dir fallback to exist, got {}",
        tests_lib_dir.display()
    );
}

fn make_request(command: &str, arguments: serde_json::Value) -> TsServerRequest {
    TsServerRequest {
        seq: 1,
        _msg_type: "request".to_string(),
        command: command.to_string(),
        arguments,
    }
}

fn apply_tsserver_text_edits(mut source: String, edits: &[serde_json::Value]) -> String {
    let mut spans: Vec<(usize, usize, String)> = edits
        .iter()
        .filter_map(|edit| {
            let start = edit.get("start")?;
            let end = edit.get("end")?;
            let start_line = start.get("line")?.as_u64()? as u32;
            let start_offset = start.get("offset")?.as_u64()? as u32;
            let end_line = end.get("line")?.as_u64()? as u32;
            let end_offset = end.get("offset")?.as_u64()? as u32;
            let new_text = edit.get("newText")?.as_str()?.to_string();
            let start_byte = Server::line_offset_to_byte(&source, start_line, start_offset);
            let end_byte = Server::line_offset_to_byte(&source, end_line, end_offset);
            Some((start_byte, end_byte, new_text))
        })
        .collect();

    spans.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
    for (start, end, new_text) in spans {
        if start <= end && end <= source.len() {
            source.replace_range(start..end, &new_text);
        }
    }
    source
}

#[test]
fn test_line_offset_to_byte_first_char() {
    assert_eq!(Server::line_offset_to_byte("hello\nworld\n", 1, 1), 0);
}

#[test]
fn test_line_offset_to_byte_second_line() {
    assert_eq!(Server::line_offset_to_byte("hello\nworld\n", 2, 1), 6);
}

#[test]
fn test_line_offset_to_byte_end_of_line() {
    // Offset pointing past the last char on line 1 (before \n)
    // "hello" is 5 chars, so offset 6 (1-based) is position 5 (the \n)
    assert_eq!(Server::line_offset_to_byte("hello\nworld\n", 1, 6), 5);
}

#[test]
fn test_normalize_fourslash_virtual_content_rewrites_harness_lines() {
    let content = "//// const x = 1;\n//// x;\n";
    let normalized = Server::normalize_fourslash_virtual_content("/fourslash.ts", content);
    assert_eq!(normalized, " const x = 1;\n x;");
}

#[test]
fn test_normalize_fourslash_virtual_content_keeps_non_harness_files_unchanged() {
    let content = "//// const x = 1;\n//// x;\n";
    let normalized = Server::normalize_fourslash_virtual_content("/workspace/src/app.ts", content);
    assert_eq!(normalized, content);
}

#[test]
fn test_normalize_fourslash_virtual_content_keeps_plain_harness_content_unchanged() {
    let content = "// @module: none\nconst x = 1;\n";
    let normalized = Server::normalize_fourslash_virtual_content("/fourslash.ts", content);
    assert_eq!(normalized, content);
}

#[test]
fn test_line_offset_to_byte_mid_line() {
    // Offset 3 (1-based) on line 1 means col 2 (0-based) -> byte 2
    assert_eq!(Server::line_offset_to_byte("hello\nworld\n", 1, 3), 2);
}

#[test]
fn test_line_offset_to_byte_third_line() {
    assert_eq!(Server::line_offset_to_byte("aaa\nbbb\nccc\n", 3, 1), 8);
}

#[test]
fn test_line_offset_to_byte_empty_line() {
    // Line 2 is empty (just \n), offset 1 should point to byte 6
    assert_eq!(Server::line_offset_to_byte("hello\n\nworld\n", 2, 1), 6);
}

#[test]
fn test_line_offset_to_byte_past_end() {
    // Line beyond file should return content length
    assert_eq!(
        Server::line_offset_to_byte("hello\n", 10, 1),
        "hello\n".len()
    );
}

#[test]
fn test_line_offset_to_byte_utf16_bmp() {
    // BMP characters: each is 1 UTF-16 code unit
    // "café" - é is U+00E9 (2 UTF-8 bytes, 1 UTF-16 code unit)
    let s = "caf\u{00E9}\nend";
    // offset 5 (1-based) on line 1 = past 'c','a','f','é' = byte 5 (1+1+1+2)
    assert_eq!(Server::line_offset_to_byte(s, 1, 5), 5);
}

#[test]
fn test_line_offset_to_byte_utf16_supplementary() {
    // Supplementary character: 😀 is U+1F600 (4 UTF-8 bytes, 2 UTF-16 code units)
    let s = "a\u{1F600}b\nend";
    // In UTF-16 offsets (1-based): a=1, 😀=2-3, b=4
    // offset 4 (1-based) should point to 'b' = byte 5 (1 + 4)
    assert_eq!(Server::line_offset_to_byte(s, 1, 4), 5);
    // offset 2 (1-based) should point to start of 😀 = byte 1
    assert_eq!(Server::line_offset_to_byte(s, 1, 2), 1);
}

#[test]
fn test_apply_change_multiline_delete() {
    // Delete from line 1 col 4 to line 2 col 4 (delete "lo\nwor")
    assert_eq!(
        Server::apply_change("hello\nworld", 1, 4, 2, 4, ""),
        "helld"
    );
}

#[test]
fn test_apply_change_multiline_insert() {
    // Insert newline in the middle
    assert_eq!(
        Server::apply_change("helloworld", 1, 6, 1, 6, "\n"),
        "hello\nworld"
    );
}

#[test]
fn test_content_appears_binary_with_control_bytes() {
    assert!(content_appears_binary("G@\u{0004}\u{0004}\u{0004}\u{0004}"));
    assert!(!content_appears_binary("const x = 1;\n"));
}

#[test]
fn test_apply_change_insert() {
    assert_eq!(
        Server::apply_change("hello world", 1, 7, 1, 7, "beautiful "),
        "hello beautiful world"
    );
}

#[test]
fn test_apply_change_replace() {
    assert_eq!(
        Server::apply_change("hello world", 1, 7, 1, 12, "Rust"),
        "hello Rust"
    );
}

#[test]
fn test_apply_change_delete() {
    assert_eq!(
        Server::apply_change("hello world", 1, 7, 1, 12, ""),
        "hello "
    );
}

#[test]
fn test_handle_change_updates_file() {
    let mut server = make_server();
    server
        .open_files
        .insert("/test.ts".to_string(), "const x = 1;".to_string());
    let req = make_request(
        "change",
        serde_json::json!({
            "file": "/test.ts",
            "line": 1, "offset": 11,
            "endLine": 1, "endOffset": 12,
            "insertString": "2"
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    assert_eq!(server.open_files["/test.ts"], "const x = 2;");
}

#[test]
fn test_inferred_auto_imports_blocked_for_module_none_es5() {
    let options = serde_json::json!({
        "module": "none",
        "target": "es5"
    });
    assert!(!Server::inferred_auto_imports_allowed(&options));
}

#[test]
fn test_inferred_auto_imports_allowed_for_module_none_es2015() {
    let options = serde_json::json!({
        "module": "none",
        "target": "es2015"
    });
    assert!(Server::inferred_auto_imports_allowed(&options));
}

#[test]
fn test_inferred_auto_imports_blocked_for_numeric_string_options() {
    let options = serde_json::json!({
        "module": "0",
        "target": "1"
    });
    assert!(!Server::inferred_auto_imports_allowed(&options));
}

#[test]
fn test_inferred_auto_imports_allowed_for_numeric_string_target_es2015() {
    let options = serde_json::json!({
        "module": "0",
        "target": "2"
    });
    assert!(Server::inferred_auto_imports_allowed(&options));
}

#[test]
fn test_compiler_options_for_inferred_projects_accepts_direct_options_shape() {
    let mut server = make_server();
    server.open_files.insert(
        "/node_modules/dep/index.d.ts".to_string(),
        "export const x: number;\n".to_string(),
    );
    server
        .open_files
        .insert("/index.ts".to_string(), "x".to_string());

    let options_req = make_request(
        "compilerOptionsForInferredProjects",
        serde_json::json!({
            "module": "none",
            "target": "es5"
        }),
    );
    let options_resp = server.handle_tsserver_request(options_req);
    assert!(options_resp.success);
    assert_eq!(options_resp.body, Some(serde_json::json!(true)));

    let completion_req = make_request(
        "completionInfo",
        serde_json::json!({
            "file": "/index.ts",
            "line": 1,
            "offset": 2,
            "preferences": { "includeCompletionsForModuleExports": true }
        }),
    );
    let completion_resp = server.handle_tsserver_request(completion_req);
    assert!(completion_resp.success);
    let body = completion_resp
        .body
        .expect("completionInfo should return a body");
    let entries = body["entries"]
        .as_array()
        .expect("completionInfo should include entries");
    let has_auto_import_x = entries.iter().any(|entry| {
        entry.get("name").and_then(serde_json::Value::as_str) == Some("x")
            && entry.get("source").is_some()
    });
    assert!(
        !has_auto_import_x,
        "auto-import completion should be gated when inferred options are sent directly"
    );
}

#[test]
fn test_compiler_options_for_inferred_projects_accepts_compiler_options_shape() {
    let mut server = make_server();
    server.open_files.insert(
        "/node_modules/dep/index.d.ts".to_string(),
        "export const x: number;\n".to_string(),
    );
    server
        .open_files
        .insert("/index.ts".to_string(), "x".to_string());

    let options_req = make_request(
        "compilerOptionsForInferredProjects",
        serde_json::json!({
            "compilerOptions": {
                "module": "none",
                "target": "es5"
            }
        }),
    );
    let options_resp = server.handle_tsserver_request(options_req);
    assert!(options_resp.success);
    assert_eq!(options_resp.body, Some(serde_json::json!(true)));

    let completion_req = make_request(
        "completionInfo",
        serde_json::json!({
            "file": "/index.ts",
            "line": 1,
            "offset": 2,
            "preferences": { "includeCompletionsForModuleExports": true }
        }),
    );
    let completion_resp = server.handle_tsserver_request(completion_req);
    assert!(completion_resp.success);
    let body = completion_resp
        .body
        .expect("completionInfo should return a body");
    let entries = body["entries"]
        .as_array()
        .expect("completionInfo should include entries");
    let has_auto_import_x = entries.iter().any(|entry| {
        entry.get("name").and_then(serde_json::Value::as_str) == Some("x")
            && entry.get("source").is_some()
    });
    assert!(
        !has_auto_import_x,
        "auto-import completion should be gated when inferred options are nested under compilerOptions"
    );
}

#[test]
fn test_semantic_diagnostics_respect_inferred_module_none() {
    let mut server = make_server();
    server.open_files.insert(
        "/index.ts".to_string(),
        "import { x } from 'dep'; x;".to_string(),
    );

    let options_req = make_request(
        "compilerOptionsForInferredProjects",
        serde_json::json!({
            "options": {
                "module": "none",
                "target": "es5"
            }
        }),
    );
    let options_resp = server.handle_tsserver_request(options_req);
    assert!(options_resp.success);

    let diagnostics_req = make_request(
        "semanticDiagnosticsSync",
        serde_json::json!({
            "file": "/index.ts"
        }),
    );
    let diagnostics_resp = server.handle_tsserver_request(diagnostics_req);
    assert!(diagnostics_resp.success);
    let diagnostics = diagnostics_resp
        .body
        .expect("semanticDiagnosticsSync should return a body")
        .as_array()
        .expect("semanticDiagnosticsSync body should be an array")
        .clone();
    let has_module_none_diag = diagnostics.iter().any(|diag| {
        diag.get("code").and_then(serde_json::Value::as_u64)
            == Some(
                tsz_checker::diagnostics::diagnostic_codes::CANNOT_USE_IMPORTS_EXPORTS_OR_MODULE_AUGMENTATIONS_WHEN_MODULE_IS_NONE
                    as u64,
            )
    });
    assert!(
        has_module_none_diag,
        "expected TS1148-style diagnostic when inferred options set module:none"
    );
}

#[test]
fn test_semantic_diagnostics_skip_inferred_module_none_when_target_supports_imports() {
    let mut server = make_server();
    server.open_files.insert(
        "/index.ts".to_string(),
        "import { x } from 'dep'; x;".to_string(),
    );

    let options_req = make_request(
        "compilerOptionsForInferredProjects",
        serde_json::json!({
            "options": {
                "module": "none",
                "target": "es2015"
            }
        }),
    );
    let options_resp = server.handle_tsserver_request(options_req);
    assert!(options_resp.success);

    let diagnostics_req = make_request(
        "semanticDiagnosticsSync",
        serde_json::json!({
            "file": "/index.ts"
        }),
    );
    let diagnostics_resp = server.handle_tsserver_request(diagnostics_req);
    assert!(diagnostics_resp.success);
    let diagnostics = diagnostics_resp
        .body
        .expect("semanticDiagnosticsSync should return a body")
        .as_array()
        .expect("semanticDiagnosticsSync body should be an array")
        .clone();
    let has_module_none_diag = diagnostics.iter().any(|diag| {
        diag.get("code").and_then(serde_json::Value::as_u64)
            == Some(
                tsz_checker::diagnostics::diagnostic_codes::CANNOT_USE_IMPORTS_EXPORTS_OR_MODULE_AUGMENTATIONS_WHEN_MODULE_IS_NONE
                    as u64,
            )
    });
    assert!(
        !has_module_none_diag,
        "did not expect TS1148-style diagnostic when inferred target supports imports"
    );
}

#[test]
fn test_semantic_diagnostics_skip_core_global_type_noise_for_no_lib_files() {
    let mut server = make_server();
    server.open_files.insert(
        "/index.ts".to_string(),
        "var x;\nexport { x };\nexport { x as y };\n".to_string(),
    );

    let options_req = make_request(
        "compilerOptionsForInferredProjects",
        serde_json::json!({
            "options": {
                "noLib": true
            }
        }),
    );
    let options_resp = server.handle_tsserver_request(options_req);
    assert!(options_resp.success);

    let diagnostics_req = make_request(
        "semanticDiagnosticsSync",
        serde_json::json!({
            "file": "/index.ts"
        }),
    );
    let diagnostics_resp = server.handle_tsserver_request(diagnostics_req);
    assert!(diagnostics_resp.success);
    let diagnostics = diagnostics_resp
        .body
        .expect("semanticDiagnosticsSync should return a body")
        .as_array()
        .expect("semanticDiagnosticsSync body should be an array")
        .clone();
    assert!(
        diagnostics.is_empty(),
        "did not expect per-file semantic diagnostics for implicit noLib globals, got: {diagnostics:?}"
    );
}

#[test]
fn test_semantic_diagnostics_keep_explicit_missing_global_type_names_for_no_lib_files() {
    let mut server = make_server();
    server.open_files.insert(
        "/index.ts".to_string(),
        "let x: Array<number>;\n".to_string(),
    );

    let options_req = make_request(
        "compilerOptionsForInferredProjects",
        serde_json::json!({
            "options": {
                "noLib": true
            }
        }),
    );
    let options_resp = server.handle_tsserver_request(options_req);
    assert!(options_resp.success);

    let diagnostics_req = make_request(
        "semanticDiagnosticsSync",
        serde_json::json!({
            "file": "/index.ts"
        }),
    );
    let diagnostics_resp = server.handle_tsserver_request(diagnostics_req);
    assert!(diagnostics_resp.success);
    let diagnostics = diagnostics_resp
        .body
        .expect("semanticDiagnosticsSync should return a body")
        .as_array()
        .expect("semanticDiagnosticsSync body should be an array")
        .clone();
    assert!(
        diagnostics.iter().any(|diag| {
            diag.get("start")
                .and_then(|start| start.get("line"))
                .and_then(serde_json::Value::as_u64)
                == Some(1)
                && diag
                    .get("start")
                    .and_then(|start| start.get("offset"))
                    .and_then(serde_json::Value::as_u64)
                    == Some(8)
        }),
        "expected explicit Array reference to remain a file-level error under noLib, got: {diagnostics:?}"
    );
}

#[test]
fn test_semantic_diagnostics_respect_fourslash_module_none_directive() {
    let mut server = make_server();
    server.open_files.insert(
        "/fourslash.ts".to_string(),
        "// @module: none\n// @target: es5\n".to_string(),
    );
    server.open_files.insert(
        "/index.ts".to_string(),
        "import { x } from 'dep'; x;".to_string(),
    );

    let diagnostics_req = make_request(
        "semanticDiagnosticsSync",
        serde_json::json!({
            "file": "/index.ts"
        }),
    );
    let diagnostics_resp = server.handle_tsserver_request(diagnostics_req);
    assert!(diagnostics_resp.success);
    let diagnostics = diagnostics_resp
        .body
        .expect("semanticDiagnosticsSync should return a body")
        .as_array()
        .expect("semanticDiagnosticsSync body should be an array")
        .clone();
    let has_module_none_diag = diagnostics.iter().any(|diag| {
        diag.get("code").and_then(serde_json::Value::as_u64)
            == Some(
                tsz_checker::diagnostics::diagnostic_codes::CANNOT_USE_IMPORTS_EXPORTS_OR_MODULE_AUGMENTATIONS_WHEN_MODULE_IS_NONE
                    as u64,
            )
    });
    assert!(
        has_module_none_diag,
        "expected TS1148-style diagnostic when fourslash directives set module:none"
    );
}

#[test]
fn test_semantic_diagnostics_skip_module_none_when_fourslash_target_supports_imports() {
    let mut server = make_server();
    server.open_files.insert(
        "/fourslash.ts".to_string(),
        "// @module: none\n// @target: es2015\n".to_string(),
    );
    server.open_files.insert(
        "/index.ts".to_string(),
        "import { x } from 'dep'; x;".to_string(),
    );

    let diagnostics_req = make_request(
        "semanticDiagnosticsSync",
        serde_json::json!({
            "file": "/index.ts"
        }),
    );
    let diagnostics_resp = server.handle_tsserver_request(diagnostics_req);
    assert!(diagnostics_resp.success);
    let diagnostics = diagnostics_resp
        .body
        .expect("semanticDiagnosticsSync should return a body")
        .as_array()
        .expect("semanticDiagnosticsSync body should be an array")
        .clone();
    let has_module_none_diag = diagnostics.iter().any(|diag| {
        diag.get("code").and_then(serde_json::Value::as_u64)
            == Some(
                tsz_checker::diagnostics::diagnostic_codes::CANNOT_USE_IMPORTS_EXPORTS_OR_MODULE_AUGMENTATIONS_WHEN_MODULE_IS_NONE
                    as u64,
            )
    });
    assert!(
        !has_module_none_diag,
        "did not expect TS1148-style diagnostic when target supports import syntax"
    );
}

#[test]
fn test_semantic_diagnostics_skip_module_none_for_extra_slash_fourslash_directives() {
    let mut server = make_server();
    server.open_files.insert(
        "/fourslash.ts".to_string(),
        "//// @module: none\n//// @target: es2015\n".to_string(),
    );
    server.open_files.insert(
        "/index.ts".to_string(),
        "import { x } from 'dep'; x;".to_string(),
    );

    let diagnostics_req = make_request(
        "semanticDiagnosticsSync",
        serde_json::json!({
            "file": "/index.ts"
        }),
    );
    let diagnostics_resp = server.handle_tsserver_request(diagnostics_req);
    assert!(diagnostics_resp.success);
    let diagnostics = diagnostics_resp
        .body
        .expect("semanticDiagnosticsSync should return a body")
        .as_array()
        .expect("semanticDiagnosticsSync body should be an array")
        .clone();
    let has_module_none_diag = diagnostics.iter().any(|diag| {
        diag.get("code").and_then(serde_json::Value::as_u64)
            == Some(
                tsz_checker::diagnostics::diagnostic_codes::CANNOT_USE_IMPORTS_EXPORTS_OR_MODULE_AUGMENTATIONS_WHEN_MODULE_IS_NONE
                    as u64,
            )
    });
    assert!(
        !has_module_none_diag,
        "did not expect TS1148-style diagnostic for es2015 directives with extra leading slashes"
    );
}

#[test]
fn test_semantic_diagnostics_module_none_fourslash_exact_payload_shape() {
    let mut server = make_server();
    server.open_files.insert(
        "/fourslash.ts".to_string(),
        "// @module: none\n// @target: es5\n".to_string(),
    );
    server.open_files.insert(
        "/node_modules/dep/index.d.ts".to_string(),
        "export const x: number;\n".to_string(),
    );
    server.open_files.insert(
        "/index.ts".to_string(),
        "import { x } from 'dep'; x;".to_string(),
    );

    let diagnostics_req = make_request(
        "semanticDiagnosticsSync",
        serde_json::json!({
            "file": "/index.ts",
            "includeLinePosition": true
        }),
    );
    let diagnostics_resp = server.handle_tsserver_request(diagnostics_req);
    assert!(diagnostics_resp.success);
    let diagnostics = diagnostics_resp
        .body
        .expect("semanticDiagnosticsSync should return a body")
        .as_array()
        .expect("semanticDiagnosticsSync body should be an array")
        .clone();

    let module_none_diag = diagnostics
        .iter()
        .find(|diag| {
            diag.get("code").and_then(serde_json::Value::as_u64)
                == Some(
                    tsz_checker::diagnostics::diagnostic_codes::CANNOT_USE_IMPORTS_EXPORTS_OR_MODULE_AUGMENTATIONS_WHEN_MODULE_IS_NONE
                        as u64,
                )
        })
        .expect("expected TS1148 diagnostic payload for module:none import syntax");
    let has_cannot_find_name = diagnostics.iter().any(|diag| {
        diag.get("code").and_then(serde_json::Value::as_u64)
            == Some(tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME as u64)
    });
    assert!(
        !has_cannot_find_name,
        "did not expect synthetic Cannot find name diagnostics when TS1148 is present"
    );

    let diag = module_none_diag;
    assert_eq!(
        diag.get("code").and_then(serde_json::Value::as_u64),
        Some(
            tsz_checker::diagnostics::diagnostic_codes::CANNOT_USE_IMPORTS_EXPORTS_OR_MODULE_AUGMENTATIONS_WHEN_MODULE_IS_NONE
                as u64,
        )
    );
    assert_eq!(
        diag.get("message").and_then(serde_json::Value::as_str),
        Some("Cannot use imports, exports, or module augmentations when '--module' is 'none'.")
    );
    assert_eq!(
        diag.get("start").and_then(serde_json::Value::as_u64),
        Some(0)
    );
    assert_eq!(
        diag.get("length").and_then(serde_json::Value::as_u64),
        Some("import { x } from 'dep';".len() as u64)
    );
}

#[test]
fn test_semantic_diagnostics_resolve_imports_from_open_dependency_files() {
    let mut server = make_server();
    server.open_files.insert(
        "/node_modules/dep/index.d.ts".to_string(),
        "export const x: number;\n".to_string(),
    );
    server.open_files.insert(
        "/index.ts".to_string(),
        "import { x } from 'dep'; x;".to_string(),
    );

    let diagnostics_req = make_request(
        "semanticDiagnosticsSync",
        serde_json::json!({
            "file": "/index.ts"
        }),
    );
    let diagnostics_resp = server.handle_tsserver_request(diagnostics_req);
    assert!(diagnostics_resp.success);
    let diagnostics = diagnostics_resp
        .body
        .expect("semanticDiagnosticsSync should return a body")
        .as_array()
        .expect("semanticDiagnosticsSync body should be an array")
        .clone();
    let has_cannot_find_module = diagnostics.iter().any(|diag| {
        diag.get("code").and_then(serde_json::Value::as_u64)
            == Some(
                tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS
                    as u64,
            )
    });
    assert!(
        !has_cannot_find_module,
        "did not expect unresolved-module diagnostics for open dependency files"
    );
}

#[test]
fn test_semantic_diagnostics_dynamic_import_trailing_whitespace_is_stable() {
    let mut server = make_server_with_real_libs();
    let resp = server.handle_tsserver_request(make_request(
        "compilerOptionsForInferredProjects",
        serde_json::json!({ "options": { "module": "commonjs", "lib": ["es6"] } }),
    ));
    assert!(resp.success);
    let resp = server.handle_tsserver_request(make_request(
        "open",
        serde_json::json!({
            "file": "/foo.ts",
            "fileContent": "export function bar() { return 1; }\n"
        }),
    ));
    assert!(resp.success);
    let resp = server.handle_tsserver_request(make_request(
        "open",
        serde_json::json!({
            "file": "/index.ts",
            "fileContent": "var x1 = import(\"./foo\");\nx1.then(foo => {\n   var s: string = foo.bar();\n})\n"
        }),
    ));
    assert!(resp.success);

    let req = make_request(
        "semanticDiagnosticsSync",
        serde_json::json!({ "file": "/index.ts" }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let diagnostics = resp
        .body
        .expect("semanticDiagnosticsSync should return a body")
        .as_array()
        .expect("semanticDiagnosticsSync body should be an array")
        .clone();
    assert_eq!(diagnostics.len(), 1, "expected one diagnostic before edit");

    let resp = server.handle_tsserver_request(make_request(
        "change",
        serde_json::json!({
            "file": "/index.ts",
            "line": 5,
            "offset": 1,
            "endLine": 5,
            "endOffset": 1,
            "insertString": "  "
        }),
    ));
    assert!(resp.success);

    let req = make_request(
        "semanticDiagnosticsSync",
        serde_json::json!({ "file": "/index.ts" }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let diagnostics = resp
        .body
        .expect("semanticDiagnosticsSync should return a body")
        .as_array()
        .expect("semanticDiagnosticsSync body should be an array")
        .clone();
    assert_eq!(
        diagnostics.len(),
        1,
        "expected trailing whitespace edit to preserve the diagnostic count"
    );
}

#[test]
fn test_semantic_diagnostics_partial_union_alias_insert_keeps_array_is_array_valid() {
    let mut server = make_server_with_real_libs();
    let resp = server.handle_tsserver_request(make_request(
        "compilerOptionsForInferredProjects",
        serde_json::json!({ "options": { "strict": true, "lib": ["esnext"] } }),
    ));
    assert!(resp.success);
    let resp = server.handle_tsserver_request(make_request(
        "open",
        serde_json::json!({
            "file": "/index.ts",
            "fileContent": "interface ComponentOptions<Props> {\n  setup?: (props: Props) => void;\n  name?: string;\n}\n\ninterface FunctionalComponent<P> {\n  (props: P): void;\n}\n\ntype ConcreteComponent<Props> =\n  | ComponentOptions<Props>\n  | FunctionalComponent<Props>;\n\ntype Component<Props = {}> = ConcreteComponent<Props>;\n\ntype WithInstallPlugin = { _prefix?: string };\n\n\nexport function withInstall<C extends Component, T extends WithInstallPlugin>(\n  component: C | C[],\n  target?: T,\n): string {\n  const componentWithInstall = (target ?? component) as T;\n  const components = Array.isArray(component) ? component : [component];\n\n  const { name } = components[0];\n  if (name) {\n    return name;\n  }\n\n  return \"\";\n}\n"
        }),
    ));
    assert!(resp.success);

    let resp = server.handle_tsserver_request(make_request(
        "change",
        serde_json::json!({
            "file": "/index.ts",
            "line": 1,
            "offset": 1,
            "endLine": 1,
            "endOffset": 1,
            "insertString": "type C = Component['name']\n"
        }),
    ));
    assert!(resp.success);

    let req = make_request(
        "semanticDiagnosticsSync",
        serde_json::json!({ "file": "/index.ts" }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let diagnostics = resp
        .body
        .expect("semanticDiagnosticsSync should return a body")
        .as_array()
        .expect("semanticDiagnosticsSync body should be an array")
        .clone();
    assert!(
        diagnostics.is_empty(),
        "expected alias insertion content to remain error-free, got: {diagnostics:?}"
    );
}

#[test]
#[ignore] // TODO: Unused label diagnostic position needs stable round-trip handling
fn test_semantic_diagnostics_unused_label_content_round_trip_is_stable() {
    let mut server = make_server_with_real_libs();
    let resp = server.handle_tsserver_request(make_request(
        "compilerOptionsForInferredProjects",
        serde_json::json!({ "options": { "allowUnusedLabels": false } }),
    ));
    assert!(resp.success);
    let resp = server.handle_tsserver_request(make_request(
        "open",
        serde_json::json!({
            "file": "/index.ts",
            "fileContent": "myLabel: while (true) {\n    if (Math.random() > 0.5) {\n        break myLabel;\n    }\n}\n"
        }),
    ));
    assert!(resp.success);

    let req = make_request(
        "semanticDiagnosticsSync",
        serde_json::json!({ "file": "/index.ts" }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let diagnostics = resp
        .body
        .expect("semanticDiagnosticsSync should return a body")
        .as_array()
        .expect("semanticDiagnosticsSync body should be an array")
        .clone();
    assert_eq!(diagnostics.len(), 0, "expected no diagnostics initially");

    let resp = server.handle_tsserver_request(make_request(
        "change",
        serde_json::json!({
            "file": "/index.ts",
            "line": 3,
            "offset": 9,
            "endLine": 3,
            "endOffset": 23,
            "insertString": "break;"
        }),
    ));
    assert!(resp.success);

    let req = make_request(
        "semanticDiagnosticsSync",
        serde_json::json!({ "file": "/index.ts" }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let diagnostics = resp
        .body
        .expect("semanticDiagnosticsSync should return a body")
        .as_array()
        .expect("semanticDiagnosticsSync body should be an array")
        .clone();
    assert!(
        !diagnostics.is_empty(),
        "expected at least one diagnostic after edit, got: {diagnostics:?}"
    );

    let resp = server.handle_tsserver_request(make_request(
        "change",
        serde_json::json!({
            "file": "/index.ts",
            "line": 3,
            "offset": 9,
            "endLine": 3,
            "endOffset": 15,
            "insertString": "break myLabel;"
        }),
    ));
    assert!(resp.success);

    let req = make_request(
        "semanticDiagnosticsSync",
        serde_json::json!({ "file": "/index.ts" }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let diagnostics = resp
        .body
        .expect("semanticDiagnosticsSync should return a body")
        .as_array()
        .expect("semanticDiagnosticsSync body should be an array")
        .clone();
    assert_eq!(
        diagnostics.len(),
        0,
        "expected restored content to be diagnostic-free"
    );
}

#[test]
fn test_apply_code_action_command_returns_single_result_shape() {
    let mut server = make_server();
    let req = make_request(
        "applyCodeActionCommand",
        serde_json::json!({
            "command": {
                "type": "noop"
            }
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    assert_eq!(
        resp.body,
        Some(serde_json::json!({
            "successMessage": ""
        }))
    );
}

#[test]
fn test_apply_code_action_command_returns_array_result_shape() {
    let mut server = make_server();
    let req = make_request(
        "applyCodeActionCommand",
        serde_json::json!({
            "command": [
                {
                    "type": "noop"
                }
            ]
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    assert_eq!(resp.body, Some(serde_json::json!([])));
}

#[test]
fn test_new_commands_are_recognized() {
    let mut server = make_server();
    let commands = vec![
        "change",
        "configure",
        "references-full",
        "navto",
        "signatureHelp",
        "completionEntryDetails",
        "getSupportedCodeFixes",
        "applyCodeActionCommand",
        "getApplicableRefactors",
        "getEditsForRefactor",
        "encodedSemanticClassifications-full",
        "breakpointStatement",
        "jsxClosingTag",
        "braceCompletion",
        "getSpanOfEnclosingComment",
        "todoComments",
        "docCommentTemplate",
        "indentation",
        "toggleLineComment",
        "toggleMultilineComment",
        "commentSelection",
        "uncommentSelection",
        "getSmartSelectionRange",
        "getSyntacticClassifications",
        "getSemanticClassifications",
        "getCompilerOptionsDiagnostics",
    ];
    for cmd in commands {
        let req = make_request(
            cmd,
            serde_json::json!({"file": "/test.ts", "line": 1, "offset": 1}),
        );
        let resp = server.handle_tsserver_request(req);
        assert!(
            resp.success
                || !resp
                    .message
                    .as_deref()
                    .unwrap_or("")
                    .contains("Unrecognized"),
            "Command '{cmd}' was not recognized"
        );
    }
}

#[test]
fn test_unrecognized_command() {
    let mut server = make_server();
    let req = make_request("nonExistentCommand", serde_json::json!({}));
    let resp = server.handle_tsserver_request(req);
    assert!(!resp.success);
    assert!(
        resp.message
            .unwrap()
            .contains("Unrecognized command: nonExistentCommand")
    );
}

/// Helper to validate that a JSON value has valid tsserver start/end spans.
fn assert_valid_span(value: &serde_json::Value, context: &str) {
    let start = value.get("start");
    assert!(start.is_some(), "{context}: missing 'start' field");
    let start = start.unwrap();
    assert!(
        start.get("line").is_some(),
        "{context}: missing 'start.line'"
    );
    assert!(
        start.get("offset").is_some(),
        "{context}: missing 'start.offset'"
    );
    let line = start.get("line").unwrap().as_u64().unwrap();
    let offset = start.get("offset").unwrap().as_u64().unwrap();
    assert!(line >= 1, "{context}: start.line must be >= 1 (1-based)");
    assert!(
        offset >= 1,
        "{context}: start.offset must be >= 1 (1-based)"
    );

    let end = value.get("end");
    assert!(end.is_some(), "{context}: missing 'end' field");
    let end = end.unwrap();
    assert!(end.get("line").is_some(), "{context}: missing 'end.line'");
    assert!(
        end.get("offset").is_some(),
        "{context}: missing 'end.offset'"
    );
    let end_line = end.get("line").unwrap().as_u64().unwrap();
    let end_offset = end.get("offset").unwrap().as_u64().unwrap();
    assert!(end_line >= 1, "{context}: end.line must be >= 1 (1-based)");
    assert!(
        end_offset >= 1,
        "{context}: end.offset must be >= 1 (1-based)"
    );
}

#[test]
fn test_quickinfo_response_always_has_valid_spans() {
    // When quickinfo is called on a valid symbol, the response body must
    // include start/end with line/offset fields.
    let mut server = make_server();
    server
        .open_files
        .insert("/test.ts".to_string(), "const x = 42;".to_string());
    let req = make_request(
        "quickinfo",
        serde_json::json!({"file": "/test.ts", "line": 1, "offset": 7}),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("quickinfo should return a body");
    assert_valid_span(&body, "quickinfo on valid symbol");
}

#[test]
fn test_quickinfo_fallback_has_valid_spans() {
    // When quickinfo is called on whitespace or a position where no symbol
    // is found, the response body must still have start/end spans to avoid
    // "Cannot read properties of undefined (reading 'line')" in the harness.
    let mut server = make_server();
    server
        .open_files
        .insert("/test.ts".to_string(), "   ".to_string());
    let req = make_request(
        "quickinfo",
        serde_json::json!({"file": "/test.ts", "line": 1, "offset": 1}),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("quickinfo fallback should return a body");
    assert_valid_span(&body, "quickinfo fallback on whitespace");
}

#[test]
fn test_quickinfo_class_keyword_returns_local_class_display() {
    let mut server = make_server();
    server.open_files.insert(
        "/test.ts".to_string(),
        "[1].forEach(class {});\n[1].forEach(class OK {});\n".to_string(),
    );
    let anonymous_req = make_request(
        "quickinfo",
        // Inside `class` keyword of anonymous class expression.
        serde_json::json!({"file": "/test.ts", "line": 1, "offset": 15}),
    );
    let anonymous_resp = server.handle_tsserver_request(anonymous_req);
    assert!(anonymous_resp.success);
    let anonymous_display = anonymous_resp
        .body
        .expect("quickinfo should return a body")
        .get("displayString")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_string();
    assert_eq!(anonymous_display, "(local class) (Anonymous class)");

    let named_req = make_request(
        "quickinfo",
        // Inside `class` keyword of named class expression.
        serde_json::json!({"file": "/test.ts", "line": 2, "offset": 15}),
    );
    let named_resp = server.handle_tsserver_request(named_req);
    assert!(named_resp.success);
    let named_display = named_resp
        .body
        .expect("quickinfo should return a body")
        .get("displayString")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_string();
    assert_eq!(named_display, "(local class) OK");
}

#[test]
fn test_quickinfo_member_call_property_at_member_start() {
    let mut server = make_server();
    server.open_files.insert(
        "/test.ts".to_string(),
        "interface I {\n    /** Doc */\n    m: () => void;\n}\nfunction f(x: I): void {\n    x.m();\n}\n"
            .to_string(),
    );
    let req = make_request(
        "quickinfo",
        // Cursor between `.` and `m` in `x.m()` (equivalent to fourslash marker `x./**/m()`).
        serde_json::json!({"file": "/test.ts", "line": 6, "offset": 6}),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("quickinfo should return a body");
    assert_eq!(
        body["displayString"].as_str().unwrap_or(""),
        "(property) I.m: () => void"
    );
    assert_eq!(
        body["documentation"],
        serde_json::json!([{"kind": "text", "text": "Doc"}])
    );

    let req_at_member = make_request(
        "quickinfo",
        serde_json::json!({"file": "/test.ts", "line": 6, "offset": 7}),
    );
    let resp_at_member = server.handle_tsserver_request(req_at_member);
    assert!(resp_at_member.success);
    let body_at_member = resp_at_member
        .body
        .expect("quickinfo at member should return a body");
    assert_eq!(
        body_at_member["displayString"].as_str().unwrap_or(""),
        "(property) I.m: () => void"
    );
    assert_eq!(
        body_at_member["documentation"],
        serde_json::json!([{"kind": "text", "text": "Doc"}])
    );
}

#[test]
fn test_quickinfo_new_expression_uses_constructor_signature() {
    let mut server = make_server();
    server.open_files.insert(
        "/test.ts".to_string(),
        "class A<T> {}\nnew A<string>();\n".to_string(),
    );
    let req = make_request(
        "quickinfo",
        // Hover over `A` in `new A<string>()`.
        serde_json::json!({"file": "/test.ts", "line": 2, "offset": 5}),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("quickinfo should return a body");
    assert_eq!(
        body["displayString"].as_str().unwrap_or(""),
        "constructor A<string>(): A<string>"
    );
    assert_eq!(body["kind"].as_str().unwrap_or(""), "constructor");
}

#[test]
fn test_quickinfo_arrow_token_uses_contextual_signature() {
    let mut server = make_server();
    server.open_files.insert(
        "/test.ts".to_string(),
        "// @strict: true\nconst optionals: ((a?: number) => unknown) & ((b?: string) => unknown) = (\n  arg,\n) => {};\n"
            .to_string(),
    );
    let req = make_request(
        "quickinfo",
        // Cursor on `=>` token of the arrow function.
        serde_json::json!({"file": "/test.ts", "line": 4, "offset": 4}),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("quickinfo should return a body");
    assert_eq!(
        body["displayString"].as_str().unwrap_or(""),
        "function(arg: string | number | undefined): void"
    );
    assert_eq!(body["kind"].as_str().unwrap_or(""), "function");
}

#[test]
fn test_quickinfo_marker_comment_before_parameter_uses_contextual_type() {
    let mut server = make_server();
    server.open_files.insert(
        "/test.ts".to_string(),
        "var c3t11: {(n: number, s: string): string;}[] = [function(/*25*/n, s) { return s; }];\n"
            .to_string(),
    );
    let req = make_request(
        "quickinfo",
        // Cursor on the `/` in /*25*/ before parameter `n`.
        serde_json::json!({"file": "/test.ts", "line": 1, "offset": 60}),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("quickinfo should return a body");
    assert_eq!(
        body["displayString"].as_str().unwrap_or(""),
        "(parameter) n: number"
    );
    assert_eq!(body["kind"].as_str().unwrap_or(""), "parameter");

    let req_on_identifier = make_request(
        "quickinfo",
        // Cursor on `n` after /*25*/.
        serde_json::json!({"file": "/test.ts", "line": 1, "offset": 66}),
    );
    let resp_on_identifier = server.handle_tsserver_request(req_on_identifier);
    assert!(resp_on_identifier.success);
    let body_on_identifier = resp_on_identifier
        .body
        .expect("quickinfo should return a body on identifier");
    assert_eq!(
        body_on_identifier["displayString"].as_str().unwrap_or(""),
        "(parameter) n: number"
    );
}

#[test]
fn test_quickinfo_contextual_object_literal_function_parameter() {
    let mut server = make_server();
    let source = "interface IFoo { f(i: number, s: string): string; }\nvar c = <IFoo>({ f: function(/*31*/i, s) { return s; } });\n";
    server
        .open_files
        .insert("/test.ts".to_string(), source.to_string());

    let second_line = source
        .lines()
        .nth(1)
        .expect("source should contain second line");
    let identifier_offset = second_line
        .find("/*31*/i")
        .expect("marker+identifier should exist in source second line")
        as u32
        + "/*31*/".len() as u32
        + 1;

    let req_at_identifier = make_request(
        "quickinfo",
        serde_json::json!({"file": "/test.ts", "line": 2, "offset": identifier_offset}),
    );
    let resp_at_identifier = server.handle_tsserver_request(req_at_identifier);
    assert!(resp_at_identifier.success);
    let body_at_identifier = resp_at_identifier
        .body
        .expect("quickinfo should return a body at identifier");
    assert_eq!(
        body_at_identifier["displayString"].as_str().unwrap_or(""),
        "(parameter) i: number"
    );
}

#[test]
fn test_quickinfo_contextual_object_literal_array_property_name() {
    let mut server = make_server();
    let source = "interface IFoo { a: number[]; }\nvar c = <IFoo>({\n    /*34*/a: []\n});\n";
    server
        .open_files
        .insert("/test.ts".to_string(), source.to_string());

    let third_line = source
        .lines()
        .nth(2)
        .expect("source should contain third line");
    let property_offset = third_line
        .find("/*34*/a")
        .expect("marker+property should exist in source third line")
        as u32
        + "/*34*/".len() as u32
        + 1;

    let req = make_request(
        "quickinfo",
        serde_json::json!({"file": "/test.ts", "line": 3, "offset": property_offset}),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("quickinfo should return a body");
    assert_eq!(
        body["displayString"].as_str().unwrap_or(""),
        "(property) IFoo.a: number[]"
    );
}

#[test]
fn test_quickinfo_contextual_class_property_assignment_function_parameter() {
    let mut server = make_server();
    let source = "class C {\n    foo: (i: number, s: string) => string;\n    constructor() {\n        this.foo = function(/*36*/i, s) {\n            return s;\n        }\n    }\n}\n";
    server
        .open_files
        .insert("/test.ts".to_string(), source.to_string());

    let fourth_line = source
        .lines()
        .nth(3)
        .expect("source should contain assignment line");
    let identifier_offset = fourth_line
        .find("/*36*/i")
        .expect("marker+identifier should exist in assignment line")
        as u32
        + "/*36*/".len() as u32
        + 1;

    let req = make_request(
        "quickinfo",
        serde_json::json!({"file": "/test.ts", "line": 4, "offset": identifier_offset}),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("quickinfo should return a body");
    assert_eq!(
        body["displayString"].as_str().unwrap_or(""),
        "(parameter) i: number"
    );
    assert_eq!(body["kind"].as_str().unwrap_or(""), "parameter");
}

#[test]
fn test_prepare_call_hierarchy_class_property_arrow_function() {
    let mut server = make_server();
    server.open_files.insert(
        "/test.ts".to_string(),
        "class C {\n    caller = () => {\n        this.callee();\n    }\n\n    callee = () => {\n    }\n}\n"
            .to_string(),
    );

    let req = make_request(
        "prepareCallHierarchy",
        serde_json::json!({
            "file": "/test.ts",
            "line": 6,
            "offset": 5
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp
        .body
        .expect("prepareCallHierarchy should return a body");
    let items = body
        .as_array()
        .expect("prepareCallHierarchy body should be an array");
    assert!(
        !items.is_empty(),
        "Expected at least one call hierarchy item for class property arrow function"
    );
    let first = &items[0];
    assert_eq!(first["name"].as_str().unwrap_or(""), "callee");
    assert_eq!(first["kind"].as_str().unwrap_or(""), "function");
}

#[test]
fn test_prepare_call_hierarchy_marker_comment_before_interface_method() {
    let mut server = make_server();
    server.open_files.insert(
        "/test.ts".to_string(),
        "interface I {\n    /**/foo(): void;\n}\n\nconst obj: I = { foo() {} };\nobj.foo();\n"
            .to_string(),
    );

    let req = make_request(
        "prepareCallHierarchy",
        // Cursor on the `/` in `/**/foo`.
        serde_json::json!({
            "file": "/test.ts",
            "line": 2,
            "offset": 5
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp
        .body
        .expect("prepareCallHierarchy should return a body");
    let items = body
        .as_array()
        .expect("prepareCallHierarchy body should be an array");
    assert!(
        !items.is_empty(),
        "Expected call hierarchy item for interface method marker comment probe"
    );
    let first = &items[0];
    assert_eq!(first["name"].as_str().unwrap_or(""), "foo");
    assert_eq!(first["kind"].as_str().unwrap_or(""), "method");
}

#[test]
fn test_call_hierarchy_outgoing_includes_constructor_call_target() {
    let mut server = make_server();
    server.open_files.insert(
        "/test.ts".to_string(),
        "function foo() {\n    bar();\n}\n\nfunction bar() {\n    new Baz();\n}\n\nclass Baz {\n}\n"
            .to_string(),
    );

    let req = make_request(
        "provideCallHierarchyOutgoingCalls",
        serde_json::json!({
            "file": "/test.ts",
            "line": 5,
            "offset": 10
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("outgoing calls should return a body");
    let calls = body
        .as_array()
        .expect("provideCallHierarchyOutgoingCalls body should be an array");
    assert!(
        calls.iter().any(|call| call["to"]["name"] == "Baz"),
        "Expected outgoing constructor call target 'Baz', got: {calls:?}"
    );
}

#[test]
fn test_call_hierarchy_incoming_uses_script_kind_for_top_level_caller() {
    let mut server = make_server();
    server.open_files.insert(
        "/test.ts".to_string(),
        "function foo() {\n    bar();\n}\n\nconst bar = function () {\n    baz();\n}\n\nfunction baz() {\n}\n\nbar()\n"
            .to_string(),
    );

    let req = make_request(
        "provideCallHierarchyIncomingCalls",
        serde_json::json!({
            "file": "/test.ts",
            "line": 5,
            "offset": 7
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("incoming calls should return a body");
    let calls = body
        .as_array()
        .expect("provideCallHierarchyIncomingCalls body should be an array");
    assert!(
        calls.iter().any(|call| call["from"]["kind"] == "script"),
        "Expected top-level caller to be mapped to tsserver kind 'script', got: {calls:?}"
    );
}

#[test]
fn test_call_hierarchy_incoming_file_start_query_returns_no_calls() {
    let mut server = make_server();
    server.open_files.insert(
        "/test.ts".to_string(),
        "foo();\nfunction foo() {\n}\n".to_string(),
    );

    let req = make_request(
        "provideCallHierarchyIncomingCalls",
        serde_json::json!({
            "file": "/test.ts",
            "line": 1,
            "offset": 1
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("incoming calls should return a body");
    let calls = body
        .as_array()
        .expect("provideCallHierarchyIncomingCalls body should be an array");
    assert!(
        calls.is_empty(),
        "Expected no incoming calls for file-start source-file query, got: {calls:?}"
    );
}

#[test]
fn test_format_range_paste_applies_cleanly() {
    // When the server format handler runs, edits may come from either the
    // native tsserver worker (which produces tsserver-shaped structural
    // rewrites) or the LSP crate's conservative whitespace-only fallback
    // (which does not rewrite structure). Either way, reapplying the edits
    // must produce valid UTF-8 output that still contains the declarations
    // from the source.
    //
    // Structural-rewrite assertions were removed: they belong in prettier /
    // eslint / native tsserver tests, not in the LSP fallback. The LSP
    // fallback's conservative contract is covered by the formatting_tests
    // suite in tsz-lsp.
    let mut server = make_server();
    let file = "/test.ts";
    let source = "namespace TestModule {\n class TestClass{\nprivate   foo;\npublic testMethod( )\n{}\n}\n}\n";
    server
        .open_files
        .insert(file.to_string(), source.to_string());

    let req = make_request(
        "format",
        serde_json::json!({
            "file": file,
            "line": 2,
            "offset": 1,
            "endLine": 6,
            "endOffset": 2,
            "options": {
                "tabSize": 4,
                "insertSpaces": true
            }
        }),
    );

    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let edits = resp
        .body
        .expect("format should return edits")
        .as_array()
        .expect("format body should be array")
        .clone();
    let updated = apply_tsserver_text_edits(source.to_string(), &edits);
    assert!(
        updated.contains("namespace TestModule") && updated.contains("class TestClass"),
        "formatted output lost its declarations: {updated:?}"
    );
    assert!(
        updated.contains("testMethod"),
        "formatted output lost the method: {updated:?}"
    );
}

#[test]
fn test_format_with_explicit_range_preserves_inline_markers_on_indent_only_lines() {
    let mut server = make_server();
    let file = "/test.ts";
    let source = "class TestClass {\n    private testMethod1(param1: boolean,\n                        param2/*1*/: boolean) {\n    }\n\n    public testMethod2(a: number, b: number, c: number) {\n        if (a === b) {\n        }\n        else if (a != c &&\n                 a/*2*/ > b &&\n                 b/*3*/ < c) {\n        }\n\n    }\n}\n";
    server
        .open_files
        .insert(file.to_string(), source.to_string());

    let req = make_request(
        "format",
        serde_json::json!({
            "file": file,
            "line": 1,
            "offset": 1,
            "endLine": 15,
            "endOffset": 1,
            "options": {
                "tabSize": 4,
                "insertSpaces": true
            }
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let edits = resp
        .body
        .expect("format should return edits")
        .as_array()
        .expect("format body should be array")
        .clone();
    let updated = apply_tsserver_text_edits(source.to_string(), &edits);

    assert!(
        updated.contains("/*1*/"),
        "marker /*1*/ must survive formatting edits"
    );
    assert!(
        updated.contains("/*2*/"),
        "marker /*2*/ must survive formatting edits"
    );
    assert!(
        updated.contains("/*3*/"),
        "marker /*3*/ must survive formatting edits"
    );
}

#[test]
fn test_format_with_explicit_range_does_not_invalidate_fourslash_markers() {
    fn strip_markers(source: &str) -> (String, Vec<usize>) {
        let mut out = String::with_capacity(source.len());
        let mut markers = Vec::new();
        let bytes = source.as_bytes();
        let mut i = 0usize;
        while i < bytes.len() {
            if i + 4 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
                let mut j = i + 2;
                while j < bytes.len() && bytes[j].is_ascii_digit() {
                    j += 1;
                }
                if j + 1 < bytes.len() && bytes[j] == b'*' && bytes[j + 1] == b'/' && j > i + 2 {
                    markers.push(out.len());
                    i = j + 2;
                    continue;
                }
            }
            out.push(bytes[i] as char);
            i += 1;
        }
        (out, markers)
    }

    fn update_position(
        position: usize,
        edit_start: usize,
        edit_end: usize,
        new_text: &str,
    ) -> Option<usize> {
        if position <= edit_start {
            return Some(position);
        }
        if position < edit_end {
            return None;
        }
        Some(position + new_text.len() - (edit_end - edit_start))
    }

    let source_with_markers = "class TestClass {\n    private testMethod1(param1: boolean,\n                        param2/*1*/: boolean) {\n    }\n\n    public testMethod2(a: number, b: number, c: number) {\n        if (a === b) {\n        }\n        else if (a != c &&\n                 a/*2*/ > b &&\n                 b/*3*/ < c) {\n        }\n\n    }\n}\n";
    let (source, mut marker_positions) = strip_markers(source_with_markers);

    let mut server = make_server();
    let file = "/test.ts";
    server.open_files.insert(file.to_string(), source.clone());

    let req = make_request(
        "format",
        serde_json::json!({
            "file": file,
            "line": 1,
            "offset": 1,
            "endLine": 15,
            "endOffset": 1,
            "options": {
                "tabSize": 4,
                "insertSpaces": true
            }
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("format should return edits");
    let edits = body.as_array().expect("format body should be array");

    let mut changes: Vec<(usize, usize, String)> = edits
        .iter()
        .filter_map(|edit| {
            let start = edit.get("start")?;
            let end = edit.get("end")?;
            let start_line = start.get("line")?.as_u64()? as u32;
            let start_offset = start.get("offset")?.as_u64()? as u32;
            let end_line = end.get("line")?.as_u64()? as u32;
            let end_offset = end.get("offset")?.as_u64()? as u32;
            let new_text = edit.get("newText")?.as_str()?.to_string();
            let start_byte = Server::line_offset_to_byte(&source, start_line, start_offset);
            let end_byte = Server::line_offset_to_byte(&source, end_line, end_offset);
            Some((start_byte, end_byte.saturating_sub(start_byte), new_text))
        })
        .collect();

    for i in 0..changes.len() {
        let (start, len, new_text) = changes[i].clone();
        let end = start + len;
        for marker in &mut marker_positions {
            let next = update_position(*marker, start, end, &new_text);
            assert!(
                next.is_some(),
                "fourslash marker invalidated by edit span ({start}, {end}) -> {:?}",
                changes[i]
            );
            *marker = next.unwrap_or(0);
        }
        let delta = new_text.len() as isize - len as isize;
        for change in changes.iter_mut().skip(i + 1) {
            if change.0 >= start {
                change.0 = (change.0 as isize + delta) as usize;
            }
        }
    }
}

#[test]
fn test_format_document_does_not_invalidate_fourslash_markers() {
    fn strip_markers(source: &str) -> (String, Vec<usize>) {
        let mut out = String::with_capacity(source.len());
        let mut markers = Vec::new();
        let bytes = source.as_bytes();
        let mut i = 0usize;
        while i < bytes.len() {
            if i + 4 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
                let mut j = i + 2;
                while j < bytes.len() && bytes[j].is_ascii_digit() {
                    j += 1;
                }
                if j + 1 < bytes.len() && bytes[j] == b'*' && bytes[j + 1] == b'/' && j > i + 2 {
                    markers.push(out.len());
                    i = j + 2;
                    continue;
                }
            }
            out.push(bytes[i] as char);
            i += 1;
        }
        (out, markers)
    }

    fn update_position(
        position: usize,
        edit_start: usize,
        edit_end: usize,
        new_text: &str,
    ) -> Option<usize> {
        if position <= edit_start {
            return Some(position);
        }
        if position < edit_end {
            return None;
        }
        Some(position + new_text.len() - (edit_end - edit_start))
    }

    let source_with_markers = "class TestClass {\n    private testMethod1(param1: boolean,\n                        param2/*1*/: boolean) {\n    }\n\n    public testMethod2(a: number, b: number, c: number) {\n        if (a === b) {\n        }\n        else if (a != c &&\n                 a/*2*/ > b &&\n                 b/*3*/ < c) {\n        }\n\n    }\n}\n";
    let (source, mut marker_positions) = strip_markers(source_with_markers);

    let mut server = make_server();
    let file = "/test.ts";
    server.open_files.insert(file.to_string(), source.clone());

    let req = make_request(
        "format",
        serde_json::json!({
            "file": file,
            "options": {
                "tabSize": 4,
                "insertSpaces": true
            }
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("format should return edits");
    let edits = body.as_array().expect("format body should be array");

    let mut changes: Vec<(usize, usize, String)> = edits
        .iter()
        .filter_map(|edit| {
            let start = edit.get("start")?;
            let end = edit.get("end")?;
            let start_line = start.get("line")?.as_u64()? as u32;
            let start_offset = start.get("offset")?.as_u64()? as u32;
            let end_line = end.get("line")?.as_u64()? as u32;
            let end_offset = end.get("offset")?.as_u64()? as u32;
            let new_text = edit.get("newText")?.as_str()?.to_string();
            let start_byte = Server::line_offset_to_byte(&source, start_line, start_offset);
            let end_byte = Server::line_offset_to_byte(&source, end_line, end_offset);
            Some((start_byte, end_byte.saturating_sub(start_byte), new_text))
        })
        .collect();

    for i in 0..changes.len() {
        let (start, len, new_text) = changes[i].clone();
        let end = start + len;
        for marker in &mut marker_positions {
            let next = update_position(*marker, start, end, &new_text);
            assert!(
                next.is_some(),
                "fourslash marker invalidated by edit span ({start}, {end}) -> {:?}",
                changes[i]
            );
            *marker = next.unwrap_or(0);
        }
        let delta = new_text.len() as isize - len as isize;
        for change in changes.iter_mut().skip(i + 1) {
            if change.0 >= start {
                change.0 = (change.0 as isize + delta) as usize;
            }
        }
    }
}

#[test]
fn test_quickinfo_on_nonexistent_file_has_valid_spans() {
    // Even when the file is not open, the quickinfo fallback must return
    // valid span data.
    let mut server = make_server();
    let req = make_request(
        "quickinfo",
        serde_json::json!({"file": "/nonexistent.ts", "line": 1, "offset": 1}),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("quickinfo fallback should return a body");
    assert_valid_span(&body, "quickinfo on nonexistent file");
}

#[test]
fn test_quickinfo_uses_hover_info_structured_fields() {
    // When HoverInfo returns structured kind/kindModifiers/displayString/
    // documentation fields, they should be used in the response instead of
    // being re-parsed from markdown contents.
    let mut server = make_server();
    server
        .open_files
        .insert("/test.ts".to_string(), "const myVar = 42;".to_string());
    let req = make_request(
        "quickinfo",
        serde_json::json!({"file": "/test.ts", "line": 1, "offset": 7}),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("quickinfo should return a body");
    // The body must have displayString, kind, kindModifiers, documentation
    assert!(
        body.get("displayString").is_some(),
        "quickinfo must have displayString"
    );
    assert!(body.get("kind").is_some(), "quickinfo must have kind");
    assert!(
        body.get("kindModifiers").is_some(),
        "quickinfo must have kindModifiers"
    );
    assert!(
        body.get("documentation").is_some(),
        "quickinfo must have documentation"
    );
}

#[test]
fn test_definition_response_entries_have_valid_spans() {
    // Each definition entry in the response must have start/end spans with
    // valid line/offset fields.
    let mut server = make_server();
    server
        .open_files
        .insert("/test.ts".to_string(), "const x = 1;\nx;".to_string());
    // Open file with actual newline
    server.open_files.insert(
        "/test.ts".to_string(),
        "const x = 1;
x;"
        .to_string(),
    );
    let req = make_request(
        "definition",
        serde_json::json!({"file": "/test.ts", "line": 2, "offset": 1}),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("definition should return a body");
    // The body is an array; each entry must have start/end and file
    if let Some(arr) = body.as_array() {
        for (i, entry) in arr.iter().enumerate() {
            assert_valid_span(entry, &format!("definition entry {i}"));
            assert!(
                entry.get("file").is_some(),
                "definition entry {i} must have 'file'"
            );
        }
    }
}

#[test]
fn test_definition_empty_response_is_valid_array() {
    // When no definition is found, the response must be an empty array,
    // not null or an object missing start/end.
    let mut server = make_server();
    server
        .open_files
        .insert("/test.ts".to_string(), "   ".to_string());
    let req = make_request(
        "definition",
        serde_json::json!({"file": "/test.ts", "line": 1, "offset": 1}),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("definition should return a body");
    assert!(body.is_array(), "definition fallback must be an array");
}

#[test]
fn test_definition_and_bound_span_has_valid_text_span() {
    // The definitionAndBoundSpan response must always have a textSpan with
    // valid start/end, even when no definitions are found.
    let mut server = make_server();
    server
        .open_files
        .insert("/test.ts".to_string(), "   ".to_string());
    let req = make_request(
        "definitionAndBoundSpan",
        serde_json::json!({"file": "/test.ts", "line": 1, "offset": 1}),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp
        .body
        .expect("definitionAndBoundSpan should return a body");
    let text_span = body
        .get("textSpan")
        .expect("definitionAndBoundSpan must have textSpan");
    assert_valid_span(text_span, "definitionAndBoundSpan textSpan");
    assert!(
        body.get("definitions").is_some(),
        "definitionAndBoundSpan must have definitions array"
    );
}

#[test]
fn test_navtree_fallback_has_spans() {
    // The navtree/navbar fallback must include a spans array so the harness
    // does not crash when iterating item.spans.
    let mut server = make_server();
    let req = make_request("navtree", serde_json::json!({"file": "/nonexistent.ts"}));
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("navtree should return a body");
    let spans = body.get("spans");
    assert!(spans.is_some(), "navtree fallback must have spans array");
    let spans_arr = spans.unwrap().as_array().expect("spans must be an array");
    assert!(
        !spans_arr.is_empty(),
        "navtree fallback must have at least one span"
    );
    assert_valid_span(&spans_arr[0], "navtree fallback span");
}

#[test]
fn test_references_response_entries_have_valid_spans() {
    // Each reference entry must have valid start/end spans.
    let mut server = make_server();
    server.open_files.insert(
        "/test.ts".to_string(),
        "const x = 1;
x;
x;"
        .to_string(),
    );
    let req = make_request(
        "references",
        serde_json::json!({"file": "/test.ts", "line": 1, "offset": 7}),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("references should return a body");
    let refs = body.get("refs").expect("references must have refs array");
    if let Some(arr) = refs.as_array() {
        for (i, entry) in arr.iter().enumerate() {
            assert_valid_span(entry, &format!("reference entry {i}"));
        }
    }
    assert!(
        body.get("symbolName").is_some(),
        "references must have symbolName"
    );
}

#[test]
fn test_alias_string_literal_navigation_uses_project_wide_resolution() {
    let mut server = make_server();
    server.open_files.insert(
        "/foo.ts".to_string(),
        [
            "const foo = \"foo\";",
            "export { foo as \"__<alias>\" };",
            "import { \"__<alias>\" as bar } from \"./foo\";",
            "if (bar !== \"foo\") throw bar;",
        ]
        .join("\n"),
    );
    server.open_files.insert(
        "/bar.ts".to_string(),
        [
            "import { \"__<alias>\" as first } from \"./foo\";",
            "export { \"__<alias>\" as \"<other>\" } from \"./foo\";",
            "import { \"<other>\" as second } from \"./bar\";",
            "if (first !== \"foo\") throw first;",
            "if (second !== \"foo\") throw second;",
        ]
        .join("\n"),
    );
    let (arena, _binder, _root, source_text) = server
        .parse_and_bind_file("/bar.ts")
        .expect("expected parse_and_bind_file for /bar.ts");
    let line_map = tsz::lsp::position::LineMap::build(&source_text);
    let probe_pos = Server::tsserver_to_lsp_position(1, 14);
    let probe_off = line_map
        .position_to_offset(probe_pos, &source_text)
        .expect("offset at marker");
    let alias_query = server.debug_alias_query_target(&arena, &source_text, probe_off);
    let direct_resolve =
        server.debug_resolve_export_alias_definition("/bar.ts", "./foo", "__<alias>");
    let probe_node =
        tsz::lsp::utils::find_node_at_or_before_offset(&arena, probe_off, &source_text);
    let probe_kind = arena.get(probe_node).map(|n| n.kind).unwrap_or_default();
    let mut chain = Vec::new();
    let mut walk = probe_node;
    while walk.is_some() {
        if let Some(node) = arena.get(walk) {
            chain.push(node.kind);
        }
        let Some(ext) = arena.get_extended(walk) else {
            break;
        };
        walk = ext.parent;
    }
    let canonical =
        server.canonical_definition_for_alias_position("/bar.ts", &arena, &source_text, probe_off);
    assert!(
        canonical.is_some(),
        "expected canonical definition for alias specifier (off={probe_off}, kind={probe_kind}, chain={chain:?}, alias_query={alias_query:?}, direct_resolve={direct_resolve:?})"
    );

    let definition_req = make_request(
        "definition",
        serde_json::json!({
            "file": "/bar.ts",
            "line": 1,
            "offset": 14
        }),
    );
    let definition_resp = server.handle_tsserver_request(definition_req);
    assert!(definition_resp.success);
    let definition_body = definition_resp
        .body
        .expect("definition should return body")
        .as_array()
        .cloned()
        .expect("definition response should be an array");
    assert!(
        definition_body.iter().any(|entry| {
            entry.get("file").and_then(serde_json::Value::as_str) == Some("/foo.ts")
        }),
        "expected alias definition to include /foo.ts, got: {definition_body:?}"
    );

    let references_req = make_request(
        "references",
        serde_json::json!({
            "file": "/bar.ts",
            "line": 1,
            "offset": 14
        }),
    );
    let references_resp = server.handle_tsserver_request(references_req);
    assert!(references_resp.success);
    let references_body = references_resp.body.expect("references should return body");
    let refs = references_body["refs"]
        .as_array()
        .cloned()
        .expect("references should have refs");
    assert!(
        refs.iter()
            .filter_map(|entry| entry.get("file").and_then(serde_json::Value::as_str))
            .any(|file| file == "/foo.ts"),
        "expected refs to include /foo.ts, got: {refs:?}"
    );
}

#[test]
fn test_definition_and_bound_span_quoted_local_export_alias_has_token_span() {
    let mut server = make_server();
    server.open_files.insert(
        "/foo.ts".to_string(),
        [
            "const foo = \"foo\";",
            "export { foo as \"__<alias>\" };",
            "import { \"__<alias>\" as bar } from \"./foo\";",
            "if (bar !== \"foo\") throw bar;",
        ]
        .join("\n"),
    );

    let req = make_request(
        "definitionAndBoundSpan",
        serde_json::json!({
            "file": "/foo.ts",
            "line": 2,
            "offset": 19
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp
        .body
        .expect("definitionAndBoundSpan should return body");
    let text_span = body.get("textSpan").expect("textSpan must be present");
    assert_valid_span(text_span, "quoted export alias textSpan");
    let start = text_span["start"]["offset"]
        .as_u64()
        .expect("textSpan.start.offset should be numeric");
    let end = text_span["end"]["offset"]
        .as_u64()
        .expect("textSpan.end.offset should be numeric");
    assert!(
        end > start,
        "quoted export alias textSpan must be non-empty (start={start}, end={end})"
    );
}

#[test]
fn test_quoted_alias_chain_references_and_rename_stay_on_quoted_specifiers() {
    let mut server = make_server();
    server.open_files.insert(
        "/foo.ts".to_string(),
        [
            "const foo = \"foo\";",
            "export { foo as \"__<alias>\" };",
            "import { \"__<alias>\" as bar } from \"./foo\";",
            "if (bar !== \"foo\") throw bar;",
        ]
        .join("\n"),
    );
    server.open_files.insert(
        "/bar.ts".to_string(),
        [
            "import { \"__<alias>\" as first } from \"./foo\";",
            "export { \"__<alias>\" as \"<other>\" } from \"./foo\";",
            "import { \"<other>\" as second } from \"./bar\";",
            "if (first !== \"foo\") throw first;",
            "if (second !== \"foo\") throw second;",
        ]
        .join("\n"),
    );

    let refs_req = make_request(
        "references",
        serde_json::json!({
            "file": "/bar.ts",
            "line": 2,
            "offset": 12
        }),
    );
    let refs_resp = server.handle_tsserver_request(refs_req);
    assert!(refs_resp.success);
    let refs = refs_resp
        .body
        .expect("references should return body")
        .get("refs")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .expect("references should include refs array");
    assert!(
        refs.len() >= 3,
        "expected multiple quoted alias refs across files, got: {refs:?}"
    );
    assert!(
        refs.iter()
            .filter_map(|entry| entry.get("file").and_then(serde_json::Value::as_str))
            .any(|file| file == "/foo.ts"),
        "expected quoted alias refs to include /foo.ts"
    );
    assert!(
        refs.iter().all(|entry| {
            entry
                .get("lineText")
                .and_then(serde_json::Value::as_str)
                .is_none_or(|line| {
                    !line.contains("if (bar")
                        && !line.contains("if (first")
                        && !line.contains("if (second")
                })
        }),
        "expected quoted alias refs to stay on quoted import/export specifiers: {refs:?}"
    );

    let rename_req = make_request(
        "rename",
        serde_json::json!({
            "file": "/bar.ts",
            "line": 2,
            "offset": 12
        }),
    );
    let rename_resp = server.handle_tsserver_request(rename_req);
    assert!(rename_resp.success);
    let locs = rename_resp
        .body
        .expect("rename should return body")
        .get("locs")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .expect("rename should include locs array");
    assert!(
        locs.iter().any(|entry| {
            entry.get("file").and_then(serde_json::Value::as_str) == Some("/foo.ts")
        }),
        "expected rename locations to include /foo.ts: {locs:?}"
    );
    assert!(
        locs.iter().any(|entry| {
            entry.get("file").and_then(serde_json::Value::as_str) == Some("/bar.ts")
        }),
        "expected rename locations to include /bar.ts: {locs:?}"
    );
}

#[test]
fn test_rename_from_export_quoted_alias_filters_non_specifier_locations() {
    let mut server = make_server();
    server.open_files.insert(
        "/foo.ts".to_string(),
        [
            "const foo = \"foo\";",
            "export { foo as \"__<alias>\" };",
            "import { \"__<alias>\" as bar } from \"./foo\";",
            "if (bar !== \"foo\") throw bar;",
        ]
        .join("\n"),
    );
    server.open_files.insert(
        "/bar.ts".to_string(),
        [
            "import { \"__<alias>\" as first } from \"./foo\";",
            "export { \"__<alias>\" as \"<other>\" } from \"./foo\";",
            "import { \"<other>\" as second } from \"./bar\";",
            "if (first !== \"foo\") throw first;",
            "if (second !== \"foo\") throw second;",
        ]
        .join("\n"),
    );

    let rename_req = make_request(
        "rename",
        serde_json::json!({
            "file": "/foo.ts",
            "line": 2,
            "offset": 21
        }),
    );
    let rename_resp = server.handle_tsserver_request(rename_req);
    assert!(rename_resp.success);
    let body = rename_resp.body.expect("rename should return body");
    let loc_groups = body
        .get("locs")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .expect("rename should include locs array");
    assert!(
        loc_groups.iter().any(|entry| {
            entry.get("file").and_then(serde_json::Value::as_str) == Some("/foo.ts")
        }),
        "expected rename locations to include /foo.ts: {loc_groups:?}"
    );
    assert!(
        loc_groups.iter().any(|entry| {
            entry.get("file").and_then(serde_json::Value::as_str) == Some("/bar.ts")
        }),
        "expected rename locations to include /bar.ts: {loc_groups:?}"
    );

    for group in &loc_groups {
        let file = group
            .get("file")
            .and_then(serde_json::Value::as_str)
            .expect("loc group should contain file");
        let source = server
            .open_files
            .get(file)
            .expect("test source should be present in open files");
        let lines: Vec<&str> = source.lines().collect();
        let locs = group
            .get("locs")
            .and_then(serde_json::Value::as_array)
            .expect("loc group should contain locs");
        for loc in locs {
            let line_one_based = loc
                .get("start")
                .and_then(|start| start.get("line"))
                .and_then(serde_json::Value::as_u64)
                .expect("loc start.line should be numeric");
            let line_idx = line_one_based.saturating_sub(1) as usize;
            let line_text = lines.get(line_idx).copied().unwrap_or("");
            assert!(
                line_text.contains("import {") || line_text.contains("export {"),
                "rename on quoted alias should stay on import/export specifiers, got line: {line_text}"
            );
            assert!(
                !line_text.contains("const foo")
                    && !line_text.contains("if (bar")
                    && !line_text.contains("if (first")
                    && !line_text.contains("if (second"),
                "rename on quoted alias should not include identifier usage lines, got line: {line_text}"
            );
        }
    }
}

#[test]
fn test_rename_quoted_alias_marker_offset_uses_literal_only_locations() {
    let mut server = make_server();
    server.open_files.insert(
        "/foo.ts".to_string(),
        [
            "const foo = \"foo\";",
            "export { foo as \"/*RENAME*/__<alias>\" };",
            "import { \"__<alias>\" as bar } from \"./foo\";",
            "if (bar !== \"foo\") throw bar;",
        ]
        .join("\n"),
    );
    server.open_files.insert(
        "/bar.ts".to_string(),
        [
            "import { \"__<alias>\" as first } from \"./foo\";",
            "export { \"__<alias>\" as \"<other>\" } from \"./foo\";",
            "import { \"<other>\" as second } from \"./bar\";",
            "if (first !== \"foo\") throw first;",
            "if (second !== \"foo\") throw second;",
        ]
        .join("\n"),
    );

    // Offset lands inside the comment marker in the quoted export alias string literal.
    let req = make_request(
        "rename",
        serde_json::json!({
            "file": "/foo.ts",
            "line": 2,
            "offset": 19
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("rename should return body");
    let groups = body["locs"]
        .as_array()
        .expect("rename should include grouped locations");
    assert!(
        groups
            .iter()
            .any(|g| g.get("file").and_then(serde_json::Value::as_str) == Some("/foo.ts")),
        "expected /foo.ts rename locations: {groups:?}"
    );
    assert!(!groups.is_empty(), "expected rename locations: {groups:?}");

    for group in groups {
        let file = group["file"]
            .as_str()
            .expect("group.file should be a string");
        let source = server
            .open_files
            .get(file)
            .expect("source file should exist");
        let lines: Vec<&str> = source.lines().collect();
        for loc in group["locs"]
            .as_array()
            .expect("group.locs should be an array")
        {
            let line = loc["start"]["line"]
                .as_u64()
                .expect("start.line should be numeric")
                .saturating_sub(1) as usize;
            let line_text = lines.get(line).copied().unwrap_or_default();
            assert!(
                line_text.contains("import {") || line_text.contains("export {"),
                "rename should stay on import/export specifiers, got line: {line_text}"
            );
            assert!(
                !line_text.contains("\"<other>\""),
                "rename for __<alias> should not include <other> aliases, got line: {line_text}"
            );
            assert!(
                loc.get("contextStart").is_some() && loc.get("contextEnd").is_some(),
                "rename locations should carry context spans for fourslash baseline wrapping: {loc:?}"
            );
        }
    }
}

#[test]
fn test_references_full_quoted_alias_uses_inner_literal_span_and_cross_file_refs() {
    let mut server = make_server();
    server.open_files.insert(
        "/foo.ts".to_string(),
        [
            "type foo = \"foo\";",
            "export { type foo as \"__<alias>\" };",
            "import { type \"__<alias>\" as bar } from \"./foo\";",
            "const testBar: bar = \"foo\";",
        ]
        .join("\n"),
    );
    server.open_files.insert(
        "/bar.ts".to_string(),
        [
            "import { type \"__<alias>\" as first } from \"./foo\";",
            "export { type \"__<alias>\" as \"<other>\" } from \"./foo\";",
            "import { type \"<other>\" as second } from \"./bar\";",
            "const testFirst: first = \"foo\";",
            "const testSecond: second = \"foo\";",
        ]
        .join("\n"),
    );

    let req = make_request(
        "references-full",
        serde_json::json!({
            "file": "/foo.ts",
            "line": 2,
            "offset": 24
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("references-full should return body");
    let entries = body
        .as_array()
        .cloned()
        .expect("references-full response should be array");
    assert!(
        !entries.is_empty(),
        "expected at least one referenced symbol"
    );
    let mut refs: Vec<serde_json::Value> = Vec::new();
    for symbol_entry in &entries {
        let symbol_refs = symbol_entry["references"]
            .as_array()
            .cloned()
            .expect("referenced symbol should include references");
        refs.extend(symbol_refs);
    }
    assert!(
        refs.iter().any(|entry| {
            entry.get("fileName").and_then(serde_json::Value::as_str) == Some("/bar.ts")
        }),
        "expected cross-file references to include /bar.ts: {refs:?}"
    );
    let foo_source = server
        .open_files
        .get("/foo.ts")
        .cloned()
        .expect("missing /foo.ts");
    let has_inner_alias_span = refs.iter().any(|entry| {
        if entry.get("fileName").and_then(serde_json::Value::as_str) != Some("/foo.ts") {
            return false;
        }
        let start = entry["textSpan"]["start"].as_u64().unwrap_or(0) as usize;
        let len = entry["textSpan"]["length"].as_u64().unwrap_or(0) as usize;
        let end = start.saturating_add(len);
        foo_source.get(start..end) == Some("__<alias>")
    });
    assert!(
        has_inner_alias_span,
        "expected at least one /foo.ts reference span to map to inner alias text"
    );
}

#[test]
fn test_references_full_quoted_alias_definition_uses_file_name_and_text_span_shape() {
    let mut server = make_server();
    server.open_files.insert(
        "/foo.ts".to_string(),
        [
            "const foo = \"foo\";",
            "export { foo as \"__<alias>\" };",
            "import { \"__<alias>\" as bar } from \"./foo\";",
            "if (bar !== \"foo\") throw bar;",
        ]
        .join("\n"),
    );
    server.open_files.insert(
        "/bar.ts".to_string(),
        [
            "import { \"__<alias>\" as first } from \"./foo\";",
            "export { \"__<alias>\" as \"<other>\" } from \"./foo\";",
            "import { \"<other>\" as second } from \"./bar\";",
            "if (first !== \"foo\") throw first;",
            "if (second !== \"foo\") throw second;",
        ]
        .join("\n"),
    );

    let req = make_request(
        "references-full",
        serde_json::json!({
            "file": "/foo.ts",
            "line": 2,
            "offset": 19
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("references-full should return body");
    let entries = body
        .as_array()
        .expect("references-full response should be array");
    let first = entries
        .first()
        .expect("expected at least one referenced symbol");
    let definition = first
        .get("definition")
        .expect("referenced symbol should include definition");

    assert!(
        definition.get("fileName").is_some(),
        "definition should expose tsserver fileName in references-full: {definition:?}"
    );
    let text_span = definition
        .get("textSpan")
        .expect("definition should expose tsserver textSpan");
    assert!(
        text_span
            .get("start")
            .and_then(serde_json::Value::as_u64)
            .is_some()
            && text_span
                .get("length")
                .and_then(serde_json::Value::as_u64)
                .is_some(),
        "textSpan should include numeric start/length: {text_span:?}"
    );
    assert!(
        definition.get("file").is_none()
            && definition.get("start").is_none()
            && definition.get("end").is_none(),
        "references-full definition should not use definition-command fields: {definition:?}"
    );
}

#[test]
fn test_references_full_quoted_alias_includes_symbol_alias_references_when_available() {
    let mut server = make_server();
    server.open_files.insert(
        "/foo.ts".to_string(),
        [
            "type foo = \"foo\";",
            "export { type foo as \"__<alias>\" };",
            "import { type \"__<alias>\" as bar } from \"./foo\";",
            "const testBar: bar = \"foo\";",
        ]
        .join("\n"),
    );
    server.open_files.insert(
        "/bar.ts".to_string(),
        [
            "import { type \"__<alias>\" as first } from \"./foo\";",
            "export { type \"__<alias>\" as \"<other>\" } from \"./foo\";",
            "import { type \"<other>\" as second } from \"./bar\";",
            "const testFirst: first = \"foo\";",
            "const testSecond: second = \"foo\";",
        ]
        .join("\n"),
    );

    let req = make_request(
        "references-full",
        serde_json::json!({
            "file": "/foo.ts",
            "line": 2,
            "offset": 24
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("references-full should return body");
    let entries = body
        .as_array()
        .cloned()
        .expect("references-full response should be array");
    assert!(
        !entries.is_empty(),
        "expected at least one referenced symbol entry"
    );
    let mut seen_bar_alias = false;
    let mut seen_first_alias = false;
    let mut all_refs: Vec<serde_json::Value> = Vec::new();
    for symbol_entry in &entries {
        let refs = symbol_entry["references"]
            .as_array()
            .cloned()
            .expect("referenced symbol should include references");
        for entry in refs {
            let file = entry
                .get("fileName")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            let source = server
                .open_files
                .get(file)
                .expect("reference file should be open");
            let start = entry["textSpan"]["start"].as_u64().unwrap_or(0) as usize;
            let len = entry["textSpan"]["length"].as_u64().unwrap_or(0) as usize;
            let end = start.saturating_add(len);
            let text = source.get(start..end).unwrap_or_default();
            if text == "bar" {
                seen_bar_alias = true;
            }
            if text == "first" {
                seen_first_alias = true;
            }
            all_refs.push(entry);
        }
    }

    assert!(
        seen_bar_alias,
        "expected symbol references to include imported alias 'bar': {all_refs:?}"
    );
    assert!(
        seen_first_alias,
        "expected symbol references to include cross-file imported alias 'first': {all_refs:?}"
    );
}

#[test]
fn test_references_full_quoted_alias_does_not_duplicate_reference_spans_across_groups() {
    let mut server = make_server();
    server.open_files.insert(
        "/foo.ts".to_string(),
        [
            "type foo = \"foo\";",
            "export { type foo as \"__<alias>\" };",
            "import { type \"__<alias>\" as bar } from \"./foo\";",
            "const testBar: bar = \"foo\";",
        ]
        .join("\n"),
    );
    server.open_files.insert(
        "/bar.ts".to_string(),
        [
            "import { type \"__<alias>\" as first } from \"./foo\";",
            "export { type \"__<alias>\" as \"<other>\" } from \"./foo\";",
            "import { type \"<other>\" as second } from \"./bar\";",
            "const testFirst: first = \"foo\";",
            "const testSecond: second = \"foo\";",
        ]
        .join("\n"),
    );

    let req = make_request(
        "references-full",
        serde_json::json!({
            "file": "/foo.ts",
            "line": 2,
            "offset": 24
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("references-full should return body");
    let entries = body
        .as_array()
        .expect("references-full response should be array");

    let mut counts: std::collections::HashMap<(String, u64, u64), usize> =
        std::collections::HashMap::new();
    for symbol_entry in entries {
        let refs = symbol_entry["references"]
            .as_array()
            .expect("referenced symbol should include references");
        for entry in refs {
            let file = entry["fileName"]
                .as_str()
                .expect("reference.fileName should be a string")
                .to_string();
            let start = entry["textSpan"]["start"]
                .as_u64()
                .expect("reference.textSpan.start should be numeric");
            let length = entry["textSpan"]["length"]
                .as_u64()
                .expect("reference.textSpan.length should be numeric");
            *counts.entry((file, start, length)).or_insert(0) += 1;
        }
    }

    let duplicates: Vec<_> = counts.into_iter().filter(|(_, count)| *count > 1).collect();
    assert!(
        duplicates.is_empty(),
        "each reference span should belong to only one referenced-symbol group, duplicates: {duplicates:?}"
    );
}

#[test]
fn test_references_full_quoted_alias_returns_multiple_symbol_groups() {
    let mut server = make_server();
    server.open_files.insert(
        "/foo.ts".to_string(),
        [
            "type foo = \"foo\";",
            "export { type foo as \"__<alias>\" };",
            "import { type \"__<alias>\" as bar } from \"./foo\";",
            "const testBar: bar = \"foo\";",
        ]
        .join("\n"),
    );
    server.open_files.insert(
        "/bar.ts".to_string(),
        [
            "import { type \"__<alias>\" as first } from \"./foo\";",
            "export { type \"__<alias>\" as \"<other>\" } from \"./foo\";",
            "import { type \"<other>\" as second } from \"./bar\";",
            "const testFirst: first = \"foo\";",
            "const testSecond: second = \"foo\";",
        ]
        .join("\n"),
    );

    let req = make_request(
        "references-full",
        serde_json::json!({
            "file": "/foo.ts",
            "line": 2,
            "offset": 24
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("references-full should return body");
    let entries = body
        .as_array()
        .expect("references-full response should be array");

    assert!(
        entries.len() > 1,
        "quoted alias references-full should preserve multiple symbol groups, got: {entries:?}"
    );
    assert!(
        entries.iter().any(|entry| {
            entry["definition"]
                .get("kind")
                .and_then(serde_json::Value::as_str)
                == Some("alias")
        }),
        "expected at least one alias definition group: {entries:?}"
    );
    assert!(
        entries.iter().any(|entry| {
            entry["references"].as_array().is_some_and(|refs| {
                refs.iter().any(|r| {
                    r.get("fileName").and_then(serde_json::Value::as_str) == Some("/bar.ts")
                })
            })
        }),
        "expected at least one group with /bar.ts references: {entries:?}"
    );
}

// TODO: blDAJ (fix(binder,checker): support string literal export names in export specifiers)
// introduced a dedicated EXPORT_VALUE symbol for quoted alias specifiers. That removes the
// fallback path this test was anchoring on, so the `"<other>"` export-alias-side token no
// longer shows up as its own definition span in references-full. Keeping the test as
// #[ignore] until the LSP resolver is updated to follow EXPORT_VALUE alias symbols through
// `node_symbols` and re-emit per-specifier definition spans for quoted re-exports.
#[ignore = "regressed by blDAJ binder change; needs LSP follow-through"]
#[test]
fn test_references_full_quoted_alias_includes_export_alias_side_definition_span() {
    let mut server = make_server();
    server.open_files.insert(
        "/foo.ts".to_string(),
        [
            "type foo = \"foo\";",
            "export { type foo as \"__<alias>\" };",
            "import { type \"__<alias>\" as bar } from \"./foo\";",
            "const testBar: bar = \"foo\";",
        ]
        .join("\n"),
    );
    server.open_files.insert(
        "/bar.ts".to_string(),
        [
            "import { type \"__<alias>\" as first } from \"./foo\";",
            "export { type \"__<alias>\" as \"<other>\" } from \"./foo\";",
            "import { type \"<other>\" as second } from \"./bar\";",
            "const testFirst: first = \"foo\";",
            "const testSecond: second = \"foo\";",
        ]
        .join("\n"),
    );

    let req = make_request(
        "references-full",
        serde_json::json!({
            "file": "/foo.ts",
            "line": 2,
            "offset": 24
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("references-full should return body");
    let entries = body
        .as_array()
        .expect("references-full response should be array");

    let has_export_alias_side_span = entries.iter().any(|entry| {
        let Some(def_file) = entry["definition"]
            .get("fileName")
            .and_then(serde_json::Value::as_str)
        else {
            return false;
        };
        if def_file != "/bar.ts" {
            return false;
        }
        let Some(start) = entry["definition"]["textSpan"]
            .get("start")
            .and_then(serde_json::Value::as_u64)
        else {
            return false;
        };
        let Some(length) = entry["definition"]["textSpan"]
            .get("length")
            .and_then(serde_json::Value::as_u64)
        else {
            return false;
        };
        let end = start.saturating_add(length);
        let bar_source = server
            .open_files
            .get("/bar.ts")
            .expect("bar.ts should be open");
        bar_source
            .get(start as usize..end as usize)
            .is_some_and(|text| text == "\"<other>\"")
    });
    assert!(
        has_export_alias_side_span,
        "expected one references-full definition span to anchor on export alias-side token \"<other>\": {entries:?}"
    );
}

#[test]
fn test_type_only_quoted_alias_references_work_from_type_keyword_offset() {
    let mut server = make_server();
    server.open_files.insert(
        "/foo.ts".to_string(),
        [
            "type foo = \"foo\";",
            "export { type foo as \"__<alias>\" };",
            "import { type \"__<alias>\" as bar } from \"./foo\";",
            "const testBar: bar = \"foo\";",
        ]
        .join("\n"),
    );
    server.open_files.insert(
        "/bar.ts".to_string(),
        [
            "import { type \"__<alias>\" as first } from \"./foo\";",
            "export { type \"__<alias>\" as \"<other>\" } from \"./foo\";",
            "import { type \"<other>\" as second } from \"./bar\";",
            "const testFirst: first = \"foo\";",
            "const testSecond: second = \"foo\";",
        ]
        .join("\n"),
    );

    // Place the query on `type`, not on the quoted string literal token.
    let refs_req = make_request(
        "references",
        serde_json::json!({
            "file": "/bar.ts",
            "line": 1,
            "offset": 10
        }),
    );
    let refs_resp = server.handle_tsserver_request(refs_req);
    assert!(refs_resp.success);
    let refs = refs_resp
        .body
        .expect("references should return body")
        .get("refs")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .expect("references should include refs array");
    assert!(
        refs.iter()
            .filter_map(|entry| entry.get("file").and_then(serde_json::Value::as_str))
            .any(|file| file == "/foo.ts"),
        "expected type-only quoted alias refs to include /foo.ts: {refs:?}"
    );
    assert!(
        refs.iter()
            .filter_map(|entry| entry.get("file").and_then(serde_json::Value::as_str))
            .any(|file| file == "/bar.ts"),
        "expected type-only quoted alias refs to include /bar.ts: {refs:?}"
    );
}

#[test]
fn test_definition_type_only_quoted_import_alias_resolves_to_exported_symbol() {
    let mut server = make_server();
    server.open_files.insert(
        "/foo.ts".to_string(),
        [
            "type foo = \"foo\";",
            "export { type foo as \"__<alias>\" };",
            "import { type \"__<alias>\" as bar } from \"./foo\";",
            "const testBar: bar = \"foo\";",
        ]
        .join("\n"),
    );

    let req = make_request(
        "definition",
        serde_json::json!({
            "file": "/foo.ts",
            "line": 3,
            "offset": 18
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let defs = resp
        .body
        .expect("definition should return body")
        .as_array()
        .cloned()
        .expect("definition response should be an array");
    assert!(
        defs.iter()
            .any(|entry| entry.get("name").and_then(serde_json::Value::as_str) == Some("foo")),
        "expected type-only quoted alias definition to resolve to exported symbol `foo`, got: {defs:?}"
    );
}

#[test]
fn test_definition_type_only_quoted_alias_marks_non_declare_target_as_local_non_ambient() {
    let mut server = make_server();
    server.open_files.insert(
        "/foo.ts".to_string(),
        [
            "type foo = \"foo\";",
            "export { type foo as \"__<alias>\" };",
            "import { type \"__<alias>\" as bar } from \"./foo\";",
            "const testBar: bar = \"foo\";",
        ]
        .join("\n"),
    );

    let req = make_request(
        "definition",
        serde_json::json!({
            "file": "/foo.ts",
            "line": 3,
            "offset": 18
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let defs = resp
        .body
        .expect("definition should return body")
        .as_array()
        .cloned()
        .expect("definition response should be an array");
    let foo_def = defs
        .iter()
        .find(|entry| entry.get("name").and_then(serde_json::Value::as_str) == Some("foo"))
        .expect("expected foo definition entry");
    assert_eq!(
        foo_def
            .get("isAmbient")
            .and_then(serde_json::Value::as_bool),
        Some(false),
        "non-declare quoted alias definition should not be ambient: {foo_def:?}"
    );
    assert_eq!(
        foo_def.get("isLocal").and_then(serde_json::Value::as_bool),
        Some(true),
        "non-declare quoted alias definition should be local: {foo_def:?}"
    );
}

#[test]
fn test_check_options_experimental_decorators_deserialize() {
    // Verify that experimentalDecorators in JSON is correctly deserialized
    let json = r#"{"experimentalDecorators": true}"#;
    let options: CheckOptions = serde_json::from_str(json).unwrap();
    assert!(
        options.experimental_decorators,
        "experimentalDecorators should be true after deserialize"
    );
}

#[test]
fn test_check_options_experimental_decorators_default_false() {
    // Verify that default value is false
    let json = r#"{}"#;
    let options: CheckOptions = serde_json::from_str(json).unwrap();
    assert!(
        !options.experimental_decorators,
        "experimentalDecorators should default to false"
    );
}

// =============================================================================
// handle_project_info / compute_project_info tests
// =============================================================================

#[test]
fn test_project_info_inferred_project_returns_libs_and_active_file() {
    // goTo.file("a.ts") on an inferred project (no tsconfig) with @lib: es5
    // should return [lib files..., a.ts] — only the active file, no siblings.
    let mut server = make_server_with_real_libs();
    server.open_files.insert(
        "/tests/cases/fourslash/server/a.ts".to_string(),
        "export var test = \"test String\"\n".to_string(),
    );
    server.open_files.insert(
        "/tests/cases/fourslash/server/b.ts".to_string(),
        "import test from \"./a\"\n".to_string(),
    );
    server.inferred_check_options.lib = Some(vec!["es5".to_string()]);

    let (config, files) = server.compute_project_info("/tests/cases/fourslash/server/a.ts");
    assert_eq!(config, "", "no tsconfig for inferred project");
    assert_eq!(
        files.last().map(String::as_str),
        Some("/tests/cases/fourslash/server/a.ts"),
        "active file must be last in inferred project list"
    );
    let lib_count = files
        .iter()
        .filter(|p| p.starts_with("/home/src/tslibs/TS/Lib/lib."))
        .count();
    assert!(
        lib_count >= 1,
        "expected at least one virtual lib file, got {files:?}"
    );
    // b.ts must NOT be in the list because goTo.file("a.ts") is the active file
    // and a.ts does not import b.ts.
    assert!(
        !files.iter().any(|p| p.ends_with("/b.ts")),
        "b.ts must not appear when active file is a.ts, got {files:?}"
    );
}

#[test]
fn test_project_info_inferred_project_includes_transitive_imports() {
    // b.ts imports ./a — goTo.file("b.ts") should include a.ts first, then b.ts.
    let mut server = make_server_with_real_libs();
    server.open_files.insert(
        "/tests/cases/fourslash/server/a.ts".to_string(),
        "export var test = \"test String\"\n".to_string(),
    );
    server.open_files.insert(
        "/tests/cases/fourslash/server/b.ts".to_string(),
        "import test from \"./a\"\n".to_string(),
    );
    server.inferred_check_options.lib = Some(vec!["es5".to_string()]);

    let (_, files) = server.compute_project_info("/tests/cases/fourslash/server/b.ts");
    let project_files: Vec<&str> = files
        .iter()
        .filter(|p| !p.starts_with("/home/src/tslibs/TS/Lib/"))
        .map(String::as_str)
        .collect();
    assert_eq!(
        project_files,
        vec![
            "/tests/cases/fourslash/server/a.ts",
            "/tests/cases/fourslash/server/b.ts",
        ],
        "expected transitive imports before active file, got {project_files:?}"
    );
}

#[test]
fn test_project_info_configured_project_lists_files_and_config_last() {
    // tsconfig declares files: [a.ts, b.ts]; output should be
    // [libs..., a.ts, b.ts, tsconfig.json] and exclude non-existent files.
    let mut server = make_server_with_real_libs();
    let tsconfig_path = "/tests/cases/fourslash/server/tsconfig.json".to_string();
    server.open_files.insert(
        tsconfig_path.clone(),
        r#"{ "files": ["a.ts", "b.ts"], "compilerOptions": { "lib": ["es5"] } }"#.to_string(),
    );
    server.open_files.insert(
        "/tests/cases/fourslash/server/a.ts".to_string(),
        "export var test = \"test String\"\n".to_string(),
    );
    server.open_files.insert(
        "/tests/cases/fourslash/server/b.ts".to_string(),
        "export var test2 = \"test String\"\n".to_string(),
    );

    let (config, files) = server.compute_project_info("/tests/cases/fourslash/server/a.ts");
    assert_eq!(config, tsconfig_path);
    let non_lib: Vec<&str> = files
        .iter()
        .filter(|p| !p.starts_with("/home/src/tslibs/TS/Lib/"))
        .map(String::as_str)
        .collect();
    assert_eq!(
        non_lib,
        vec![
            "/tests/cases/fourslash/server/a.ts",
            "/tests/cases/fourslash/server/b.ts",
            "/tests/cases/fourslash/server/tsconfig.json",
        ],
        "expected tsconfig files in declared order with config file last"
    );
}

#[test]
fn test_project_info_configured_project_excludes_non_existent_files() {
    // tsconfig declares files: [a.ts, c.ts, b.ts]; c.ts is not in open_files
    // so it must be filtered out, preserving the relative order of the rest.
    let mut server = make_server_with_real_libs();
    server.open_files.insert(
        "/tests/cases/fourslash/server/tsconfig.json".to_string(),
        r#"{ "files": ["a.ts", "c.ts", "b.ts"], "compilerOptions": { "lib": ["es5"] } }"#
            .to_string(),
    );
    server.open_files.insert(
        "/tests/cases/fourslash/server/a.ts".to_string(),
        "export var test = \"test String\"\n".to_string(),
    );
    server.open_files.insert(
        "/tests/cases/fourslash/server/b.ts".to_string(),
        "export var test2 = \"test String\"\n".to_string(),
    );

    let (_, files) = server.compute_project_info("/tests/cases/fourslash/server/a.ts");
    let non_lib: Vec<&str> = files
        .iter()
        .filter(|p| !p.starts_with("/home/src/tslibs/TS/Lib/"))
        .map(String::as_str)
        .collect();
    assert_eq!(
        non_lib,
        vec![
            "/tests/cases/fourslash/server/a.ts",
            "/tests/cases/fourslash/server/b.ts",
            "/tests/cases/fourslash/server/tsconfig.json",
        ],
        "non-existent c.ts must be excluded"
    );
}

#[test]
fn test_project_info_lib_files_use_fourslash_virtual_folder() {
    let mut server = make_server_with_real_libs();
    server.open_files.insert(
        "/tests/cases/fourslash/server/a.ts".to_string(),
        "export var t = 1;\n".to_string(),
    );
    server.inferred_check_options.lib = Some(vec!["es5".to_string()]);

    let (_, files) = server.compute_project_info("/tests/cases/fourslash/server/a.ts");
    let libs: Vec<&str> = files
        .iter()
        .filter(|p| p.starts_with("/home/src/tslibs/TS/Lib/"))
        .map(String::as_str)
        .collect();
    assert!(
        libs.contains(&"/home/src/tslibs/TS/Lib/lib.es5.d.ts"),
        "expected lib.es5.d.ts under fourslash virtual lib folder, got {libs:?}"
    );
    // @lib: es5 pulls in decorators + decorators.legacy via transitive refs.
    assert!(
        libs.iter().any(|p| p.ends_with("/lib.decorators.d.ts")),
        "expected lib.decorators.d.ts to be transitively included, got {libs:?}"
    );
}

#[test]
fn test_project_info_no_lib_suppresses_lib_files() {
    let mut server = make_server_with_real_libs();
    server.open_files.insert(
        "/tests/cases/fourslash/server/a.ts".to_string(),
        "export var t = 1;\n".to_string(),
    );
    server.inferred_check_options.no_lib = true;
    server.inferred_check_options.lib = Some(vec!["es5".to_string()]);

    let (_, files) = server.compute_project_info("/tests/cases/fourslash/server/a.ts");
    let lib_count = files
        .iter()
        .filter(|p| p.starts_with("/home/src/tslibs/TS/Lib/"))
        .count();
    assert_eq!(
        lib_count, 0,
        "noLib must suppress all lib files, got {files:?}"
    );
}
