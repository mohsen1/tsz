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
fn test_completion_info_auto_import_reads_tracked_external_project_files() {
    let mut server = make_server();

    let unique = format!(
        "tsz_extproj_completion_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after UNIX_EPOCH")
            .as_nanos()
    );
    let root = std::env::temp_dir().join(unique);
    let project_dir = root.join("project");
    let src_dir = project_dir.join("src");
    let dep_dir = project_dir.join("node_modules").join("dep");
    std::fs::create_dir_all(&src_dir).expect("should create src dir");
    std::fs::create_dir_all(&dep_dir).expect("should create dep dir");

    let package_json_path = project_dir.join("package.json");
    std::fs::write(
        &package_json_path,
        r#"{
  "dependencies": {
    "dep": "*"
  }
}"#,
    )
    .expect("should write package.json");

    let dep_index_path = dep_dir.join("index.d.ts");
    std::fs::write(&dep_index_path, "export const externalSymbol: number;\n")
        .expect("should write dep index");

    let index_path = src_dir.join("index.ts");
    let index_path_str = index_path.to_string_lossy().to_string();
    let dep_index_path_str = dep_index_path.to_string_lossy().to_string();

    server
        .open_files
        .insert(index_path_str.clone(), "externalSym".to_string());
    server.external_project_files.insert(
        "/project.csproj".to_string(),
        vec![index_path_str.clone(), dep_index_path_str],
    );

    let completion_req = make_request(
        "completionInfo",
        serde_json::json!({
            "file": index_path_str,
            "line": 1,
            "offset": 12,
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
    let has_external_auto_import = entries.iter().any(|entry| {
        entry.get("name").and_then(serde_json::Value::as_str) == Some("externalSymbol")
            && entry.get("source").and_then(serde_json::Value::as_str) == Some("dep")
    });
    assert!(
        has_external_auto_import,
        "expected auto-import completion from tracked external project dependency file"
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn test_completion_info_auto_import_includes_export_map_types_entries() {
    let mut server = make_server();
    server.open_files.insert(
        "/home/src/workspaces/project/tsconfig.json".to_string(),
        r#"{
  "compilerOptions": {
    "lib": ["es5"],
    "module": "nodenext"
  }
}"#
        .to_string(),
    );
    server.open_files.insert(
        "/home/src/workspaces/project/package.json".to_string(),
        r#"{
  "type": "module",
  "dependencies": {
    "dependency": "^1.0.0"
  }
}"#
        .to_string(),
    );
    server.open_files.insert(
        "/home/src/workspaces/project/node_modules/dependency/package.json".to_string(),
        r#"{
  "type": "module",
  "name": "dependency",
  "version": "1.0.0",
  "exports": {
    ".": { "types": "./lib/index.d.ts" },
    "./lol": { "types": "./lib/lol.d.ts" }
  }
}"#
        .to_string(),
    );
    server.open_files.insert(
        "/home/src/workspaces/project/node_modules/dependency/lib/index.d.ts".to_string(),
        "export function fooFromIndex(): void;\n".to_string(),
    );
    server.open_files.insert(
        "/home/src/workspaces/project/node_modules/dependency/lib/lol.d.ts".to_string(),
        "export function fooFromLol(): void;\n".to_string(),
    );
    server.open_files.insert(
        "/home/src/workspaces/project/src/foo.ts".to_string(),
        "fooFrom".to_string(),
    );

    let completion_req = make_request(
        "completionInfo",
        serde_json::json!({
            "file": "/home/src/workspaces/project/src/foo.ts",
            "line": 1,
            "offset": 8,
            "preferences": {
                "includeCompletionsForModuleExports": true,
                "includeInsertTextCompletions": true,
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

    let has_index = entries.iter().any(|entry| {
        entry.get("name").and_then(serde_json::Value::as_str) == Some("fooFromIndex")
            && entry.get("source").and_then(serde_json::Value::as_str) == Some("dependency")
    });
    assert!(
        has_index,
        "expected auto-import completion fooFromIndex from dependency root exports entry"
    );

    let has_lol = entries.iter().any(|entry| {
        entry.get("name").and_then(serde_json::Value::as_str) == Some("fooFromLol")
            && entry.get("source").and_then(serde_json::Value::as_str) == Some("dependency/lol")
    });
    assert!(
        has_lol,
        "expected auto-import completion fooFromLol from dependency/lol exports entry"
    );
}
