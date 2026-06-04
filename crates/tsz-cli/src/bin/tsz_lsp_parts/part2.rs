#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn uri_to_file_name_decodes_percent_encoded_file_paths() {
        assert_eq!(
            LspServer::uri_to_file_name("file:///private/tmp/tsz%20lsp%20uri%20current"),
            "/private/tmp/tsz lsp uri current"
        );
        assert_eq!(
            LspServer::uri_to_file_name("file:///tmp/hash%23percent%25.ts"),
            "/tmp/hash#percent%.ts"
        );
        assert_eq!(
            LspServer::uri_to_file_name("file:///tmp/%C3%BC.ts"),
            "/tmp/\u{00fc}.ts"
        );
    }

    #[test]
    fn uri_to_file_name_handles_localhost_authority() {
        assert_eq!(
            LspServer::uri_to_file_name("file://localhost/private/tmp/tsz%20lsp"),
            "/private/tmp/tsz lsp"
        );
    }

    #[test]
    fn uri_to_file_name_handles_non_local_authority() {
        assert_eq!(
            LspServer::uri_to_file_name("file://server/share/a%20b.ts"),
            "//server/share/a b.ts"
        );
    }

    #[test]
    fn uri_to_file_name_preserves_non_file_uris() {
        assert_eq!(
            LspServer::uri_to_file_name("untitled:Untitled-1"),
            "untitled:Untitled-1"
        );
    }

    #[test]
    fn file_name_to_uri_percent_encodes_file_paths() {
        assert_eq!(
            LspServer::file_name_to_uri("/private/tmp/tsz lsp uri current/src/a#b%.ts"),
            "file:///private/tmp/tsz%20lsp%20uri%20current/src/a%23b%25.ts"
        );
        assert_eq!(
            LspServer::file_name_to_uri("/tmp/\u{00fc}.ts"),
            "file:///tmp/%C3%BC.ts"
        );
        assert_eq!(
            LspServer::file_name_to_uri("//server/share/a b%.ts"),
            "file://server/share/a%20b%25.ts"
        );
    }

    #[test]
    fn uri_conversion_round_trips_encoded_absolute_paths() {
        let file_name = "/private/tmp/tsz lsp uri current/src/a#b%.ts";
        let uri = LspServer::file_name_to_uri(file_name);

        assert_eq!(LspServer::uri_to_file_name(&uri), file_name);
    }

    #[test]
    fn initialize_decodes_percent_encoded_workspace_root_uri() {
        let mut server = LspServer::new();
        let params = json!({
            "rootUri": "file:///private/tmp/tsz%20lsp%20uri%20current",
            "capabilities": {}
        });

        server.handle_initialize(Some(&params));

        assert_eq!(
            server.project.workspace_roots(),
            ["/private/tmp/tsz lsp uri current".to_string()]
        );
    }

    #[test]
    fn dispatch_handles_known_notifications_without_response() {
        let mut server = LspServer::new();
        let response = server.handle_message(JsonRpcMessage {
            id: None,
            method: Some("initialized".to_string()),
            params: None,
        });

        assert!(response.is_none());
        assert!(server.initialized);
    }

    #[test]
    fn dispatch_reports_unknown_request_methods() {
        let mut server = LspServer::new();
        let response = server
            .handle_message(JsonRpcMessage {
                id: Some(json!(7)),
                method: Some("workspace/notImplemented".to_string()),
                params: None,
            })
            .expect("unknown requests should produce method-not-found responses");

        assert_eq!(response.id, json!(7));
        let error = response.error.expect("unknown request should be an error");
        assert_eq!(error.code, -32601);
        assert_eq!(error.message, "Method not found: workspace/notImplemented");
    }

    #[test]
    fn dispatch_ignores_methodless_response_messages() {
        let mut server = LspServer::new();
        let response = server.handle_message(JsonRpcMessage {
            id: Some(json!(1)),
            method: None,
            params: None,
        });

        assert!(response.is_none());
    }

    // Issue #3545: tsz.applyCodeAction must enqueue workspace/applyEdit as a
    // server-to-client REQUEST (with `id`), not a notification. LSP spec
    // requires the client to respond with `ApplyWorkspaceEditResponse`.
    #[test]
    fn apply_code_action_enqueues_workspace_apply_edit_as_request() {
        let mut server = LspServer::new();
        let params = json!({
            "command": "tsz.applyCodeAction",
            "arguments": [{
                "changes": {
                    "file:///tmp/a.ts": [{
                        "range": {
                            "start": { "line": 0, "character": 0 },
                            "end": { "line": 0, "character": 0 }
                        },
                        "newText": "x"
                    }]
                }
            }]
        });

        let result = server
            .handle_execute_command(Some(params))
            .expect("execute command should succeed");
        assert_eq!(result, Value::Bool(true));

        // No notification should be queued — the message is a request.
        assert!(
            !server
                .pending_notifications
                .iter()
                .any(|n| n.method == "workspace/applyEdit"),
            "workspace/applyEdit must NOT be a notification"
        );

        // Exactly one server-to-client request, with a numeric id and the
        // expected method.
        assert_eq!(
            server.pending_server_requests.len(),
            1,
            "expected one pending server request"
        );
        let req = &server.pending_server_requests[0];
        assert_eq!(req.method, "workspace/applyEdit");
        assert!(
            matches!(req.id, Value::Number(_)),
            "request id must be numeric per JSON-RPC, got: {:?}",
            req.id
        );
    }
}
