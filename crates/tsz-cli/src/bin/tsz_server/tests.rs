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
        include_inlay_parameter_name_hints: None,
        generate_return_in_doc_template: None,
        new_line_character: None,
        plugin_configs: FxHashMap::default(),
        native_ts_worker: None,
        pending_events: Vec::new(),
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

#[test]
fn status_returns_typescript_version_not_tsz_crate_version() {
    let mut server = make_server();
    let response = server.handle_tsserver_request(make_request("status", serde_json::json!({})));
    assert!(response.success);
    let body = response.body.expect("status should return a body");
    let version = body
        .get("version")
        .and_then(serde_json::Value::as_str)
        .expect("status body should include a string version");
    assert_eq!(
        version,
        tsz_cli::help::TSC_VERSION,
        "status should report the embedded TypeScript version, not the tsz crate version"
    );
    assert_ne!(
        version,
        env!("CARGO_PKG_VERSION"),
        "status must not report the local tsz-cli crate version"
    );
}

#[test]
fn tsserver_exit_request_does_not_write_response() {
    let mut server = make_server();
    let request = r#"{"seq":1,"type":"request","command":"exit","arguments":{}}"#;
    let input = format!("Content-Length: {}\r\n\r\n{}", request.len(), request);
    let mut stdin = BufReader::new(input.as_bytes());
    let mut stdout = Vec::new();

    run_tsserver_protocol_with_io(&mut server, &mut stdin, &mut stdout)
        .expect("exit request should terminate cleanly");

    assert!(
        stdout.is_empty(),
        "exit request should not write a tsserver response, got {} bytes: {}",
        stdout.len(),
        String::from_utf8_lossy(&stdout)
    );
}

#[test]
fn test_provide_inlay_hints_respects_protocol_start_length_span() {
    let mut server = make_server();
    let source = "function f(value: number) {}\nf(1);\n";
    server
        .open_files
        .insert("/a.ts".to_string(), source.to_string());
    // Parameter hints require explicit opt-in via configure (#3793).
    server.handle_tsserver_request(make_request(
        "configure",
        serde_json::json!({
            "preferences": { "includeInlayParameterNameHints": "all" }
        }),
    ));

    let empty_response = server.handle_tsserver_request(make_request(
        "provideInlayHints",
        serde_json::json!({
            "file": "/a.ts",
            "start": 0,
            "length": 0,
        }),
    ));
    assert!(empty_response.success);
    let empty_body = empty_response
        .body
        .expect("provideInlayHints should return a body")
        .as_array()
        .cloned()
        .expect("provideInlayHints body should be an array");
    assert!(
        empty_body.is_empty(),
        "zero-length protocol span should not include hints: {empty_body:?}"
    );

    let full_response = server.handle_tsserver_request(make_request(
        "provideInlayHints",
        serde_json::json!({
            "file": "/a.ts",
            "start": 0,
            "length": source.len(),
        }),
    ));
    assert!(full_response.success);
    let full_body = full_response
        .body
        .expect("provideInlayHints should return a body")
        .as_array()
        .cloned()
        .expect("provideInlayHints body should be an array");
    assert_eq!(
        full_body.len(),
        1,
        "full protocol span should include the parameter hint: {full_body:?}"
    );
    assert_eq!(
        full_body[0].get("kind").and_then(serde_json::Value::as_str),
        Some("Parameter")
    );
    assert_eq!(
        full_body[0]
            .get("position")
            .and_then(|position| position.get("line"))
            .and_then(serde_json::Value::as_u64),
        Some(2)
    );
    assert_eq!(
        full_body[0]
            .get("position")
            .and_then(|position| position.get("offset"))
            .and_then(serde_json::Value::as_u64),
        Some(3)
    );
}

#[test]
fn test_provide_inlay_hints_default_off_for_parameters() {
    // Parameter hints are off by default in tsserver — clients must opt in via
    // `includeInlayParameterNameHints`. Regression for #3793 (and the issue's
    // first repro: configure "none" then request hints → empty array).
    let mut server = make_server();
    let source = "function f(value: number) {}\nf(1);\n";
    server
        .open_files
        .insert("/a.ts".to_string(), source.to_string());

    // Default: no configure → no parameter hints.
    let response = server.handle_tsserver_request(make_request(
        "provideInlayHints",
        serde_json::json!({"file": "/a.ts", "start": 0, "length": source.len()}),
    ));
    let body = response
        .body
        .and_then(|b| b.as_array().cloned())
        .unwrap_or_default();
    assert!(
        body.iter()
            .all(|h| h.get("kind").and_then(|k| k.as_str()) != Some("Parameter")),
        "default config must not emit parameter hints, got {body:?}"
    );

    // Explicit "none" → still empty.
    server.handle_tsserver_request(make_request(
        "configure",
        serde_json::json!({
            "preferences": { "includeInlayParameterNameHints": "none" }
        }),
    ));
    let none_response = server.handle_tsserver_request(make_request(
        "provideInlayHints",
        serde_json::json!({"file": "/a.ts", "start": 0, "length": source.len()}),
    ));
    let none_body = none_response
        .body
        .and_then(|b| b.as_array().cloned())
        .unwrap_or_default();
    assert!(
        none_body
            .iter()
            .all(|h| h.get("kind").and_then(|k| k.as_str()) != Some("Parameter")),
        "includeInlayParameterNameHints=\"none\" must produce no parameter hints, got {none_body:?}"
    );
}

#[test]
fn test_provide_inlay_hints_parameter_label_has_no_trailing_space_or_whitespace_before() {
    // tsc's parameter inlay hints carry `text: "value:"` (no trailing space)
    // and omit `whitespaceBefore` (it's implicitly false). Regression for
    // #3793.
    let mut server = make_server();
    let source = "function f(value: number) {}\nf(1);\n";
    server
        .open_files
        .insert("/a.ts".to_string(), source.to_string());
    server.handle_tsserver_request(make_request(
        "configure",
        serde_json::json!({
            "preferences": { "includeInlayParameterNameHints": "all" }
        }),
    ));

    let response = server.handle_tsserver_request(make_request(
        "provideInlayHints",
        serde_json::json!({"file": "/a.ts", "start": 0, "length": source.len()}),
    ));
    let body = response
        .body
        .and_then(|b| b.as_array().cloned())
        .expect("provideInlayHints body should be an array");
    let parameter = body
        .iter()
        .find(|h| h.get("kind").and_then(|k| k.as_str()) == Some("Parameter"))
        .expect("expected one parameter hint");
    assert_eq!(
        parameter.get("text").and_then(|t| t.as_str()),
        Some("value:"),
        "parameter text must not contain a trailing space, got {parameter:?}"
    );
    assert!(
        parameter.get("whitespaceBefore").is_none(),
        "whitespaceBefore must be omitted for parameter hints, got {parameter:?}"
    );
    assert_eq!(
        parameter.get("whitespaceAfter").and_then(|v| v.as_bool()),
        Some(true),
        "whitespaceAfter must remain true, got {parameter:?}"
    );
}

#[test]
fn linked_editing_range_returns_jsx_member_expression_tag_names() {
    let mut server = make_server();
    server.open_files.insert(
        "/a.tsx".to_string(),
        "const x = <Foo.Bar>hi</Foo.Bar>;".to_string(),
    );

    let response = server.handle_tsserver_request(make_request(
        "linkedEditingRange",
        serde_json::json!({
            "file": "/a.tsx",
            "line": 1,
            "offset": 12,
        }),
    ));

    assert!(response.success);
    let body = response
        .body
        .expect("linkedEditingRange should return a body for JSX member tags");
    assert_eq!(
        body.get("wordPattern").and_then(serde_json::Value::as_str),
        Some("[a-zA-Z0-9:\\-\\._$]*")
    );

    let ranges = body
        .get("ranges")
        .and_then(serde_json::Value::as_array)
        .expect("linkedEditingRange body should contain ranges");
    assert_eq!(ranges.len(), 2);
    assert_eq!(
        ranges[0]["start"],
        serde_json::json!({"line": 1, "offset": 12})
    );
    assert_eq!(
        ranges[0]["end"],
        serde_json::json!({"line": 1, "offset": 19})
    );
    assert_eq!(
        ranges[1]["start"],
        serde_json::json!({"line": 1, "offset": 24})
    );
    assert_eq!(
        ranges[1]["end"],
        serde_json::json!({"line": 1, "offset": 31})
    );
}

#[test]
fn jsx_closing_tag_skips_quoted_attribute_with_greater_than() {
    // Regression for #3965: an attribute whose quoted value contains `>`
    // must not corrupt the backward angle-bracket scan. The handler should
    // still locate the matching `<div` and return `</div>`.
    let cases = [
        ("/double.tsx", r#"<div title="a>b">"#, 18),
        ("/single.tsx", "<div title='a>b'>", 18),
        ("/expr.tsx", r#"<div title={"a>b"}>"#, 20),
        ("/component.tsx", r#"<MyComp x="y>z">"#, 17),
    ];

    for (file, content, offset) in cases {
        let mut server = make_server();
        server
            .open_files
            .insert(file.to_string(), content.to_string());

        let response = server.handle_tsserver_request(make_request(
            "jsxClosingTag",
            serde_json::json!({"file": file, "line": 1, "offset": offset}),
        ));
        assert!(response.success, "{file}: jsxClosingTag should succeed");
        let body = response
            .body
            .unwrap_or_else(|| panic!("{file}: expected a body, got none for {content:?}"));
        let new_text = body
            .get("newText")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_else(|| panic!("{file}: expected newText, got {body:?}"));
        // Tag name is whatever follows `<` in the input; normalize the check.
        let tag_end = content[1..]
            .find(|c: char| !(c.is_alphanumeric() || c == '.' || c == '_' || c == '-' || c == '$'))
            .map(|i| i + 1)
            .unwrap_or(content.len());
        let tag_name = &content[1..tag_end];
        let expected = format!("</{tag_name}>");
        assert_eq!(
            new_text, expected,
            "{file}: jsxClosingTag for {content:?} should be {expected}, got {new_text}"
        );
        assert_eq!(
            body.get("caretOffset").and_then(serde_json::Value::as_u64),
            Some(0),
            "{file}: jsxClosingTag must include caretOffset: 0, got {body:?}"
        );
    }
}

#[test]
fn jsx_closing_tag_skips_attribute_string_across_lines() {
    // Multi-line opening tag with a `>` inside a quoted attribute must still
    // resolve to the correct closing tag.
    let mut server = make_server();
    server.open_files.insert(
        "/multi.tsx".to_string(),
        "<div\n  title=\"a>b\"\n>".to_string(),
    );

    let response = server.handle_tsserver_request(make_request(
        "jsxClosingTag",
        serde_json::json!({"file": "/multi.tsx", "line": 3, "offset": 2}),
    ));
    assert!(response.success);
    let body = response.body.expect("jsxClosingTag should return a body");
    assert_eq!(
        body.get("newText").and_then(serde_json::Value::as_str),
        Some("</div>")
    );
    assert_eq!(
        body.get("caretOffset").and_then(serde_json::Value::as_u64),
        Some(0)
    );
}

#[test]
fn emit_output_preserves_type_only_module_marker() {
    let mut server = make_server();
    let response = server.handle_tsserver_request(make_request(
        "open",
        serde_json::json!({
            "file": "/index.ts",
            "fileContent": "export type T = string;\n",
        }),
    ));
    assert!(response.success);

    let response = server.handle_tsserver_request(make_request(
        "emit-output",
        serde_json::json!({ "file": "/index.ts" }),
    ));
    assert!(response.success);
    let body = response.body.expect("emit-output should return a body");
    assert_eq!(body["emitSkipped"], false);
    assert_eq!(body["outputFiles"][0]["name"], "/index.js");
    assert_eq!(body["outputFiles"][0]["text"], "export {};\n");
}

#[test]
fn save_to_writes_open_file_snapshot_to_tmpfile_by_ref() {
    let temp = tempfile::tempdir().expect("temp dir");
    let file = temp.path().join("a.ts");
    let tmpfile = temp.path().join("copy.ts");
    let file = file.to_string_lossy().to_string();
    let tmpfile = tmpfile.to_string_lossy().to_string();

    let mut server = make_server();
    let open = server.handle_tsserver_request(make_request(
        "open",
        serde_json::json!({
            "file": &file,
            "fileContent": "const value = 123;\n",
        }),
    ));
    assert!(open.success);

    let response = server.handle_tsserver_request(make_request(
        "saveto",
        serde_json::json!({
            "file": &file,
            "tmpfile": &tmpfile,
        }),
    ));
    assert!(response.success);
    assert_eq!(
        std::fs::read_to_string(&tmpfile).expect("tmpfile should be written"),
        "const value = 123;\n"
    );
}

#[test]
fn compile_on_save_reports_affected_files_and_emits_file() {
    let temp = tempfile::tempdir().expect("temp dir");
    let root = temp.path();
    let config = root.join("tsconfig.json");
    let a = root.join("a.ts");
    let b = root.join("b.ts");
    std::fs::write(
        &config,
        r#"{"compileOnSave":true,"compilerOptions":{"target":"es2015","module":"commonjs","outDir":"dist"},"files":["a.ts","b.ts"]}"#,
    )
    .expect("write config");
    std::fs::write(&a, "export const a = 1;\n").expect("write a");
    std::fs::write(&b, "import { a } from \"./a\";\nexport const b = a + 1;\n").expect("write b");

    let mut server = make_server();
    let a_str = a.to_string_lossy().to_string();
    let b_str = b.to_string_lossy().to_string();
    let config_str = config.to_string_lossy().to_string();

    let affected = server.handle_tsserver_request(make_request(
        "compileOnSaveAffectedFileList",
        serde_json::json!({ "file": &a_str }),
    ));
    assert!(affected.success);
    let body = affected.body.expect("affected files body");
    assert_eq!(body[0]["projectFileName"], config_str);
    assert_eq!(body[0]["fileNames"], serde_json::json!([a_str, b_str]));
    assert_eq!(body[0]["projectUsesOutFile"], false);

    let emit = server.handle_tsserver_request(make_request(
        "compileOnSaveEmitFile",
        serde_json::json!({ "file": a_str, "richResponse": true, "includeLinePosition": true }),
    ));
    assert!(emit.success);
    assert_eq!(emit.body.as_ref().unwrap()["emitSkipped"], false);
    assert!(
        emit.body.as_ref().unwrap()["diagnostics"]
            .as_array()
            .is_some()
    );
    let emitted = root.join("dist").join("a.js");
    assert!(
        emitted.exists(),
        "compile-on-save should write {}",
        emitted.display()
    );
}

#[test]
fn file_rename_updates_extensionless_relative_import() {
    let mut server = make_server();
    assert!(
        server
            .handle_tsserver_request(make_request(
                "open",
                serde_json::json!({
                    "file": "/src/index.ts",
                    "fileContent": "import { x } from \"./foo\";\nx;\n",
                }),
            ))
            .success
    );
    assert!(
        server
            .handle_tsserver_request(make_request(
                "open",
                serde_json::json!({
                    "file": "/src/foo.ts",
                    "fileContent": "export const x = 1;\n",
                }),
            ))
            .success
    );

    let response = server.handle_tsserver_request(make_request(
        "getEditsForFileRename",
        serde_json::json!({
            "oldFilePath": "/src/foo.ts",
            "newFilePath": "/src/bar.ts",
            "formatOptions": {},
            "preferences": {},
        }),
    ));
    assert!(response.success);
    let body = response.body.expect("rename should return a body");
    assert_eq!(body[0]["fileName"], "/src/index.ts");
    assert_eq!(body[0]["textChanges"][0]["newText"], "\"./bar\"");
}

#[test]
fn prepare_paste_edits_accepts_protocol_copied_text_span() {
    let mut server = make_server();
    assert!(
        server
            .handle_tsserver_request(make_request(
                "open",
                serde_json::json!({
                    "file": "/src/source.ts",
                    "fileContent": "export function helper() { return 1; }\n",
                }),
            ))
            .success
    );

    let response = server.handle_tsserver_request(make_request(
        "preparePasteEdits",
        serde_json::json!({
            "file": "/src/source.ts",
            "copiedTextSpan": [{ "start": 0, "length": 38 }],
        }),
    ));

    assert!(response.success);
    assert_eq!(response.body, Some(serde_json::json!(true)));
}

#[test]
fn brace_completion_allows_template_substitution_expressions() {
    let mut server = make_server();
    assert!(
        server
            .handle_tsserver_request(make_request(
                "open",
                serde_json::json!({
                    "file": "/src/index.ts",
                    "fileContent": "const foo = 1; const x = `${foo}`;\n",
                    "scriptKindName": "TS",
                }),
            ))
            .success
    );

    for opening_brace in ["(", "{"] {
        let response = server.handle_tsserver_request(make_request(
            "braceCompletion",
            serde_json::json!({
                "file": "/src/index.ts",
                "line": 1,
                "offset": 32,
                "openingBrace": opening_brace,
            }),
        ));
        assert!(
            response.success,
            "expected {opening_brace} inside template substitution to succeed, got {response:?}"
        );
        assert_eq!(response.body, Some(serde_json::json!(true)));
    }
}

#[test]
fn brace_completion_allows_non_quote_openings_inside_comments() {
    let mut server = make_server();
    assert!(
        server
            .handle_tsserver_request(make_request(
                "open",
                serde_json::json!({
                    "file": "/src/index.ts",
                    "fileContent": "// comment\n",
                }),
            ))
            .success
    );

    for opening_brace in ["{", "(", "["] {
        let response = server.handle_tsserver_request(make_request(
            "braceCompletion",
            serde_json::json!({
                "file": "/src/index.ts",
                "line": 1,
                "offset": 4,
                "openingBrace": opening_brace,
            }),
        ));
        assert!(
            response.success,
            "expected {opening_brace} in comment to succeed, got {response:?}"
        );
        assert_eq!(response.body, Some(serde_json::json!(true)));
    }

    let quote = server.handle_tsserver_request(make_request(
        "braceCompletion",
        serde_json::json!({
            "file": "/src/index.ts",
            "line": 1,
            "offset": 4,
            "openingBrace": "'",
        }),
    ));
    assert!(!quote.success);
    assert_eq!(quote.message.as_deref(), Some("No content available."));
    assert_eq!(quote.body, None);
}

#[test]
fn brace_completion_rejects_less_than_opening_brace() {
    let mut server = make_server();
    assert!(
        server
            .handle_tsserver_request(make_request(
                "open",
                serde_json::json!({
                    "file": "/src/index.ts",
                    "fileContent": "const x = 1;\n",
                }),
            ))
            .success
    );

    let less_than = server.handle_tsserver_request(make_request(
        "braceCompletion",
        serde_json::json!({
            "file": "/src/index.ts",
            "line": 1,
            "offset": 1,
            "openingBrace": "<",
        }),
    ));
    assert!(!less_than.success);
    assert_eq!(less_than.message.as_deref(), Some("No content available."));
    assert_eq!(less_than.body, None);

    let paren = server.handle_tsserver_request(make_request(
        "braceCompletion",
        serde_json::json!({
            "file": "/src/index.ts",
            "line": 1,
            "offset": 1,
            "openingBrace": "(",
        }),
    ));
    assert!(paren.success);
    assert_eq!(paren.body, Some(serde_json::json!(true)));
}

#[test]
fn implementation_finds_cross_file_class_implementation() {
    let mut server = make_server();
    assert!(
        server
            .handle_tsserver_request(make_request(
                "open",
                serde_json::json!({
                    "file": "/src/a.ts",
                    "fileContent": "export interface Service { run(): void }\n",
                }),
            ))
            .success
    );
    assert!(
        server
            .handle_tsserver_request(make_request(
                "open",
                serde_json::json!({
                    "file": "/src/b.ts",
                    "fileContent": "import { Service } from \"./a\";\nexport class Impl implements Service { run() {} }\n",
                }),
            ))
            .success
    );

    let response = server.handle_tsserver_request(make_request(
        "implementation",
        serde_json::json!({
            "file": "/src/a.ts",
            "line": 1,
            "offset": 18,
        }),
    ));

    assert!(response.success);
    let body = response.body.expect("implementation should return a body");
    assert_eq!(body[0]["file"], "/src/b.ts");
    assert_eq!(
        body[0]["start"],
        serde_json::json!({ "line": 2, "offset": 14 })
    );
}

#[test]
fn get_paste_edits_uses_locations_and_sibling_import_path() {
    let mut server = make_server();
    assert!(
        server
            .handle_tsserver_request(make_request(
                "open",
                serde_json::json!({
                    "file": "/src/source.ts",
                    "fileContent": "export function helper() {\n  return 1;\n}\nhelper();\n",
                }),
            ))
            .success
    );
    assert!(
        server
            .handle_tsserver_request(make_request(
                "open",
                serde_json::json!({
                    "file": "/src/target.ts",
                    "fileContent": "export function run() {\n}\n",
                }),
            ))
            .success
    );

    let response = server.handle_tsserver_request(make_request(
        "getPasteEdits",
        serde_json::json!({
            "file": "/src/target.ts",
            "pastedText": ["helper();"],
            "pasteLocations": [{
                "start": { "line": 2, "offset": 1 },
                "end": { "line": 2, "offset": 1 },
            }],
            "copiedFrom": {
                "file": "/src/source.ts",
                "spans": [{
                    "start": { "line": 4, "offset": 1 },
                    "end": { "line": 4, "offset": 10 },
                }],
            },
        }),
    ));

    assert!(response.success);
    let body = response.body.expect("paste edits should return a body");
    assert_eq!(body["fixId"], "providePostPasteEdits");
    let changes = body["edits"][0]["textChanges"]
        .as_array()
        .expect("textChanges should be an array");
    assert_eq!(
        changes[0]["span"],
        serde_json::json!({ "start": 0, "length": 0 })
    );
    assert_eq!(
        changes[0]["newText"],
        "import { helper } from \"./source\";\n\n"
    );
    assert_eq!(
        changes[1]["span"],
        serde_json::json!({ "start": 24, "length": 0 })
    );
    assert_eq!(changes[1]["newText"], "helper();");
}

#[test]
fn reset_clears_session_state_but_keeps_server_alive() {
    let mut server = make_server();
    server
        .open_files
        .insert("/test.ts".to_string(), "const x = 1;".to_string());
    server.external_project_files.insert(
        "project".to_string(),
        vec!["/test.ts".to_string(), "/dep.ts".to_string()],
    );
    server.completion_import_module_specifier_ending = Some("js".to_string());
    server.import_module_specifier_preference = Some("relative".to_string());
    server.organize_imports_type_order = Some("last".to_string());
    server.organize_imports_ignore_case = false;
    server.auto_import_file_exclude_patterns = vec!["**/dist/**".to_string()];
    server.auto_import_specifier_exclude_regexes = vec!["^internal/".to_string()];
    server.include_completions_with_class_member_snippets = false;
    server.new_line_character = Some("\r\n".to_string());
    server.allow_importing_ts_extensions = true;
    server.inferred_check_options.no_lib = true;
    server.auto_imports_allowed_for_inferred_projects = false;
    server.inferred_module_is_none_for_projects = true;
    server
        .plugin_configs
        .insert("plugin".to_string(), serde_json::json!({"enabled": true}));

    let response = server.handle_tsserver_request(make_request("tsz/reset", serde_json::json!({})));

    assert!(response.success);
    assert_eq!(response.body, Some(serde_json::json!(true)));
    assert!(server.open_files.is_empty());
    assert!(server.external_project_files.is_empty());
    assert_eq!(server.completion_import_module_specifier_ending, None);
    assert_eq!(server.import_module_specifier_preference, None);
    assert_eq!(server.organize_imports_type_order, None);
    assert!(server.organize_imports_ignore_case);
    assert!(server.auto_import_file_exclude_patterns.is_empty());
    assert!(server.auto_import_specifier_exclude_regexes.is_empty());
    assert!(server.include_completions_with_class_member_snippets);
    assert_eq!(server.new_line_character, None);
    assert!(!server.allow_importing_ts_extensions);
    assert!(!server.inferred_check_options.no_lib);
    assert!(server.inferred_check_options.lib.is_none());
    assert!(server.inferred_check_options.target.is_none());
    assert!(server.inferred_check_options.module.is_none());
    assert!(server.inferred_projectinfo_options.is_none());
    assert!(server.auto_imports_allowed_for_inferred_projects);
    assert!(!server.inferred_module_is_none_for_projects);
    assert!(server.plugin_configs.is_empty());
}

#[test]
fn organize_imports_sort_and_combine_sorts_named_imports() {
    let mut server = make_server();
    server.open_files.insert(
        "/organize-main.ts".to_string(),
        "import { b, a } from \"./organize-m\";\nconsole.log(1);\n".to_string(),
    );
    server.open_files.insert(
        "/organize-m.ts".to_string(),
        "export const a = 1;\nexport const b = 2;\n".to_string(),
    );

    let response = server.handle_tsserver_request(make_request(
        "organizeImports",
        serde_json::json!({
            "scope": {
                "type": "file",
                "args": { "file": "/organize-main.ts" }
            },
            "mode": "SortAndCombine"
        }),
    ));

    assert!(response.success);
    let body = response.body.expect("organizeImports should return a body");
    let changes = body[0]["textChanges"]
        .as_array()
        .expect("textChanges should be an array");
    assert_eq!(changes.len(), 1, "expected one sorting edit, got {body:?}");
    assert_eq!(changes[0]["newText"], " a, b ");
}

#[test]
fn organize_imports_honors_type_order_preference() {
    let mut server = make_server();
    server.open_files.insert(
        "/organize-main.ts".to_string(),
        "import { type A, type a, b, B } from \"foo\";\nconsole.log(a, b, A, B);\n".to_string(),
    );

    let response = server.handle_tsserver_request(make_request(
        "organizeImports",
        serde_json::json!({
            "scope": {
                "type": "file",
                "args": { "file": "/organize-main.ts" }
            },
            "preferences": {
                "organizeImportsIgnoreCase": "auto",
                "organizeImportsTypeOrder": "last"
            }
        }),
    ));

    assert!(response.success);
    let body = response.body.expect("organizeImports should return a body");
    let changes = body[0]["textChanges"]
        .as_array()
        .expect("textChanges should be an array");
    assert_eq!(changes.len(), 1, "expected one sorting edit, got {body:?}");
    assert_eq!(changes[0]["newText"], " b, B, type A, type a ");

    server.open_files.insert(
        "/organize-main.ts".to_string(),
        "import { type a, type A, b, B } from \"foo\";\nconsole.log(a, b, A, B);\n".to_string(),
    );

    let response = server.handle_tsserver_request(make_request(
        "organizeImports",
        serde_json::json!({
            "scope": {
                "type": "file",
                "args": { "file": "/organize-main.ts" }
            },
            "preferences": {
                "organizeImportsIgnoreCase": "auto",
                "organizeImportsTypeOrder": "last"
            }
        }),
    ));

    assert!(response.success);
    let body = response.body.expect("organizeImports should return a body");
    let changes = body[0]["textChanges"]
        .as_array()
        .expect("textChanges should be an array");
    assert_eq!(changes.len(), 1, "expected one sorting edit, got {body:?}");
    assert_eq!(changes[0]["newText"], " b, B, type a, type A ");
}

#[test]
fn test_synchronize_project_list_returns_empty_list() {
    let mut server = make_server();
    let response = server.handle_tsserver_request(make_request(
        "synchronizeProjectList",
        serde_json::json!({
            "knownProjects": [],
            "includeProjectReferenceRedirectInfo": false,
        }),
    ));

    assert!(response.success);
    assert_eq!(response.body, Some(serde_json::json!([])));
}

#[test]
fn test_synchronize_project_list_reports_external_project_files() {
    let mut server = make_server();
    let open_response = server.handle_tsserver_request(make_request(
        "openExternalProject",
        serde_json::json!({
            "projectFileName": "/project.csproj",
            "rootFiles": [
                {
                    "fileName": "/src/a.ts",
                    "content": "export const a = 1;\n",
                },
            ],
        }),
    ));
    assert!(open_response.success);
    assert_eq!(open_response.body, Some(serde_json::json!(true)));

    let response = server.handle_tsserver_request(make_request(
        "synchronizeProjectList",
        serde_json::json!({
            "knownProjects": [],
            "includeProjectReferenceRedirectInfo": true,
        }),
    ));

    assert!(response.success);
    assert_eq!(
        response.body,
        Some(serde_json::json!([{
            "info": {
                "projectName": "/project.csproj",
                "isInferred": false,
                "version": 1,
                "options": {},
                "languageServiceDisabled": false,
            },
            "files": [{
                "fileName": "/src/a.ts",
                "isSourceOfProjectReferenceRedirect": false,
            }],
            "projectErrors": [],
        }]))
    );
}

#[test]
fn test_synchronize_project_list_reports_inferred_project_for_open_file() {
    let mut server = make_server_with_real_libs();
    let open_response = server.handle_tsserver_request(make_request(
        "open",
        serde_json::json!({
            "file": "/private/tmp/tsz-probe-sync-project.ts",
            "fileContent": "const x = 1;\n",
        }),
    ));
    assert!(open_response.success);

    let response = server.handle_tsserver_request(make_request(
        "synchronizeProjectList",
        serde_json::json!({
            "knownProjects": [],
            "includeProjectReferenceRedirectInfo": false,
        }),
    ));

    assert!(response.success);
    let body = response
        .body
        .expect("synchronizeProjectList should return a body");
    let projects = body
        .as_array()
        .expect("synchronizeProjectList body should be an array");
    assert_eq!(projects.len(), 1, "expected one inferred project: {body:?}");

    let project = &projects[0];
    assert_eq!(
        project
            .pointer("/info/isInferred")
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );
    assert_eq!(
        project
            .pointer("/info/projectName")
            .and_then(serde_json::Value::as_str),
        Some("/dev/null/inferredProject1*")
    );
    assert!(
        project
            .pointer("/info/options")
            .and_then(serde_json::Value::as_object)
            .is_some(),
        "inferred project should include compiler options: {project:?}"
    );
    let files = project
        .get("files")
        .and_then(serde_json::Value::as_array)
        .expect("project files should be an array");
    assert!(
        files.iter().any(|file| file
            .as_str()
            .is_some_and(|file| file.ends_with("lib.es5.d.ts")
                || file.ends_with("lib.es5.full.d.ts")
                || file.ends_with("lib.esnext.d.ts"))),
        "inferred project should include the default lib: {files:?}"
    );
    assert!(
        files
            .iter()
            .any(|file| file.as_str() == Some("/private/tmp/tsz-probe-sync-project.ts")),
        "inferred project should include the open file: {files:?}"
    );
}

#[test]
fn project_info_uses_external_project_identity_and_files() {
    let mut server = make_server();
    let open_response = server.handle_tsserver_request(make_request(
        "openExternalProject",
        serde_json::json!({
            "projectFileName": "/workspace/external-project",
            "rootFiles": [
                {
                    "fileName": "/workspace/main.ts",
                    "content": "export const main = 1;\n",
                },
                {
                    "fileName": "/workspace/dep.ts",
                    "content": "export const dep = 1;\n",
                },
            ],
            "options": {
                "target": 1,
                "module": 1,
            },
        }),
    ));
    assert!(open_response.success);
    assert_eq!(open_response.body, Some(serde_json::json!(true)));

    let response = server.handle_tsserver_request(make_request(
        "projectInfo",
        serde_json::json!({
            "file": "/workspace/main.ts",
            "needFileNameList": true,
        }),
    ));

    assert!(response.success);
    assert_eq!(
        response.body,
        Some(serde_json::json!({
            "configFileName": "/workspace/external-project",
            "fileNames": [
                "/workspace/dep.ts",
                "/workspace/main.ts",
            ],
        }))
    );
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

fn assert_full_comment_text_changes(body: &serde_json::Value) {
    let changes = body
        .as_array()
        .expect("comment edit body should be an array");
    assert!(
        !changes.is_empty(),
        "expected at least one edit, got {body:#}"
    );
    for change in changes {
        assert!(
            change.get("start").is_none(),
            "full comment edit should not use simplified start/end shape: {change:#}"
        );
        assert!(
            change.get("end").is_none(),
            "full comment edit should not use simplified start/end shape: {change:#}"
        );
        assert!(
            change
                .get("newText")
                .and_then(serde_json::Value::as_str)
                .is_some(),
            "full comment edit should include newText: {change:#}"
        );
        let span = change
            .get("span")
            .expect("full comment edit should include span");
        assert!(
            span.get("start")
                .and_then(serde_json::Value::as_u64)
                .is_some(),
            "full comment edit span should include numeric start: {change:#}"
        );
        assert!(
            span.get("length")
                .and_then(serde_json::Value::as_u64)
                .is_some(),
            "full comment edit span should include numeric length: {change:#}"
        );
    }
}

#[test]
fn comment_edit_full_commands_return_text_changes() {
    let mut server = make_server();
    let file = "/comment-full.ts";

    server
        .open_files
        .insert(file.to_string(), "let x = 1;\nlet y = 2;\n".to_string());
    let response = server.handle_tsserver_request(make_request(
        "toggleLineComment-full",
        serde_json::json!({
            "file": file,
            "startLine": 1,
            "startOffset": 1,
            "endLine": 1,
            "endOffset": 11
        }),
    ));
    assert!(response.success);
    let body = response
        .body
        .expect("toggleLineComment-full should return edits");
    assert_full_comment_text_changes(&body);
    assert_eq!(body[0]["newText"], "//");
    assert_eq!(
        body[0]["span"],
        serde_json::json!({"start": 0, "length": 0})
    );

    server
        .open_files
        .insert(file.to_string(), "let x = 1;\nlet y = 2;\n".to_string());
    let response = server.handle_tsserver_request(make_request(
        "toggleMultilineComment-full",
        serde_json::json!({
            "file": file,
            "startLine": 1,
            "startOffset": 1,
            "endLine": 1,
            "endOffset": 10
        }),
    ));
    assert!(response.success);
    let body = response
        .body
        .expect("toggleMultilineComment-full should return edits");
    assert_full_comment_text_changes(&body);

    server
        .open_files
        .insert(file.to_string(), "let x = 1;\nlet y = 2;\n".to_string());
    let response = server.handle_tsserver_request(make_request(
        "commentSelection-full",
        serde_json::json!({
            "file": file,
            "startLine": 1,
            "startOffset": 1,
            "endLine": 2,
            "endOffset": 11
        }),
    ));
    assert!(response.success);
    let body = response
        .body
        .expect("commentSelection-full should return edits");
    assert_full_comment_text_changes(&body);

    server
        .open_files
        .insert(file.to_string(), "//let x = 1;\nlet y = 2;\n".to_string());
    let response = server.handle_tsserver_request(make_request(
        "uncommentSelection-full",
        serde_json::json!({
            "file": file,
            "startLine": 1,
            "startOffset": 1,
            "endLine": 1,
            "endOffset": 13
        }),
    ));
    assert!(response.success);
    let body = response
        .body
        .expect("uncommentSelection-full should return edits");
    assert_full_comment_text_changes(&body);
    assert_eq!(body[0]["newText"], "");
    assert_eq!(
        body[0]["span"],
        serde_json::json!({"start": 0, "length": 2})
    );
}

#[test]
fn comment_edit_simplified_command_keeps_line_offset_shape() {
    let mut server = make_server();
    let file = "/comment-simplified.ts";
    server
        .open_files
        .insert(file.to_string(), "let x = 1;\nlet y = 2;\n".to_string());

    let response = server.handle_tsserver_request(make_request(
        "toggleLineComment",
        serde_json::json!({
            "file": file,
            "startLine": 1,
            "startOffset": 1,
            "endLine": 1,
            "endOffset": 11
        }),
    ));

    assert!(response.success);
    let body = response
        .body
        .expect("toggleLineComment should return edits");
    let changes = body
        .as_array()
        .expect("comment edit body should be an array");
    assert_eq!(
        changes.len(),
        1,
        "expected one simplified edit, got {body:#}"
    );
    assert_eq!(changes[0]["newText"], "//");
    assert_eq!(
        changes[0]["start"],
        serde_json::json!({"line": 1, "offset": 1})
    );
    assert_eq!(
        changes[0]["end"],
        serde_json::json!({"line": 1, "offset": 1})
    );
    assert!(
        changes[0].get("span").is_none(),
        "simplified command should not return TextChange span"
    );
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
fn test_protocol_open_at_fourslash_path_does_not_strip_quad_slash_comments() {
    // Real client opens `/fourslash.ts` with `//// const x: string = 1;\n`.
    // tsc treats `////` as a normal comment and returns no diagnostics; tsz
    // used to apply the fourslash harness's `////`-line extraction here and
    // emit TS2322. Regression for https://github.com/mohsen1/tsz/issues/3799.
    let mut server = make_server();
    let file = "/fourslash.ts";
    server
        .open_files
        .insert(file.to_string(), "//// const x: string = 1;\n".to_string());

    let response = server.handle_tsserver_request(make_request(
        "semanticDiagnosticsSync",
        serde_json::json!({"file": file}),
    ));
    assert!(response.success);
    let diagnostics = response
        .body
        .and_then(|b| b.as_array().cloned())
        .unwrap_or_default();
    assert!(
        diagnostics.is_empty(),
        "client-supplied `////` comments must not be extracted; expected no diagnostics, got {diagnostics:?}"
    );
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
fn test_line_offset_to_byte_cr_only_second_line() {
    // tsserver treats `\r` as a line terminator. "hello" is 5 bytes, so the
    // start of line 2 is byte 6.
    assert_eq!(Server::line_offset_to_byte("hello\rworld\r", 2, 1), 6);
}

#[test]
fn test_line_offset_to_byte_cr_only_mid_second_line() {
    // Line 2, offset 3 -> 2 UTF-16 units past the start of line 2 (byte 6).
    assert_eq!(Server::line_offset_to_byte("hello\rworld\r", 2, 3), 8);
}

#[test]
fn test_line_offset_to_byte_cr_only_end_of_line() {
    // Offset just past "hello" should land on the `\r` (byte 5), not advance
    // into the next line.
    assert_eq!(Server::line_offset_to_byte("hello\rworld", 1, 6), 5);
}

#[test]
fn test_line_offset_to_byte_crlf_second_line() {
    // `\r\n` counts as a single terminator; line 2 starts after both bytes.
    assert_eq!(Server::line_offset_to_byte("hello\r\nworld\r\n", 2, 1), 7);
}

#[test]
fn test_line_offset_to_byte_crlf_third_line() {
    // Three lines separated by `\r\n` -> line 3 starts after the second
    // terminator, at byte 14.
    assert_eq!(
        Server::line_offset_to_byte("aaa\r\nbbb\r\nccc\r\n", 3, 1),
        10
    );
}

#[test]
fn test_line_offset_to_byte_mixed_line_terminators() {
    // Mixed `\n`, `\r`, and `\r\n` sequences each advance exactly one line.
    let content = "aaa\nbbb\rccc\r\nddd";
    assert_eq!(Server::line_offset_to_byte(content, 1, 1), 0);
    assert_eq!(Server::line_offset_to_byte(content, 2, 1), 4);
    assert_eq!(Server::line_offset_to_byte(content, 3, 1), 8);
    assert_eq!(Server::line_offset_to_byte(content, 4, 1), 13);
}

#[test]
fn test_apply_change_cr_only_replace_second_line() {
    // Regression for #3933: edit on line 2 of a CR-only file must land on
    // line 2, not at EOF.
    assert_eq!(
        Server::apply_change("const a = 1;\rconst b = 2;\r", 2, 11, 2, 12, "3"),
        "const a = 1;\rconst b = 3;\r"
    );
}

#[test]
fn test_apply_change_crlf_replace_second_line() {
    // CRLF line endings: edits on line 2 must land on line 2.
    assert_eq!(
        Server::apply_change("const a = 1;\r\nconst b = 2;\r\n", 2, 11, 2, 12, "3"),
        "const a = 1;\r\nconst b = 3;\r\n"
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
fn test_reload_uses_tmpfile_for_requested_open_file() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let file_path = dir.path().join("a.ts");
    let tmpfile_path = dir.path().join("tmp.ts");
    std::fs::write(&file_path, "const value = \"disk\";\n").expect("write disk file");
    std::fs::write(&tmpfile_path, "const value = 42;\n").expect("write tmpfile");

    let file = file_path.to_string_lossy().to_string();
    let tmpfile = tmpfile_path.to_string_lossy().to_string();
    let mut server = make_server();
    assert!(
        server
            .handle_tsserver_request(make_request(
                "open",
                serde_json::json!({
                    "file": file,
                    "fileContent": "const value = \"open\";\n",
                }),
            ))
            .success
    );

    let reload_response = server.handle_tsserver_request(make_request(
        "reload",
        serde_json::json!({
            "file": file,
            "tmpfile": tmpfile,
        }),
    ));

    assert!(reload_response.success);
    assert_eq!(
        reload_response.body,
        Some(serde_json::json!({ "reloadFinished": true }))
    );
    assert_eq!(server.open_files[&file], "const value = 42;\n");

    let quickinfo_response = server.handle_tsserver_request(make_request(
        "quickinfo",
        serde_json::json!({
            "file": file,
            "line": 1,
            "offset": 7,
        }),
    ));
    assert!(quickinfo_response.success);
    assert_eq!(
        quickinfo_response
            .body
            .and_then(|body| body.get("displayString").cloned()),
        Some(serde_json::json!("const value: 42"))
    );
}

#[test]
fn test_reload_projects_returns_no_body() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let file_path = dir.path().join("a.ts");
    std::fs::write(&file_path, "const value = 2;\n").expect("write disk file");

    let file = file_path.to_string_lossy().to_string();
    let mut server = make_server();
    server
        .open_files
        .insert(file.clone(), "const value = 1;\n".to_string());

    let response =
        server.handle_tsserver_request(make_request("reloadProjects", serde_json::json!({})));

    assert!(response.success);
    assert_eq!(response.body, None);
    assert_eq!(server.open_files[&file], "const value = 2;\n");
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
fn test_update_open_changed_files_edits_open_snapshot() {
    let mut server = make_server();
    let file = "/a.ts";
    server
        .open_files
        .insert(file.to_string(), "const x: string = \"ok\";".to_string());

    let update_req = make_request(
        "updateOpen",
        serde_json::json!({
            "changedFiles": [{
                "fileName": file,
                "textChanges": [{
                    "start": { "line": 1, "offset": 19 },
                    "end": { "line": 1, "offset": 23 },
                    "newText": "1"
                }]
            }]
        }),
    );
    let update_resp = server.handle_tsserver_request(update_req);
    assert!(update_resp.success);
    assert_eq!(
        server.open_files.get(file).map(String::as_str),
        Some("const x: string = 1;"),
        "updateOpen changedFiles should mutate the open-file snapshot"
    );

    let diagnostics_req = make_request(
        "semanticDiagnosticsSync",
        serde_json::json!({
            "file": file
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
        diagnostics
            .iter()
            .any(|diag| diag.get("code").and_then(serde_json::Value::as_u64) == Some(2322)),
        "semantic diagnostics should use the edited open-file snapshot, got {diagnostics:?}"
    );
}

#[test]
fn test_apply_changed_to_open_files_applies_span_edits() {
    // tsserver's `applyChangedToOpenFiles` uses byte/UTF-16-offset spans
    // (`{span: {start, length}, newText}`), unlike `updateOpen.changedFiles`
    // which uses line/offset positions. tsz used to reject the command
    // entirely. Regression for https://github.com/mohsen1/tsz/issues/3766.
    let mut server = make_server();
    let file = "/a.ts";
    server
        .open_files
        .insert(file.to_string(), "const value = 1;\n".to_string());

    let response = server.handle_tsserver_request(make_request(
        "applyChangedToOpenFiles",
        serde_json::json!({
            "changedFiles": [{
                "fileName": file,
                "changes": [{"span": {"start": 14, "length": 1}, "newText": "3"}]
            }]
        }),
    ));
    assert!(
        response.success,
        "applyChangedToOpenFiles should be recognized, got {response:?}"
    );
    assert_eq!(
        response.body,
        Some(serde_json::Value::Bool(true)),
        "tsserver returns body=true; got {response:?}"
    );
    assert_eq!(
        server.open_files.get(file).map(String::as_str),
        Some("const value = 3;\n"),
        "applyChangedToOpenFiles must rewrite the open snapshot"
    );
}

#[test]
fn test_apply_changed_to_open_files_handles_open_and_closed_files() {
    // Mirrors `updateOpen` semantics for the auxiliary `openFiles` /
    // `closedFiles` arrays: `applyChangedToOpenFiles` may carry these too.
    let mut server = make_server();
    server
        .open_files
        .insert("/old.ts".to_string(), "x".to_string());

    let response = server.handle_tsserver_request(make_request(
        "applyChangedToOpenFiles",
        serde_json::json!({
            "openFiles": [{"file": "/new.ts", "fileContent": "const y = 1;"}],
            "closedFiles": ["/old.ts"]
        }),
    ));
    assert!(response.success);
    assert!(server.open_files.contains_key("/new.ts"));
    assert!(!server.open_files.contains_key("/old.ts"));
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
fn test_semantic_diagnostics_preserve_options_with_numeric_inferred_target() {
    let mut server = make_server();

    let options_resp = server.handle_tsserver_request(make_request(
        "compilerOptionsForInferredProjects",
        serde_json::json!({
            "options": {
                "noImplicitAny": true,
                "target": 2
            }
        }),
    ));
    assert!(options_resp.success);

    let open_resp = server.handle_tsserver_request(make_request(
        "open",
        serde_json::json!({
            "file": "/index.ts",
            "fileContent": "function f(x) { return x; }\n"
        }),
    ));
    assert!(open_resp.success);

    let diagnostics_resp = server.handle_tsserver_request(make_request(
        "semanticDiagnosticsSync",
        serde_json::json!({
            "file": "/index.ts"
        }),
    ));
    assert!(diagnostics_resp.success);
    let diagnostics = diagnostics_resp
        .body
        .expect("semanticDiagnosticsSync should return a body")
        .as_array()
        .expect("semanticDiagnosticsSync body should be an array")
        .clone();
    assert!(
        diagnostics.iter().any(|diag| {
            diag.get("code").and_then(serde_json::Value::as_u64)
                == Some(
                    tsz_checker::diagnostics::diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE
                        as u64,
                )
        }),
        "expected noImplicitAny to survive numeric target payload, got: {diagnostics:?}"
    );
}

#[test]
fn test_semantic_diagnostics_include_line_position_uses_utf16_length() {
    let mut server = make_server_with_real_libs();
    server.open_files.insert(
        "/index.ts".to_string(),
        "const café: string = 1;\n".to_string(),
    );

    let diagnostics_resp = server.handle_tsserver_request(make_request(
        "semanticDiagnosticsSync",
        serde_json::json!({
            "file": "/index.ts",
            "includeLinePosition": true
        }),
    ));

    assert!(diagnostics_resp.success);
    let diagnostics = diagnostics_resp
        .body
        .expect("semanticDiagnosticsSync should return a body")
        .as_array()
        .expect("semanticDiagnosticsSync body should be an array")
        .clone();
    let diagnostic = diagnostics
        .first()
        .unwrap_or_else(|| panic!("expected a semantic diagnostic, got {diagnostics:?}"));
    assert_eq!(diagnostic.get("start"), Some(&serde_json::json!(6)));
    assert_eq!(diagnostic.get("length"), Some(&serde_json::json!(4)));
    assert_eq!(
        diagnostic.get("startLocation"),
        Some(&serde_json::json!({ "line": 1, "offset": 7 }))
    );
    assert_eq!(
        diagnostic.get("endLocation"),
        Some(&serde_json::json!({ "line": 1, "offset": 11 }))
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
fn applicable_refactors_include_tsserver_extract_actions() {
    let mut server = make_server();
    assert!(
        server
            .handle_tsserver_request(make_request(
                "open",
                serde_json::json!({
                    "file": "/src/a.ts",
                    "fileContent": "function f() {\n  const y = 1 + 2;\n  return y;\n}\n",
                }),
            ))
            .success
    );

    let response = server.handle_tsserver_request(make_request(
        "getApplicableRefactors",
        serde_json::json!({
            "file": "/src/a.ts",
            "startLine": 2,
            "startOffset": 13,
            "endLine": 2,
            "endOffset": 18,
        }),
    ));

    assert!(response.success);
    let body = response.body.expect("refactors should return a body");
    let action_names = body
        .as_array()
        .expect("refactors should be an array")
        .iter()
        .flat_map(|refactor| {
            refactor
                .get("actions")
                .and_then(|actions| actions.as_array())
                .into_iter()
                .flatten()
        })
        .filter_map(|action| action.get("name").and_then(|name| name.as_str()))
        .collect::<Vec<_>>();

    for expected in [
        "function_scope_0",
        "function_scope_1",
        "constant_scope_0",
        "constant_scope_1",
    ] {
        assert!(
            action_names.contains(&expected),
            "expected {expected} in applicable refactors, got {body:#}"
        );
    }
    assert!(
        !action_names.contains(&"constant_extractedConstant"),
        "did not expect non-tsserver action name, got {body:#}"
    );
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
fn breakpoint_and_comment_span_commands_return_numeric_text_spans() {
    let mut server = make_server();
    let file = "/text-span-shapes.tsx";
    server.open_files.insert(
        file.to_string(),
        "function f() {\n  const x = 1; // TODO: work\n  /* block comment */\n}\n".to_string(),
    );

    let cases = [
        make_request(
            "breakpointStatement",
            serde_json::json!({"file": file, "line": 2, "offset": 9}),
        ),
        make_request(
            "getSpanOfEnclosingComment",
            serde_json::json!({"file": file, "line": 2, "offset": 17, "onlyMultiLine": false}),
        ),
        make_request(
            "getSpanOfEnclosingComment",
            serde_json::json!({"file": file, "line": 3, "offset": 5, "onlyMultiLine": true}),
        ),
    ];

    for req in cases {
        let command = req.command.clone();
        let resp = server.handle_tsserver_request(req);
        assert!(resp.success, "{command} should succeed");
        let body = resp.body.expect("command should return a body");
        assert!(
            body.get("start")
                .and_then(serde_json::Value::as_u64)
                .is_some(),
            "{command} should return numeric TextSpan start: {body:?}"
        );
        assert!(
            body.get("length")
                .and_then(serde_json::Value::as_u64)
                .is_some(),
            "{command} should return numeric TextSpan length: {body:?}"
        );
        assert!(
            body.get("textSpan").is_none(),
            "{command} should not wrap the TextSpan: {body:?}"
        );
    }
}

#[test]
fn test_full_protocol_dispatcher_commands_are_recognized() {
    let mut server = make_server();
    let file = "/full-routes.ts";
    server.open_files.insert(
        file.to_string(),
        [
            "function add(a: number, b: number) {",
            "  return a + b;",
            "}",
            "add(1, 2);",
            "const value = add;",
        ]
        .join("\n"),
    );

    let requests = [
        make_request(
            "quickinfo-full",
            serde_json::json!({"file": file, "line": 4, "offset": 1}),
        ),
        make_request(
            "completions-full",
            serde_json::json!({"file": file, "line": 5, "offset": 15}),
        ),
        make_request(
            "signatureHelp-full",
            serde_json::json!({"file": file, "line": 4, "offset": 6}),
        ),
        make_request(
            "format-full",
            serde_json::json!({
                "file": file,
                "line": 1,
                "offset": 1,
                "options": {
                    "indentSize": 2,
                    "tabSize": 2,
                    "convertTabsToSpaces": true,
                },
            }),
        ),
        make_request("outliningSpans", serde_json::json!({"file": file})),
        make_request("fileReferences-full", serde_json::json!({"file": file})),
        make_request("navbar-full", serde_json::json!({"file": file})),
        make_request(
            "selectionRange-full",
            serde_json::json!({"file": file, "locations": [{"line": 2, "offset": 12}]}),
        ),
    ];

    for req in requests {
        let command = req.command.clone();
        let resp = server.handle_tsserver_request(req);
        assert!(
            resp.success,
            "{command} should be routed to a handler, got: {resp:?}"
        );
    }
}

#[test]
fn selection_range_full_returns_numeric_text_spans() {
    let mut server = make_server();
    let file = "/selection-full.ts";
    server.open_files.insert(
        file.to_string(),
        "function f() {\n  return 1 + 2;\n}\n".to_string(),
    );

    let resp = server.handle_tsserver_request(make_request(
        "selectionRange-full",
        serde_json::json!({
            "file": file,
            "locations": [{ "line": 2, "offset": 12 }]
        }),
    ));

    assert!(resp.success);
    let body = resp.body.expect("selectionRange-full should return a body");
    let ranges = body
        .as_array()
        .expect("selectionRange-full body should be an array");
    assert_eq!(ranges.len(), 1);
    let text_span = ranges[0]
        .get("textSpan")
        .expect("selectionRange-full should include textSpan");
    assert!(
        text_span
            .get("start")
            .and_then(serde_json::Value::as_u64)
            .is_some(),
        "full selection range should use numeric TextSpan start: {text_span:?}"
    );
    assert!(
        text_span
            .get("length")
            .and_then(serde_json::Value::as_u64)
            .is_some(),
        "full selection range should use numeric TextSpan length: {text_span:?}"
    );
    assert!(
        text_span
            .get("start")
            .and_then(|start| start.get("line"))
            .is_none(),
        "full selection range should not use simplified line/offset shape: {text_span:?}"
    );
}

#[test]
fn test_indentation_returns_absolute_position() {
    let mut server = make_server();
    server.open_files.insert(
        "/indent.ts".to_string(),
        "const a = 1;\nconst b = 2;\n".to_string(),
    );
    let req = make_request(
        "indentation",
        serde_json::json!({
            "file": "/indent.ts",
            "line": 2,
            "offset": 5,
            "options": { "indentSize": 2, "tabSize": 2 }
        }),
    );

    let resp = server.handle_tsserver_request(req);

    assert!(resp.success);
    let body = resp.body.expect("indentation should return a body");
    assert_eq!(body.get("position"), Some(&serde_json::json!(17)));
    assert_eq!(body.get("indentation"), Some(&serde_json::json!(0)));
}

#[test]
fn test_todo_comments_report_utf16_position_after_non_bmp_text() {
    let mut server = make_server();
    let file = "/todo-unicode.ts";
    let source = "const s = \"😀\"; // TODO after emoji\n";
    server
        .open_files
        .insert(file.to_string(), source.to_string());

    let response = server.handle_tsserver_request(make_request(
        "todoComments",
        serde_json::json!({
            "file": file,
            "descriptors": [{ "text": "TODO", "priority": 1 }],
        }),
    ));

    assert!(response.success);
    let body = response.body.expect("todoComments should return a body");
    let comments = body
        .as_array()
        .expect("todoComments body should be an array");
    assert_eq!(comments.len(), 1, "expected one TODO comment, got {body:#}");
    assert_eq!(comments[0]["message"], "TODO after emoji");
    assert_eq!(
        comments[0]["position"], 19,
        "todoComments position should use UTF-16 offsets, not UTF-8 byte offsets: {body:#}"
    );
}

#[test]
fn test_todo_comments_match_inside_template_substitutions() {
    // Comments inside `${...}` are real comments and tsc reports TODOs from
    // them. Regression for https://github.com/mohsen1/tsz/issues/4003 — the
    // backtick-to-backtick skip used to mask block and line comments inside
    // template substitutions.
    let mut server = make_server();
    let file = "/todo-template.ts";
    let source = "const s = `${/* TODO inside substitution */ 1}`;\n// TODO outside\n".to_string();
    server.open_files.insert(file.to_string(), source);

    let response = server.handle_tsserver_request(make_request(
        "todoComments",
        serde_json::json!({
            "file": file,
            "descriptors": [{ "text": "TODO", "priority": 1 }],
        }),
    ));
    assert!(response.success);
    let body = response.body.expect("todoComments should return a body");
    let comments = body
        .as_array()
        .expect("todoComments body should be an array");
    let messages: Vec<&str> = comments
        .iter()
        .map(|c| c["message"].as_str().unwrap_or(""))
        .collect();
    assert!(
        messages
            .iter()
            .any(|m| m.starts_with("TODO inside substitution")),
        "expected TODO inside ${{...}} substitution, got {messages:?}"
    );
    assert!(
        messages.iter().any(|m| m.starts_with("TODO outside")),
        "expected outside TODO to still match, got {messages:?}"
    );
}

#[test]
fn test_doc_comment_template_omits_returns_when_configure_disables_pref() {
    // tsserver lets clients turn off `@returns` via the
    // `generateReturnInDocTemplate` preference. tsz-server was hard-coding the
    // default. Regression for https://github.com/mohsen1/tsz/issues/3972.
    let mut server = make_server();
    let file = "/doc-template.ts";
    server.open_files.insert(
        file.to_string(),
        "function f(a: number) { return a; }\n".to_string(),
    );

    let configure = server.handle_tsserver_request(make_request(
        "configure",
        serde_json::json!({
            "preferences": { "generateReturnInDocTemplate": false }
        }),
    ));
    assert!(configure.success);

    let response = server.handle_tsserver_request(make_request(
        "docCommentTemplate",
        serde_json::json!({ "file": file, "line": 1, "offset": 1 }),
    ));
    assert!(response.success);
    let new_text = response
        .body
        .as_ref()
        .and_then(|b| b.get("newText"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(
        new_text.contains("@param a"),
        "@param should still be present, got {new_text:?}"
    );
    assert!(
        !new_text.contains("@returns"),
        "@returns must be omitted when generateReturnInDocTemplate=false, got {new_text:?}"
    );
}

#[test]
fn test_todo_comments_skip_template_text_outside_substitutions() {
    // Plain template text (no `${...}`) must not produce false TODO matches
    // — only real comments do. Locks the existing template-skip behavior.
    let mut server = make_server();
    let file = "/todo-template-text.ts";
    let source = "const s = `hello // TODO not a comment` ;\n".to_string();
    server.open_files.insert(file.to_string(), source);

    let response = server.handle_tsserver_request(make_request(
        "todoComments",
        serde_json::json!({
            "file": file,
            "descriptors": [{ "text": "TODO", "priority": 1 }],
        }),
    ));
    assert!(response.success);
    let body = response.body.expect("todoComments should return a body");
    let comments = body
        .as_array()
        .expect("todoComments body should be an array");
    assert!(
        comments.is_empty(),
        "TODO inside template *text* must not match, got {body:#}"
    );
}

#[test]
fn test_doc_comment_template_per_request_arg_overrides_configure_pref() {
    // Per-request `generateReturnInDocTemplate` takes precedence over the
    // configured user preference, matching tsserver's resolution order.
    let mut server = make_server();
    let file = "/doc-template-override.ts";
    server.open_files.insert(
        file.to_string(),
        "function f(a: number) { return a; }\n".to_string(),
    );

    server.handle_tsserver_request(make_request(
        "configure",
        serde_json::json!({
            "preferences": { "generateReturnInDocTemplate": false }
        }),
    ));

    let response = server.handle_tsserver_request(make_request(
        "docCommentTemplate",
        serde_json::json!({
            "file": file,
            "line": 1,
            "offset": 1,
            "generateReturnInDocTemplate": true
        }),
    ));
    let new_text = response
        .body
        .as_ref()
        .and_then(|b| b.get("newText"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(
        new_text.contains("@returns"),
        "per-request arg=true must override configure=false, got {new_text:?}"
    );
}

#[test]
fn test_todo_comments_handle_nested_template_in_substitution() {
    // Nested templates inside substitutions: the outer substitution contains
    // an inner backtick literal whose own substitution holds the TODO.
    let mut server = make_server();
    let file = "/todo-nested-template.ts";
    let source = "const s = `${`${/* TODO nested */ 1}`}`;\n".to_string();
    server.open_files.insert(file.to_string(), source);

    let response = server.handle_tsserver_request(make_request(
        "todoComments",
        serde_json::json!({
            "file": file,
            "descriptors": [{ "text": "TODO", "priority": 1 }],
        }),
    ));
    assert!(response.success);
    let body = response.body.expect("todoComments should return a body");
    let comments = body
        .as_array()
        .expect("todoComments body should be an array");
    let messages: Vec<&str> = comments
        .iter()
        .map(|c| c["message"].as_str().unwrap_or(""))
        .collect();
    assert!(
        messages.iter().any(|m| m.starts_with("TODO nested")),
        "expected nested TODO to match, got {messages:?}"
    );
}

#[test]
fn test_doc_comment_template_default_includes_returns() {
    // No configure, no per-request arg: default is `true` (matches tsc).
    let mut server = make_server();
    let file = "/doc-template-default.ts";
    server.open_files.insert(
        file.to_string(),
        "function f(a: number) { return a; }\n".to_string(),
    );

    let response = server.handle_tsserver_request(make_request(
        "docCommentTemplate",
        serde_json::json!({ "file": file, "line": 1, "offset": 1 }),
    ));
    let new_text = response
        .body
        .as_ref()
        .and_then(|b| b.get("newText"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(
        new_text.contains("@returns"),
        "default behavior should include @returns, got {new_text:?}"
    );
}

#[test]
fn test_encoded_syntactic_classifications_full_returns_token_triples() {
    // Issue #3717: tsserver protocol command was unimplemented; the
    // dispatch returned `Unrecognized command`. The handler now walks
    // tokens and emits `(start, length, classificationId)` triples.
    // For `const x = 1;`, tsc emits 5 triples corresponding to the 5
    // non-trivia tokens.
    let mut server = make_server();
    let file = "/a.ts";
    server
        .open_files
        .insert(file.to_string(), "const x = 1;".to_string());

    let response = server.handle_tsserver_request(make_request(
        "encodedSyntacticClassifications-full",
        serde_json::json!({"file": file, "start": 0, "length": 12}),
    ));
    assert!(response.success);
    let body = response.body.expect("body");
    let spans = body
        .get("spans")
        .and_then(|v| v.as_array())
        .expect("spans array");
    assert_eq!(body.get("endOfLineState").and_then(|v| v.as_u64()), Some(0));

    let to_u32 = |v: &serde_json::Value| v.as_u64().unwrap_or(0) as u32;
    let triples: Vec<(u32, u32, u32)> = spans
        .chunks(3)
        .filter_map(|c| {
            if c.len() == 3 {
                Some((to_u32(&c[0]), to_u32(&c[1]), to_u32(&c[2])))
            } else {
                None
            }
        })
        .collect();
    // tsc class IDs: keyword=3, identifier=2, operator=5, number=4, punctuation=10
    assert_eq!(
        triples,
        vec![
            (0, 5, 3),   // const
            (6, 1, 2),   // x
            (8, 1, 5),   // =
            (10, 1, 4),  // 1
            (11, 1, 10), // ;
        ],
        "tokens must be ordered (start, length, classId) triples matching tsc"
    );
}

#[test]
fn test_geterr_emits_diagnostic_events_then_request_completed() {
    // Issue #3544: `geterr` returned success but never emitted any of the
    // async diagnostic events tsserver clients expect. After the fix, the
    // response acknowledges and the queued events follow:
    //   syntaxDiag -> semanticDiag -> suggestionDiag (per file)
    //   requestCompleted (after all files)
    let mut server = make_server();
    let file = "/a.ts";
    server
        .open_files
        .insert(file.to_string(), "const x: string = 1;\n".to_string());

    let response = server.handle_tsserver_request(make_request(
        "geterr",
        serde_json::json!({"delay": 0, "files": [file]}),
    ));
    assert!(response.success);

    let events = server.drain_pending_events();
    let event_names: Vec<&str> = events
        .iter()
        .filter_map(|e| e.get("event").and_then(|v| v.as_str()))
        .collect();
    assert_eq!(
        event_names,
        vec![
            "syntaxDiag",
            "semanticDiag",
            "suggestionDiag",
            "requestCompleted"
        ],
        "expected ordered diagnostic events + requestCompleted, got {event_names:?}"
    );

    // Each diag event must carry the file and a diagnostics array.
    for (idx, name) in ["syntaxDiag", "semanticDiag", "suggestionDiag"]
        .iter()
        .enumerate()
    {
        let body = events[idx].get("body").expect("event body");
        assert_eq!(
            body.get("file").and_then(|v| v.as_str()),
            Some(file),
            "{name} body must name the file"
        );
        assert!(
            body.get("diagnostics").is_some_and(|d| d.is_array()),
            "{name} body must carry a diagnostics array, got {body:?}"
        );
    }

    // requestCompleted carries the originating request_seq.
    let last = events.last().expect("requestCompleted event");
    assert!(
        last.get("body")
            .and_then(|b| b.get("request_seq"))
            .is_some(),
        "requestCompleted must carry request_seq, got {last:?}"
    );
}

#[test]
fn test_encoded_syntactic_classifications_command_is_routed() {
    // Locks the routing: command must not surface as "Unrecognized" any
    // more (issue #3717's primary symptom).
    let mut server = make_server();
    server
        .open_files
        .insert("/a.ts".to_string(), "const x = 1;".to_string());
    let response = server.handle_tsserver_request(make_request(
        "encodedSyntacticClassifications-full",
        serde_json::json!({"file": "/a.ts", "start": 0, "length": 12}),
    ));
    assert!(response.success);
    assert!(
        !response
            .message
            .as_deref()
            .unwrap_or("")
            .contains("Unrecognized")
    );
}

#[test]
fn test_geterr_for_project_emits_events_for_open_files() {
    // `geterrForProject` mirrors `geterr` but covers all open files when
    // no project graph is available.
    let mut server = make_server();
    server
        .open_files
        .insert("/a.ts".to_string(), "const x = 1;\n".to_string());
    server
        .open_files
        .insert("/b.ts".to_string(), "const y = 2;\n".to_string());

    let response = server.handle_tsserver_request(make_request(
        "geterrForProject",
        serde_json::json!({"delay": 0, "file": "/a.ts"}),
    ));
    assert!(response.success);

    let events = server.drain_pending_events();
    let last = events.last().expect("requestCompleted event");
    assert_eq!(
        last.get("event").and_then(|v| v.as_str()),
        Some("requestCompleted"),
        "requestCompleted must be the final event"
    );
    // Three events per file (syntax/semantic/suggestion) + 1 completion.
    assert_eq!(
        events.len(),
        server.open_files.len() * 3 + 1,
        "expected 3 events per open file + 1 completion, got {} events",
        events.len()
    );
}

#[test]
fn test_signature_help_has_no_body_without_signature() {
    let mut server = make_server();
    server
        .open_files
        .insert("/test.ts".to_string(), "const x = 1;\n".to_string());
    let req = make_request(
        "signatureHelp",
        serde_json::json!({"file": "/test.ts", "line": 1, "offset": 1}),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    assert!(
        resp.body.is_none(),
        "signatureHelp should omit body when no signature exists"
    );
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

#[test]
fn test_watch_change_is_unrecognized() {
    let mut server = make_server();
    let req = make_request(
        "watchChange",
        serde_json::json!({
            "id": 1,
            "created": ["/tmp/x.ts"],
            "changed": [],
            "deleted": [],
        }),
    );
    let resp = server.handle_tsserver_request(req);

    assert!(!resp.success);
    assert!(
        resp.message
            .unwrap()
            .contains("Unrecognized command: watchChange")
    );
}

/// Obsolete TS5-era selection/classification commands are not part of the
/// TS6.0.3 protocol; tsserver replies with `success: false` and an
/// "Unrecognized JSON command" message. tsz-server previously routed them
/// to placeholder handlers that returned `success: true`.
#[test]
fn test_obsolete_selection_and_classification_commands_are_unrecognized() {
    let mut server = make_server();
    let file = "/obsolete-cmds.ts";
    server.open_files.insert(
        file.to_string(),
        "const answer = 42;\nfunction f(x: number) { return answer + x; }\n".to_string(),
    );

    for cmd in [
        "getSmartSelectionRange",
        "getSyntacticClassifications",
        "getSemanticClassifications",
    ] {
        let req = make_request(
            cmd,
            serde_json::json!({
                "file": file,
                "start": 0,
                "length": 64,
            }),
        );
        let resp = server.handle_tsserver_request(req);
        assert!(!resp.success, "{cmd}: should report success=false");
        assert!(
            resp.body.is_none(),
            "{cmd}: should not return a placeholder body",
        );
        let message = resp.message.unwrap_or_default();
        assert!(
            message.contains("Unrecognized") && message.contains(cmd),
            "{cmd}: expected unrecognized-command message, got {message:?}",
        );
    }
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
fn test_quickinfo_preserves_non_ascii_identifier_display() {
    let mut server = make_server();
    server.open_files.insert(
        "/index.ts".to_string(),
        "const café = 1;\ncafé;\n".to_string(),
    );
    let req = make_request(
        "quickinfo",
        serde_json::json!({"file": "/index.ts", "line": 2, "offset": 2}),
    );

    let resp = server.handle_tsserver_request(req);

    assert!(resp.success);
    let body = resp.body.expect("quickinfo should return a body");
    assert_eq!(
        body.get("displayString"),
        Some(&serde_json::json!("const café: 1"))
    );
    assert_eq!(
        body.get("start"),
        Some(&serde_json::json!({ "line": 2, "offset": 1 }))
    );
    assert_eq!(
        body.get("end"),
        Some(&serde_json::json!({ "line": 2, "offset": 5 }))
    );
}

#[test]
fn test_quickinfo_has_no_body_without_info() {
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
    assert!(
        resp.body.is_none(),
        "quickinfo should omit body when no quickinfo exists"
    );
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
    assert_eq!(body["documentation"], serde_json::json!("Doc"));

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
    assert_eq!(body_at_member["documentation"], serde_json::json!("Doc"));
}

#[test]
fn test_quickinfo_documentation_is_protocol_string() {
    let mut server = make_server();
    server.open_files.insert(
        "/test.ts".to_string(),
        "/** Adds one. */\nfunction f(a: number): string { return String(a + 1); }\nf(1);\nconst n = 1;\nn;\n"
            .to_string(),
    );

    let documented = server.handle_tsserver_request(make_request(
        "quickinfo",
        serde_json::json!({"file": "/test.ts", "line": 3, "offset": 1}),
    ));
    assert!(documented.success);
    let documented_body = documented.body.expect("quickinfo should return a body");
    assert_eq!(
        documented_body["documentation"],
        serde_json::json!("Adds one.")
    );

    let undocumented = server.handle_tsserver_request(make_request(
        "quickinfo",
        serde_json::json!({"file": "/test.ts", "line": 5, "offset": 1}),
    ));
    assert!(undocumented.success);
    let undocumented_body = undocumented.body.expect("quickinfo should return a body");
    assert_eq!(undocumented_body["documentation"], serde_json::json!(""));
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
fn test_quickinfo_parameter_uses_contextual_type() {
    let mut server = make_server();
    server.open_files.insert(
        "/test.ts".to_string(),
        "var c3t11: {(n: number, s: string): string;}[] = [function(/*25*/n, s) { return s; }];\n"
            .to_string(),
    );
    let req = make_request(
        "quickinfo",
        // Cursor on `n` after /*25*/.
        serde_json::json!({"file": "/test.ts", "line": 1, "offset": 66}),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body_on_identifier = resp.body.expect("quickinfo should return a body");
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

/// Issue #3753: outgoing call to an imported function should point at the
/// exported declaration in the imported module's source file, not at the
/// local import binding in the importer.
#[test]
fn test_call_hierarchy_outgoing_resolves_through_import() {
    let mut server = make_server();
    server.open_files.insert(
        "/a.ts".to_string(),
        "export function target() {}\n".to_string(),
    );
    server.open_files.insert(
        "/b.ts".to_string(),
        "import { target } from \"./a\";\nexport function caller() { target(); }\n".to_string(),
    );

    let req = make_request(
        "provideCallHierarchyOutgoingCalls",
        serde_json::json!({
            "file": "/b.ts",
            "line": 2,
            "offset": 17
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("outgoing calls should return a body");
    let calls = body
        .as_array()
        .expect("provideCallHierarchyOutgoingCalls body should be an array");

    let target_call = calls
        .iter()
        .find(|call| call["to"]["name"] == "target")
        .unwrap_or_else(|| panic!("expected outgoing target 'target', got: {calls:?}"));

    let to_file = target_call["to"]["file"].as_str().unwrap_or("");
    assert!(
        to_file.ends_with("/a.ts"),
        "Expected outgoing target to resolve to /a.ts (the imported module), got file={to_file:?} call={target_call:?}"
    );
    assert!(
        !to_file.ends_with("/b.ts"),
        "Outgoing target must not stay on the importer's local binding (/b.ts). Got file={to_file:?} call={target_call:?}"
    );

    // The selectionSpan should anchor on the exported function in /a.ts —
    // the identifier `target` lives at column 17 (1-based) of line 1.
    let selection_start = &target_call["to"]["selectionSpan"]["start"];
    assert_eq!(
        selection_start["line"].as_u64(),
        Some(1),
        "expected selection at line 1 of a.ts, got: {target_call:?}"
    );
}

/// Issue #3753 (incoming half): asking for incoming calls on a function
/// exported from /a.ts must include callers in /b.ts that reach it via
/// `import { target } from "./a"`. Without the cross-file scan tsz only
/// reported within-file callers, so the response was an empty array.
#[test]
fn test_call_hierarchy_incoming_includes_cross_file_caller() {
    let mut server = make_server();
    server.open_files.insert(
        "/a.ts".to_string(),
        "export function target() {}\n".to_string(),
    );
    server.open_files.insert(
        "/b.ts".to_string(),
        "import { target } from \"./a\";\nexport function caller() { target(); }\n".to_string(),
    );

    let req = make_request(
        "provideCallHierarchyIncomingCalls",
        serde_json::json!({
            "file": "/a.ts",
            "line": 1,
            "offset": 17
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("incoming calls should return a body");
    let calls = body
        .as_array()
        .expect("provideCallHierarchyIncomingCalls body should be an array");

    let caller = calls
        .iter()
        .find(|call| call["from"]["name"] == "caller")
        .unwrap_or_else(|| panic!("expected cross-file caller 'caller', got: {calls:?}"));

    let from_file = caller["from"]["file"].as_str().unwrap_or("");
    assert!(
        from_file.ends_with("/b.ts"),
        "cross-file caller should live in /b.ts, got file={from_file:?} call={caller:?}"
    );
}

/// Aliased imports — `import { target as t }` — must also be discovered as
/// callers when the local binding `t` is invoked. The exported-name match
/// is what counts, not the local name.
#[test]
fn test_call_hierarchy_incoming_handles_aliased_cross_file_import() {
    let mut server = make_server();
    server.open_files.insert(
        "/a.ts".to_string(),
        "export function target() {}\n".to_string(),
    );
    server.open_files.insert(
        "/b.ts".to_string(),
        "import { target as t } from \"./a\";\nexport function caller() { t(); }\n".to_string(),
    );

    let req = make_request(
        "provideCallHierarchyIncomingCalls",
        serde_json::json!({
            "file": "/a.ts",
            "line": 1,
            "offset": 17
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("incoming calls should return a body");
    let calls = body
        .as_array()
        .expect("provideCallHierarchyIncomingCalls body should be an array");

    assert!(
        calls.iter().any(|c| c["from"]["name"] == "caller"
            && c["from"]["file"]
                .as_str()
                .is_some_and(|f| f.ends_with("/b.ts"))),
        "expected cross-file caller via aliased import, got: {calls:?}"
    );
}

/// Issue #3753 follow-up: namespace imports — `import * as ns from "./a"`
/// followed by `ns.target()` must register as a cross-file caller of the
/// exported `target` in /a.ts.
#[test]
fn test_call_hierarchy_incoming_handles_namespace_import_member_call() {
    let mut server = make_server();
    server.open_files.insert(
        "/a.ts".to_string(),
        "export function target() {}\n".to_string(),
    );
    server.open_files.insert(
        "/b.ts".to_string(),
        "import * as ns from \"./a\";\nexport function caller() { ns.target(); }\n".to_string(),
    );

    let req = make_request(
        "provideCallHierarchyIncomingCalls",
        serde_json::json!({
            "file": "/a.ts",
            "line": 1,
            "offset": 17
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("incoming calls should return a body");
    let calls = body
        .as_array()
        .expect("provideCallHierarchyIncomingCalls body should be an array");

    assert!(
        calls.iter().any(|c| c["from"]["name"] == "caller"
            && c["from"]["file"]
                .as_str()
                .is_some_and(|f| f.ends_with("/b.ts"))),
        "expected cross-file caller via `ns.target()` namespace import, got: {calls:?}"
    );
}

/// Namespace import where the user calls a *different* member of the
/// imported namespace must NOT register as a caller of `target` (no false
/// positives from same-namespace, different-member calls).
#[test]
fn test_call_hierarchy_incoming_namespace_skips_other_members() {
    let mut server = make_server();
    server.open_files.insert(
        "/a.ts".to_string(),
        "export function target() {}\nexport function other() {}\n".to_string(),
    );
    server.open_files.insert(
        "/b.ts".to_string(),
        "import * as ns from \"./a\";\nexport function caller() { ns.other(); }\n".to_string(),
    );

    let req = make_request(
        "provideCallHierarchyIncomingCalls",
        serde_json::json!({
            "file": "/a.ts",
            "line": 1,
            "offset": 17
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("incoming calls should return a body");
    let calls = body
        .as_array()
        .expect("provideCallHierarchyIncomingCalls body should be an array");

    assert!(
        !calls.iter().any(|c| c["from"]["name"] == "caller"),
        "must not list caller — it calls ns.other(), not ns.target(). got: {calls:?}"
    );
}

/// A different file that has its own `target` (not imported from /a.ts) must
/// not pollute the incoming-calls answer for /a.ts's `target`.
#[test]
fn test_call_hierarchy_incoming_skips_unrelated_imports() {
    let mut server = make_server();
    server.open_files.insert(
        "/a.ts".to_string(),
        "export function target() {}\n".to_string(),
    );
    server.open_files.insert(
        "/c.ts".to_string(),
        "function target() {}\nexport function localCaller() { target(); }\n".to_string(),
    );

    let req = make_request(
        "provideCallHierarchyIncomingCalls",
        serde_json::json!({
            "file": "/a.ts",
            "line": 1,
            "offset": 17
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("incoming calls should return a body");
    let calls = body
        .as_array()
        .expect("provideCallHierarchyIncomingCalls body should be an array");

    assert!(
        !calls.iter().any(|c| c["from"]["name"] == "localCaller"),
        "must not report localCaller from /c.ts (no import edge from /a.ts), got: {calls:?}"
    );
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

/// Issue #3753 follow-up: default-export incoming calls.
///
/// `/a.ts` declares `export default function target()`. `/b.ts` invokes it
/// via a default-import (`import target from "./a"`). tsc reports `caller`
/// in /b.ts as an incoming call of `target`; the cross-file caller scan
/// previously only fired the default-import branch when the user asked
/// for incoming calls on a symbol literally named "default", so for this
/// shape (where the prepared name is "target") it returned [].
#[test]
fn test_call_hierarchy_incoming_handles_default_import_for_default_export() {
    let mut server = make_server();
    server.open_files.insert(
        "/a.ts".to_string(),
        "export default function target() {}\n".to_string(),
    );
    server.open_files.insert(
        "/b.ts".to_string(),
        "import target from \"./a\";\nexport function caller() { target(); }\n".to_string(),
    );

    let req = make_request(
        "provideCallHierarchyIncomingCalls",
        serde_json::json!({
            "file": "/a.ts",
            "line": 1,
            "offset": 25,
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("incoming calls should return a body");
    let calls = body
        .as_array()
        .expect("provideCallHierarchyIncomingCalls body should be an array");

    let caller = calls
        .iter()
        .find(|c| c["from"]["name"] == "caller")
        .unwrap_or_else(|| panic!("expected default-import caller 'caller', got: {calls:?}"));

    let from_file = caller["from"]["file"].as_str().unwrap_or("");
    assert!(
        from_file.ends_with("/b.ts"),
        "cross-file caller should live in /b.ts, got file={from_file:?} call={caller:?}"
    );
}

/// Issue #3753 follow-up: a default-import bound to a *different* local
/// name still reaches the default export. The local-name choice is the
/// importer's affair; tsc keys the resolution off the export name.
#[test]
fn test_call_hierarchy_incoming_handles_renamed_default_import() {
    let mut server = make_server();
    server.open_files.insert(
        "/a.ts".to_string(),
        "export default function target() {}\n".to_string(),
    );
    server.open_files.insert(
        "/b.ts".to_string(),
        "import renamed from \"./a\";\nexport function caller() { renamed(); }\n".to_string(),
    );

    let req = make_request(
        "provideCallHierarchyIncomingCalls",
        serde_json::json!({
            "file": "/a.ts",
            "line": 1,
            "offset": 25,
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("incoming calls should return a body");
    let calls = body
        .as_array()
        .expect("provideCallHierarchyIncomingCalls body should be an array");

    assert!(
        calls.iter().any(|c| c["from"]["name"] == "caller"
            && c["from"]["file"]
                .as_str()
                .is_some_and(|f| f.ends_with("/b.ts"))),
        "expected renamed default-import caller in /b.ts, got: {calls:?}"
    );
}

/// Issue #3753 follow-up: `import { default as target }` is a named-import
/// shape that resolves to the default export. The named-import branch must
/// match `default` against a default-exported target alongside its
/// declared name.
#[test]
fn test_call_hierarchy_incoming_handles_named_default_alias_for_default_export() {
    let mut server = make_server();
    server.open_files.insert(
        "/a.ts".to_string(),
        "export default function target() {}\n".to_string(),
    );
    server.open_files.insert(
        "/b.ts".to_string(),
        "import { default as target } from \"./a\";\nexport function caller() { target(); }\n"
            .to_string(),
    );

    let req = make_request(
        "provideCallHierarchyIncomingCalls",
        serde_json::json!({
            "file": "/a.ts",
            "line": 1,
            "offset": 25,
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("incoming calls should return a body");
    let calls = body
        .as_array()
        .expect("provideCallHierarchyIncomingCalls body should be an array");

    assert!(
        calls.iter().any(|c| c["from"]["name"] == "caller"
            && c["from"]["file"]
                .as_str()
                .is_some_and(|f| f.ends_with("/b.ts"))),
        "expected `import {{ default as target }}` caller in /b.ts, got: {calls:?}"
    );
}

/// Default-export detection must stay scoped to default-exported symbols.
/// A regular `export function NAME` must not start matching default-import
/// bindings — those bind to `default`, not `NAME`. Without this guard, any
/// `import x from "./a"` in any other file would falsely register against
/// every named export of /a.ts.
#[test]
fn test_call_hierarchy_incoming_named_export_does_not_capture_unrelated_default_import() {
    let mut server = make_server();
    server.open_files.insert(
        "/a.ts".to_string(),
        "export function target() {}\nexport default function other() {}\n".to_string(),
    );
    server.open_files.insert(
        "/b.ts".to_string(),
        "import other from \"./a\";\nexport function caller() { other(); }\n".to_string(),
    );

    let req = make_request(
        "provideCallHierarchyIncomingCalls",
        serde_json::json!({
            "file": "/a.ts",
            "line": 1,
            "offset": 17,
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("incoming calls should return a body");
    let calls = body
        .as_array()
        .expect("provideCallHierarchyIncomingCalls body should be an array");

    assert!(
        !calls.iter().any(|c| c["from"]["name"] == "caller"),
        "named export `target` must not capture the default-import of `other`, got: {calls:?}"
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
fn test_quickinfo_on_nonexistent_file_has_no_body() {
    let mut server = make_server();
    let req = make_request(
        "quickinfo",
        serde_json::json!({"file": "/nonexistent.ts", "line": 1, "offset": 1}),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    assert!(
        resp.body.is_none(),
        "quickinfo should omit body when the file cannot be resolved"
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
fn test_definition_and_bound_span_has_no_body_without_definition() {
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
    assert!(
        resp.body.is_none(),
        "definitionAndBoundSpan should omit body when no definition exists"
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
    let probe_kind = arena.kind_at(probe_node).unwrap_or_default();
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

// TODO: LSP needs support for quoted export alias definition spans.
// introduced a dedicated EXPORT_VALUE symbol for quoted alias specifiers. That removes the
// fallback path this test was anchoring on, so the `"<other>"` export-alias-side token no
// longer shows up as its own definition span in references-full. Keeping the test as
// #[ignore] until the LSP resolver is updated to follow EXPORT_VALUE alias symbols through
// `node_symbols` and re-emit per-specifier definition spans for quoted re-exports.
#[ignore = "pending: needs LSP follow-through"]
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

    // Symbol metadata (`name`) lives on the `-full` shape, not on plain
    // `definition` (see #4002). Use `definition-full` to inspect it.
    let req = make_request(
        "definition-full",
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
        .expect("definition-full should return body")
        .as_array()
        .cloned()
        .expect("definition-full response should be an array");
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

    // `isAmbient` / `isLocal` live on the `-full` shape, not on plain
    // `definition` (see #4002). Use `definition-full` to inspect them.
    let req = make_request(
        "definition-full",
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
        .expect("definition-full should return body")
        .as_array()
        .cloned()
        .expect("definition-full response should be an array");
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

#[test]
fn test_check_options_legacy_options_round_trip_to_checker_options() {
    // Issue #3579: the legacy `check` request previously hardcoded several
    // checker options to `false` even when the client supplied them. Now
    // they're plumbed through. Locks the deserialize+plumbing for the six
    // options the issue called out.
    let json = serde_json::json!({
        "verbatimModuleSyntax": true,
        "erasableSyntaxOnly": true,
        "allowImportingTsExtensions": true,
        "rewriteRelativeImportExtensions": true,
        "allowUmdGlobalAccess": true,
        "preserveConstEnums": true,
    });
    let options: CheckOptions = serde_json::from_value(json).unwrap();
    assert!(options.verbatim_module_syntax);
    assert!(options.erasable_syntax_only);
    assert!(options.allow_importing_ts_extensions);
    assert!(options.rewrite_relative_import_extensions);
    assert!(options.allow_umd_global_access);
    assert!(options.preserve_const_enums);

    // Shape-check the conversion to `CheckerOptions` — we don't run a full
    // check here (libs aren't loaded in the unit test), but the field
    // round-trip is the meaningful behavior change.
    let server = make_server();
    let checker = server.build_checker_options(&options);
    assert!(checker.verbatim_module_syntax);
    assert!(checker.erasable_syntax_only);
    assert!(checker.allow_importing_ts_extensions);
    assert!(checker.rewrite_relative_import_extensions);
    assert!(checker.allow_umd_global_access);
    assert!(checker.preserve_const_enums);
}

#[test]
fn test_check_options_legacy_options_default_false() {
    let options: CheckOptions = serde_json::from_str(r#"{}"#).unwrap();
    assert!(!options.verbatim_module_syntax);
    assert!(!options.erasable_syntax_only);
    assert!(!options.allow_importing_ts_extensions);
    assert!(!options.rewrite_relative_import_extensions);
    assert!(!options.allow_umd_global_access);
    assert!(!options.preserve_const_enums);
}

#[test]
fn test_check_options_deserializes_numeric_tsserver_enums() {
    let options: CheckOptions = serde_json::from_value(serde_json::json!({
        "noImplicitAny": true,
        "target": 2,
        "module": 1
    }))
    .unwrap();

    assert_eq!(options.target.as_deref(), Some("es2015"));
    assert_eq!(options.module.as_deref(), Some("commonjs"));
    assert_eq!(options.no_implicit_any, Some(true));
    assert_eq!(
        Server::parse_target(&Some("2".to_string())),
        tsz::emitter::ScriptTarget::ES2015
    );
    assert_eq!(
        Server::parse_module(&Some("1".to_string())),
        tsz::ModuleKind::CommonJS
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
fn test_project_info_inferred_project_uses_numeric_target_for_libs() {
    let mut server = make_server_with_real_libs();
    server
        .open_files
        .insert("/main.ts".to_string(), "const value = 1;\n".to_string());

    let options_resp = server.handle_tsserver_request(make_request(
        "compilerOptionsForInferredProjects",
        serde_json::json!({
            "options": {
                "target": 99
            }
        }),
    ));
    assert!(options_resp.success);

    let project_info_resp = server.handle_tsserver_request(make_request(
        "projectInfo",
        serde_json::json!({
            "file": "/main.ts",
            "needFileNameList": true
        }),
    ));

    assert!(project_info_resp.success);
    let file_names = project_info_resp
        .body
        .and_then(|body| body.get("fileNames").cloned())
        .and_then(|file_names| file_names.as_array().cloned())
        .expect("projectInfo should include fileNames");
    assert!(
        file_names.iter().any(|file| file
            .as_str()
            .is_some_and(|path| path.ends_with("lib.esnext.full.d.ts"))),
        "numeric target 99 should select ESNext libs, got {file_names:?}"
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
fn test_project_info_configured_project_expands_include_files() {
    let mut server = make_server_with_real_libs();
    let tsconfig_path = "/tests/cases/fourslash/server/tsconfig.json".to_string();
    server.open_files.insert(
        tsconfig_path.clone(),
        r#"{ "include": ["src/**/*.ts"], "compilerOptions": { "lib": ["es5"] } }"#.to_string(),
    );
    server.open_files.insert(
        "/tests/cases/fourslash/server/src/a.ts".to_string(),
        "export const a = 1;\n".to_string(),
    );
    server.open_files.insert(
        "/tests/cases/fourslash/server/src/b.ts".to_string(),
        "export const b = 2;\n".to_string(),
    );
    server.open_files.insert(
        "/tests/cases/fourslash/server/other.ts".to_string(),
        "export const other = 3;\n".to_string(),
    );

    let (config, files) = server.compute_project_info("/tests/cases/fourslash/server/src/a.ts");
    assert_eq!(config, tsconfig_path);
    let non_lib: Vec<&str> = files
        .iter()
        .filter(|p| !p.starts_with("/home/src/tslibs/TS/Lib/"))
        .map(String::as_str)
        .collect();
    assert_eq!(
        non_lib,
        vec![
            "/tests/cases/fourslash/server/src/a.ts",
            "/tests/cases/fourslash/server/src/b.ts",
            "/tests/cases/fourslash/server/tsconfig.json",
        ],
        "include glob should add matching project files before the config"
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
fn test_project_info_lib_files_use_real_paths_outside_fourslash() {
    // Production tsserver clients send real on-disk paths, not the fourslash
    // VFS mount. projectInfo must return the actual lib paths the server is
    // using, not the harness's `/home/src/tslibs/TS/Lib/...` rewrites.
    // Regression for https://github.com/mohsen1/tsz/issues/3779.
    let mut server = make_server_with_real_libs();
    server.open_files.insert(
        "/private/tmp/tsz-projectinfo-repro/a.ts".to_string(),
        "const value = 1;\n".to_string(),
    );
    server.inferred_check_options.lib = Some(vec!["es5".to_string()]);

    let (_, files) = server.compute_project_info("/private/tmp/tsz-projectinfo-repro/a.ts");

    let leaked: Vec<&str> = files
        .iter()
        .filter(|p| p.starts_with("/home/src/tslibs/TS/Lib/"))
        .map(String::as_str)
        .collect();
    assert!(
        leaked.is_empty(),
        "production paths must not contain fourslash VFS lib paths, got {leaked:?}"
    );

    let lib_files: Vec<&str> = files
        .iter()
        .filter(|p| p.contains("lib.") && p.ends_with(".d.ts"))
        .map(String::as_str)
        .collect();
    assert!(
        lib_files.iter().any(|p| p.ends_with("/lib.es5.d.ts")),
        "expected real lib.es5.d.ts path among lib files, got {lib_files:?}"
    );
    for path in &lib_files {
        assert!(
            std::path::Path::new(path).is_absolute(),
            "lib path must be absolute (real on-disk path), got {path:?}"
        );
        assert!(
            !path.starts_with("/home/src/tslibs/"),
            "lib path must not be a fourslash VFS path, got {path:?}"
        );
    }
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

mod brace_code_map {
    use super::super::{build_code_map, scan_forward};

    fn match_brace(src: &str, open_pos: usize) -> Option<usize> {
        let bytes = src.as_bytes();
        let map = build_code_map(bytes);
        assert!(
            map[open_pos],
            "test setup error: open position {open_pos} is not in code"
        );
        scan_forward(bytes, &map, open_pos, b'{', b'}')
    }

    #[test]
    fn brace_match_skips_close_brace_inside_regex_literal() {
        // Repro from issue #4013: the `}` inside `/}/` must be ignored when
        // matching the outer block braces.
        let src = "if (true) { const r = /}/; }";
        let open = src.find('{').unwrap();
        let outer_close = src.rfind('}').unwrap();
        assert_eq!(match_brace(src, open), Some(outer_close));
    }

    #[test]
    fn brace_match_skips_close_brace_inside_regex_character_class() {
        // `[}]` is a character class containing only `}`; outer braces still
        // pair with the trailing `}`.
        let src = "{ const r = /[}]/; }";
        let open = src.find('{').unwrap();
        let outer_close = src.rfind('}').unwrap();
        assert_eq!(match_brace(src, open), Some(outer_close));
    }

    #[test]
    fn brace_match_handles_regex_with_flags() {
        let src = "{ const r = /}/gi; }";
        let open = src.find('{').unwrap();
        let outer_close = src.rfind('}').unwrap();
        assert_eq!(match_brace(src, open), Some(outer_close));
    }

    #[test]
    fn brace_match_treats_division_as_code_not_regex() {
        // After a value-producing token (`)`), `/` is division, not a regex.
        // The `}` inside the comment is non-code; `b` and `c` are identifiers.
        // This guards against the regex heuristic mis-firing on division.
        let src = "{ let x = (a) / b / c; }";
        let open = src.find('{').unwrap();
        let outer_close = src.rfind('}').unwrap();
        assert_eq!(match_brace(src, open), Some(outer_close));
    }

    #[test]
    fn brace_match_handles_regex_after_return_keyword() {
        let src = "function f() { return /}/.test(\"\"); }";
        let open = src.find('{').unwrap();
        let outer_close = src.rfind('}').unwrap();
        assert_eq!(match_brace(src, open), Some(outer_close));
    }

    #[test]
    fn brace_match_handles_regex_with_escaped_slash() {
        let src = "{ const r = /\\/}/; }";
        let open = src.find('{').unwrap();
        let outer_close = src.rfind('}').unwrap();
        assert_eq!(match_brace(src, open), Some(outer_close));
    }
}

// Issue #3718: refactor handlers must accept position-only requests
// (`line`/`offset`) in addition to range requests
// (`startLine`/`startOffset`/`endLine`/`endOffset`). tsserver dispatches
// `FileLocationOrRangeRequestArgs` to the language service in either form.
#[test]
fn parse_refactor_request_range_accepts_range_form() {
    let request = make_request(
        "getApplicableRefactors",
        serde_json::json!({
            "file": "/a.ts",
            "startLine": 2,
            "startOffset": 13,
            "endLine": 2,
            "endOffset": 18,
        }),
    );
    let span = Server::parse_refactor_request_range(&request).expect("range form must parse");
    assert_eq!(span, (2, 13, 2, 18));
}

#[test]
fn parse_refactor_request_range_accepts_position_only_form_as_zero_width() {
    // Issue #3718: when only `line`/`offset` are sent, treat the request as a
    // zero-width range at that position so the handler proceeds instead of
    // bailing through the optional-chained `?` on the missing range fields.
    let request = make_request(
        "getApplicableRefactors",
        serde_json::json!({
            "file": "/a.ts",
            "line": 2,
            "offset": 9,
        }),
    );
    let span = Server::parse_refactor_request_range(&request).expect("position-only must parse");
    assert_eq!(span, (2, 9, 2, 9));
}

#[test]
fn parse_refactor_request_range_prefers_explicit_range_over_position() {
    // If both forms are present, the explicit range wins (matches tsserver).
    let request = make_request(
        "getApplicableRefactors",
        serde_json::json!({
            "file": "/a.ts",
            "startLine": 1, "startOffset": 1, "endLine": 1, "endOffset": 5,
            "line": 9, "offset": 9,
        }),
    );
    let span = Server::parse_refactor_request_range(&request).expect("range form must win");
    assert_eq!(span, (1, 1, 1, 5));
}

#[test]
fn parse_refactor_request_range_returns_none_without_position_or_range() {
    let request = make_request(
        "getApplicableRefactors",
        serde_json::json!({"file": "/a.ts"}),
    );
    assert!(Server::parse_refactor_request_range(&request).is_none());
}

#[test]
fn applicable_refactors_position_only_request_returns_success_not_error() {
    // End-to-end smoke check that position-only requests no longer fail the
    // request shape. The body may be empty depending on whether an
    // extractable expression sits at the cursor; the contract is that the
    // request completes successfully (it used to return `None` from the
    // closure when `startLine` was missing).
    let mut server = make_server();
    assert!(
        server
            .handle_tsserver_request(make_request(
                "open",
                serde_json::json!({
                    "file": "/src/pos.ts",
                    "fileContent": "function f() {\n  const value = 1 + 2;\n  return value;\n}\n",
                }),
            ))
            .success
    );
    let response = server.handle_tsserver_request(make_request(
        "getApplicableRefactors",
        serde_json::json!({"file": "/src/pos.ts", "line": 2, "offset": 9}),
    ));
    assert!(response.success);
    assert!(response.body.is_some_and(|b| b.is_array()));
}

// Issue #3803: getApplicableRefactors must NOT emit the `_scope_1` actions
// when the request range has no enclosing function — only the global scope
// is meaningful — and every action must carry a `range`.
#[test]
fn applicable_refactors_top_level_expression_emits_one_scope_with_range() {
    let mut server = make_server();
    assert!(
        server
            .handle_tsserver_request(make_request(
                "open",
                serde_json::json!({
                    "file": "/src/top.ts",
                    "fileContent": "const y = 1 + 2;\n",
                }),
            ))
            .success
    );

    let response = server.handle_tsserver_request(make_request(
        "getApplicableRefactors",
        serde_json::json!({
            "file": "/src/top.ts",
            "startLine": 1,
            "startOffset": 11,
            "endLine": 1,
            "endOffset": 16,
        }),
    ));

    assert!(response.success);
    let body = response.body.expect("refactors should return a body");
    let refactors = body.as_array().expect("refactors must be an array");
    let actions: Vec<_> = refactors
        .iter()
        .flat_map(|r| {
            r.get("actions")
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
        })
        .collect();

    let names: Vec<&str> = actions
        .iter()
        .filter_map(|a| a.get("name").and_then(serde_json::Value::as_str))
        .collect();
    assert!(
        names.contains(&"function_scope_0"),
        "expected function_scope_0, got {names:?}"
    );
    assert!(
        names.contains(&"constant_scope_0"),
        "expected constant_scope_0, got {names:?}"
    );
    assert!(
        !names.contains(&"function_scope_1"),
        "must not emit function_scope_1 at module level, got {names:?}"
    );
    assert!(
        !names.contains(&"constant_scope_1"),
        "must not emit constant_scope_1 at module level, got {names:?}"
    );

    for action in &actions {
        assert!(
            action.get("range").is_some(),
            "action missing range: {action:#}"
        );
    }

    let fn_action = actions
        .iter()
        .find(|a| a.get("name").and_then(serde_json::Value::as_str) == Some("function_scope_0"))
        .expect("function_scope_0 must exist");
    let desc = fn_action
        .get("description")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    assert!(
        desc.contains("global scope"),
        "function_scope_0 should target global scope at module level, got: {desc}"
    );
}

// Issue #3784: emit-output should honor the owning project's
// `compilerOptions.module` and `compilerOptions.outDir` so the result
// matches what tsc reports for the same file.
#[test]
fn emit_output_honors_tsconfig_module_and_out_dir() {
    let temp = tempfile::tempdir().expect("temp dir");
    let root = temp.path();
    let config = root.join("tsconfig.json");
    let a = root.join("a.ts");
    std::fs::write(
        &config,
        r#"{"compilerOptions":{"target":"es2015","module":"commonjs","outDir":"dist"},"files":["a.ts"]}"#,
    )
    .expect("write config");
    std::fs::write(&a, "export const value = 1;\n").expect("write a");

    let mut server = make_server();
    let a_str = a.to_string_lossy().to_string();

    let open =
        server.handle_tsserver_request(make_request("open", serde_json::json!({ "file": &a_str })));
    assert!(open.success);

    let response = server.handle_tsserver_request(make_request(
        "emit-output",
        serde_json::json!({ "file": &a_str }),
    ));
    assert!(response.success);
    let body = response.body.expect("emit-output body");

    // Output path should land under outDir/dist/a.js, not next to the source.
    let expected_out = root.join("dist").join("a.js");
    let expected_out_str = expected_out.to_string_lossy().to_string();
    assert_eq!(
        body["outputFiles"][0]["name"], expected_out_str,
        "expected outDir-relative path, got: {body:#}"
    );

    // CommonJS module output should have an exports.value assignment.
    let text = body["outputFiles"][0]["text"]
        .as_str()
        .expect("output text");
    assert!(
        text.contains("exports.value"),
        "expected CommonJS exports lowering, got: {text}"
    );
    assert!(
        !text.contains("export const"),
        "must not preserve ES module syntax when module=commonjs, got: {text}"
    );
}

// Issue #3731: tsserver's jsxClosingTag returns a closing tag for ALL JSX
// elements including intrinsic HTML void elements like <input>; tsz used
// to suppress them.
#[test]
fn jsx_closing_tag_returns_close_for_intrinsic_void_elements() {
    let void_cases = [
        ("/input.tsx", "<input>", 8, "</input>"),
        ("/img.tsx", "<img>", 6, "</img>"),
        ("/br.tsx", "<br>", 5, "</br>"),
        ("/hr.tsx", "<hr>", 5, "</hr>"),
    ];

    for (file, content, offset, expected_close) in void_cases {
        let mut server = make_server();
        server
            .open_files
            .insert(file.to_string(), content.to_string());

        let response = server.handle_tsserver_request(make_request(
            "jsxClosingTag",
            serde_json::json!({"file": file, "line": 1, "offset": offset}),
        ));
        assert!(response.success, "{file}: jsxClosingTag should succeed");
        let body = response
            .body
            .unwrap_or_else(|| panic!("{file}: expected a body for {content:?}"));
        assert_eq!(
            body.get("newText").and_then(serde_json::Value::as_str),
            Some(expected_close),
            "{file}: expected {expected_close}, got body={body:?}"
        );
        assert_eq!(
            body.get("caretOffset").and_then(serde_json::Value::as_u64),
            Some(0),
            "{file}: must include caretOffset: 0, got body={body:?}"
        );
    }
}

// Issue #3718: getApplicableRefactors and getEditsForRefactor accept
// FileLocationOrRangeRequestArgs — a client may send `{ line, offset }`
// instead of `{ startLine, startOffset, endLine, endOffset }`. Pre-fix,
// the handlers bailed via `?` because the range fields were absent.
#[test]
fn applicable_refactors_accepts_position_only_request() {
    let mut server = make_server();
    server.open_files.insert(
        "/src/pos.ts".to_string(),
        "function f() {\n  const value = 1 + 2;\n  return value;\n}\n".to_string(),
    );

    let response = server.handle_tsserver_request(make_request(
        "getApplicableRefactors",
        serde_json::json!({
            "file": "/src/pos.ts",
            "line": 2,
            "offset": 17,
        }),
    ));

    assert!(response.success);
    let body = response
        .body
        .expect("position-only refactor request must return a body");
    assert!(
        body.is_array(),
        "position-only refactor body must be an array, got: {body:?}"
    );
}

#[test]
fn edits_for_refactor_accepts_position_only_request() {
    let mut server = make_server();
    server.open_files.insert(
        "/src/pos2.ts".to_string(),
        "function f() {\n  const value = 1 + 2;\n  return value;\n}\n".to_string(),
    );

    let response = server.handle_tsserver_request(make_request(
        "getEditsForRefactor",
        serde_json::json!({
            "file": "/src/pos2.ts",
            "line": 2,
            "offset": 17,
            "refactor": "Extract Symbol",
            "action": "constant_scope_0",
        }),
    ));

    assert!(response.success);
    let body = response
        .body
        .expect("position-only edits request must return a body");
    assert!(
        body.get("edits").is_some(),
        "edits-for-refactor body must contain `edits`: {body:?}"
    );
}

// Issue #3752: docCommentTemplate must NOT scan into a nested same-line
// function-like signature when the documented declaration is a non-callable
// kind (type alias, interface, class, …). tsc returns the unchanged
// one-line `/** */` template in those cases.
#[test]
fn doc_comment_template_non_callable_type_alias_returns_one_line() {
    let mut server = make_server();
    server.open_files.insert(
        "/src/type_alias.ts".to_string(),
        "/** */\ntype F = (x: string) => number;\n".to_string(),
    );

    let response = server.handle_tsserver_request(make_request(
        "docCommentTemplate",
        serde_json::json!({
            "file": "/src/type_alias.ts",
            "line": 1,
            "offset": 4,
            "generateReturnInDocTemplate": true,
        }),
    ));

    assert!(response.success);
    let body = response.body.expect("docCommentTemplate body");
    assert_eq!(
        body.get("newText").and_then(serde_json::Value::as_str),
        Some("/** */"),
        "type alias must not extract @param from nested signature, got: {body:?}"
    );
    assert_eq!(
        body.get("caretOffset").and_then(serde_json::Value::as_u64),
        Some(3)
    );
}

#[test]
fn doc_comment_template_non_callable_interface_returns_one_line() {
    let mut server = make_server();
    server.open_files.insert(
        "/src/iface.ts".to_string(),
        "/** */\ninterface I { m(x: string): void; }\n".to_string(),
    );

    let response = server.handle_tsserver_request(make_request(
        "docCommentTemplate",
        serde_json::json!({
            "file": "/src/iface.ts",
            "line": 1,
            "offset": 4,
            "generateReturnInDocTemplate": true,
        }),
    ));

    assert!(response.success);
    let body = response.body.expect("docCommentTemplate body");
    assert_eq!(
        body.get("newText").and_then(serde_json::Value::as_str),
        Some("/** */"),
        "interface must not extract @param from nested method signature, got: {body:?}"
    );
}

#[test]
fn doc_comment_template_non_callable_class_returns_one_line() {
    let mut server = make_server();
    server.open_files.insert(
        "/src/class.ts".to_string(),
        "/** */\nclass C { constructor(public x: number) {} }\n".to_string(),
    );

    let response = server.handle_tsserver_request(make_request(
        "docCommentTemplate",
        serde_json::json!({
            "file": "/src/class.ts",
            "line": 1,
            "offset": 4,
            "generateReturnInDocTemplate": true,
        }),
    ));

    assert!(response.success);
    let body = response.body.expect("docCommentTemplate body");
    assert_eq!(
        body.get("newText").and_then(serde_json::Value::as_str),
        Some("/** */"),
        "class must not extract @param from nested constructor, got: {body:?}"
    );
}

// Sanity: a regular function declaration must STILL extract @param tags.
#[test]
fn doc_comment_template_function_decl_still_extracts_params() {
    let mut server = make_server();
    server.open_files.insert(
        "/src/fn.ts".to_string(),
        "/** */\nfunction f(x: string, y: number) {}\n".to_string(),
    );

    let response = server.handle_tsserver_request(make_request(
        "docCommentTemplate",
        serde_json::json!({
            "file": "/src/fn.ts",
            "line": 1,
            "offset": 4,
            "generateReturnInDocTemplate": false,
        }),
    ));

    assert!(response.success);
    let body = response.body.expect("docCommentTemplate body");
    let new_text = body
        .get("newText")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    assert!(
        new_text.contains("@param x"),
        "function decl should extract @param x, got: {new_text}"
    );
    assert!(
        new_text.contains("@param y"),
        "function decl should extract @param y, got: {new_text}"
    );
}

// Issue #3798: getMoveToRefactoringFileSuggestions should derive newFileName
// from the selected declaration and include configured project files in the
// candidate list, not just open files.
#[test]
fn move_to_file_suggestions_derive_new_file_name_from_declaration() {
    let temp = tempfile::tempdir().expect("temp dir");
    let root = temp.path();
    let config = root.join("tsconfig.json");
    let a = root.join("a.ts");
    let b = root.join("b.ts");
    std::fs::write(
        &config,
        r#"{"compilerOptions":{"target":"es2020","module":"esnext"},"files":["a.ts","b.ts"]}"#,
    )
    .expect("config");
    std::fs::write(&a, "export function moveMe() {}\n").expect("a");
    std::fs::write(&b, "export const other = 1;\n").expect("b");

    let mut server = make_server();
    let a_str = a.to_string_lossy().to_string();
    let b_str = b.to_string_lossy().to_string();

    // Open ONLY a.ts. tsserver should still surface b.ts via the project's file list.
    let open =
        server.handle_tsserver_request(make_request("open", serde_json::json!({ "file": &a_str })));
    assert!(open.success);

    let response = server.handle_tsserver_request(make_request(
        "getMoveToRefactoringFileSuggestions",
        serde_json::json!({
            "file": &a_str,
            "startLine": 1,
            "startOffset": 1,
            "endLine": 1,
            "endOffset": 28,
        }),
    ));
    assert!(response.success);
    let body = response.body.expect("move-to suggestions body");

    let new_file = body["newFileName"]
        .as_str()
        .expect("newFileName must be a string");
    assert!(
        new_file.ends_with("/moveMe.ts") || new_file.ends_with("\\moveMe.ts"),
        "newFileName should be derived from the selected declaration `moveMe`, got: {new_file}"
    );

    let files = body["files"].as_array().expect("files must be an array");
    let file_set: std::collections::HashSet<&str> =
        files.iter().filter_map(serde_json::Value::as_str).collect();
    assert!(
        file_set.contains(b_str.as_str()),
        "files must include the configured project's b.ts even though it's not open, got: {files:?}"
    );
    // Don't include the source file itself
    assert!(
        !file_set.contains(a_str.as_str()),
        "files must not include the source file itself, got: {files:?}"
    );
}
