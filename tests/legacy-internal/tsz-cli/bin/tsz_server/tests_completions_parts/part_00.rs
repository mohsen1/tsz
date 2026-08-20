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
fn test_completion_entry_details_js_function_variable_uses_jsdoc_function_type() {
    let mut server = make_server();
    server.open_files.insert(
        "/a.js".to_string(),
        "/**\n * Modify the parameter\n * @param {string} p1\n */\nvar foo = function (p1) { }\nfo"
            .to_string(),
    );

    let req = make_request(
        "completionEntryDetails",
        serde_json::json!({
            "file": "/a.js",
            "line": 6,
            "offset": 3,
            "entryNames": ["foo"]
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
    let display_text = first
        .get("displayParts")
        .and_then(serde_json::Value::as_array)
        .map(|parts| {
            parts
                .iter()
                .filter_map(|part| part.get("text").and_then(serde_json::Value::as_str))
                .collect::<String>()
        })
        .unwrap_or_default();
    assert_eq!(display_text, "var foo: (p1: string) => void");
    let tags = first
        .get("tags")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    assert_eq!(
        tags,
        vec![serde_json::json!({
            "name": "param",
            "text": "p1"
        })]
    );
}

#[test]
fn test_completion_entry_details_with_completion_entry_payload_preserves_plain_param_tag_text() {
    let mut server = make_server();
    server.open_files.insert(
        "/a.js".to_string(),
        "/**\n * Modify the parameter\n * @param {string} p1\n */\nvar foo = function (p1) { }\nexports.foo = foo;\nfo".to_string(),
    );
    server.open_files.insert(
        "/b.ts".to_string(),
        "import a = require(\"./a\");\na.fo".to_string(),
    );

    let assert_plain_param_tags = |server: &mut Server, file: &str, line: u32, offset: u32| {
        let info_req = make_request(
            "completionInfo",
            serde_json::json!({
                "file": file,
                "line": line,
                "offset": offset
            }),
        );
        let info_resp = server.handle_tsserver_request(info_req);
        assert!(info_resp.success);
        let info_body = info_resp.body.expect("completionInfo should return a body");
        let entries = info_body["entries"]
            .as_array()
            .expect("completionInfo should include entries");
        let foo_entry = entries
            .iter()
            .find(|entry| entry.get("name").and_then(serde_json::Value::as_str) == Some("foo"))
            .cloned()
            .expect("completionInfo should include `foo`");

        let mut entry_name = serde_json::Map::new();
        entry_name.insert("name".to_string(), serde_json::json!("foo"));
        if let Some(source) = foo_entry.get("source") {
            entry_name.insert("source".to_string(), source.clone());
        }
        if let Some(data) = foo_entry.get("data") {
            entry_name.insert("data".to_string(), data.clone());
        }

        let details_req = make_request(
            "completionEntryDetails",
            serde_json::json!({
                "file": file,
                "line": line,
                "offset": offset,
                "entryNames": [serde_json::Value::Object(entry_name)],
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
        let tags = first
            .get("tags")
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default();
        assert_eq!(
            tags,
            vec![serde_json::json!({
                "name": "param",
                "text": "p1"
            })],
            "expected `@param` tag text to remain plain `p1`, got: {first:?}"
        );
    };

    assert_plain_param_tags(&mut server, "/a.js", 7, 3);
    assert_plain_param_tags(&mut server, "/b.ts", 2, 5);
}

#[test]
fn test_completion_default_and_named_auto_import_conflict_preserves_distinct_entries() {
    let mut server = make_server();
    server.open_files.insert(
        "/home/src/workspaces/project/tsconfig.json".to_string(),
        r#"{ "compilerOptions": { "noLib": true, "lib": ["es5"] } }"#.to_string(),
    );
    server.open_files.insert(
        "/home/src/workspaces/project/someModule.ts".to_string(),
        "export const someModule = 0;\nexport default 1;\n".to_string(),
    );
    server.open_files.insert(
        "/home/src/workspaces/project/index.ts".to_string(),
        "someMo".to_string(),
    );

    let completion_req = make_request(
        "completionInfo",
        serde_json::json!({
            "file": "/home/src/workspaces/project/index.ts",
            "line": 1,
            "offset": 7,
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
    let conflict_entries: Vec<&serde_json::Value> = entries
        .iter()
        .filter(|entry| {
            entry.get("name").and_then(serde_json::Value::as_str) == Some("someModule")
                && entry.get("source").and_then(serde_json::Value::as_str) == Some("./someModule")
        })
        .collect();
    assert!(
        conflict_entries.len() >= 2,
        "expected distinct auto-import entries for default and named exports, got: {conflict_entries:?}"
    );
    let kinds: Vec<&str> = conflict_entries
        .iter()
        .filter_map(|entry| entry.get("kind").and_then(serde_json::Value::as_str))
        .collect();
    assert!(
        kinds.contains(&"property"),
        "expected default-export auto-import entry kind `property`, got kinds: {kinds:?}"
    );
    assert!(
        kinds.contains(&"const"),
        "expected named-export auto-import entry kind `const`, got kinds: {kinds:?}"
    );
    let first_conflict = conflict_entries
        .first()
        .expect("expected at least one conflicting completion entry");
    assert_eq!(
        first_conflict
            .get("kind")
            .and_then(serde_json::Value::as_str),
        Some("property"),
        "expected default-export entry to sort before named export variant"
    );
    assert_eq!(
        first_conflict
            .get("data")
            .and_then(|data| data.get("exportName"))
            .and_then(serde_json::Value::as_str),
        Some("default"),
        "expected first conflicting completion entry to carry exportName=default"
    );

    let details_req = make_request(
        "completionEntryDetails",
        serde_json::json!({
            "file": "/home/src/workspaces/project/index.ts",
            "line": 1,
            "offset": 7,
            "entryNames": [
                {
                    "name": "someModule",
                    "source": "./someModule",
                    "data": {
                        "exportName": "default"
                    }
                },
                {
                    "name": "someModule",
                    "source": "./someModule",
                    "data": {
                        "exportName": "someModule"
                    }
                }
            ],
            "preferences": {
                "includeCompletionsForModuleExports": true
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
    let to_display_text = |detail: &serde_json::Value| {
        detail
            .get("displayParts")
            .and_then(serde_json::Value::as_array)
            .map(|parts| {
                parts
                    .iter()
                    .filter_map(|part| part.get("text").and_then(serde_json::Value::as_str))
                    .collect::<String>()
            })
            .unwrap_or_default()
    };
    let kind_and_text: Vec<(String, String)> = details
        .iter()
        .map(|detail| {
            (
                detail
                    .get("kind")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                to_display_text(detail),
            )
        })
        .collect();
    assert!(
        kind_and_text
            .iter()
            .any(|(kind, text)| kind == "property" && text == "(property) default: 1"),
        "expected default-export completion detail `(property) default: 1`, got: {kind_and_text:?}"
    );
    assert!(
        kind_and_text
            .iter()
            .any(|(kind, text)| kind == "const" && text == "const someModule: 0"),
        "expected named-export completion detail `const someModule: 0`, got: {kind_and_text:?}"
    );
    for detail in details {
        let tags = detail
            .get("tags")
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default();
        assert_eq!(
            tags,
            Vec::<serde_json::Value>::new(),
            "expected auto-import completion detail tags to be an empty array, got: {detail:?}"
        );
    }

    let first_entry_name = first_conflict
        .get("name")
        .cloned()
        .unwrap_or(serde_json::json!("someModule"));
    let first_entry_source = first_conflict
        .get("source")
        .cloned()
        .unwrap_or(serde_json::json!("./someModule"));
    let first_entry_data = first_conflict
        .get("data")
        .cloned()
        .unwrap_or(serde_json::json!({ "exportName": "default" }));
    let first_entry_details_req = make_request(
        "completionEntryDetails",
        serde_json::json!({
            "file": "/home/src/workspaces/project/index.ts",
            "line": 1,
            "offset": 7,
            "entryNames": [{
                "name": first_entry_name,
                "source": first_entry_source,
                "data": first_entry_data
            }],
            "preferences": {
                "includeCompletionsForModuleExports": true
            }
        }),
    );
    let first_entry_details_resp = server.handle_tsserver_request(first_entry_details_req);
    assert!(first_entry_details_resp.success);
    let first_entry_details_body = first_entry_details_resp
        .body
        .expect("completionEntryDetails should return a body");
    let first_entry_details = first_entry_details_body
        .as_array()
        .expect("completionEntryDetails should return an array");
    let first_detail = first_entry_details
        .first()
        .expect("completionEntryDetails should include one entry");
    let text_changes = first_detail
        .get("codeActions")
        .and_then(serde_json::Value::as_array)
        .and_then(|actions| actions.first())
        .and_then(|action| action.get("changes"))
        .and_then(serde_json::Value::as_array)
        .and_then(|changes| changes.first())
        .and_then(|change| change.get("textChanges"))
        .and_then(serde_json::Value::as_array)
        .expect("completion detail should include text changes for auto-import");
    let mut updated_text = "someMo".to_string();
    let mut edits: Vec<(usize, usize, String)> = text_changes
        .iter()
        .filter_map(|change| {
            let start = change
                .get("span")
                .and_then(|span| span.get("start"))
                .and_then(serde_json::Value::as_u64)? as usize;
            let length = change
                .get("span")
                .and_then(|span| span.get("length"))
                .and_then(serde_json::Value::as_u64)? as usize;
            let new_text = change
                .get("newText")
                .and_then(serde_json::Value::as_str)?
                .to_string();
            Some((start, start + length, new_text))
        })
        .collect();
    edits.sort_by_key(|edit| std::cmp::Reverse(edit.0));
    for (start, end, new_text) in edits {
        updated_text.replace_range(start..end, &new_text);
    }
    assert_eq!(
        updated_text, "import someModule from \"./someModule\";\r\n\r\nsomeMo",
        "expected default-export completion code action to insert default import"
    );
}

#[test]
fn test_completion_info_commonjs_require_member_fallback_includes_export_assignment_member() {
    let mut server = make_server();
    server.open_files.insert(
        "/a.js".to_string(),
        "/**\n * Modify the parameter\n * @param {string} p1\n */\nvar foo = function (p1) { }\nexports.foo = foo;\n".to_string(),
    );
    server.open_files.insert(
        "/b.ts".to_string(),
        "import a = require(\"./a\");\na.fo".to_string(),
    );

    let completion_req = make_request(
        "completionInfo",
        serde_json::json!({
            "file": "/b.ts",
            "line": 2,
            "offset": 5
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
        entries
            .iter()
            .any(|entry| entry.get("name").and_then(serde_json::Value::as_str) == Some("foo")),
        "expected require() member completion for exported `foo`, got: {entries:?}"
    );

    let details_req = make_request(
        "completionEntryDetails",
        serde_json::json!({
            "file": "/b.ts",
            "line": 2,
            "offset": 5,
            "entryNames": ["foo"]
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
    let display_text = first
        .get("displayParts")
        .and_then(serde_json::Value::as_array)
        .map(|parts| {
            parts
                .iter()
                .filter_map(|part| part.get("text").and_then(serde_json::Value::as_str))
                .collect::<String>()
        })
        .unwrap_or_default();
    assert_eq!(
        display_text,
        "(alias) var foo: (p1: string) => void\nimport a.foo"
    );
    let tags = first
        .get("tags")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    assert_eq!(
        tags,
        vec![serde_json::json!({
            "name": "param",
            "text": "p1"
        })]
    );
}

#[test]
fn test_completion_info_includes_reexported_config_auto_import() {
    let mut server = make_server();
    server.open_files.insert(
        "/home/src/workspaces/project/tsconfig.json".to_string(),
        r#"{ "compilerOptions": { "module": "commonjs", "lib": ["es5"] } }"#.to_string(),
    );
    server.open_files.insert(
        "/home/src/workspaces/project/package.json".to_string(),
        r#"{ "dependencies": { "@jest/types": "*", "ts-jest": "*" } }"#.to_string(),
    );
    server.open_files.insert(
        "/home/src/workspaces/project/node_modules/@jest/types/package.json".to_string(),
        r#"{ "name": "@jest/types" }"#.to_string(),
    );
    server.open_files.insert(
        "/home/src/workspaces/project/node_modules/@jest/types/index.d.ts".to_string(),
        "import type * as Config from \"./Config\";\nexport type { Config };\n".to_string(),
    );
    server.open_files.insert(
        "/home/src/workspaces/project/node_modules/@jest/types/Config.d.ts".to_string(),
        "export interface ConfigGlobals {\n  [K: string]: unknown;\n}\n".to_string(),
    );
    server.open_files.insert(
        "/home/src/workspaces/project/node_modules/ts-jest/index.d.ts".to_string(),
        "export {};\ndeclare module \"@jest/types\" {\n  namespace Config {\n    interface ConfigGlobals {\n      'ts-jest': any;\n    }\n  }\n}\n".to_string(),
    );
    server.open_files.insert(
        "/home/src/workspaces/project/index.ts".to_string(),
        "C/**/".to_string(),
    );

    let assert_has_config_completion = |entries: &[serde_json::Value],
                                        context: &str|
     -> Result<(), String> {
        if entries.iter().any(|entry| {
            entry.get("name").and_then(serde_json::Value::as_str) == Some("Config")
                && entry.get("source").and_then(serde_json::Value::as_str) == Some("@jest/types")
                && entry.get("hasAction").and_then(serde_json::Value::as_bool) == Some(true)
        }) {
            Ok(())
        } else {
            Err(format!(
                "{context}: expected `Config` auto-import completion from @jest/types, got entries: {entries:?}"
            ))
        }
    };

    let req_c = make_request(
        "completionInfo",
        serde_json::json!({
            "file": "/home/src/workspaces/project/index.ts",
            "line": 1,
            "offset": 2,
            "preferences": {
                "includeCompletionsForModuleExports": true
            }
        }),
    );
    let resp_c = server.handle_tsserver_request(req_c);
    assert!(resp_c.success);
    let body_c = resp_c.body.expect("completionInfo should return a body");
    let entries_c = body_c["entries"]
        .as_array()
        .expect("completionInfo should include entries");
    assert_has_config_completion(entries_c, "prefix `C`").unwrap();
    let req_c_legacy = make_request(
        "completions",
        serde_json::json!({
            "file": "/home/src/workspaces/project/index.ts",
            "line": 1,
            "offset": 2,
            "preferences": {
                "includeCompletionsForModuleExports": true
            }
        }),
    );
    let resp_c_legacy = server.handle_tsserver_request(req_c_legacy);
    assert!(resp_c_legacy.success);
    let body_c_legacy = resp_c_legacy
        .body
        .expect("completions should return a body");
    let entries_c_legacy = body_c_legacy
        .as_array()
        .expect("legacy completions should return a top-level entries array");
    assert_has_config_completion(entries_c_legacy, "legacy `completions` prefix `C`").unwrap();

    server.open_files.insert(
        "/home/src/workspaces/project/index.ts".to_string(),
        "Co/**/".to_string(),
    );
    let req_co = make_request(
        "completionInfo",
        serde_json::json!({
            "file": "/home/src/workspaces/project/index.ts",
            "line": 1,
            "offset": 3,
            "preferences": {
                "includeCompletionsForModuleExports": true,
                "allowIncompleteCompletions": true
            }
        }),
    );
    let resp_co = server.handle_tsserver_request(req_co);
    assert!(resp_co.success);
    let body_co = resp_co.body.expect("completionInfo should return a body");
    let entries_co = body_co["entries"]
        .as_array()
        .expect("completionInfo should include entries");
    assert_has_config_completion(entries_co, "prefix `Co`").unwrap();
    let req_co_legacy = make_request(
        "completions",
        serde_json::json!({
            "file": "/home/src/workspaces/project/index.ts",
            "line": 1,
            "offset": 3,
            "preferences": {
                "includeCompletionsForModuleExports": true,
                "allowIncompleteCompletions": true
            }
        }),
    );
    let resp_co_legacy = server.handle_tsserver_request(req_co_legacy);
    assert!(resp_co_legacy.success);
    let body_co_legacy = resp_co_legacy
        .body
        .expect("completions should return a body");
    let entries_co_legacy = body_co_legacy
        .as_array()
        .expect("legacy completions should return a top-level entries array");
    assert_has_config_completion(entries_co_legacy, "legacy `completions` prefix `Co`").unwrap();
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

