use super::*;

#[test]
fn response_taxonomy_success_response_is_success_with_body() {
    let server = make_server();
    let request = make_request("status", serde_json::json!({}));
    let resp = server.success_response(42, &request, Some(serde_json::json!({"k": "v"})));
    assert_eq!(resp.seq, 42);
    assert_eq!(resp.msg_type, "response");
    assert_eq!(resp.command, "status");
    assert_eq!(resp.request_seq, request.seq);
    assert!(resp.success);
    assert!(resp.message.is_none());
    assert_eq!(resp.body, Some(serde_json::json!({"k": "v"})));
}

#[test]
fn response_taxonomy_success_response_supports_empty_result_body() {
    let server = make_server();
    let request = make_request("completions", serde_json::json!({}));
    let resp = server.success_response(1, &request, Some(serde_json::json!([])));
    assert!(resp.success);
    assert_eq!(resp.body, Some(serde_json::json!([])));
    assert!(resp.message.is_none());
}

#[test]
fn response_taxonomy_acknowledge_response_has_no_body() {
    let server = make_server();
    let request = make_request("geterr", serde_json::json!({}));
    let resp = server.acknowledge_response(7, &request);
    assert!(resp.success);
    assert!(resp.message.is_none());
    assert!(resp.body.is_none());
}

#[test]
fn response_taxonomy_unimplemented_response_is_failure_with_reason() {
    let server = make_server();
    let request = make_request("someCmd", serde_json::json!({}));
    let resp = server.unimplemented_response(99, &request, "no command host available");
    assert!(!resp.success);
    assert!(resp.body.is_none());
    let message = resp.message.expect("must carry a reason");
    assert!(message.contains("not implemented"), "got {message:?}");
    assert!(message.contains("someCmd"), "got {message:?}");
    assert!(
        message.contains("no command host available"),
        "got {message:?}"
    );
}

#[test]
fn response_taxonomy_unsupported_response_is_failure_with_reason() {
    let server = make_server();
    let request = make_request("debugCommand", serde_json::json!({}));
    let resp = server.unsupported_response(11, &request, "outside tsz scope");
    assert!(!resp.success);
    assert!(resp.body.is_none());
    let message = resp.message.expect("must carry a reason");
    assert!(message.contains("not supported"), "got {message:?}");
    assert!(message.contains("debugCommand"), "got {message:?}");
    assert!(message.contains("outside tsz scope"), "got {message:?}");
}

#[test]
fn response_taxonomy_unrecognized_command_dispatches_to_failure() {
    let mut server = make_server();
    let request = make_request("nonExistentTsserverCommand", serde_json::json!({}));
    let resp = server.handle_tsserver_request(request);
    assert!(!resp.success);
    assert!(resp.body.is_none());
    let message = resp.message.expect("must carry a reason");
    assert!(message.contains("Unrecognized"), "got {message:?}");
    assert!(
        message.contains("nonExistentTsserverCommand"),
        "got {message:?}"
    );
}

#[test]
fn response_taxonomy_geterr_returns_acknowledge_after_emitting_events() {
    let mut server = make_server();
    server
        .open_files
        .insert("/test.ts".to_string(), "const x: string = 1;".to_string());
    let request = make_request(
        "geterr",
        serde_json::json!({"files": ["/test.ts"], "delay": 0}),
    );
    let resp = server.handle_tsserver_request(request);
    assert!(resp.success);
    assert!(resp.body.is_none());
    assert!(resp.message.is_none());
    let has_request_completed = server.pending_events.iter().any(|event| {
        event.get("event").and_then(|value| value.as_str()) == Some("requestCompleted")
    });
    assert!(has_request_completed, "geterr must emit requestCompleted");
}

#[test]
fn response_taxonomy_get_code_fixes_returns_empty_success_for_no_matches() {
    let mut server = make_server();
    server.open_files.insert(
        "/codefix-empty.ts".to_string(),
        "const x = 1;\n".to_string(),
    );
    let request = make_request(
        "getCodeFixes",
        serde_json::json!({
            "file": "/codefix-empty.ts",
            "startLine": 1,
            "startOffset": 1,
            "endLine": 1,
            "endOffset": 2,
            "errorCodes": [],
        }),
    );
    let resp = server.handle_tsserver_request(request);
    assert!(resp.success);
    assert!(resp.message.is_none());
    assert_eq!(resp.body, Some(serde_json::json!([])));
}
