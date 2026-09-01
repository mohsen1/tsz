//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-cli/src/bin/tsz_lsp.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN f39abe0a35fabf2a19943c4bfc8e4756d6799862b85c4f8f0b5a4ba8d962bbd5 1702 uri_to_file_name_decodes_percent_encoded_file_paths
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
// TSZ_INLINE_TEST_END f39abe0a35fabf2a19943c4bfc8e4756d6799862b85c4f8f0b5a4ba8d962bbd5

// TSZ_INLINE_TEST_BEGIN e70b76cae7b048ca4f9cd5661daf584fc553e2aba797f42a49291ac82d7b0360 1718 uri_to_file_name_handles_localhost_authority
    #[test]
    fn uri_to_file_name_handles_localhost_authority() {
        assert_eq!(
            LspServer::uri_to_file_name("file://localhost/private/tmp/tsz%20lsp"),
            "/private/tmp/tsz lsp"
        );
    }
// TSZ_INLINE_TEST_END e70b76cae7b048ca4f9cd5661daf584fc553e2aba797f42a49291ac82d7b0360

// TSZ_INLINE_TEST_BEGIN 8557d5236cb19c59648562e13a5915af16c3c38b17c8773715ecbc37c8263fef 1726 uri_to_file_name_handles_non_local_authority
    #[test]
    fn uri_to_file_name_handles_non_local_authority() {
        assert_eq!(
            LspServer::uri_to_file_name("file://server/share/a%20b.ts"),
            "//server/share/a b.ts"
        );
    }
// TSZ_INLINE_TEST_END 8557d5236cb19c59648562e13a5915af16c3c38b17c8773715ecbc37c8263fef

// TSZ_INLINE_TEST_BEGIN 8194caff62c5a2d0b228e2cad88082337aec018b90d67a1cd91859cffd19d0b9 1734 uri_to_file_name_preserves_non_file_uris
    #[test]
    fn uri_to_file_name_preserves_non_file_uris() {
        assert_eq!(
            LspServer::uri_to_file_name("untitled:Untitled-1"),
            "untitled:Untitled-1"
        );
    }
// TSZ_INLINE_TEST_END 8194caff62c5a2d0b228e2cad88082337aec018b90d67a1cd91859cffd19d0b9

// TSZ_INLINE_TEST_BEGIN 6b18264eb3a58f7d29594d6e3698cb0c8ba0fcf9a53af1fb2f52bc474a1feeea 1742 file_name_to_uri_percent_encodes_file_paths
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
// TSZ_INLINE_TEST_END 6b18264eb3a58f7d29594d6e3698cb0c8ba0fcf9a53af1fb2f52bc474a1feeea

// TSZ_INLINE_TEST_BEGIN 127b6b1f7a8cfaa1c86e943e5aff666b4b87c9166aa31cd86b9139ef995811f2 1758 uri_conversion_round_trips_encoded_absolute_paths
    #[test]
    fn uri_conversion_round_trips_encoded_absolute_paths() {
        let file_name = "/private/tmp/tsz lsp uri current/src/a#b%.ts";
        let uri = LspServer::file_name_to_uri(file_name);

        assert_eq!(LspServer::uri_to_file_name(&uri), file_name);
    }
// TSZ_INLINE_TEST_END 127b6b1f7a8cfaa1c86e943e5aff666b4b87c9166aa31cd86b9139ef995811f2

// TSZ_INLINE_TEST_BEGIN 854c76de6a5523e83a54948b71d225c44e6c06e5ef0051e555963fa86084efbe 1766 initialize_decodes_percent_encoded_workspace_root_uri
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
// TSZ_INLINE_TEST_END 854c76de6a5523e83a54948b71d225c44e6c06e5ef0051e555963fa86084efbe

// TSZ_INLINE_TEST_BEGIN 429e7d4ec2017074847b3ba1a48677116b479f2a596f7de819710f767f6aa0ee 1782 dispatch_handles_known_notifications_without_response
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
// TSZ_INLINE_TEST_END 429e7d4ec2017074847b3ba1a48677116b479f2a596f7de819710f767f6aa0ee

// TSZ_INLINE_TEST_BEGIN 671d94342b296537fbe27de3a4cc381d4e68ed331625af93ab31d91da3e6dd30 1795 dispatch_reports_unknown_request_methods
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
// TSZ_INLINE_TEST_END 671d94342b296537fbe27de3a4cc381d4e68ed331625af93ab31d91da3e6dd30

// TSZ_INLINE_TEST_BEGIN fbc9cf1ad9bec6a2722fe66b97ff66816c0c6f1dede44de79986a5113efe9828 1812 dispatch_ignores_methodless_response_messages
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
// TSZ_INLINE_TEST_END fbc9cf1ad9bec6a2722fe66b97ff66816c0c6f1dede44de79986a5113efe9828

// TSZ_INLINE_TEST_BEGIN b22de29cb36ca027bcac396ff5f9f67499c319da7555985387a1178d31ed10f6 1827 apply_code_action_enqueues_workspace_apply_edit_as_request
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
// TSZ_INLINE_TEST_END b22de29cb36ca027bcac396ff5f9f67499c319da7555985387a1178d31ed10f6
