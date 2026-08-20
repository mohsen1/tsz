#[test]
fn test_completion_info_auto_import_discovers_dependency_files_from_disk() {
    let mut server = make_server();

    let unique = format!(
        "tsz_completion_dep_scan_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after UNIX_EPOCH")
            .as_nanos()
    );
    let root = std::env::temp_dir().join(unique);
    let project_dir = root.join("project");
    let src_dir = project_dir.join("src");
    let dep_lib_dir = project_dir
        .join("node_modules")
        .join("dependency")
        .join("lib");
    std::fs::create_dir_all(&src_dir).expect("should create src dir");
    std::fs::create_dir_all(&dep_lib_dir).expect("should create dependency lib dir");

    let tsconfig_path = project_dir.join("tsconfig.json");
    std::fs::write(
        &tsconfig_path,
        r#"{
  "compilerOptions": {
    "lib": ["es5"],
    "module": "nodenext"
  }
}"#,
    )
    .expect("should write tsconfig");

    let package_json_path = project_dir.join("package.json");
    std::fs::write(
        &package_json_path,
        r#"{
  "dependencies": {
    "dependency": "^1.0.0"
  }
}"#,
    )
    .expect("should write package.json");

    let dep_package_json_path = project_dir
        .join("node_modules")
        .join("dependency")
        .join("package.json");
    std::fs::write(
        &dep_package_json_path,
        r#"{
  "name": "dependency",
  "version": "1.0.0",
  "exports": {
    ".": { "types": "./lib/index.d.ts" },
    "./lol": { "types": "./lib/lol.d.ts" }
  }
}"#,
    )
    .expect("should write dependency package.json");

    let dep_index_path = dep_lib_dir.join("index.d.ts");
    std::fs::write(
        &dep_index_path,
        "export declare function fooFromIndex(): void;\n",
    )
    .expect("should write dependency index declarations");

    let dep_lol_path = dep_lib_dir.join("lol.d.ts");
    std::fs::write(
        &dep_lol_path,
        "export declare function fooFromLol(): void;\n",
    )
    .expect("should write dependency lol declarations");

    let source_path = src_dir.join("foo.ts");
    let source_path_str = source_path.to_string_lossy().to_string();
    std::fs::write(&source_path, "fooFrom").expect("should write source file");

    // Intentionally keep dependency declaration files out of open_files to
    // mirror auto-import provider scenarios where package files are discovered
    // from disk.
    server
        .open_files
        .insert(source_path_str.clone(), "fooFrom".to_string());
    server.open_files.insert(
        tsconfig_path.to_string_lossy().to_string(),
        std::fs::read_to_string(&tsconfig_path).expect("should read tsconfig"),
    );
    server.open_files.insert(
        package_json_path.to_string_lossy().to_string(),
        std::fs::read_to_string(&package_json_path).expect("should read package.json"),
    );

    let completion_req = make_request(
        "completionInfo",
        serde_json::json!({
            "file": source_path_str,
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
        "expected fooFromIndex auto-import completion discovered from on-disk dependency files"
    );

    let has_lol = entries.iter().any(|entry| {
        entry.get("name").and_then(serde_json::Value::as_str) == Some("fooFromLol")
            && entry.get("source").and_then(serde_json::Value::as_str) == Some("dependency/lol")
    });
    assert!(
        has_lol,
        "expected fooFromLol auto-import completion discovered from on-disk dependency files"
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn test_completion_info_auto_import_with_fourslash_marker_position() {
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
  "dependencies": {
    "dependency": "^1.0.0"
  }
}"#
        .to_string(),
    );
    server.open_files.insert(
        "/home/src/workspaces/project/node_modules/dependency/package.json".to_string(),
        r#"{
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
        "export declare function fooFromIndex(): void;\n".to_string(),
    );
    server.open_files.insert(
        "/home/src/workspaces/project/node_modules/dependency/lib/lol.d.ts".to_string(),
        "export declare function fooFromLol(): void;\n".to_string(),
    );

    let source_text = "fooFrom/**/";
    server.open_files.insert(
        "/home/src/workspaces/project/src/foo.ts".to_string(),
        source_text.to_string(),
    );
    let marker_offset = source_text.find('/').expect("marker start exists");
    let completion_req = make_request(
        "completionInfo",
        serde_json::json!({
            "file": "/home/src/workspaces/project/src/foo.ts",
            "line": 1,
            "offset": marker_offset + 1,
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
        "expected fooFromIndex auto-import completion when cursor is at fourslash marker position"
    );
}

#[test]
fn test_completion_info_auto_import_allows_bare_package_without_requester_package_json_loaded() {
    let mut server = make_server();

    let source_path = "/virtual/workspace/project/src/index.ts";
    let dep_package_path = "/virtual/workspace/project/node_modules/dep/package.json";
    let dep_types_path = "/virtual/workspace/project/node_modules/dep/index.d.ts";

    // Keep requester package.json absent to mirror adapter snapshots where only
    // source and dependency files are tracked.
    server
        .open_files
        .insert(source_path.to_string(), "DependencySymb".to_string());
    server.open_files.insert(
        dep_package_path.to_string(),
        r#"{
  "name": "dep",
  "version": "1.0.0",
  "types": "./index.d.ts"
}"#
        .to_string(),
    );
    server.open_files.insert(
        dep_types_path.to_string(),
        "export declare class DependencySymbol {}\n".to_string(),
    );

    let completion_req = make_request(
        "completionInfo",
        serde_json::json!({
            "file": source_path,
            "line": 1,
            "offset": 14,
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

    let has_dep_auto_import = entries.iter().any(|entry| {
        entry.get("name").and_then(serde_json::Value::as_str) == Some("DependencySymbol")
            && entry.get("source").and_then(serde_json::Value::as_str) == Some("dep")
    });
    assert!(
        has_dep_auto_import,
        "expected dep auto-import completion when requester package.json is not loaded"
    );
}

#[test]
fn test_completion_info_auto_import_includes_peer_dependency_from_workspace_package_json() {
    let mut server = make_server();
    server.open_files.insert(
        "/home/src/workspaces/project/node_modules/common-dependency/package.json".to_string(),
        r#"{ "name": "common-dependency" }"#.to_string(),
    );
    server.open_files.insert(
        "/home/src/workspaces/project/node_modules/common-dependency/index.d.ts".to_string(),
        "export declare class CommonDependency {}\n".to_string(),
    );
    server.open_files.insert(
        "/home/src/workspaces/project/node_modules/package-dependency/package.json".to_string(),
        r#"{ "name": "package-dependency" }"#.to_string(),
    );
    server.open_files.insert(
        "/home/src/workspaces/project/node_modules/package-dependency/index.d.ts".to_string(),
        "export declare class PackageDependency\n".to_string(),
    );
    server.open_files.insert(
        "/home/src/workspaces/project/package.json".to_string(),
        r#"{
  "private": true,
  "dependencies": {
    "common-dependency": "*"
  }
}"#
        .to_string(),
    );
    server.open_files.insert(
        "/home/src/workspaces/project/tsconfig.json".to_string(),
        r#"{
  "compilerOptions": { "lib": ["es5"] },
  "files": [],
  "references": [{ "path": "packages/a" }]
}"#
        .to_string(),
    );
    server.open_files.insert(
        "/home/src/workspaces/project/packages/a/tsconfig.json".to_string(),
        r#"{
  "compilerOptions": { "lib": ["es5"], "target": "esnext", "composite": true }
}"#
        .to_string(),
    );
    server.open_files.insert(
        "/home/src/workspaces/project/packages/a/package.json".to_string(),
        r#"{
  "peerDependencies": {
    "package-dependency": "*"
  }
}"#
        .to_string(),
    );
    server.open_files.insert(
        "/home/src/workspaces/project/packages/a/index.ts".to_string(),
        "".to_string(),
    );

    let completion_req = make_request(
        "completionInfo",
        serde_json::json!({
            "file": "/home/src/workspaces/project/packages/a/index.ts",
            "line": 1,
            "offset": 1,
            "preferences": {
                "includeCompletionsForModuleExports": true
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

    let has_common = entries.iter().any(|entry| {
        entry.get("name").and_then(serde_json::Value::as_str) == Some("CommonDependency")
            && entry.get("source").and_then(serde_json::Value::as_str) == Some("common-dependency")
    });
    assert!(
        has_common,
        "expected CommonDependency auto-import from root dependency package"
    );

    let has_peer = entries.iter().any(|entry| {
        entry.get("name").and_then(serde_json::Value::as_str) == Some("PackageDependency")
            && entry.get("source").and_then(serde_json::Value::as_str) == Some("package-dependency")
    });
    assert!(
        has_peer,
        "expected PackageDependency auto-import from peerDependencies package"
    );
}

#[test]
fn test_completion_info_auto_import_workspace_dependency_prefers_bare_package_source() {
    let mut server = make_server();
    server.open_files.insert(
        "/home/src/workspaces/project/tsconfig.json".to_string(),
        r#"{ "compilerOptions": { "lib": ["es5"], "module": "commonjs" } }"#.to_string(),
    );
    server.open_files.insert(
        "/home/src/workspaces/project/package.json".to_string(),
        r#"{ "dependencies": { "mylib": "file:packages/mylib" } }"#.to_string(),
    );
    server.open_files.insert(
        "/home/src/workspaces/project/packages/mylib/package.json".to_string(),
        r#"{ "name": "mylib", "version": "1.0.0", "main": "index.js", "types": "index" }"#
            .to_string(),
    );
    server.open_files.insert(
        "/home/src/workspaces/project/packages/mylib/index.ts".to_string(),
        r#"export * from "./mySubDir";"#.to_string(),
    );
    server.open_files.insert(
        "/home/src/workspaces/project/packages/mylib/mySubDir/index.ts".to_string(),
        r#"export * from "./myClass";
export * from "./myClass2";"#
            .to_string(),
    );
    server.open_files.insert(
        "/home/src/workspaces/project/packages/mylib/mySubDir/myClass.ts".to_string(),
        "export class MyClass {}".to_string(),
    );
    server.open_files.insert(
        "/home/src/workspaces/project/packages/mylib/mySubDir/myClass2.ts".to_string(),
        "export class MyClass2 {}".to_string(),
    );
    server.open_files.insert(
        "/home/src/workspaces/project/src/index.ts".to_string(),
        "MyClass".to_string(),
    );

    let completion_req = make_request(
        "completionInfo",
        serde_json::json!({
            "file": "/home/src/workspaces/project/src/index.ts",
            "line": 1,
            "offset": 7,
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

    let my_class_entries: Vec<&serde_json::Value> = entries
        .iter()
        .filter(|entry| entry.get("name").and_then(serde_json::Value::as_str) == Some("MyClass"))
        .collect();
    assert!(
        !my_class_entries.is_empty(),
        "expected at least one MyClass auto-import entry"
    );
    let sources: Vec<&str> = my_class_entries
        .iter()
        .filter_map(|entry| entry.get("source").and_then(serde_json::Value::as_str))
        .collect();
    assert!(
        sources.contains(&"mylib"),
        "expected MyClass auto-import candidates to include bare package source, got {sources:?}"
    );
    assert_eq!(
        sources.first().copied(),
        Some("mylib"),
        "expected bare package source to be ranked first for MyClass, got {sources:?}"
    );
}

#[test]
fn test_completion_info_auto_import_includes_wildcard_exports_subpath_entries() {
    let mut server = make_server();
    server.open_files.insert(
        "/home/src/workspaces/project/node_modules/pkg/package.json".to_string(),
        r#"{
  "name": "pkg",
  "version": "1.0.0",
  "exports": {
    "./*": "./a/*.js",
    "./b/*.js": "./b/*.js",
    "./c/*": "./c/*",
    "./d/*": { "import": "./d/*.mjs" }
  }
}"#
        .to_string(),
    );
    server.open_files.insert(
        "/home/src/workspaces/project/node_modules/pkg/a/a1.d.ts".to_string(),
        "export const a1: number;".to_string(),
    );
    server.open_files.insert(
        "/home/src/workspaces/project/node_modules/pkg/b/b1.d.ts".to_string(),
        "export const b1: number;".to_string(),
    );
    server.open_files.insert(
        "/home/src/workspaces/project/node_modules/pkg/c/c1.d.ts".to_string(),
        "export const c1: number;".to_string(),
    );
    server.open_files.insert(
        "/home/src/workspaces/project/node_modules/pkg/c/subfolder/c2.d.mts".to_string(),
        "export const c2: number;".to_string(),
    );
    server.open_files.insert(
        "/home/src/workspaces/project/node_modules/pkg/d/d1.d.mts".to_string(),
        "export const d1: number;".to_string(),
    );
    server.open_files.insert(
        "/home/src/workspaces/project/package.json".to_string(),
        r#"{
  "type": "module",
  "dependencies": {
    "pkg": "1.0.0"
  }
}"#
        .to_string(),
    );
    server.open_files.insert(
        "/home/src/workspaces/project/tsconfig.json".to_string(),
        r#"{
  "compilerOptions": {
    "module": "nodenext",
    "lib": ["es5"]
  }
}"#
        .to_string(),
    );
    server.open_files.insert(
        "/home/src/workspaces/project/main.ts".to_string(),
        "a".to_string(),
    );

    let completion_req = make_request(
        "completionInfo",
        serde_json::json!({
            "file": "/home/src/workspaces/project/main.ts",
            "line": 1,
            "offset": 2,
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

    let has_a1 = entries.iter().any(|entry| {
        entry.get("name").and_then(serde_json::Value::as_str) == Some("a1")
            && entry.get("source").and_then(serde_json::Value::as_str) == Some("pkg/a1")
    });
    assert!(
        has_a1,
        "expected wildcard exports auto-import entry a1 from pkg/a1"
    );
}

#[test]
fn test_completion_info_auto_import_includes_string_exports_entrypoint() {
    let mut server = make_server();
    server.open_files.insert(
        "/home/src/workspaces/project/tsconfig.json".to_string(),
        r#"{
  "compilerOptions": {
    "module": "nodenext",
    "lib": ["es5"]
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
  "name": "dependency",
  "version": "1.0.0",
  "main": "./lib/index.js",
  "exports": "./lib/lol.d.ts"
}"#
        .to_string(),
    );
    server.open_files.insert(
        "/home/src/workspaces/project/node_modules/dependency/lib/index.d.ts".to_string(),
        "export function fooFromIndex(): void;".to_string(),
    );
    server.open_files.insert(
        "/home/src/workspaces/project/node_modules/dependency/lib/lol.d.ts".to_string(),
        "export function fooFromLol(): void;".to_string(),
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

    let has_lol = entries.iter().any(|entry| {
        entry.get("name").and_then(serde_json::Value::as_str) == Some("fooFromLol")
            && entry.get("source").and_then(serde_json::Value::as_str) == Some("dependency")
    });
    assert!(
        has_lol,
        "expected fooFromLol auto-import completion from string exports package entrypoint"
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
    // String literal completions include surrounding quotes in the name
    // (matching tsc tsserver behavior).
    assert!(
        names.contains(&"click") || names.contains(&"\"click\""),
        "expected 'click' completion, got {names:?}"
    );
    assert!(
        names.contains(&"drag") || names.contains(&"\"drag\""),
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
    let completion_entries = completions_body
        .as_array()
        .expect("legacy completions should return a top-level entries array");
    let completion_names: Vec<&str> = completion_entries
        .iter()
        .filter_map(|entry| entry.get("name").and_then(serde_json::Value::as_str))
        .collect();
    assert!(
        completion_names.contains(&"click") || completion_names.contains(&"\"click\""),
        "expected 'click' in completions, got {completion_names:?}"
    );
    assert!(
        completion_names.contains(&"drag") || completion_names.contains(&"\"drag\""),
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
fn test_completion_info_member_probe_handles_marker_comment_after_dot() {
    let mut server = make_server();
    server.open_files.insert(
        "/a.ts".to_string(),
        "class C<T> {\n    static foo(x: number) { }\n    x: T;\n}\n\nnamespace C {\n    export function f(x: typeof C) {\n        x./*1*/\n    }\n}\n"
            .to_string(),
    );

    let completion_req = make_request(
        "completionInfo",
        serde_json::json!({
            "file": "/a.ts",
            "line": 8,
            "offset": 11
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
    let names: Vec<&str> = entries
        .iter()
        .filter_map(|entry| entry.get("name").and_then(serde_json::Value::as_str))
        .collect();

    assert!(
        names.contains(&"foo"),
        "expected static class member `foo` from marker-adjacent member completion, got {names:?}"
    );
    assert!(
        names.contains(&"f"),
        "expected merged namespace member `f` from marker-adjacent member completion, got {names:?}"
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

