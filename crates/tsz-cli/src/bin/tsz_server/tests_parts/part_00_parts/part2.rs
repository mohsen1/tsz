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
