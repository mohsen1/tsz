use super::*;

impl LspServer {
    // ─── Message dispatch ───────────────────────────────────────────────

    pub(super) fn handle_message(&mut self, msg: JsonRpcMessage) -> Option<JsonRpcResponse> {
        let method = msg.method.as_deref();
        let id = msg.id.clone();

        if let Some(method) = method
            && self.handle_notification_method(method, msg.params.clone())
        {
            return None;
        }

        // Check if this request was already cancelled
        if self.is_cancelled(&id)
            && let Some(id_val) = id
        {
            let id_str = match &id_val {
                Value::Number(n) => n.to_string(),
                Value::String(s) => s.clone(),
                _ => String::new(),
            };
            self.cancelled_requests.remove(&id_str);
            return Some(self.error_response(
                Some(id_val),
                -32800,
                "Request cancelled".to_string(),
            ));
        }

        match method {
            Some("initialize") => {
                let result = self.handle_initialize(msg.params.as_ref());
                Some(self.success_response(id, result))
            }
            Some("shutdown") => {
                self.shutdown_requested = true;
                Some(self.success_response(id, Value::Null))
            }

            // ── Language features ───────────────────────────────────────
            Some("textDocument/hover") => {
                let r = self.handle_hover(msg.params);
                Some(self.make_response(id, r))
            }
            Some("textDocument/completion") => {
                let r = self.handle_completion(msg.params);
                Some(self.make_response(id, r))
            }
            Some("completionItem/resolve") => {
                let r = self.handle_completion_resolve(msg.params);
                Some(self.make_response(id, r))
            }
            Some("textDocument/definition") | Some("textDocument/declaration") => {
                let r = self.handle_definition(msg.params);
                Some(self.make_response(id, r))
            }
            Some("textDocument/typeDefinition") => {
                let r = self.handle_type_definition(msg.params);
                Some(self.make_response(id, r))
            }
            Some("textDocument/references") => {
                let r = self.handle_references(msg.params);
                Some(self.make_response(id, r))
            }
            Some("textDocument/implementation") => {
                let r = self.handle_implementation(msg.params);
                Some(self.make_response(id, r))
            }
            Some("textDocument/documentSymbol") => {
                let r = self.handle_document_symbol(msg.params);
                Some(self.make_response(id, r))
            }
            Some("textDocument/formatting") => {
                let r = self.handle_formatting(msg.params);
                Some(self.make_response(id, r))
            }
            Some("textDocument/rename") => {
                let r = self.handle_rename(msg.params);
                Some(self.make_response(id, r))
            }
            Some("textDocument/prepareRename") => {
                let r = self.handle_prepare_rename(msg.params);
                Some(self.make_response(id, r))
            }
            Some("textDocument/codeAction") => {
                let r = self.handle_code_action(msg.params);
                Some(self.make_response(id, r))
            }
            Some("textDocument/codeLens") => {
                let r = self.handle_code_lens(msg.params);
                Some(self.make_response(id, r))
            }
            Some("codeLens/resolve") => {
                let r = self.handle_code_lens_resolve(msg.params);
                Some(self.make_response(id, r))
            }
            Some("textDocument/selectionRange") => {
                let r = self.handle_selection_range(msg.params);
                Some(self.make_response(id, r))
            }
            Some("textDocument/foldingRange") => {
                let r = self.handle_folding_range(msg.params);
                Some(self.make_response(id, r))
            }
            Some("textDocument/signatureHelp") => {
                let r = self.handle_signature_help(msg.params);
                Some(self.make_response(id, r))
            }
            Some("textDocument/semanticTokens/full") => {
                let r = self.handle_semantic_tokens_full(msg.params);
                Some(self.make_response(id, r))
            }
            Some("textDocument/semanticTokens/range") => {
                let r = self.handle_semantic_tokens_range(msg.params);
                Some(self.make_response(id, r))
            }
            Some("textDocument/documentHighlight") => {
                let r = self.handle_document_highlight(msg.params);
                Some(self.make_response(id, r))
            }
            Some("textDocument/inlayHint") => {
                let r = self.handle_inlay_hint(msg.params);
                Some(self.make_response(id, r))
            }
            Some("inlayHint/resolve") => {
                let r = self.handle_inlay_hint_resolve(msg.params);
                Some(self.make_response(id, r))
            }
            Some("textDocument/documentColor") => {
                let r = self.handle_document_color(msg.params);
                Some(self.make_response(id, r))
            }
            Some("textDocument/colorPresentation") => {
                let r = self.handle_color_presentation(msg.params);
                Some(self.make_response(id, r))
            }
            Some("textDocument/documentLink") => {
                let r = self.handle_document_link(msg.params);
                Some(self.make_response(id, r))
            }
            Some("textDocument/linkedEditingRange") => {
                let r = self.handle_linked_editing_range(msg.params);
                Some(self.make_response(id, r))
            }
            Some("textDocument/prepareCallHierarchy") => {
                let r = self.handle_prepare_call_hierarchy(msg.params);
                Some(self.make_response(id, r))
            }
            Some("callHierarchy/incomingCalls") => {
                let r = self.handle_incoming_calls(msg.params);
                Some(self.make_response(id, r))
            }
            Some("callHierarchy/outgoingCalls") => {
                let r = self.handle_outgoing_calls(msg.params);
                Some(self.make_response(id, r))
            }
            Some("textDocument/prepareTypeHierarchy") => {
                let r = self.handle_prepare_type_hierarchy(msg.params);
                Some(self.make_response(id, r))
            }
            Some("typeHierarchy/supertypes") => {
                let r = self.handle_supertypes(msg.params);
                Some(self.make_response(id, r))
            }
            Some("typeHierarchy/subtypes") => {
                let r = self.handle_subtypes(msg.params);
                Some(self.make_response(id, r))
            }
            Some("workspace/symbol") => {
                let r = self.handle_workspace_symbol(msg.params);
                Some(self.make_response(id, r))
            }

            // ── Range formatting ──────────────────────────────────────
            Some("textDocument/rangeFormatting") => {
                let r = self.handle_range_formatting(msg.params);
                Some(self.make_response(id, r))
            }

            // ── On-type formatting ────────────────────────────────────
            Some("textDocument/onTypeFormatting") => {
                let r = self.handle_on_type_formatting(msg.params);
                Some(self.make_response(id, r))
            }

            // ── Execute command ────────────────────────────────────────
            Some("workspace/executeCommand") => {
                let r = self.handle_execute_command(msg.params);
                Some(self.make_response(id, r))
            }

            // ── Diagnostic pull model (LSP 3.17) ─────────────────
            Some("textDocument/diagnostic") => {
                let r = self.handle_document_diagnostic(msg.params);
                Some(self.make_response(id, r))
            }
            Some("workspace/diagnostic") => {
                let r = self.handle_workspace_diagnostic(msg.params);
                Some(self.make_response(id, r))
            }

            // ── File operations ─────────────────────────────────────
            Some("workspace/willRenameFiles") => {
                let r = self.handle_will_rename_files(msg.params);
                Some(self.make_response(id, r))
            }
            Some("workspace/willCreateFiles") => {
                // Acknowledge but no edits needed for file creation
                Some(self.success_response(id, Value::Null))
            }
            Some("workspace/willDeleteFiles") => {
                // Acknowledge but no edits needed for file deletion
                Some(self.success_response(id, Value::Null))
            }

            // Unknown request → method not found
            Some(method) if id.is_some() => {
                Some(self.error_response(id, -32601, format!("Method not found: {method}")))
            }
            _ => None,
        }
    }
}
