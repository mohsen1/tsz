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

