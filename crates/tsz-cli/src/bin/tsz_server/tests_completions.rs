use super::*;

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
fn test_completion_info_bare_identifier_expression_is_not_new_identifier_location() {
    let mut server = make_server();
    server
        .open_files
        .insert("/index.ts".to_string(), "x".to_string());

    let req = make_request(
        "completionInfo",
        serde_json::json!({
            "file": "/index.ts",
            "line": 1,
            "offset": 2,
            "preferences": { "includeCompletionsForModuleExports": true }
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("completionInfo should return a body");
    assert_eq!(body["isNewIdentifierLocation"], serde_json::json!(false));
}

#[test]
fn test_completion_info_bare_identifier_expression_does_not_replace_auto_import_with_class_member_snippet()
 {
    let mut server = make_server();

    let open_external = make_request(
        "openExternalProject",
        serde_json::json!({
            "projectFileName": "/project.csproj",
            "options": {
                "module": "none",
                "moduleResolution": "bundler",
                "target": "es2015"
            },
            "rootFiles": [
                {
                    "fileName": "/node_modules/dep/index.d.ts",
                    "content": "export const x: number;\n"
                },
                {
                    "fileName": "/index.ts",
                    "content": " x/**/"
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
            "offset": 4,
            "preferences": {
                "allowIncompleteCompletions": true,
                "includeCompletionsForModuleExports": true,
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

    let auto_import_entry = entries
        .iter()
        .find(|entry| {
            entry.get("name").and_then(serde_json::Value::as_str) == Some("x")
                && entry.get("source").and_then(serde_json::Value::as_str) == Some("dep")
        })
        .expect("expected auto-import completion for `x` from `dep`");
    assert!(
        auto_import_entry.get("insertText").is_none(),
        "auto-import entry should not be rewritten as a class member snippet"
    );
    assert!(
        !entries.iter().any(|entry| {
            entry.get("name").and_then(serde_json::Value::as_str) == Some("x")
                && entry.get("source").and_then(serde_json::Value::as_str)
                    == Some("ClassMemberSnippet/")
        }),
        "bare identifier completions should not synthesize class member snippets"
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
fn test_get_code_fixes_type_module_main_prefers_subpath_without_index() {
    let mut server = make_server();
    server.open_files.insert(
        "/node_modules/pkg/package.json".to_string(),
        r#"{
  "name": "pkg",
  "version": "1.0.0",
  "main": "lib",
  "type": "module"
}"#
        .to_string(),
    );
    server.open_files.insert(
        "/node_modules/pkg/lib/index.js".to_string(),
        "export function foo() {}".to_string(),
    );
    server.open_files.insert(
        "/package.json".to_string(),
        r#"{
  "dependencies": {
    "pkg": "*"
  }
}"#
        .to_string(),
    );
    server
        .open_files
        .insert("/index.ts".to_string(), "foo".to_string());

    let fixes_req = make_request(
        "getCodeFixes",
        serde_json::json!({
            "file": "/index.ts",
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

    specifiers.sort();
    specifiers.dedup();
    assert_eq!(specifiers, vec!["pkg/lib".to_string()]);
}

#[test]
fn test_open_external_project_get_code_fixes_type_module_main_prefers_subpath_without_index() {
    let mut server = make_server();

    let open_external = make_request(
        "openExternalProject",
        serde_json::json!({
            "projectFileName": "/project.csproj",
            "options": {
                "allowJs": true
            },
            "rootFiles": [
                {
                    "fileName": "/node_modules/pkg/package.json",
                    "content": "{\n  \"name\": \"pkg\",\n  \"version\": \"1.0.0\",\n  \"main\": \"lib\",\n  \"type\": \"module\"\n}\n"
                },
                {
                    "fileName": "/node_modules/pkg/lib/index.js",
                    "content": "export function foo() {}"
                },
                {
                    "fileName": "/package.json",
                    "content": "{\n  \"dependencies\": {\n    \"pkg\": \"*\"\n  }\n}\n"
                },
                {
                    "fileName": "/index.ts",
                    "content": "foo"
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
            "endOffset": 4,
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
    specifiers.sort();
    specifiers.dedup();
    assert_eq!(specifiers, vec!["pkg/lib".to_string()]);

    let close_external = make_request(
        "closeExternalProject",
        serde_json::json!({ "projectFileName": "/project.csproj" }),
    );
    let close_resp = server.handle_tsserver_request(close_external);
    assert!(close_resp.success);
}

#[test]
fn test_get_code_fixes_package_json_imports_respect_module_specifier_preference() {
    let mut server = make_server();
    server.open_files.insert(
        "/package.json".to_string(),
        r##"{
  "imports": {
    "#*": "./src/*.ts"
  }
}"##
        .to_string(),
    );
    server.open_files.insert(
        "/src/a/b/c/something.ts".to_string(),
        "export function something(name: string): any {}".to_string(),
    );
    server
        .open_files
        .insert("/a.ts".to_string(), "something".to_string());
    server
        .open_files
        .insert("/src/a/b/c/d.ts".to_string(), "something".to_string());

    let mut module_specifiers_for = |file: &str, preferences: serde_json::Value| -> Vec<String> {
        let fixes_req = make_request(
            "getCodeFixes",
            serde_json::json!({
                "file": file,
                "startLine": 1,
                "startOffset": 1,
                "endLine": 1,
                "endOffset": 10,
                "errorCodes": [2304],
                "preferences": preferences
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
        specifiers.sort();
        specifiers.dedup();
        specifiers
    };

    assert_eq!(
        module_specifiers_for(
            "/a.ts",
            serde_json::json!({
                "importModuleSpecifierPreference": "relative"
            })
        ),
        vec!["./src/a/b/c/something".to_string()]
    );
    assert_eq!(
        module_specifiers_for(
            "/a.ts",
            serde_json::json!({
                "importModuleSpecifierPreference": "project-relative"
            })
        ),
        vec!["./src/a/b/c/something".to_string()]
    );
    assert_eq!(
        module_specifiers_for(
            "/src/a/b/c/d.ts",
            serde_json::json!({
                "importModuleSpecifierPreference": "non-relative"
            })
        ),
        vec!["#a/b/c/something".to_string()]
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
    let completion_entries = completions_body["entries"]
        .as_array()
        .expect("completions should include entries");
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
#[ignore = "regression: verbatim CommonJS fallback import action not generated after LSP refactors"]
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
fn test_completion_info_member_method_omits_plain_call_insert_text() {
    let mut server = make_server();
    server.open_files.insert(
        "/index.ts".to_string(),
        "declare class m3d { foo(): void }\nconst r = new m3d();\nr.".to_string(),
    );

    let completion_req = make_request(
        "completionInfo",
        serde_json::json!({
            "file": "/index.ts",
            "line": 3,
            "offset": 3,
            "preferences": {
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
    let foo_entry = entries
        .iter()
        .find(|entry| entry.get("name").and_then(serde_json::Value::as_str) == Some("foo"))
        .expect("expected foo completion entry");
    assert!(
        foo_entry.get("insertText").is_none(),
        "plain member method completions should omit insertText"
    );
}

#[test]
fn test_completion_info_global_function_omits_plain_call_insert_text() {
    let mut server = make_server();
    server.open_files.insert(
        "/index.ts".to_string(),
        "declare function decodeURI(uri: string): string;\ndeco".to_string(),
    );

    let completion_req = make_request(
        "completionInfo",
        serde_json::json!({
            "file": "/index.ts",
            "line": 2,
            "offset": 5,
            "preferences": {
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
    let decode_uri_entry = entries
        .iter()
        .find(|entry| entry.get("name").and_then(serde_json::Value::as_str) == Some("decodeURI"))
        .expect("expected decodeURI completion entry");
    assert!(
        decode_uri_entry.get("insertText").is_none(),
        "plain global function completions should omit insertText"
    );
}

#[test]
fn test_completion_info_class_member_snippet_sets_new_identifier_location() {
    let mut server = make_server();
    server.open_files.insert(
        "/node.ts".to_string(),
        "import Container from \"./container.js\";\nimport Document from \"./document.js\";\n\ndeclare namespace Node {\n  class Node extends Node_ {}\n\n  export { Node as default };\n}\n\ndeclare abstract class Node_ {\n  parent: Container | Document | undefined;\n}\n\ndeclare class Node extends Node_ {}\n\nexport = Node;".to_string(),
    );
    server.open_files.insert(
        "/document.ts".to_string(),
        "import Container from \"./container.js\";\n\ndeclare namespace Document {\n  export { Document_ as default };\n}\n\ndeclare class Document_ extends Container {}\n\ndeclare class Document extends Document_ {}\n\nexport = Document;".to_string(),
    );
    server.open_files.insert(
        "/container.ts".to_string(),
        "import Node from \"./node.js\";\n\ndeclare namespace Container {\n  export { Container_ as default };\n}\n\ndeclare abstract class Container_ extends Node {\n  p\n}\n\ndeclare class Container extends Container_ {}\n\nexport = Container;".to_string(),
    );

    let completion_req = make_request(
        "completionInfo",
        serde_json::json!({
            "file": "/container.ts",
            "line": 8,
            "offset": 4,
            "preferences": {
                "includeCompletionsWithInsertText": true,
                "includeCompletionsWithClassMemberSnippets": true
            }
        }),
    );
    let completion_resp = server.handle_tsserver_request(completion_req);
    assert!(completion_resp.success);
    let completion_body = completion_resp
        .body
        .expect("completionInfo should return a body");
    assert_eq!(
        completion_body["isNewIdentifierLocation"],
        serde_json::json!(true)
    );
    let entries = completion_body["entries"]
        .as_array()
        .expect("completionInfo should include entries");
    assert!(entries.iter().any(|entry| {
        entry.get("source").and_then(serde_json::Value::as_str) == Some("ClassMemberSnippet/")
    }));
}

#[test]
fn test_completion_info_class_member_declaration_prefix_is_new_identifier_location() {
    let mut server = make_server();
    server.open_files.insert(
        "/index.ts".to_string(),
        "class B {\n  blah\n  con\n}".to_string(),
    );

    let completion_req = make_request(
        "completionInfo",
        serde_json::json!({
            "file": "/index.ts",
            "line": 3,
            "offset": 6,
            "preferences": {
                "includeCompletionsWithInsertText": true
            }
        }),
    );
    let completion_resp = server.handle_tsserver_request(completion_req);
    assert!(completion_resp.success);
    let completion_body = completion_resp
        .body
        .expect("completionInfo should return a body");
    assert_eq!(
        completion_body["isNewIdentifierLocation"],
        serde_json::json!(true)
    );
    let entries = completion_body["entries"]
        .as_array()
        .expect("completionInfo should include entries");
    assert!(entries.iter().any(|entry| {
        entry.get("name").and_then(serde_json::Value::as_str) == Some("constructor")
    }));
}

#[test]
fn test_completion_info_class_member_snippet_quotes_constructor_property_name() {
    let mut server = make_server();
    server.open_files.insert(
        "/KlassConstructor.ts".to_string(),
        "type GenericConstructor<T> = new (...args: any[]) => T;\nexport type KlassConstructor<Cls extends GenericConstructor<any>> = GenericConstructor<InstanceType<Cls>> & { [k in keyof Cls]: Cls[k] };\n".to_string(),
    );
    server.open_files.insert(
        "/ElementNode.ts".to_string(),
        "import { KlassConstructor } from \"./KlassConstructor\";\nexport class ElementNode {\n  [\"constructor\"]!: KlassConstructor<typeof ElementNode>;\n}\n".to_string(),
    );
    server.open_files.insert(
        "/index.ts".to_string(),
        "import { ElementNode } from \"./ElementNode\";\nclass C extends ElementNode {\n  \n}\n"
            .to_string(),
    );

    let completion_req = make_request(
        "completionInfo",
        serde_json::json!({
            "file": "/index.ts",
            "line": 3,
            "offset": 3,
            "preferences": {
                "includeCompletionsWithInsertText": true,
                "includeCompletionsWithClassMemberSnippets": true
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
    let constructor_entry = entries
        .iter()
        .find(|entry| {
            entry.get("name").and_then(serde_json::Value::as_str) == Some("[\"constructor\"]")
                && entry.get("source").and_then(serde_json::Value::as_str)
                    == Some("ClassMemberSnippet/")
        })
        .expect("expected class member snippet completion for computed constructor property");
    assert_eq!(
        constructor_entry
            .get("insertText")
            .and_then(serde_json::Value::as_str),
        Some("[\"constructor\"]: KlassConstructor<typeof ElementNode>;")
    );
}

#[test]
fn test_completion_info_auto_import_dependency_filter_hides_unlisted_bare_package() {
    let mut server = make_server();
    server.open_files.insert(
        "/node_modules/@types/react/index.d.ts".to_string(),
        "export declare function useMemo(): void;\nexport declare function useState(): void;\n"
            .to_string(),
    );
    server
        .open_files
        .insert("/package.json".to_string(), "{}".to_string());
    server
        .open_files
        .insert("/index.ts".to_string(), "useMemo/**/".to_string());

    let completion_req = make_request(
        "completionInfo",
        serde_json::json!({
            "file": "/index.ts",
            "line": 1,
            "offset": 8,
            "preferences": {
                "includeCompletionsForModuleExports": true
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
    assert!(
        !entries.iter().any(|entry| {
            entry.get("name").and_then(serde_json::Value::as_str) == Some("useMemo")
                && entry.get("source").and_then(serde_json::Value::as_str) == Some("react")
        }),
        "expected dependency filter to hide bare package auto-imports without a listed dependency"
    );
}

#[test]
fn test_completion_info_auto_import_dependency_filter_allows_existing_imported_package() {
    let mut server = make_server();
    server.open_files.insert(
        "/node_modules/@types/react/index.d.ts".to_string(),
        "export declare function useMemo(): void;\nexport declare function useState(): void;\n"
            .to_string(),
    );
    server
        .open_files
        .insert("/package.json".to_string(), "{}".to_string());
    server.open_files.insert(
        "/index.ts".to_string(),
        "import { useState } from \"react\";\nuseMemo/**/\n".to_string(),
    );

    let completion_req = make_request(
        "completionInfo",
        serde_json::json!({
            "file": "/index.ts",
            "line": 2,
            "offset": 8,
            "preferences": {
                "includeCompletionsForModuleExports": true
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
    assert!(
        entries.iter().any(|entry| {
            entry.get("name").and_then(serde_json::Value::as_str) == Some("useMemo")
                && entry.get("source").and_then(serde_json::Value::as_str) == Some("react")
        }),
        "expected existing imports to keep same-package auto-import candidates available"
    );
}

#[test]
fn test_completion_info_auto_import_dependency_filter_ignores_invalid_package_json() {
    let mut server = make_server();
    server.open_files.insert(
        "/node_modules/react/index.d.ts".to_string(),
        "export declare const React: any;\n".to_string(),
    );
    server.open_files.insert(
        "/node_modules/react/package.json".to_string(),
        r#"{ "name": "react", "types": "./index.d.ts" }"#.to_string(),
    );
    server.open_files.insert(
        "/node_modules/fake-react/index.d.ts".to_string(),
        "export declare const ReactFake: any;\n".to_string(),
    );
    server.open_files.insert(
        "/node_modules/fake-react/package.json".to_string(),
        r#"{ "name": "fake-react", "types": "./index.d.ts" }"#.to_string(),
    );
    server.open_files.insert(
        "/package.json".to_string(),
        "{\n  \"mod\"\n  \"dependencies\": { \"react\": \"*\" }\n}\n".to_string(),
    );
    server
        .open_files
        .insert("/src/index.ts".to_string(), "const x = Re/**/".to_string());

    let completion_req = make_request(
        "completionInfo",
        serde_json::json!({
            "file": "/src/index.ts",
            "line": 1,
            "offset": 12,
            "preferences": {
                "includeCompletionsForModuleExports": true
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
    assert!(
        entries.iter().any(|entry| {
            entry.get("name").and_then(serde_json::Value::as_str) == Some("React")
                && entry.get("source").and_then(serde_json::Value::as_str) == Some("react")
        }),
        "expected invalid package.json not to suppress normal auto-import candidates"
    );
    assert!(
        entries.iter().any(|entry| {
            entry.get("name").and_then(serde_json::Value::as_str) == Some("ReactFake")
                && entry.get("source").and_then(serde_json::Value::as_str) == Some("fake-react")
        }),
        "expected invalid package.json not to filter unrelated package candidates"
    );
}

#[test]
fn test_completion_info_auto_import_dependency_filter_ignores_plain_string_literals() {
    let mut server = make_server();
    server.open_files.insert(
        "/node_modules/@types/react/index.d.ts".to_string(),
        "export declare function useMemo(): void;\n".to_string(),
    );
    server
        .open_files
        .insert("/package.json".to_string(), "{}".to_string());
    server.open_files.insert(
        "/index.ts".to_string(),
        "const pkg = \"react\";\nuseMemo/**/\n".to_string(),
    );

    let completion_req = make_request(
        "completionInfo",
        serde_json::json!({
            "file": "/index.ts",
            "line": 2,
            "offset": 8,
            "preferences": {
                "includeCompletionsForModuleExports": true
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
    assert!(
        !entries.iter().any(|entry| {
            entry.get("name").and_then(serde_json::Value::as_str) == Some("useMemo")
                && entry.get("source").and_then(serde_json::Value::as_str) == Some("react")
        }),
        "expected plain string literals not to bypass dependency-based auto-import filtering"
    );
}

#[test]
fn test_completion_info_auto_import_dependency_filter_allows_require_usage() {
    let mut server = make_server();
    server.open_files.insert(
        "/node_modules/@types/react/index.d.ts".to_string(),
        "export declare function useMemo(): void;\n".to_string(),
    );
    server
        .open_files
        .insert("/package.json".to_string(), "{}".to_string());
    server.open_files.insert(
        "/index.ts".to_string(),
        "const loaded = require(\"react\");\nuseMemo/**/\n".to_string(),
    );

    let completion_req = make_request(
        "completionInfo",
        serde_json::json!({
            "file": "/index.ts",
            "line": 2,
            "offset": 8,
            "preferences": {
                "includeCompletionsForModuleExports": true
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
    assert!(
        entries.iter().any(|entry| {
            entry.get("name").and_then(serde_json::Value::as_str) == Some("useMemo")
                && entry.get("source").and_then(serde_json::Value::as_str) == Some("react")
        }),
        "expected require()-based package usage to keep same-package auto-import candidates available"
    );
}
