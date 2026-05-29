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
            "file": "/project/foo.ts",
            "fileContent": "export function bar() { return 1; }\n"
        }),
    ));
    assert!(resp.success);
    let resp = server.handle_tsserver_request(make_request(
        "open",
        serde_json::json!({
            "file": "/project/index.ts",
            "fileContent": "var x1 = import(\"./foo\");\nx1.then(foo => {\n   var s: string = foo.bar();\n})\n"
        }),
    ));
    assert!(resp.success);

    let req = make_request(
        "semanticDiagnosticsSync",
        serde_json::json!({ "file": "/project/index.ts" }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let diagnostics = resp
        .body
        .expect("semanticDiagnosticsSync should return a body")
        .as_array()
        .expect("semanticDiagnosticsSync body should be an array")
        .clone();
    assert_eq!(diagnostics.len(), 1, "expected one diagnostic before edit, got: {diagnostics:?}");

    let resp = server.handle_tsserver_request(make_request(
        "change",
        serde_json::json!({
            "file": "/project/index.ts",
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
        serde_json::json!({ "file": "/project/index.ts" }),
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
fn test_apply_code_action_command_returns_unsupported_for_single_command() {
    let mut server = make_server();
    let req = make_request(
        "applyCodeActionCommand",
        serde_json::json!({"command": {"type": "noop"}}),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(!resp.success);
    assert!(resp.body.is_none());
    let message = resp.message.expect("response must carry a reason");
    assert!(message.contains("not supported"), "got {message:?}");
    assert!(
        message.contains("applyCodeActionCommand"),
        "message must name the command, got {message:?}"
    );
}

#[test]
fn test_apply_code_action_command_returns_unsupported_for_array_command() {
    let mut server = make_server();
    let req = make_request(
        "applyCodeActionCommand",
        serde_json::json!({"command": [{"type": "noop"}]}),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(!resp.success);
    assert!(resp.body.is_none());
    let message = resp.message.expect("response must carry a reason");
    assert!(message.contains("not supported"), "got {message:?}");
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

