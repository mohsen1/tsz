use super::*;

mod tests_completions;
mod tests_navigation;

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
