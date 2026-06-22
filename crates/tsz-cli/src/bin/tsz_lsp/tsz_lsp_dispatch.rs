use super::*;

enum JsonRpcMessageLifecycle {
    Notification {
        method: String,
        params: Option<Value>,
    },
    Request {
        id: Option<Value>,
        method: String,
        params: Option<Value>,
    },
    Response,
}

impl JsonRpcMessageLifecycle {
    fn classify(msg: JsonRpcMessage) -> Self {
        let Some(method) = msg.method else {
            return Self::Response;
        };

        if LspServer::is_notification_method(&method) {
            return Self::Notification {
                method,
                params: msg.params,
            };
        }

        Self::Request {
            id: msg.id,
            method,
            params: msg.params,
        }
    }
}

impl LspServer {
    // ─── Message dispatch ───────────────────────────────────────────────

    pub(super) fn handle_message(&mut self, msg: JsonRpcMessage) -> Option<JsonRpcResponse> {
        match JsonRpcMessageLifecycle::classify(msg) {
            JsonRpcMessageLifecycle::Notification { method, params } => {
                self.handle_notification_method(&method, params);
                None
            }
            JsonRpcMessageLifecycle::Request { id, method, params } => {
                self.handle_request_message(id, &method, params)
            }
            JsonRpcMessageLifecycle::Response => None,
        }
    }

    fn handle_request_message(
        &mut self,
        id: Option<Value>,
        method: &str,
        params: Option<Value>,
    ) -> Option<JsonRpcResponse> {
        if let Some(cancelled_id) = self.take_cancelled_request_id(&id) {
            return Some(self.error_response(
                Some(cancelled_id),
                -32800,
                "Request cancelled".to_string(),
            ));
        }

        // When memory-pressure eviction is enabled, a request may target a file
        // that was evicted while cold. Rehydrate it from disk before dispatch so
        // the feature handlers see it. No-op (not even a map lookup) by default.
        if self.project.memory_budget_bytes().is_some()
            && let Some(uri) = Self::extract_uri(&params)
        {
            self.project
                .ensure_file_loaded(&Self::uri_to_file_name(&uri));
        }

        self.handle_request_method(id, method, params)
    }

    fn take_cancelled_request_id(&mut self, id: &Option<Value>) -> Option<Value> {
        if !self.is_cancelled(id) {
            return None;
        }

        if let Some(id_val) = id {
            let id_str = match id_val {
                Value::Number(n) => n.to_string(),
                Value::String(s) => s.clone(),
                _ => String::new(),
            };
            self.cancelled_requests.remove(&id_str);
            Some(id_val.clone())
        } else {
            None
        }
    }

    fn handle_request_method(
        &mut self,
        id: Option<Value>,
        method: &str,
        params: Option<Value>,
    ) -> Option<JsonRpcResponse> {
        match method {
            "initialize" => {
                let result = self.handle_initialize(params.as_ref());
                Some(self.success_response(id, result))
            }
            "shutdown" => {
                self.shutdown_requested = true;
                Some(self.success_response(id, Value::Null))
            }

            // ── Language features ───────────────────────────────────────
            "textDocument/hover" => {
                let r = self.handle_hover(params);
                Some(self.make_response(id, r))
            }
            "textDocument/completion" => {
                let r = self.handle_completion(params);
                Some(self.make_response(id, r))
            }
            "completionItem/resolve" => {
                let r = self.handle_completion_resolve(params);
                Some(self.make_response(id, r))
            }
            "textDocument/definition" | "textDocument/declaration" => {
                let r = self.handle_definition(params);
                Some(self.make_response(id, r))
            }
            "textDocument/typeDefinition" => {
                let r = self.handle_type_definition(params);
                Some(self.make_response(id, r))
            }
            "textDocument/references" => {
                let r = self.handle_references(params);
                Some(self.make_response(id, r))
            }
            "textDocument/implementation" => {
                let r = self.handle_implementation(params);
                Some(self.make_response(id, r))
            }
            "textDocument/documentSymbol" => {
                let r = self.handle_document_symbol(params);
                Some(self.make_response(id, r))
            }
            "textDocument/formatting" => {
                let r = self.handle_formatting(params);
                Some(self.make_response(id, r))
            }
            "textDocument/rename" => {
                let r = self.handle_rename(params);
                Some(self.make_response(id, r))
            }
            "textDocument/prepareRename" => {
                let r = self.handle_prepare_rename(params);
                Some(self.make_response(id, r))
            }
            "textDocument/codeAction" => {
                let r = self.handle_code_action(params);
                Some(self.make_response(id, r))
            }
            "textDocument/codeLens" => {
                let r = self.handle_code_lens(params);
                Some(self.make_response(id, r))
            }
            "codeLens/resolve" => {
                let r = self.handle_code_lens_resolve(params);
                Some(self.make_response(id, r))
            }
            "textDocument/selectionRange" => {
                let r = self.handle_selection_range(params);
                Some(self.make_response(id, r))
            }
            "textDocument/foldingRange" => {
                let r = self.handle_folding_range(params);
                Some(self.make_response(id, r))
            }
            "textDocument/signatureHelp" => {
                let r = self.handle_signature_help(params);
                Some(self.make_response(id, r))
            }
            "textDocument/semanticTokens/full" => {
                let r = self.handle_semantic_tokens_full(params);
                Some(self.make_response(id, r))
            }
            "textDocument/semanticTokens/range" => {
                let r = self.handle_semantic_tokens_range(params);
                Some(self.make_response(id, r))
            }
            "textDocument/documentHighlight" => {
                let r = self.handle_document_highlight(params);
                Some(self.make_response(id, r))
            }
            "textDocument/inlayHint" => {
                let r = self.handle_inlay_hint(params);
                Some(self.make_response(id, r))
            }
            "inlayHint/resolve" => {
                let r = self.handle_inlay_hint_resolve(params);
                Some(self.make_response(id, r))
            }
            "textDocument/documentColor" => {
                let r = self.handle_document_color(params);
                Some(self.make_response(id, r))
            }
            "textDocument/colorPresentation" => {
                let r = self.handle_color_presentation(params);
                Some(self.make_response(id, r))
            }
            "textDocument/documentLink" => {
                let r = self.handle_document_link(params);
                Some(self.make_response(id, r))
            }
            "textDocument/linkedEditingRange" => {
                let r = self.handle_linked_editing_range(params);
                Some(self.make_response(id, r))
            }
            "textDocument/prepareCallHierarchy" => {
                let r = self.handle_prepare_call_hierarchy(params);
                Some(self.make_response(id, r))
            }
            "callHierarchy/incomingCalls" => {
                let r = self.handle_incoming_calls(params);
                Some(self.make_response(id, r))
            }
            "callHierarchy/outgoingCalls" => {
                let r = self.handle_outgoing_calls(params);
                Some(self.make_response(id, r))
            }
            "textDocument/prepareTypeHierarchy" => {
                let r = self.handle_prepare_type_hierarchy(params);
                Some(self.make_response(id, r))
            }
            "typeHierarchy/supertypes" => {
                let r = self.handle_supertypes(params);
                Some(self.make_response(id, r))
            }
            "typeHierarchy/subtypes" => {
                let r = self.handle_subtypes(params);
                Some(self.make_response(id, r))
            }
            "workspace/symbol" => {
                let r = self.handle_workspace_symbol(params);
                Some(self.make_response(id, r))
            }

            // ── Range formatting ──────────────────────────────────────
            "textDocument/rangeFormatting" => {
                let r = self.handle_range_formatting(params);
                Some(self.make_response(id, r))
            }

            // ── On-type formatting ────────────────────────────────────
            "textDocument/onTypeFormatting" => {
                let r = self.handle_on_type_formatting(params);
                Some(self.make_response(id, r))
            }

            // ── Execute command ────────────────────────────────────────
            "workspace/executeCommand" => {
                let r = self.handle_execute_command(params);
                Some(self.make_response(id, r))
            }

            // ── Diagnostic pull model (LSP 3.17) ─────────────────
            "textDocument/diagnostic" => {
                let r = self.handle_document_diagnostic(params);
                Some(self.make_response(id, r))
            }
            "workspace/diagnostic" => {
                let r = self.handle_workspace_diagnostic(params);
                Some(self.make_response(id, r))
            }

            // ── File operations ─────────────────────────────────────
            "workspace/willRenameFiles" => {
                let r = self.handle_will_rename_files(params);
                Some(self.make_response(id, r))
            }
            "workspace/willCreateFiles" => {
                // Acknowledge but no edits needed for file creation
                Some(self.success_response(id, Value::Null))
            }
            "workspace/willDeleteFiles" => {
                // Acknowledge but no edits needed for file deletion
                Some(self.success_response(id, Value::Null))
            }

            // Unknown request → method not found
            method if id.is_some() => {
                Some(self.error_response(id, -32601, format!("Method not found: {method}")))
            }
            _ => None,
        }
    }
}
