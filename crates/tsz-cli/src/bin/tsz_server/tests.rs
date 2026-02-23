use super::*;

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
        auto_imports_allowed_for_inferred_projects: true,
        inferred_module_is_none_for_projects: false,
        auto_import_specifier_exclude_regexes: Vec::new(),
        include_completions_with_class_member_snippets: false,
    }
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
        serde_json::json!([{"kind":"text","text":"Doc"}])
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
        serde_json::json!([{"kind":"text","text":"Doc"}])
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
fn test_format_range_paste_matches_fourslash_auto_formatting_on_paste() {
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
    let expected = "namespace TestModule {\n    class TestClass {\n        private foo;\n        public testMethod() { }\n    }\n}\n";
    assert_eq!(updated, expected);
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
fn test_completion_info_member_excludes_private_class_property() {
    let mut server = make_server();
    let source = "class n {\n    constructor (public x: number, public y: number, private z: string) { }\n}\nvar t = new n(0, 1, '');\nt.";
    server
        .open_files
        .insert("/test.ts".to_string(), source.to_string());
    let req = make_request(
        "completionInfo",
        serde_json::json!({"file": "/test.ts", "line": 5, "offset": 3}),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("completionInfo should return a body");
    assert_eq!(body["isMemberCompletion"], serde_json::json!(true));

    let entries = body["entries"]
        .as_array()
        .expect("completionInfo should include entries");
    let names: Vec<&str> = entries
        .iter()
        .filter_map(|entry| entry.get("name").and_then(serde_json::Value::as_str))
        .collect();

    assert!(
        names.contains(&"x"),
        "Expected public member x in completions"
    );
    assert!(
        names.contains(&"y"),
        "Expected public member y in completions"
    );
    assert!(
        !names.contains(&"z"),
        "Private member z should not be suggested in member completions"
    );
}

#[test]
fn test_completion_info_global_keywords_rank_ahead_of_globals() {
    let mut server = make_server();
    server
        .open_files
        .insert("/index.ts".to_string(), "".to_string());
    server.open_files.insert(
        "/lib.ts".to_string(),
        "export const Button = 1;\n".to_string(),
    );

    let req = make_request(
        "completionInfo",
        serde_json::json!({
            "file": "/index.ts",
            "line": 1,
            "offset": 1,
            "preferences": { "includeCompletionsForModuleExports": true }
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("completionInfo should return a body");
    assert_eq!(body["isMemberCompletion"], serde_json::json!(false));

    let entries = body["entries"]
        .as_array()
        .expect("completionInfo should include entries");
    let names: Vec<&str> = entries
        .iter()
        .filter_map(|entry| entry.get("name").and_then(serde_json::Value::as_str))
        .collect();

    let abstract_idx = names
        .iter()
        .position(|name| *name == "abstract")
        .expect("Expected keyword 'abstract' in completion list");
    let array_idx = names
        .iter()
        .position(|name| *name == "Array")
        .expect("Expected global 'Array' in completion list");
    assert!(
        abstract_idx < array_idx,
        "Expected keyword ordering to rank before globals"
    );
}

#[test]
fn test_completion_entry_details_auto_import_omits_documentation() {
    let mut server = make_server();
    server.open_files.insert(
        "/a.ts".to_string(),
        "export function foo() {}\n".to_string(),
    );
    server
        .open_files
        .insert("/b.ts".to_string(), "fo;\n".to_string());

    let req = make_request(
        "completionEntryDetails",
        serde_json::json!({
            "file": "/b.ts",
            "line": 1,
            "offset": 3,
            "entryNames": [{ "name": "foo", "source": "./a" }],
            "preferences": { "includeCompletionsForModuleExports": true }
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp
        .body
        .expect("completionEntryDetails should return a body");
    let details = body
        .as_array()
        .expect("completionEntryDetails should return an array");
    let first = details
        .first()
        .expect("completionEntryDetails should include one entry");
    assert!(
        first.get("documentation").is_none(),
        "auto-import completion details should omit documentation to match tsserver parity"
    );
}

#[test]
fn test_completion_entry_details_auto_import_uses_update_description_when_import_exists() {
    let mut server = make_server();
    server.open_files.insert(
        "/a.ts".to_string(),
        "export const existing = 1;\nexport function foo() {}\n".to_string(),
    );
    server.open_files.insert(
        "/b.ts".to_string(),
        "import { existing } from \"./a\";\nfo;\n".to_string(),
    );

    let req = make_request(
        "completionEntryDetails",
        serde_json::json!({
            "file": "/b.ts",
            "line": 2,
            "offset": 3,
            "entryNames": [{ "name": "foo", "source": "./a" }],
            "preferences": { "includeCompletionsForModuleExports": true }
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp
        .body
        .expect("completionEntryDetails should return a body");
    let details = body
        .as_array()
        .expect("completionEntryDetails should return an array");
    let first = details
        .first()
        .expect("completionEntryDetails should include one entry");
    let code_actions = first
        .get("codeActions")
        .and_then(serde_json::Value::as_array)
        .expect("auto-import completion should include code actions");
    let description = code_actions
        .first()
        .and_then(|action| action.get("description"))
        .and_then(serde_json::Value::as_str)
        .expect("code action should include a description");
    assert_eq!(description, "Update import from \"./a\"");
}

#[test]
fn test_auto_import_description_prefers_module_specifier_from_edit_text() {
    let edit = tsz::lsp::rename::TextEdit {
        range: tsz::lsp::position::Range::new(
            tsz::lsp::position::Position::new(0, 0),
            tsz::lsp::position::Position::new(0, 0),
        ),
        new_text: "import type { I } from \"./mod.js\";\n\n".to_string(),
    };
    let description = Server::auto_import_code_action_description(
        "const x: I;",
        "/a.mts",
        Some("./mod"),
        &[edit],
        "I",
    );
    assert_eq!(description, "Add import from \"./mod.js\"");
}

#[test]
fn test_auto_import_description_mts_fallback_source_adds_js_extension() {
    let edit = tsz::lsp::rename::TextEdit {
        range: tsz::lsp::position::Range::new(
            tsz::lsp::position::Position::new(0, 0),
            tsz::lsp::position::Position::new(0, 0),
        ),
        new_text: "import type { I }".to_string(),
    };
    let description = Server::auto_import_code_action_description(
        "const x: I;",
        "/a.mts",
        Some("./mod"),
        &[edit],
        "I",
    );
    assert_eq!(description, "Add import from \"./mod.js\"");
}

#[test]
fn test_normalize_mts_auto_import_edit_text_uses_import_type_and_js_extension() {
    let normalized = Server::normalize_mts_auto_import_edit_text(
        "/a.mts",
        tsz::lsp::completions::CompletionItemKind::Interface,
        "",
        "import { I } from \"./mod\";\n\n",
    );
    assert_eq!(normalized, "import type { I } from \"./mod.js\";\n\n");
}

#[test]
fn test_normalize_mts_auto_import_edit_text_preserves_existing_type_only_members() {
    let normalized = Server::normalize_mts_auto_import_edit_text(
        "/a.mts",
        tsz::lsp::completions::CompletionItemKind::Class,
        "import type { I } from \"./mod.js\";\n\nconst x: I = new C",
        "import { C, I } from \"./mod\";\n\n",
    );
    assert_eq!(normalized, "import { C, type I } from \"./mod.js\";\n\n");
}

#[test]
fn test_get_code_fixes_uses_configured_auto_import_specifier_exclude_regexes() {
    let mut server = make_server();
    server.open_files.insert(
        "/tsconfig.json".to_string(),
        r#"{
  "compilerOptions": {
    "module": "preserve",
    "paths": {
      "@app/*": ["./src/*"]
    }
  }
}"#
        .to_string(),
    );
    server.open_files.insert(
        "/src/utils.ts".to_string(),
        "export function add(a: number, b: number) {}".to_string(),
    );
    server
        .open_files
        .insert("/src/index.ts".to_string(), "add".to_string());

    let mut module_specifiers_for_prefs = |preferences: serde_json::Value| -> Vec<String> {
        let configure_req = make_request(
            "configure",
            serde_json::json!({ "preferences": preferences }),
        );
        let configure_resp = server.handle_tsserver_request(configure_req);
        assert!(configure_resp.success);

        let fixes_req = make_request(
            "getCodeFixes",
            serde_json::json!({
                "file": "/src/index.ts",
                "startLine": 1,
                "startOffset": 1,
                "endLine": 1,
                "endOffset": 4,
                "errorCodes": [2304]
            }),
        );
        let fixes_resp = server.handle_tsserver_request(fixes_req);
        assert!(fixes_resp.success);
        let fixes = fixes_resp
            .body
            .expect("getCodeFixes should return a body")
            .as_array()
            .expect("getCodeFixes body should be an array")
            .clone();

        let mut specifiers = Vec::new();
        for fix in fixes {
            if fix.get("fixName").and_then(serde_json::Value::as_str) != Some("import") {
                continue;
            }
            let Some(changes) = fix.get("changes").and_then(serde_json::Value::as_array) else {
                continue;
            };
            for change in changes {
                let Some(text_changes) = change
                    .get("textChanges")
                    .and_then(serde_json::Value::as_array)
                else {
                    continue;
                };
                for text_change in text_changes {
                    let Some(new_text) = text_change
                        .get("newText")
                        .and_then(serde_json::Value::as_str)
                    else {
                        continue;
                    };
                    if let Some(capture) = new_text
                        .split("from ")
                        .nth(1)
                        .and_then(|rest| rest.split(['"', '\'']).nth(1))
                    {
                        specifiers.push(capture.to_string());
                    }
                }
            }
        }
        specifiers
    };

    assert_eq!(
        module_specifiers_for_prefs(serde_json::json!({})),
        vec!["./utils".to_string()]
    );
    assert_eq!(
        module_specifiers_for_prefs(serde_json::json!({
            "autoImportSpecifierExcludeRegexes": ["^\\./"]
        })),
        vec!["@app/utils".to_string()]
    );
    assert_eq!(
        module_specifiers_for_prefs(serde_json::json!({
            "importModuleSpecifierPreference": "non-relative"
        })),
        vec!["@app/utils".to_string()]
    );
    assert_eq!(
        module_specifiers_for_prefs(serde_json::json!({
            "importModuleSpecifierPreference": "non-relative",
            "autoImportSpecifierExcludeRegexes": ["^@app/"]
        })),
        vec!["./utils".to_string()]
    );
    assert!(
        module_specifiers_for_prefs(serde_json::json!({
            "autoImportSpecifierExcludeRegexes": ["utils"]
        }))
        .is_empty()
    );
}

#[test]
fn test_get_code_fixes_supports_jsonc_jsconfig_paths_shortest_preference() {
    let mut server = make_server();
    server.open_files.insert(
        "/package1/jsconfig.json".to_string(),
        r#"{
  "compilerOptions": {
    checkJs: true,
    "paths": {
      "package1/*": ["./*"],
      "package2/*": ["../package2/*"]
    },
    "baseUrl": "."
  },
  "include": [
    ".",
    "../package2"
  ]
}"#
        .to_string(),
    );
    server
        .open_files
        .insert("/package1/file1.js".to_string(), "bar".to_string());
    server.open_files.insert(
        "/package2/file1.js".to_string(),
        "export const bar = 0;".to_string(),
    );

    let configure_req = make_request(
        "configure",
        serde_json::json!({
            "preferences": {
                "includeCompletionsForModuleExports": true,
                "importModuleSpecifierPreference": "shortest"
            }
        }),
    );
    let configure_resp = server.handle_tsserver_request(configure_req);
    assert!(configure_resp.success);

    let fixes_req = make_request(
        "getCodeFixes",
        serde_json::json!({
            "file": "/package1/file1.js",
            "startLine": 1,
            "startOffset": 1,
            "endLine": 1,
            "endOffset": 4,
            "errorCodes": [2304]
        }),
    );
    let fixes_resp = server.handle_tsserver_request(fixes_req);
    assert!(fixes_resp.success);

    let fixes = fixes_resp
        .body
        .expect("getCodeFixes should return a body")
        .as_array()
        .expect("getCodeFixes body should be an array")
        .clone();

    let mut specifiers = Vec::new();
    for fix in fixes {
        if fix.get("fixName").and_then(serde_json::Value::as_str) != Some("import") {
            continue;
        }
        let Some(changes) = fix.get("changes").and_then(serde_json::Value::as_array) else {
            continue;
        };
        for change in changes {
            let Some(text_changes) = change
                .get("textChanges")
                .and_then(serde_json::Value::as_array)
            else {
                continue;
            };
            for text_change in text_changes {
                let Some(new_text) = text_change
                    .get("newText")
                    .and_then(serde_json::Value::as_str)
                else {
                    continue;
                };
                if let Some(capture) = new_text
                    .split("from ")
                    .nth(1)
                    .and_then(|rest| rest.split(['"', '\'']).nth(1))
                {
                    specifiers.push(capture.to_string());
                }
            }
        }
    }

    assert_eq!(specifiers, vec!["package2/file1".to_string()]);
}

#[test]
fn test_open_external_project_populates_auto_import_code_fixes() {
    let mut server = make_server();

    let open_external = make_request(
        "openExternalProject",
        serde_json::json!({
            "projectFileName": "/project.csproj",
            "rootFiles": [
                {
                    "fileName": "/node_modules/lib/index.d.ts",
                    "content": "declare module \"ambient\" { export const x: number; }\ndeclare module \"ambient/utils\" { export const x: number; }\n"
                },
                {
                    "fileName": "/index.ts",
                    "content": "x"
                }
            ]
        }),
    );
    let open_resp = server.handle_tsserver_request(open_external);
    assert!(open_resp.success);

    let fixes_req = make_request(
        "getCodeFixes",
        serde_json::json!({
            "file": "/index.ts",
            "startLine": 1,
            "startOffset": 1,
            "endLine": 1,
            "endOffset": 2,
            "errorCodes": [2304],
            "preferences": { "includeCompletionsForModuleExports": true }
        }),
    );
    let fixes_resp = server.handle_tsserver_request(fixes_req);
    assert!(fixes_resp.success);
    let fixes = fixes_resp
        .body
        .expect("getCodeFixes should return a body")
        .as_array()
        .expect("getCodeFixes body should be an array")
        .clone();

    let mut specifiers = Vec::new();
    for fix in fixes {
        if fix.get("fixName").and_then(serde_json::Value::as_str) != Some("import") {
            continue;
        }
        let Some(changes) = fix.get("changes").and_then(serde_json::Value::as_array) else {
            continue;
        };
        for change in changes {
            let Some(text_changes) = change
                .get("textChanges")
                .and_then(serde_json::Value::as_array)
            else {
                continue;
            };
            for text_change in text_changes {
                let Some(new_text) = text_change
                    .get("newText")
                    .and_then(serde_json::Value::as_str)
                else {
                    continue;
                };
                if let Some(capture) = new_text
                    .split("from ")
                    .nth(1)
                    .and_then(|rest| rest.split(['"', '\'']).nth(1))
                {
                    specifiers.push(capture.to_string());
                }
            }
        }
    }
    assert_eq!(
        specifiers,
        vec!["ambient".to_string(), "ambient/utils".to_string()]
    );

    let close_external = make_request(
        "closeExternalProject",
        serde_json::json!({ "projectFileName": "/project.csproj" }),
    );
    let close_resp = server.handle_tsserver_request(close_external);
    assert!(close_resp.success);
    assert!(
        !server
            .open_files
            .contains_key("/node_modules/lib/index.d.ts")
    );
    assert!(!server.open_files.contains_key("/index.ts"));
}

#[test]
fn test_open_external_project_tracks_root_files_without_inline_content() {
    let mut server = make_server();

    let open_external = make_request(
        "openExternalProject",
        serde_json::json!({
            "projectFileName": "/project.csproj",
            "rootFiles": [
                { "fileName": "/virtual/index.ts" },
                { "fileName": "/node_modules/.pnpm/mobx@6.0.4/node_modules/mobx/dist/mobx.d.ts" }
            ]
        }),
    );
    let open_resp = server.handle_tsserver_request(open_external);
    assert!(open_resp.success);

    let tracked = server
        .external_project_files
        .get("/project.csproj")
        .expect("expected tracked external project files");
    assert!(
        tracked.iter().any(|path| path == "/virtual/index.ts"),
        "expected virtual root file path to be tracked, got {tracked:?}"
    );
    assert!(
        tracked
            .iter()
            .any(|path| path == "/node_modules/.pnpm/mobx@6.0.4/node_modules/mobx/dist/mobx.d.ts"),
        "expected node_modules root file path to be tracked, got {tracked:?}"
    );
}

#[test]
fn test_open_external_project_module_none_es5_blocks_auto_import_completions() {
    let mut server = make_server();

    let open_external = make_request(
        "openExternalProject",
        serde_json::json!({
            "projectFileName": "/project.csproj",
            "options": {
                "module": "none",
                "target": "es5"
            },
            "rootFiles": [
                {
                    "fileName": "/node_modules/dep/index.d.ts",
                    "content": "export const x: number;\n"
                },
                {
                    "fileName": "/index.ts",
                    "content": "x"
                }
            ]
        }),
    );
    let open_resp = server.handle_tsserver_request(open_external);
    assert!(open_resp.success);

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
        "auto-import completion should be gated for module:none + target:es5 inferred project"
    );
}

#[test]
fn test_completion_info_partial_ambient_file_exclusion_keeps_merged_module_exports() {
    let mut server = make_server();
    server.open_files.insert(
        "/ambient1.d.ts".to_string(),
        "declare module \"foo\" { export const x = 1; }\n".to_string(),
    );
    server.open_files.insert(
        "/ambient2.d.ts".to_string(),
        "declare module \"foo\" { export const y = 2; }\n".to_string(),
    );
    server
        .open_files
        .insert("/index.ts".to_string(), "".to_string());

    let completion_req = make_request(
        "completionInfo",
        serde_json::json!({
            "file": "/index.ts",
            "line": 1,
            "offset": 1,
            "preferences": {
                "allowIncompleteCompletions": true,
                "includeCompletionsForModuleExports": true,
                "autoImportFileExcludePatterns": ["/**/ambient1.d.ts"]
            }
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

    let has_x_from_foo = entries.iter().any(|entry| {
        entry.get("name").and_then(serde_json::Value::as_str) == Some("x")
            && entry.get("source").and_then(serde_json::Value::as_str) == Some("foo")
    });
    let has_y_from_foo = entries.iter().any(|entry| {
        entry.get("name").and_then(serde_json::Value::as_str) == Some("y")
            && entry.get("source").and_then(serde_json::Value::as_str) == Some("foo")
    });

    assert!(
        has_x_from_foo,
        "expected ambient export `x` from module `foo` to remain when only one declaration file is excluded"
    );
    assert!(
        has_y_from_foo,
        "expected ambient export `y` from module `foo` to remain when only one declaration file is excluded"
    );
}

#[test]
fn test_completion_info_full_ambient_file_exclusion_hides_merged_module_exports() {
    let mut server = make_server();
    server.open_files.insert(
        "/ambient1.d.ts".to_string(),
        "declare module \"foo\" { export const x = 1; }\n".to_string(),
    );
    server.open_files.insert(
        "/ambient2.d.ts".to_string(),
        "declare module \"foo\" { export const y = 2; }\n".to_string(),
    );
    server
        .open_files
        .insert("/index.ts".to_string(), "".to_string());

    let completion_req = make_request(
        "completionInfo",
        serde_json::json!({
            "file": "/index.ts",
            "line": 1,
            "offset": 1,
            "preferences": {
                "allowIncompleteCompletions": true,
                "includeCompletionsForModuleExports": true,
                "autoImportFileExcludePatterns": ["/**/ambient*"]
            }
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

    assert!(
        !entries.iter().any(|entry| {
            entry.get("source").and_then(serde_json::Value::as_str) == Some("foo")
        }),
        "expected ambient module `foo` completions to be excluded when all declaration files are excluded"
    );
}

#[test]
fn test_completion_info_contextual_string_literal_keyof_constraint() {
    let mut server = make_server();
    let source = "interface Events { click: any; drag: any; }\ndeclare function addListener<K extends keyof Events>(type: K, listener: (ev: Events[K]) => any): void;\naddListener(\"\")\n";
    server
        .open_files
        .insert("/test.ts".to_string(), source.to_string());

    let req = make_request(
        "completionInfo",
        serde_json::json!({
            "file": "/test.ts",
            "line": 3,
            "offset": 14
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("completionInfo should return a body");
    let entries = body["entries"]
        .as_array()
        .expect("completionInfo should include entries");
    let names: Vec<&str> = entries
        .iter()
        .filter_map(|entry| entry.get("name").and_then(serde_json::Value::as_str))
        .collect();
    assert!(
        names.contains(&"click"),
        "expected 'click' completion, got {names:?}"
    );
    assert!(
        names.contains(&"drag"),
        "expected 'drag' completion, got {names:?}"
    );

    let completions_req = make_request(
        "completions",
        serde_json::json!({
            "file": "/test.ts",
            "line": 3,
            "offset": 14
        }),
    );
    let completions_resp = server.handle_tsserver_request(completions_req);
    assert!(completions_resp.success);
    let completions_body = completions_resp
        .body
        .expect("completions should return a body");
    let completion_entries = completions_body["entries"]
        .as_array()
        .expect("completions should include entries");
    let completion_names: Vec<&str> = completion_entries
        .iter()
        .filter_map(|entry| entry.get("name").and_then(serde_json::Value::as_str))
        .collect();
    assert!(
        completion_names.contains(&"click"),
        "expected 'click' in completions, got {completion_names:?}"
    );
    assert!(
        completion_names.contains(&"drag"),
        "expected 'drag' in completions, got {completion_names:?}"
    );
}

#[test]
fn test_completion_info_globals_exclude_synthetic_commonjs_helpers() {
    let mut server = make_server();
    server.open_files.insert(
        "/tsconfig.json".to_string(),
        r#"{
  "compilerOptions": {
    "module": "commonjs",
    "lib": ["es5"]
  }
}"#
        .to_string(),
    );
    server
        .open_files
        .insert("/index.ts".to_string(), "".to_string());

    let completion_req = make_request(
        "completionInfo",
        serde_json::json!({
            "file": "/index.ts",
            "line": 1,
            "offset": 1,
            "preferences": {
                "allowIncompleteCompletions": true
            }
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

    let names: std::collections::HashSet<&str> = entries
        .iter()
        .filter_map(|entry| entry.get("name").and_then(serde_json::Value::as_str))
        .collect();
    assert!(
        !names.contains("exports"),
        "expected synthetic CommonJS helper `exports` to be excluded from globals completions"
    );
    assert!(
        !names.contains("require"),
        "expected synthetic CommonJS helper `require` to be excluded from globals completions"
    );
}

#[test]
fn test_completion_info_auto_import_export_equals_type_only_preferred() {
    let mut server = make_server();
    server.open_files.insert(
        "/tsconfig.json".to_string(),
        r#"{
  "compilerOptions": {
    "verbatimModuleSyntax": true,
    "module": "esnext",
    "moduleResolution": "bundler"
  }
}"#
        .to_string(),
    );
    server.open_files.insert(
        "/ts.d.ts".to_string(),
        "declare namespace ts {\n  interface SourceFile {\n    text: string;\n  }\n  function createSourceFile(): SourceFile;\n}\nexport = ts;\n".to_string(),
    );
    server.open_files.insert(
        "/types.ts".to_string(),
        "export interface VFS {\n  getSourceFile(path: string): ts/**/\n}\n".to_string(),
    );

    let completion_req = make_request(
        "completionInfo",
        serde_json::json!({
            "file": "/types.ts",
            "line": 2,
            "offset": 34,
            "preferences": {
                "includeCompletionsForModuleExports": true,
                "allowIncompleteCompletions": true
            }
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

    let has_ts_auto_import = entries.iter().any(|entry| {
        entry.get("name").and_then(serde_json::Value::as_str) == Some("ts")
            && entry.get("source").and_then(serde_json::Value::as_str) == Some("./ts")
            && entry.get("hasAction").and_then(serde_json::Value::as_bool) == Some(true)
            && entry.get("sortText").and_then(serde_json::Value::as_str) == Some("16")
    });
    let ts_entries: Vec<&serde_json::Value> = entries
        .iter()
        .filter(|entry| entry.get("name").and_then(serde_json::Value::as_str) == Some("ts"))
        .collect();
    assert_eq!(
        ts_entries.len(),
        1,
        "expected a single `ts` completion entry, got: {ts_entries:?}"
    );
    let ts_entry = ts_entries[0];
    let source_display = ts_entry
        .get("sourceDisplay")
        .and_then(serde_json::Value::as_array)
        .and_then(|parts| parts.first())
        .and_then(|part| part.get("text"))
        .and_then(serde_json::Value::as_str);
    assert_eq!(
        source_display,
        Some("./ts"),
        "expected completionInfo sourceDisplay display parts for `ts`, got: {ts_entry:?}"
    );
    assert!(
        has_ts_auto_import,
        "expected ts auto-import completion from ./ts, got entries: {entries:?}"
    );

    let details_req = make_request(
        "completionEntryDetails",
        serde_json::json!({
            "file": "/types.ts",
            "line": 2,
            "offset": 34,
            "entryNames": [{ "name": "ts", "source": "./ts" }],
            "preferences": {
                "includeCompletionsForModuleExports": true,
                "allowIncompleteCompletions": true
            }
        }),
    );
    let details_resp = server.handle_tsserver_request(details_req);
    assert!(details_resp.success);
    let details_body = details_resp
        .body
        .expect("completionEntryDetails should return a body");
    let details = details_body
        .as_array()
        .expect("completionEntryDetails should return an array");
    let first = details
        .first()
        .expect("completionEntryDetails should include one entry");
    let code_actions = first
        .get("codeActions")
        .and_then(serde_json::Value::as_array)
        .expect("completion details should include auto-import code actions");
    let text_changes = code_actions
        .first()
        .and_then(|action| action.get("changes"))
        .and_then(serde_json::Value::as_array)
        .and_then(|changes| changes.first())
        .and_then(|change| change.get("textChanges"))
        .and_then(serde_json::Value::as_array)
        .expect("auto-import code action should include text changes");
    let import_text = text_changes
        .first()
        .and_then(|change| change.get("newText"))
        .and_then(serde_json::Value::as_str)
        .expect("auto-import text change should include newText");
    assert!(
        import_text.contains("import type ts from \"./ts\";"),
        "expected type-only default import text edit, got: {import_text}"
    );
}

#[test]
fn test_completion_info_verbatim_commonjs_auto_imports_include_require_member_forms() {
    let mut server = make_server();
    server.open_files.insert(
        "/node_modules/@types/node/path.d.ts".to_string(),
        "declare module 'path' {\n  namespace path {\n    interface PlatformPath {\n      normalize(p: string): string;\n      join(...paths: string[]): string;\n      resolve(...pathSegments: string[]): string;\n      isAbsolute(p: string): boolean;\n    }\n  }\n  const path: path.PlatformPath;\n  export = path;\n}\n"
            .to_string(),
    );
    server.open_files.insert(
        "/cool-name.js".to_string(),
        "module.exports = {\n  explode: () => {}\n}\n".to_string(),
    );
    server.open_files.insert(
        "/a.ts".to_string(),
        "// @module: node18\n// @verbatimModuleSyntax: true\n// @allowJs: true\n/**/\n".to_string(),
    );

    let completion_req = make_request(
        "completionInfo",
        serde_json::json!({
            "file": "/a.ts",
            "line": 4,
            "offset": 1,
            "preferences": {
                "includeCompletionsForModuleExports": true,
                "allowIncompleteCompletions": true
            }
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

    let normalize = entries
        .iter()
        .find(|entry| {
            entry.get("name").and_then(serde_json::Value::as_str) == Some("normalize")
                && entry.get("source").and_then(serde_json::Value::as_str) == Some("path")
        })
        .expect("expected `normalize` auto-import entry from `path`");
    assert_eq!(
        normalize
            .get("insertText")
            .and_then(serde_json::Value::as_str),
        Some("path.normalize")
    );

    let explode = entries
        .iter()
        .find(|entry| {
            entry.get("name").and_then(serde_json::Value::as_str) == Some("explode")
                && entry.get("source").and_then(serde_json::Value::as_str) == Some("./cool-name")
        })
        .expect("expected `explode` auto-import entry from `./cool-name`");
    assert_eq!(
        explode
            .get("insertText")
            .and_then(serde_json::Value::as_str),
        Some("coolName.explode")
    );

    let details_req = make_request(
        "completionEntryDetails",
        serde_json::json!({
            "file": "/a.ts",
            "line": 4,
            "offset": 1,
            "entryNames": [{ "name": "normalize", "source": "path" }],
            "preferences": {
                "includeCompletionsForModuleExports": true,
                "allowIncompleteCompletions": true
            }
        }),
    );
    let details_resp = server.handle_tsserver_request(details_req);
    assert!(details_resp.success);
    let details_body = details_resp
        .body
        .expect("completionEntryDetails should return a body");
    let details = details_body
        .as_array()
        .expect("completionEntryDetails should return an array");
    let first = details
        .first()
        .expect("completionEntryDetails should include one entry");
    let text_changes = first
        .get("codeActions")
        .and_then(serde_json::Value::as_array)
        .and_then(|actions| actions.first())
        .and_then(|action| action.get("changes"))
        .and_then(serde_json::Value::as_array)
        .and_then(|changes| changes.first())
        .and_then(|change| change.get("textChanges"))
        .and_then(serde_json::Value::as_array)
        .expect("completion details should include text changes");
    let import_text = text_changes
        .first()
        .and_then(|change| change.get("newText"))
        .and_then(serde_json::Value::as_str)
        .expect("completion details should include import text");
    assert!(
        import_text.contains("import path = require(\"path\");"),
        "expected `import = require` edit for `path`, got: {import_text}"
    );
}

#[test]
fn test_get_code_fixes_verbatim_commonjs_fallback_rewrites_missing_member() {
    let mut server = make_server();
    server.open_files.insert(
        "/node_modules/@types/node/path.d.ts".to_string(),
        "declare module 'path' {\n  namespace path {\n    interface PlatformPath {\n      normalize(p: string): string;\n    }\n  }\n  const path: path.PlatformPath;\n  export = path;\n}\n"
            .to_string(),
    );
    server.open_files.insert(
        "/a.ts".to_string(),
        "// @module: node18\n// @verbatimModuleSyntax: true\n// @allowJs: true\nnormalize\n"
            .to_string(),
    );

    let req = TsServerRequest {
        seq: 1,
        _msg_type: "request".to_string(),
        command: "getCodeFixes".to_string(),
        arguments: serde_json::json!({
            "file": "/a.ts",
            "startLine": 4,
            "startOffset": 1,
            "endLine": 4,
            "endOffset": 10,
            "errorCodes": [2304]
        }),
    };
    let resp = server.handle_get_code_fixes(1, &req);
    assert!(resp.success, "expected getCodeFixes to succeed");
    let body = resp.body.expect("expected getCodeFixes body");
    let actions = body
        .as_array()
        .expect("expected getCodeFixes actions array");
    let action = actions
        .iter()
        .find(|action| {
            action
                .get("description")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|desc| desc.contains("Add import from \"path\""))
        })
        .expect("expected verbatim CommonJS fallback import action");
    let text_changes = action
        .get("changes")
        .and_then(serde_json::Value::as_array)
        .and_then(|changes| changes.first())
        .and_then(|change| change.get("textChanges"))
        .and_then(serde_json::Value::as_array)
        .expect("expected fallback action text changes");
    assert!(
        text_changes.iter().any(|change| {
            change
                .get("newText")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|text| text.contains("import path = require(\"path\");"))
        }),
        "expected fallback action to add `import path = require(\"path\")`"
    );
    assert!(
        text_changes.iter().any(|change| {
            change
                .get("newText")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|text| text == "path.normalize")
        }),
        "expected fallback action to rewrite `normalize` usage to `path.normalize`"
    );
}

#[test]
fn test_completion_entry_details_upgrades_type_only_named_import_for_value_usage() {
    let mut server = make_server();
    server.open_files.insert(
        "/tsconfig.json".to_string(),
        r#"{
  "compilerOptions": {
    "module": "node18",
    "verbatimModuleSyntax": true
  }
}"#
        .to_string(),
    );
    server.open_files.insert(
        "/mod.ts".to_string(),
        "export const value = 0;\nexport class C { constructor(v: any) {} }\nexport interface I {}\n"
            .to_string(),
    );
    let source_text = "import type { I } from \"./mod.js\";\n\nconst x: I = new /**/\n";
    server
        .open_files
        .insert("/a.mts".to_string(), source_text.to_string());

    let completion_req = make_request(
        "completionInfo",
        serde_json::json!({
            "file": "/a.mts",
            "line": 3,
            "offset": 18,
            "preferences": {
                "includeCompletionsForModuleExports": true,
                "allowIncompleteCompletions": true
            }
        }),
    );
    let completion_resp = server.handle_tsserver_request(completion_req);
    assert!(completion_resp.success);
    let completion_body = completion_resp
        .body
        .expect("completionInfo should return a body");
    let entries = completion_body["entries"]
        .as_array()
        .expect("completionInfo should include entries");
    let c_entry = entries
        .iter()
        .find(|entry| {
            entry.get("name").and_then(serde_json::Value::as_str) == Some("C")
                && entry.get("hasAction").and_then(serde_json::Value::as_bool) == Some(true)
        })
        .or_else(|| {
            entries
                .iter()
                .find(|entry| entry.get("name").and_then(serde_json::Value::as_str) == Some("C"))
        })
        .expect("expected completionInfo to include `C` entry");
    let source = c_entry
        .get("source")
        .and_then(serde_json::Value::as_str)
        .expect("expected `C` completion entry to include source")
        .to_string();
    assert_eq!(
        source, "./mod",
        "expected tsserver completion source to remain extensionless for .mts auto-import entries"
    );

    let details_req = make_request(
        "completionEntryDetails",
        serde_json::json!({
            "file": "/a.mts",
            "line": 3,
            "offset": 18,
            "entryNames": [{ "name": "C", "source": source }],
            "preferences": {
                "includeCompletionsForModuleExports": true,
                "allowIncompleteCompletions": true
            }
        }),
    );
    let details_resp = server.handle_tsserver_request(details_req);
    assert!(details_resp.success);
    let details_body = details_resp
        .body
        .expect("completionEntryDetails should return a body");
    let details = details_body
        .as_array()
        .expect("completionEntryDetails should return an array");
    let first = details
        .first()
        .expect("completionEntryDetails should include one entry");
    let code_actions = first
        .get("codeActions")
        .and_then(serde_json::Value::as_array)
        .expect("completion details should include auto-import code actions");
    let text_changes = code_actions
        .first()
        .and_then(|action| action.get("changes"))
        .and_then(serde_json::Value::as_array)
        .and_then(|changes| changes.first())
        .and_then(|change| change.get("textChanges"))
        .and_then(serde_json::Value::as_array)
        .expect("auto-import code action should include text changes");
    let import_text = text_changes
        .first()
        .and_then(|change| change.get("newText"))
        .and_then(serde_json::Value::as_str)
        .expect("auto-import text change should include newText");
    assert!(
        import_text.contains("import { C, type I } from \"./mod.js\";"),
        "expected value auto-import to upgrade existing type-only named import, got: {import_text}"
    );
    let mut updated_text = source_text.to_string();
    let mut spans: Vec<(usize, usize, String)> = text_changes
        .iter()
        .filter_map(|change| {
            let span = change.get("span")?;
            let start = span.get("start")?.as_u64()? as usize;
            let length = span.get("length")?.as_u64()? as usize;
            let new_text = change.get("newText")?.as_str()?.to_string();
            Some((start, start + length, new_text))
        })
        .collect();
    spans.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
    for (start, end, new_text) in spans {
        if start <= end && end <= updated_text.len() {
            updated_text.replace_range(start..end, &new_text);
        }
    }
    assert!(
        updated_text.contains("import { C, type I } from \"./mod.js\";"),
        "expected applied edits to contain merged value+type import, got: {updated_text}"
    );
    assert!(
        !updated_text.contains("import type { I } from \"./mod.js\";"),
        "expected applied edits to remove prior type-only import line, got: {updated_text}"
    );
}

#[test]
fn test_completion_info_class_member_snippet_includes_import_code_action() {
    let mut server = make_server();
    server.open_files.insert(
        "/node_modules/@sapphire/pieces/index.d.ts".to_string(),
        "interface Container {\n  stores: unknown;\n}\n\ndeclare class Piece {\n  container: Container;\n}\n\nexport { Piece, type Container };\n".to_string(),
    );
    server.open_files.insert(
        "/index.ts".to_string(),
        "import { Piece } from \"@sapphire/pieces\";\nclass FullPiece extends Piece {\n  c/**/\n}\n"
            .to_string(),
    );

    let completion_req = make_request(
        "completionInfo",
        serde_json::json!({
            "file": "/index.ts",
            "line": 3,
            "offset": 4,
            "preferences": {
                "includeCompletionsWithClassMemberSnippets": true,
                "includeCompletionsWithInsertText": true
            }
        }),
    );
    let completion_resp = server.handle_tsserver_request(completion_req);
    assert!(completion_resp.success);
    let completion_body = completion_resp
        .body
        .expect("completionInfo should return a body");
    let entries = completion_body["entries"]
        .as_array()
        .expect("completionInfo should include entries");
    let container_entry = entries
        .iter()
        .find(|entry| {
            entry.get("name").and_then(serde_json::Value::as_str) == Some("container")
                && entry.get("source").and_then(serde_json::Value::as_str)
                    == Some("ClassMemberSnippet/")
        })
        .expect("expected class member snippet completion for `container`");
    assert_eq!(
        container_entry
            .get("insertText")
            .and_then(serde_json::Value::as_str),
        Some("container: Container;")
    );
    assert_eq!(
        container_entry
            .get("filterText")
            .and_then(serde_json::Value::as_str),
        Some("container")
    );
    assert_eq!(
        container_entry
            .get("hasAction")
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );
}

#[test]
fn test_completion_info_accepts_top_level_class_member_snippet_preferences() {
    let mut server = make_server();
    server.open_files.insert(
        "/node_modules/@sapphire/pieces/index.d.ts".to_string(),
        "interface Container {\n  stores: unknown;\n}\n\ndeclare class Piece {\n  container: Container;\n}\n\nexport { Piece, type Container };\n".to_string(),
    );
    server.open_files.insert(
        "/index.ts".to_string(),
        "import { Piece } from \"@sapphire/pieces\";\nclass FullPiece extends Piece {\n  c/**/\n}\n"
            .to_string(),
    );

    let completion_req = make_request(
        "completionInfo",
        serde_json::json!({
            "file": "/index.ts",
            "line": 3,
            "offset": 4,
            "includeCompletionsWithClassMemberSnippets": true,
            "includeCompletionsWithInsertText": true
        }),
    );
    let completion_resp = server.handle_tsserver_request(completion_req);
    assert!(completion_resp.success);
    let completion_body = completion_resp
        .body
        .expect("completionInfo should return a body");
    let entries = completion_body["entries"]
        .as_array()
        .expect("completionInfo should include entries");
    let container_entry = entries
        .iter()
        .find(|entry| {
            entry.get("name").and_then(serde_json::Value::as_str) == Some("container")
                && entry.get("source").and_then(serde_json::Value::as_str)
                    == Some("ClassMemberSnippet/")
        })
        .expect("expected class member snippet completion for `container`");
    assert!(
        container_entry.get("isSnippet").is_none(),
        "class member snippet entries should not set isSnippet"
    );
    assert_eq!(
        container_entry
            .get("insertText")
            .and_then(serde_json::Value::as_str),
        Some("container: Container;")
    );
}

#[test]
fn test_completion_info_class_member_snippet_includes_getter_from_augmented_alias_chain() {
    let mut server = make_server();
    server.open_files.insert(
        "/node_modules/@sapphire/pieces/index.d.ts".to_string(),
        "interface Container {\n  stores: unknown;\n}\n\ndeclare class Piece {\n  get container(): Container;\n}\n\ndeclare class AliasPiece extends Piece {}\n\nexport { AliasPiece, type Container };\n".to_string(),
    );
    server.open_files.insert(
        "/node_modules/@sapphire/framework/index.d.ts".to_string(),
        "import { AliasPiece } from \"@sapphire/pieces\";\n\ndeclare class Command extends AliasPiece {}\n\ndeclare module \"@sapphire/pieces\" {\n  interface Container {\n    client: unknown;\n  }\n}\n\nexport { Command };\n".to_string(),
    );
    server.open_files.insert(
        "/index.ts".to_string(),
        "import \"@sapphire/pieces\";\nimport { Command } from \"@sapphire/framework\";\nclass PingCommand extends Command {\n  /**/\n}\n"
            .to_string(),
    );

    let completion_req = make_request(
        "completionInfo",
        serde_json::json!({
            "file": "/index.ts",
            "line": 4,
            "offset": 3,
            "preferences": {
                "includeCompletionsWithClassMemberSnippets": true,
                "includeCompletionsWithInsertText": true
            }
        }),
    );
    let completion_resp = server.handle_tsserver_request(completion_req);
    assert!(completion_resp.success);
    let completion_body = completion_resp
        .body
        .expect("completionInfo should return a body");
    let entries = completion_body["entries"]
        .as_array()
        .expect("completionInfo should include entries");
    let container_entry = entries
        .iter()
        .find(|entry| {
            entry.get("name").and_then(serde_json::Value::as_str) == Some("container")
                && entry.get("source").and_then(serde_json::Value::as_str)
                    == Some("ClassMemberSnippet/")
        })
        .expect("expected class member snippet completion for inherited getter `container`");
    assert_eq!(
        container_entry
            .get("insertText")
            .and_then(serde_json::Value::as_str),
        Some("get container(): Container {\n}")
    );
}

#[test]
fn test_completion_info_uses_configure_class_member_snippet_preference() {
    let mut server = make_server();
    server.open_files.insert(
        "/node_modules/@sapphire/pieces/index.d.ts".to_string(),
        "interface Container {\n  stores: unknown;\n}\n\ndeclare class Piece {\n  container: Container;\n}\n\nexport { Piece, type Container };\n".to_string(),
    );
    server.open_files.insert(
        "/index.ts".to_string(),
        "import { Piece } from \"@sapphire/pieces\";\nclass FullPiece extends Piece {\n  /**/\n}\n"
            .to_string(),
    );

    let configure_req = make_request(
        "configure",
        serde_json::json!({
            "preferences": {
                "includeCompletionsWithClassMemberSnippets": true
            }
        }),
    );
    let configure_resp = server.handle_tsserver_request(configure_req);
    assert!(configure_resp.success);

    let completion_req = make_request(
        "completionInfo",
        serde_json::json!({
            "file": "/index.ts",
            "line": 3,
            "offset": 3
        }),
    );
    let completion_resp = server.handle_tsserver_request(completion_req);
    assert!(completion_resp.success);
    let completion_body = completion_resp
        .body
        .expect("completionInfo should return a body");
    let entries = completion_body["entries"]
        .as_array()
        .expect("completionInfo should include entries");
    let container_entry = entries
        .iter()
        .find(|entry| {
            entry.get("name").and_then(serde_json::Value::as_str) == Some("container")
                && entry.get("source").and_then(serde_json::Value::as_str)
                    == Some("ClassMemberSnippet/")
        })
        .expect("expected class member snippet completion after configure preference");
    assert!(
        container_entry.get("isSnippet").is_none(),
        "class member snippet entries should not set isSnippet"
    );
    assert_eq!(
        container_entry
            .get("insertText")
            .and_then(serde_json::Value::as_str),
        Some("container: Container;")
    );
}

#[test]
fn test_completion_info_class_member_snippet_export_list_augmentation_shape() {
    let mut server = make_server();
    server.open_files.insert(
        "/node_modules/@sapphire/pieces/index.d.ts".to_string(),
        "interface Container {\n  stores: unknown;\n}\n\ndeclare class Piece {\n  container: Container;\n}\n\nexport { Piece, type Container };\n".to_string(),
    );
    server.open_files.insert(
        "/augmentation.ts".to_string(),
        "declare module \"@sapphire/pieces\" {\n  interface Container {\n    client: unknown;\n  }\n  export { Container };\n}\n".to_string(),
    );
    server.open_files.insert(
        "/index.ts".to_string(),
        "import { Piece } from \"@sapphire/pieces\";\nclass FullPiece extends Piece {\n  /**/\n}\n"
            .to_string(),
    );

    let configure_req = make_request(
        "configure",
        serde_json::json!({
            "preferences": {
                "includeCompletionsWithClassMemberSnippets": true,
                "includeCompletionsWithInsertText": true
            }
        }),
    );
    let configure_resp = server.handle_tsserver_request(configure_req);
    assert!(configure_resp.success);

    let completion_req = make_request(
        "completionInfo",
        serde_json::json!({
            "file": "/index.ts",
            "line": 3,
            "offset": 4
        }),
    );
    let completion_resp = server.handle_tsserver_request(completion_req);
    assert!(completion_resp.success);
    let completion_body = completion_resp
        .body
        .expect("completionInfo should return a body");
    let entries = completion_body["entries"]
        .as_array()
        .expect("completionInfo should include entries");
    let container_entry = entries
        .iter()
        .find(|entry| {
            entry.get("name").and_then(serde_json::Value::as_str) == Some("container")
                && entry.get("source").and_then(serde_json::Value::as_str)
                    == Some("ClassMemberSnippet/")
        })
        .expect("expected class member snippet completion for container");
    assert!(
        container_entry.get("isSnippet").is_none(),
        "class member snippet entries should not set isSnippet"
    );
    assert_eq!(
        container_entry
            .get("insertText")
            .and_then(serde_json::Value::as_str),
        Some("container: Container;")
    );
}

#[test]
fn test_completion_entry_details_class_member_snippet_export_list_augmentation_import_order() {
    let mut server = make_server();
    server.open_files.insert(
        "/node_modules/@sapphire/pieces/index.d.ts".to_string(),
        "interface Container {\n  stores: unknown;\n}\n\ndeclare class Piece {\n  get container(): Container;\n}\n\ndeclare class AliasPiece extends Piece {}\n\nexport { AliasPiece, type Container };\n".to_string(),
    );
    server.open_files.insert(
        "/node_modules/@sapphire/framework/index.d.ts".to_string(),
        "import { AliasPiece } from \"@sapphire/pieces\";\n\ndeclare class Command extends AliasPiece {}\n\ndeclare module \"@sapphire/pieces\" {\n  interface Container {\n    client: unknown;\n  }\n}\n\nexport { Command };\n".to_string(),
    );
    server.open_files.insert(
        "/index.ts".to_string(),
        "import \"@sapphire/pieces\";\nimport { Command } from \"@sapphire/framework\";\nclass PingCommand extends Command {\n  /**/\n}\n".to_string(),
    );

    let completion_req = make_request(
        "completionInfo",
        serde_json::json!({
            "file": "/index.ts",
            "line": 4,
            "offset": 4,
            "preferences": {
                "includeCompletionsWithClassMemberSnippets": true,
                "includeCompletionsWithInsertText": true
            }
        }),
    );
    let completion_resp = server.handle_tsserver_request(completion_req);
    assert!(completion_resp.success);
    let completion_body = completion_resp
        .body
        .expect("completionInfo should return a body");
    let entries = completion_body["entries"]
        .as_array()
        .expect("completionInfo should include entries");
    let container_entry = entries
        .iter()
        .find(|entry| {
            entry.get("name").and_then(serde_json::Value::as_str) == Some("container")
                && entry.get("source").and_then(serde_json::Value::as_str)
                    == Some("ClassMemberSnippet/")
        })
        .expect("expected class member snippet completion for container");
    let container_data = container_entry
        .get("data")
        .cloned()
        .unwrap_or(serde_json::Value::Null);

    let details_req = make_request(
        "completionEntryDetails-full",
        serde_json::json!({
            "file": "/index.ts",
            "line": 4,
            "offset": 4,
            "entryNames": [{
                "name": "container",
                "source": "ClassMemberSnippet/",
                "data": container_data
            }],
            "preferences": {
                "includeCompletionsWithClassMemberSnippets": true,
                "includeCompletionsWithInsertText": true
            }
        }),
    );
    let details_resp = server.handle_tsserver_request(details_req);
    assert!(details_resp.success);
    let details_body = details_resp
        .body
        .expect("completionEntryDetails should return a body");
    let details = details_body
        .as_array()
        .expect("completionEntryDetails should return an array");
    let first = details
        .first()
        .expect("completionEntryDetails should include one entry");
    let text_changes = first
        .get("codeActions")
        .and_then(serde_json::Value::as_array)
        .and_then(|actions| actions.first())
        .and_then(|action| action.get("changes"))
        .and_then(serde_json::Value::as_array)
        .and_then(|changes| changes.first())
        .and_then(|change| change.get("textChanges"))
        .and_then(serde_json::Value::as_array)
        .expect("class member snippet details should include text changes");
    let first_change = text_changes
        .first()
        .expect("expected at least one text change for class member snippet");
    assert_eq!(
        text_changes.len(),
        1,
        "class member snippet import action should include exactly one synthesized text change"
    );

    assert_eq!(
        first_change
            .get("newText")
            .and_then(serde_json::Value::as_str),
        Some("import { Container } from \"@sapphire/pieces\";\n")
    );
    let expected_start = server
        .open_files
        .get("/index.ts")
        .and_then(|source| source.find("class PingCommand").map(|n| n as u64))
        .expect("expected class declaration in /index.ts");
    assert_eq!(
        first_change
            .get("span")
            .and_then(|span| span.get("start"))
            .and_then(serde_json::Value::as_u64),
        Some(expected_start),
        "import should be inserted after the existing import block"
    );
}

#[test]
fn test_completion_info_class_member_snippet_method_trims_trailing_param_comma() {
    let mut server = make_server();
    server.open_files.insert(
        "/node_modules/@types/vscode/index.d.ts".to_string(),
        "declare module \"vscode\" {\n  export class Position {\n    readonly line: number;\n    readonly character: number;\n  }\n}\n".to_string(),
    );
    server.open_files.insert(
        "/src/motion.ts".to_string(),
        "import { Position } from \"vscode\";\n\nexport abstract class MoveQuoteMatch {\n  public override async execActionWithCount(\n    position: Position,\n  ): Promise<void> {}\n}\n\ndeclare module \"vscode\" {\n  interface Position {\n    toString(): string;\n  }\n}\n".to_string(),
    );
    server.open_files.insert(
        "/src/smartQuotes.ts".to_string(),
        "import { MoveQuoteMatch } from \"./motion\";\n\nexport class MoveInsideNextQuote extends MoveQuoteMatch {\n  /**/\n  keys = [\"i\", \"n\", \"q\"];\n}\n".to_string(),
    );

    let completion_req = make_request(
        "completionInfo",
        serde_json::json!({
            "file": "/src/smartQuotes.ts",
            "line": 4,
            "offset": 4,
            "preferences": {
                "includeCompletionsWithClassMemberSnippets": true,
                "includeCompletionsWithInsertText": true
            }
        }),
    );
    let completion_resp = server.handle_tsserver_request(completion_req);
    assert!(completion_resp.success);
    let completion_body = completion_resp
        .body
        .expect("completionInfo should return a body");
    let entries = completion_body["entries"]
        .as_array()
        .expect("completionInfo should include entries");
    let method_entry = entries
        .iter()
        .find(|entry| {
            entry.get("name").and_then(serde_json::Value::as_str) == Some("execActionWithCount")
                && entry.get("source").and_then(serde_json::Value::as_str)
                    == Some("ClassMemberSnippet/")
        })
        .expect("expected class member snippet completion for `execActionWithCount`");
    assert_eq!(
        method_entry
            .get("insertText")
            .and_then(serde_json::Value::as_str),
        Some("public execActionWithCount(position: Position): Promise<void> {\n}")
    );

    let details_req = make_request(
        "completionEntryDetails",
        serde_json::json!({
            "file": "/src/smartQuotes.ts",
            "line": 4,
            "offset": 4,
            "entryNames": [{
                "name": "execActionWithCount",
                "source": "ClassMemberSnippet/"
            }],
            "preferences": {
                "includeCompletionsWithClassMemberSnippets": true,
                "includeCompletionsWithInsertText": true
            }
        }),
    );
    let details_resp = server.handle_tsserver_request(details_req);
    assert!(details_resp.success);
    let details_body = details_resp
        .body
        .expect("completionEntryDetails should return a body");
    let details = details_body
        .as_array()
        .expect("completionEntryDetails should return an array");
    let first = details
        .first()
        .expect("completionEntryDetails should include one entry");
    let text_changes = first
        .get("codeActions")
        .and_then(serde_json::Value::as_array)
        .and_then(|actions| actions.first())
        .and_then(|action| action.get("changes"))
        .and_then(serde_json::Value::as_array)
        .and_then(|changes| changes.first())
        .and_then(|change| change.get("textChanges"))
        .and_then(serde_json::Value::as_array)
        .expect("class member snippet details should include text changes");
    let first_change = text_changes
        .first()
        .expect("class member snippet should include an import text change");
    assert_eq!(
        first_change
            .get("span")
            .and_then(|span| span.get("start"))
            .and_then(serde_json::Value::as_u64),
        Some(0)
    );
    assert_eq!(
        first_change
            .get("span")
            .and_then(|span| span.get("length"))
            .and_then(serde_json::Value::as_u64),
        Some(0)
    );
    assert_eq!(
        first_change
            .get("newText")
            .and_then(serde_json::Value::as_str),
        Some("import { Position } from \"vscode\";\n")
    );
}

#[test]
fn test_completion_info_class_member_snippet_export_equals_default_parent() {
    let mut server = make_server();
    server.open_files.insert(
        "/node.ts".to_string(),
        "import Container from \"./container.js\";\nimport Document from \"./document.js\";\n\ndeclare namespace Node {\n  class Node extends Node_ {}\n\n  export { Node as default };\n}\n\ndeclare abstract class Node_ {\n  parent: Container | Document | undefined;\n}\n\ndeclare class Node extends Node_ {}\n\nexport = Node;\n".to_string(),
    );
    server.open_files.insert(
        "/document.ts".to_string(),
        "import Container from \"./container.js\";\n\ndeclare namespace Document {\n  export { Document_ as default };\n}\n\ndeclare class Document_ extends Container {}\n\ndeclare class Document extends Document_ {}\n\nexport = Document;\n".to_string(),
    );
    server.open_files.insert(
        "/container.ts".to_string(),
        "import Node from \"./node.js\";\n\ndeclare namespace Container {\n  export { Container_ as default };\n}\n\ndeclare abstract class Container_ extends Node {\n  p\n}\n\ndeclare class Container extends Container_ {}\n\nexport = Container;\n".to_string(),
    );

    let completion_req = make_request(
        "completionInfo",
        serde_json::json!({
            "file": "/container.ts",
            "line": 8,
            "offset": 4,
            "preferences": {
                "includeCompletionsWithClassMemberSnippets": true,
                "includeCompletionsWithInsertText": true
            }
        }),
    );
    let completion_resp = server.handle_tsserver_request(completion_req);
    assert!(completion_resp.success);
    let completion_body = completion_resp
        .body
        .expect("completionInfo should return a body");
    let entries = completion_body["entries"]
        .as_array()
        .expect("completionInfo should include entries");
    let parent_entry = entries
        .iter()
        .find(|entry| {
            entry.get("name").and_then(serde_json::Value::as_str) == Some("parent")
                && entry.get("source").and_then(serde_json::Value::as_str)
                    == Some("ClassMemberSnippet/")
        })
        .expect("expected class member snippet completion for `parent`");
    assert_eq!(
        parent_entry
            .get("insertText")
            .and_then(serde_json::Value::as_str),
        Some("parent: Container_ | Document_ | undefined;")
    );

    let details_req = make_request(
        "completionEntryDetails",
        serde_json::json!({
            "file": "/container.ts",
            "line": 8,
            "offset": 4,
            "entryNames": [{
                "name": "parent",
                "source": "ClassMemberSnippet/"
            }]
        }),
    );
    let details_resp = server.handle_tsserver_request(details_req);
    assert!(details_resp.success);
    let details_body = details_resp
        .body
        .expect("completionEntryDetails should return a body");
    let details = details_body
        .as_array()
        .expect("completionEntryDetails should return an array");
    let first = details
        .first()
        .expect("completionEntryDetails should include one entry");
    let code_actions = first
        .get("codeActions")
        .and_then(serde_json::Value::as_array)
        .expect("expected class member snippet completion details to include code actions");
    assert_eq!(code_actions.len(), 1);
}

#[test]
fn test_completion_entry_details_mts_type_position_adds_import_type_named_clause() {
    let mut server = make_server();
    server.open_files.insert(
        "/tsconfig.json".to_string(),
        r#"{
  "compilerOptions": {
    "module": "node18",
    "verbatimModuleSyntax": true
  }
}"#
        .to_string(),
    );
    server.open_files.insert(
        "/mod.ts".to_string(),
        "export const value = 0;\nexport class C { constructor(v: any) {} }\nexport interface I {}\n"
            .to_string(),
    );
    server
        .open_files
        .insert("/a.mts".to_string(), "const x: /**/\n".to_string());

    let completion_req = make_request(
        "completionInfo",
        serde_json::json!({
            "file": "/a.mts",
            "line": 1,
            "offset": 10,
            "preferences": {
                "includeCompletionsForModuleExports": true,
                "allowIncompleteCompletions": true
            }
        }),
    );
    let completion_resp = server.handle_tsserver_request(completion_req);
    assert!(completion_resp.success);
    let completion_body = completion_resp
        .body
        .expect("completionInfo should return a body");
    let entries = completion_body["entries"]
        .as_array()
        .expect("completionInfo should include entries");
    let i_entry = entries
        .iter()
        .find(|entry| {
            entry.get("name").and_then(serde_json::Value::as_str) == Some("I")
                && entry.get("source").and_then(serde_json::Value::as_str) == Some("./mod")
        })
        .expect("expected completionInfo to include `I` auto-import from ./mod");

    let details_req = make_request(
        "completionEntryDetails",
        serde_json::json!({
            "file": "/a.mts",
            "line": 1,
            "offset": 10,
            "entryNames": [{
                "name": "I",
                "source": i_entry.get("source").and_then(serde_json::Value::as_str).expect("source")
            }],
            "preferences": {
                "includeCompletionsForModuleExports": true,
                "allowIncompleteCompletions": true
            }
        }),
    );
    let details_resp = server.handle_tsserver_request(details_req);
    assert!(details_resp.success);
    let details_body = details_resp
        .body
        .expect("completionEntryDetails should return a body");
    let details = details_body
        .as_array()
        .expect("completionEntryDetails should return an array");
    let first = details
        .first()
        .expect("completionEntryDetails should include one entry");
    let code_actions = first
        .get("codeActions")
        .and_then(serde_json::Value::as_array)
        .expect("completion details should include auto-import code actions");
    let text_changes = code_actions
        .first()
        .and_then(|action| action.get("changes"))
        .and_then(serde_json::Value::as_array)
        .and_then(|changes| changes.first())
        .and_then(|change| change.get("textChanges"))
        .and_then(serde_json::Value::as_array)
        .expect("auto-import code action should include text changes");
    let import_text = text_changes
        .first()
        .and_then(|change| change.get("newText"))
        .and_then(serde_json::Value::as_str)
        .expect("auto-import text change should include newText");
    assert!(
        import_text.starts_with("import type { I } from \"./mod.js\";"),
        "expected type-position auto-import to emit `import type` named clause with .js extension, got: {import_text}"
    );
}

#[test]
fn test_completion_info_auto_import_file_exclude_patterns_exclude_node_modules_package_tree() {
    let mut server = make_server();
    server.open_files.insert(
        "/home/src/workspaces/project/tsconfig.json".to_string(),
        r#"{
  "compilerOptions": {
    "module": "commonjs"
  }
}"#
        .to_string(),
    );
    server.open_files.insert(
        "/home/src/workspaces/project/node_modules/aws-sdk/package.json".to_string(),
        r#"{ "name": "aws-sdk", "version": "2.0.0", "main": "index.js" }"#.to_string(),
    );
    server.open_files.insert(
        "/home/src/workspaces/project/node_modules/aws-sdk/index.d.ts".to_string(),
        "export * from \"./clients/s3\";\n".to_string(),
    );
    server.open_files.insert(
        "/home/src/workspaces/project/node_modules/aws-sdk/clients/s3.d.ts".to_string(),
        "export declare class S3 {}\n".to_string(),
    );
    server.open_files.insert(
        "/home/src/workspaces/project/package.json".to_string(),
        r#"{ "dependencies": { "aws-sdk": "*" } }"#.to_string(),
    );
    server.open_files.insert(
        "/home/src/workspaces/project/index.ts".to_string(),
        "S3/**/\n".to_string(),
    );

    let completion_req = make_request(
        "completionInfo",
        serde_json::json!({
            "file": "/home/src/workspaces/project/index.ts",
            "line": 1,
            "offset": 3,
            "preferences": {
                "includeCompletionsForModuleExports": true,
                "autoImportFileExcludePatterns": ["**/node_modules/aws-sdk"]
            }
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
    assert!(
        !entries
            .iter()
            .any(|entry| { entry.get("name").and_then(serde_json::Value::as_str) == Some("S3") }),
        "expected `S3` to be excluded, got entries: {entries:?}"
    );
}

#[test]
fn test_completion_info_auto_import_file_exclude_patterns_keeps_button_from_main() {
    let mut server = make_server();
    server.open_files.insert(
        "/lib/components/button/Button.ts".to_string(),
        "export function Button() {}\n".to_string(),
    );
    server.open_files.insert(
        "/lib/components/button/index.ts".to_string(),
        "export * from \"./Button\";\n".to_string(),
    );
    server.open_files.insert(
        "/lib/components/index.ts".to_string(),
        "export * from \"./button\";\n".to_string(),
    );
    server.open_files.insert(
        "/lib/main.ts".to_string(),
        "export { Button } from \"./components\";\n".to_string(),
    );
    server.open_files.insert(
        "/lib/index.ts".to_string(),
        "export * from \"./main\";\n".to_string(),
    );
    server
        .open_files
        .insert("/i-hate-index-files.ts".to_string(), "Button\n".to_string());

    let completion_req = make_request(
        "completionInfo",
        serde_json::json!({
            "file": "/i-hate-index-files.ts",
            "line": 1,
            "offset": 7,
            "preferences": {
                "allowIncompleteCompletions": true,
                "includeCompletionsForModuleExports": true,
                "autoImportFileExcludePatterns": ["/**/index.*"]
            }
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
    assert!(
        entries.iter().any(|entry| {
            entry.get("name").and_then(serde_json::Value::as_str) == Some("Button")
                && entry.get("source").and_then(serde_json::Value::as_str) == Some("./lib/main")
        }),
        "expected auto-import `Button` from `./lib/main`, got entries: {entries:?}"
    );
    assert_eq!(
        entries
            .iter()
            .filter(|entry| {
                entry.get("name").and_then(serde_json::Value::as_str) == Some("Button")
            })
            .count(),
        1,
        "expected exactly one `Button` completion entry, got entries: {entries:?}"
    );

    let completions_req = make_request(
        "completions",
        serde_json::json!({
            "file": "/i-hate-index-files.ts",
            "line": 1,
            "offset": 7,
            "preferences": {
                "allowIncompleteCompletions": true,
                "includeCompletionsForModuleExports": true,
                "autoImportFileExcludePatterns": ["/**/index.*"]
            }
        }),
    );
    let completions_resp = server.handle_tsserver_request(completions_req);
    assert!(completions_resp.success);
    let completions_body = completions_resp
        .body
        .expect("completions should return a body");
    let completions_entries = completions_body["entries"]
        .as_array()
        .expect("completions should include entries");
    assert_eq!(
        completions_entries
            .iter()
            .filter(|entry| {
                entry.get("name").and_then(serde_json::Value::as_str) == Some("Button")
            })
            .count(),
        1,
        "expected exactly one `Button` completion entry from `completions`, got entries: {completions_entries:?}"
    );

    let configure_req = make_request(
        "configure",
        serde_json::json!({
            "preferences": {
                "allowIncompleteCompletions": true,
                "includeCompletionsForModuleExports": true,
                "autoImportFileExcludePatterns": ["/**/index.*"]
            }
        }),
    );
    let configure_resp = server.handle_tsserver_request(configure_req);
    assert!(configure_resp.success);

    let completions_from_configured_req = make_request(
        "completions",
        serde_json::json!({
            "file": "/i-hate-index-files.ts",
            "line": 1,
            "offset": 7
        }),
    );
    let completions_from_configured_resp =
        server.handle_tsserver_request(completions_from_configured_req);
    assert!(completions_from_configured_resp.success);
    let completions_from_configured_body = completions_from_configured_resp
        .body
        .expect("configured completions should return a body");
    let completions_from_configured_entries = completions_from_configured_body["entries"]
        .as_array()
        .expect("configured completions should include entries");
    assert_eq!(
        completions_from_configured_entries
            .iter()
            .filter(|entry| {
                entry.get("name").and_then(serde_json::Value::as_str) == Some("Button")
            })
            .count(),
        1,
        "expected exactly one `Button` completion entry after configure, got entries: {completions_from_configured_entries:?}"
    );
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
    assert!(!entries.is_empty(), "expected at least one referenced symbol");
    let refs = entries[0]["references"]
        .as_array()
        .cloned()
        .expect("referenced symbol should include references");
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
