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
    let completions_entries = completions_body
        .as_array()
        .expect("legacy completions should return a top-level entries array");
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
    let completions_from_configured_entries = completions_from_configured_body
        .as_array()
        .expect("legacy configured completions should return a top-level entries array");
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

#[test]
fn test_get_code_fixes_import_nested_package_manifest_inside_package() {
    let mut server = make_server();
    server.open_files.insert(
        "/project/app.tsx".to_string(),
        "const state = useMemo(() => 'Hello', []);".to_string(),
    );
    server.open_files.insert(
        "/project/component.tsx".to_string(),
        "import { useEffect } from \"preact/hooks\";".to_string(),
    );
    server.open_files.insert(
        "/project/node_modules/preact/package.json".to_string(),
        r#"{"name":"preact","types":"src/index.d.ts"}"#.to_string(),
    );
    server.open_files.insert(
        "/project/node_modules/preact/hooks/package.json".to_string(),
        r#"{"name":"hooks","types":"src/index.d.ts"}"#.to_string(),
    );
    server.open_files.insert(
        "/project/node_modules/preact/hooks/src/index.d.ts".to_string(),
        "export declare function useEffect(): void;\nexport declare function useMemo(): void;\n"
            .to_string(),
    );

    let req = make_request(
        "getCodeFixes",
        serde_json::json!({
            "file": "/project/app.tsx",
            "startLine": 1,
            "startOffset": 1,
            "endLine": 1,
            "endOffset": 1,
            "errorCodes": [2304],
            "preferences": {}
        }),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let fixes = resp
        .body
        .expect("getCodeFixes should return a body")
        .as_array()
        .expect("getCodeFixes body should be an array")
        .clone();

    let new_texts: Vec<String> = fixes
        .iter()
        .filter(|fix| fix.get("fixName").and_then(serde_json::Value::as_str) == Some("import"))
        .flat_map(|fix| {
            fix.get("changes")
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
                .flat_map(|change| {
                    change
                        .get("textChanges")
                        .and_then(serde_json::Value::as_array)
                        .into_iter()
                        .flatten()
                })
                .filter_map(|text_change| {
                    text_change
                        .get("newText")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string)
                })
                .collect::<Vec<_>>()
        })
        .collect();

    assert!(
        new_texts
            .iter()
            .any(|text| text.contains("import { useMemo } from \"preact/hooks\";")),
        "expected exact useMemo import fix from preact/hooks, got {new_texts:#?}"
    );
}

// ---------------------------------------------------------------------------
// Issue #3955: tsserver protocol distinguishes the legacy `completions` body
// from `completionInfo` and the full-protocol `completions-full` body. The
// legacy `completions` command returns a top-level `CompletionEntry[]`, while
// `completionInfo`/`completions-full` return a `CompletionInfo` object whose
// `entries` field carries the same array.
// ---------------------------------------------------------------------------

#[test]
fn legacy_completions_returns_top_level_entries_array() {
    let mut server = make_server();
    server
        .open_files
        .insert("/a.ts".to_string(), "const alpha = 1;\nal".to_string());

    let req = make_request(
        "completions",
        serde_json::json!({"file": "/a.ts", "line": 2, "offset": 3}),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("legacy completions should return a body");
    let entries = body
        .as_array()
        .expect("legacy completions body must be a CompletionEntry[] array");
    assert!(
        entries.iter().any(|entry| {
            entry.get("name").and_then(serde_json::Value::as_str) == Some("alpha")
        }),
        "expected `alpha` in legacy completions entries, got {entries:?}"
    );
    assert!(
        body.get("entries").is_none(),
        "legacy completions body must not be the CompletionInfo object shape, got {body:?}"
    );
    assert!(
        body.get("isGlobalCompletion").is_none(),
        "legacy completions body must omit CompletionInfo-only fields, got {body:?}"
    );
    assert!(
        body.get("isMemberCompletion").is_none(),
        "legacy completions body must omit CompletionInfo-only fields, got {body:?}"
    );
    assert!(
        body.get("isNewIdentifierLocation").is_none(),
        "legacy completions body must omit CompletionInfo-only fields, got {body:?}"
    );
}

#[test]
fn completion_info_returns_completion_info_object_shape() {
    let mut server = make_server();
    server
        .open_files
        .insert("/a.ts".to_string(), "const alpha = 1;\nal".to_string());

    let req = make_request(
        "completionInfo",
        serde_json::json!({"file": "/a.ts", "line": 2, "offset": 3}),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("completionInfo should return a body");
    assert!(
        body.is_object(),
        "completionInfo body must be the CompletionInfo object shape, got {body:?}"
    );
    let entries = body["entries"]
        .as_array()
        .expect("completionInfo body must include an `entries` array");
    assert!(
        entries.iter().any(|entry| {
            entry.get("name").and_then(serde_json::Value::as_str) == Some("alpha")
        }),
        "expected `alpha` inside completionInfo.entries, got {entries:?}"
    );
    assert!(
        body.get("isNewIdentifierLocation").is_some(),
        "completionInfo body must include CompletionInfo-only fields, got {body:?}"
    );
}

#[test]
fn completions_full_returns_completion_info_object_shape() {
    let mut server = make_server();
    server
        .open_files
        .insert("/a.ts".to_string(), "const alpha = 1;\nal".to_string());

    let req = make_request(
        "completions-full",
        serde_json::json!({"file": "/a.ts", "line": 2, "offset": 3}),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("completions-full should return a body");
    assert!(
        body.is_object(),
        "completions-full body must be the CompletionInfo object shape, got {body:?}"
    );
    let entries = body["entries"]
        .as_array()
        .expect("completions-full body must include an `entries` array");
    assert!(
        entries.iter().any(|entry| {
            entry.get("name").and_then(serde_json::Value::as_str) == Some("alpha")
        }),
        "expected `alpha` inside completions-full.entries, got {entries:?}"
    );
}

#[test]
fn legacy_completions_inside_line_comment_returns_empty_array() {
    let mut server = make_server();
    server
        .open_files
        .insert("/a.ts".to_string(), "// hello al".to_string());

    let req = make_request(
        "completions",
        serde_json::json!({"file": "/a.ts", "line": 1, "offset": 12}),
    );
    let resp = server.handle_tsserver_request(req);
    assert!(resp.success);
    let body = resp.body.expect("legacy completions should return a body");
    let entries = body
        .as_array()
        .expect("legacy completions body must be an array even inside line comments");
    assert!(
        entries.is_empty(),
        "legacy completions inside a line comment must be an empty array, got {entries:?}"
    );
}
