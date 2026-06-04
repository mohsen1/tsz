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
