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
