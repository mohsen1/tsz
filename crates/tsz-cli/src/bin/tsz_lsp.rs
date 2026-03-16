#![recursion_limit = "256"]
//! TSZ Language Server Protocol (LSP) Server
//!
//! A fully-featured LSP server for TypeScript/JavaScript backed by the
//! `tsz-lsp` crate's `Project` infrastructure for multi-file state management.
//!
//! Usage:
//!   tsz-lsp              # Start LSP server (default: stdio)
//!   tsz-lsp --version    # Show version
//!   tsz-lsp --help       # Show help
//!
//! Supported LSP features:
//! - textDocument/hover
//! - textDocument/completion (with auto-import additionalTextEdits)
//! - completionItem/resolve (lazy documentation loading)
//! - textDocument/definition
//! - textDocument/declaration
//! - textDocument/typeDefinition
//! - textDocument/references
//! - textDocument/implementation
//! - textDocument/documentSymbol
//! - textDocument/formatting
//! - textDocument/rangeFormatting
//! - textDocument/onTypeFormatting
//! - textDocument/rename
//! - textDocument/prepareRename
//! - textDocument/codeAction (quickfix, refactor, source.organizeImports)
//! - textDocument/codeLens
//! - codeLens/resolve
//! - textDocument/selectionRange
//! - textDocument/foldingRange
//! - textDocument/signatureHelp
//! - textDocument/semanticTokens/full
//! - textDocument/semanticTokens/range
//! - textDocument/documentHighlight
//! - textDocument/inlayHint
//! - inlayHint/resolve
//! - textDocument/documentLink
//! - textDocument/documentColor
//! - textDocument/colorPresentation
//! - textDocument/linkedEditingRange
//! - textDocument/diagnostic (pull model, LSP 3.17)
//! - workspace/diagnostic (pull model, LSP 3.17)
//! - textDocument/publishDiagnostics (server-initiated, with stale dependent updates)
//! - callHierarchy/incomingCalls
//! - callHierarchy/outgoingCalls
//! - textDocument/prepareCallHierarchy
//! - typeHierarchy/supertypes
//! - typeHierarchy/subtypes
//! - textDocument/prepareTypeHierarchy
//! - workspace/symbol
//! - workspace/willRenameFiles (auto-update imports on file rename)
//! - workspace/didRenameFiles (auto-update imports on file rename)
//! - workspace/didChangeConfiguration
//! - workspace/didChangeWatchedFiles
//! - workspace/executeCommand

use anyhow::{Context, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use tracing::{debug, info, trace};

use tsz::lsp::{
    CompletionItemData, CompletionItemKind, FormattingOptions, Position, Project, Range,
};

/// TSZ Language Server
#[derive(Parser, Debug)]
#[command(
    name = "tsz-lsp",
    version,
    about = "TSZ Language Server Protocol Server"
)]
struct Args {
    /// Communication mode (currently only stdio is supported)
    #[arg(long, default_value = "stdio")]
    mode: String,

    /// Log file path for debugging
    #[arg(long)]
    log_file: Option<std::path::PathBuf>,

    /// Enable verbose logging
    #[arg(long, short)]
    verbose: bool,
}

// =============================================================================
// JSON-RPC Types
// =============================================================================

#[derive(Debug, Deserialize)]
struct JsonRpcMessage {
    id: Option<Value>,
    method: Option<String>,
    params: Option<Value>,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

/// A JSON-RPC notification (no id field).
#[derive(Debug, Serialize)]
struct JsonRpcNotification {
    jsonrpc: String,
    method: String,
    params: Value,
}

// =============================================================================
// LSP Server State
// =============================================================================

struct LspServer {
    /// Multi-file project state backed by the tsz-lsp infrastructure.
    project: Project,
    /// Whether the server has been initialized.
    initialized: bool,
    /// Shutdown requested.
    shutdown_requested: bool,
    /// Pending diagnostics notifications to send after handling a request.
    pending_notifications: Vec<JsonRpcNotification>,
}

impl LspServer {
    fn new() -> Self {
        Self {
            project: Project::new(),
            initialized: false,
            shutdown_requested: false,
            pending_notifications: Vec::new(),
        }
    }

    // ─── Message dispatch ───────────────────────────────────────────────

    fn handle_message(&mut self, msg: JsonRpcMessage) -> Option<JsonRpcResponse> {
        let method = msg.method.as_deref();
        let id = msg.id.clone();

        match method {
            Some("initialize") => Some(self.success_response(id, self.handle_initialize())),
            Some("initialized") => {
                self.initialized = true;
                None
            }
            Some("shutdown") => {
                self.shutdown_requested = true;
                Some(self.success_response(id, Value::Null))
            }
            Some("exit") => {
                std::process::exit(i32::from(!self.shutdown_requested));
            }

            // ── Document lifecycle ──────────────────────────────────────
            Some("textDocument/didOpen") => {
                self.handle_did_open(msg.params);
                None
            }
            Some("textDocument/didChange") => {
                self.handle_did_change(msg.params);
                None
            }
            Some("textDocument/didClose") => {
                self.handle_did_close(msg.params);
                None
            }
            Some("textDocument/didSave") => {
                self.handle_did_save(msg.params);
                None
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
            Some("textDocument/definition") => {
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

            // ── Declaration (same as definition for TS) ───────────────
            Some("textDocument/declaration") => {
                let r = self.handle_definition(msg.params);
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

            // ── Workspace notifications ───────────────────────────────
            Some("workspace/didChangeConfiguration") => {
                self.handle_did_change_configuration(msg.params);
                None
            }
            Some("workspace/didChangeWatchedFiles") => {
                self.handle_did_change_watched_files(msg.params);
                None
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
            Some("workspace/didRenameFiles") => {
                self.handle_did_rename_files(msg.params);
                None
            }

            // Unknown request → method not found
            Some(method) if id.is_some() => {
                Some(self.error_response(id, -32601, format!("Method not found: {method}")))
            }
            _ => None,
        }
    }

    // ─── Response helpers ───────────────────────────────────────────────

    fn success_response(&self, id: Option<Value>, result: Value) -> JsonRpcResponse {
        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: id.unwrap_or(Value::Null),
            result: Some(result),
            error: None,
        }
    }

    fn make_response(&self, id: Option<Value>, result: Result<Value>) -> JsonRpcResponse {
        match result {
            Ok(value) => self.success_response(id, value),
            Err(e) => JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: id.unwrap_or(Value::Null),
                result: None,
                error: Some(JsonRpcError {
                    code: -32603,
                    message: e.to_string(),
                    data: None,
                }),
            },
        }
    }

    fn error_response(&self, id: Option<Value>, code: i32, message: String) -> JsonRpcResponse {
        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: id.unwrap_or(Value::Null),
            result: None,
            error: Some(JsonRpcError {
                code,
                message,
                data: None,
            }),
        }
    }

    // ─── Param extraction helpers ───────────────────────────────────────

    fn extract_uri(params: &Option<Value>) -> Option<String> {
        params
            .as_ref()?
            .get("textDocument")?
            .get("uri")?
            .as_str()
            .map(String::from)
    }

    fn extract_position(params: &Option<Value>) -> Option<(String, Position)> {
        let uri = Self::extract_uri(params)?;
        let pos = params.as_ref()?.get("position")?;
        let line = pos.get("line")?.as_u64()? as u32;
        let character = pos.get("character")?.as_u64()? as u32;
        Some((uri, Position::new(line, character)))
    }

    fn extract_range(params: &Option<Value>, key: &str) -> Option<Range> {
        let r = params.as_ref()?.get(key)?;
        let start = r.get("start")?;
        let end = r.get("end")?;
        Some(Range::new(
            Position::new(
                start.get("line")?.as_u64()? as u32,
                start.get("character")?.as_u64()? as u32,
            ),
            Position::new(
                end.get("line")?.as_u64()? as u32,
                end.get("character")?.as_u64()? as u32,
            ),
        ))
    }

    /// Convert a file URI to the internal file name used by Project.
    fn uri_to_file_name(uri: &str) -> String {
        // Strip file:// prefix if present, keeping the path
        if let Some(path) = uri.strip_prefix("file://") {
            path.to_string()
        } else {
            uri.to_string()
        }
    }

    /// Convert an internal file name back to a URI.
    fn file_name_to_uri(file_name: &str) -> String {
        if file_name.starts_with('/') {
            format!("file://{file_name}")
        } else {
            file_name.to_string()
        }
    }

    // ─── Diagnostic publishing ──────────────────────────────────────────

    fn publish_diagnostics(&mut self, uri: &str) {
        let file_name = Self::uri_to_file_name(uri);
        if let Some(diagnostics) = self.project.get_diagnostics(&file_name) {
            let lsp_diags: Vec<Value> = diagnostics.iter().map(Self::diagnostic_to_json).collect();

            self.pending_notifications.push(JsonRpcNotification {
                jsonrpc: "2.0".to_string(),
                method: "textDocument/publishDiagnostics".to_string(),
                params: serde_json::json!({
                    "uri": uri,
                    "diagnostics": lsp_diags,
                }),
            });
        }
    }

    fn diagnostic_to_json(d: &tsz::lsp::LspDiagnostic) -> Value {
        let mut diag = serde_json::json!({
            "range": Self::range_to_json(&d.range),
            "message": d.message,
        });
        if let Some(severity) = d.severity {
            diag["severity"] = Value::from(severity as u8);
        }
        if let Some(code) = d.code {
            diag["code"] = Value::from(code);
        }
        if let Some(ref source) = d.source {
            diag["source"] = Value::from(source.as_str());
        } else {
            diag["source"] = Value::from("tsz");
        }
        if let Some(ref related) = d.related_information {
            let ri: Vec<Value> = related
                .iter()
                .map(|r| {
                    serde_json::json!({
                        "location": {
                            "uri": Self::file_name_to_uri(&r.location.file_path),
                            "range": Self::range_to_json(&r.location.range),
                        },
                        "message": r.message,
                    })
                })
                .collect();
            diag["relatedInformation"] = Value::Array(ri);
        }
        // Tags for unnecessary/deprecated
        let mut tags = Vec::new();
        if d.reports_unnecessary == Some(true) {
            tags.push(Value::from(1)); // Unnecessary
        }
        if d.reports_deprecated == Some(true) {
            tags.push(Value::from(2)); // Deprecated
        }
        if !tags.is_empty() {
            diag["tags"] = Value::Array(tags);
        }
        diag
    }

    // ─── JSON conversion helpers ────────────────────────────────────────

    fn position_to_json(pos: &Position) -> Value {
        serde_json::json!({ "line": pos.line, "character": pos.character })
    }

    fn range_to_json(range: &Range) -> Value {
        serde_json::json!({
            "start": Self::position_to_json(&range.start),
            "end": Self::position_to_json(&range.end),
        })
    }

    fn location_to_json(loc: &tsz::lsp::Location) -> Value {
        serde_json::json!({
            "uri": Self::file_name_to_uri(&loc.file_path),
            "range": Self::range_to_json(&loc.range),
        })
    }

    const fn completion_kind_to_lsp(kind: CompletionItemKind) -> u32 {
        match kind {
            CompletionItemKind::Variable
            | CompletionItemKind::Let
            | CompletionItemKind::Parameter => 6, // Variable
            CompletionItemKind::Const => 21, // Constant
            CompletionItemKind::Function => 3,
            CompletionItemKind::Class => 7,
            CompletionItemKind::Method => 2,
            CompletionItemKind::Property => 10,
            CompletionItemKind::Keyword => 14,
            CompletionItemKind::Interface | CompletionItemKind::TypeAlias => 8, // Interface
            CompletionItemKind::Enum => 13,
            CompletionItemKind::Module | CompletionItemKind::Alias => 9, // Module
            CompletionItemKind::TypeParameter => 25,
            CompletionItemKind::Constructor => 4,
        }
    }

    const fn symbol_kind_to_lsp(kind: tsz::lsp::SymbolKind) -> u32 {
        kind as u32
    }

    // ─── Initialize ─────────────────────────────────────────────────────

    fn handle_initialize(&self) -> Value {
        serde_json::json!({
            "capabilities": {
                "textDocumentSync": {
                    "openClose": true,
                    "change": 2,  // Incremental sync
                    "save": { "includeText": false }
                },
                "hoverProvider": true,
                "completionProvider": {
                    "triggerCharacters": [".", "<", "/", "\"", "'", "`", "@"],
                    "resolveProvider": true,
                },
                "definitionProvider": true,
                "declarationProvider": true,
                "typeDefinitionProvider": true,
                "referencesProvider": true,
                "implementationProvider": true,
                "documentSymbolProvider": true,
                "documentFormattingProvider": true,
                "documentRangeFormattingProvider": true,
                "documentOnTypeFormattingProvider": {
                    "firstTriggerCharacter": ";",
                    "moreTriggerCharacter": ["}", "\n"]
                },
                "renameProvider": { "prepareProvider": true },
                "codeActionProvider": {
                    "codeActionKinds": [
                        "quickfix",
                        "refactor",
                        "refactor.extract",
                        "source",
                        "source.organizeImports",
                        "source.fixAll"
                    ]
                },
                "codeLensProvider": { "resolveProvider": true },
                "selectionRangeProvider": true,
                "foldingRangeProvider": true,
                "signatureHelpProvider": {
                    "triggerCharacters": ["(", ","],
                    "retriggerCharacters": [")"],
                },
                "semanticTokensProvider": {
                    "legend": {
                        "tokenTypes": [
                            "namespace", "type", "class", "enum", "interface",
                            "struct", "typeParameter", "parameter", "variable",
                            "property", "enumMember", "event", "function", "method",
                            "macro", "keyword", "modifier", "comment", "string",
                            "number", "regexp", "operator"
                        ],
                        "tokenModifiers": [
                            "declaration", "definition", "readonly", "static",
                            "deprecated", "abstract", "async", "modification",
                            "documentation", "defaultLibrary"
                        ]
                    },
                    "full": true,
                    "range": true,
                },
                "documentHighlightProvider": true,
                "inlayHintProvider": { "resolveProvider": true },
                "colorProvider": true,
                "documentLinkProvider": { "resolveProvider": false },
                "linkedEditingRangeProvider": true,
                "callHierarchyProvider": true,
                "typeHierarchyProvider": true,
                "workspaceSymbolProvider": true,
                "diagnosticProvider": {
                    "interFileDependencies": true,
                    "workspaceDiagnostics": true,
                },
                "executeCommandProvider": {
                    "commands": ["tsz.organizeImports", "tsz.applyCodeAction"]
                },
                "workspace": {
                    "workspaceFolders": {
                        "supported": true,
                        "changeNotifications": true
                    },
                    "fileOperations": {
                        "willRename": {
                            "filters": [{
                                "scheme": "file",
                                "pattern": { "glob": "**/*.{ts,tsx,js,jsx,mts,cts,mjs,cjs}" }
                            }]
                        },
                        "didRename": {
                            "filters": [{
                                "scheme": "file",
                                "pattern": { "glob": "**/*.{ts,tsx,js,jsx,mts,cts,mjs,cjs}" }
                            }]
                        }
                    }
                }
            },
            "serverInfo": {
                "name": "tsz-lsp",
                "version": env!("CARGO_PKG_VERSION")
            }
        })
    }

    // ─── Document lifecycle ─────────────────────────────────────────────

    fn handle_did_open(&mut self, params: Option<Value>) {
        if let Some((uri, text)) = (|| {
            let p = params.as_ref()?;
            let td = p.get("textDocument")?;
            let uri = td.get("uri")?.as_str()?.to_string();
            let text = td.get("text")?.as_str()?.to_string();
            Some((uri, text))
        })() {
            let file_name = Self::uri_to_file_name(&uri);
            self.project.set_file(file_name, text);
            self.publish_diagnostics(&uri);
        }
    }

    fn handle_did_change(&mut self, params: Option<Value>) {
        if let Some(p) = params.as_ref() {
            let uri = match p
                .get("textDocument")
                .and_then(|t| t.get("uri"))
                .and_then(|u| u.as_str())
            {
                Some(u) => u.to_string(),
                None => return,
            };
            let file_name = Self::uri_to_file_name(&uri);
            let changes = match p.get("contentChanges").and_then(|c| c.as_array()) {
                Some(c) => c,
                None => return,
            };

            if changes.is_empty() {
                return;
            }

            // Try incremental sync first: apply each change as a text edit
            let has_range_changes = changes.iter().any(|c| c.get("range").is_some());

            if has_range_changes {
                // Incremental: apply text edits via Project::update_file
                let edits: Vec<tsz::lsp::FormattingTextEdit> = changes
                    .iter()
                    .filter_map(|change| {
                        let range_val = change.get("range")?;
                        let start = range_val.get("start")?;
                        let end = range_val.get("end")?;
                        let range = Range::new(
                            Position::new(
                                start.get("line")?.as_u64()? as u32,
                                start.get("character")?.as_u64()? as u32,
                            ),
                            Position::new(
                                end.get("line")?.as_u64()? as u32,
                                end.get("character")?.as_u64()? as u32,
                            ),
                        );
                        let text = change.get("text")?.as_str()?.to_string();
                        Some(tsz::lsp::FormattingTextEdit {
                            range,
                            new_text: text,
                        })
                    })
                    .collect();

                if !edits.is_empty() {
                    // Convert FormattingTextEdit to TextEdit for update_file
                    let text_edits: Vec<tsz::lsp::TextEdit> = edits
                        .into_iter()
                        .map(|e| tsz::lsp::TextEdit {
                            range: e.range,
                            new_text: e.new_text,
                        })
                        .collect();
                    self.project.update_file(&file_name, &text_edits);
                }
            } else {
                // Full sync fallback: take the last content change
                if let Some(text) = changes
                    .last()
                    .and_then(|c| c.get("text"))
                    .and_then(|t| t.as_str())
                {
                    self.project.set_file(file_name.clone(), text.to_string());
                }
            }

            // Publish diagnostics for the changed file
            self.publish_diagnostics(&uri);

            // Publish stale diagnostics for dependent files
            self.publish_stale_diagnostics();
        }
    }

    fn handle_did_save(&mut self, params: Option<Value>) {
        // On save, re-publish diagnostics for the saved file and dependents
        if let Some(uri) = Self::extract_uri(&params) {
            self.publish_diagnostics(&uri);
            self.publish_stale_diagnostics();
        }
    }

    /// Publish diagnostics for all files that have been marked stale
    /// (e.g., dependents of a changed file).
    fn publish_stale_diagnostics(&mut self) {
        let stale = self.project.get_stale_diagnostics();
        for (file_name, diagnostics) in stale {
            let uri = Self::file_name_to_uri(&file_name);
            let lsp_diags: Vec<Value> = diagnostics.iter().map(Self::diagnostic_to_json).collect();
            self.pending_notifications.push(JsonRpcNotification {
                jsonrpc: "2.0".to_string(),
                method: "textDocument/publishDiagnostics".to_string(),
                params: serde_json::json!({
                    "uri": uri,
                    "diagnostics": lsp_diags,
                }),
            });
        }
    }

    fn handle_did_close(&mut self, params: Option<Value>) {
        if let Some(uri) = Self::extract_uri(&params) {
            let file_name = Self::uri_to_file_name(&uri);
            self.project.remove_file(&file_name);
            // Clear diagnostics for closed file
            self.pending_notifications.push(JsonRpcNotification {
                jsonrpc: "2.0".to_string(),
                method: "textDocument/publishDiagnostics".to_string(),
                params: serde_json::json!({
                    "uri": uri,
                    "diagnostics": [],
                }),
            });
        }
    }

    // ─── Diagnostic Pull Model ────────────────────────────────────────

    fn handle_document_diagnostic(&mut self, params: Option<Value>) -> Result<Value> {
        let uri = Self::extract_uri(&params).ok_or_else(|| anyhow::anyhow!("Missing uri"))?;
        let file_name = Self::uri_to_file_name(&uri);

        let diagnostics = self.project.get_diagnostics(&file_name).unwrap_or_default();

        let lsp_diags: Vec<Value> = diagnostics.iter().map(Self::diagnostic_to_json).collect();

        Ok(serde_json::json!({
            "kind": "full",
            "items": lsp_diags,
        }))
    }

    fn handle_workspace_diagnostic(&mut self, _params: Option<Value>) -> Result<Value> {
        let mut items = Vec::new();

        let file_names: Vec<String> = self.project.file_names().map(|s| s.to_string()).collect();
        for file_name in &file_names {
            let diagnostics = self.project.get_diagnostics(file_name).unwrap_or_default();

            let lsp_diags: Vec<Value> = diagnostics.iter().map(Self::diagnostic_to_json).collect();

            items.push(serde_json::json!({
                "kind": "full",
                "uri": Self::file_name_to_uri(file_name),
                "items": lsp_diags,
            }));
        }

        Ok(serde_json::json!({
            "items": items,
        }))
    }

    // ─── Hover ──────────────────────────────────────────────────────────

    fn handle_hover(&mut self, params: Option<Value>) -> Result<Value> {
        let (uri, position) = Self::extract_position(&params)
            .ok_or_else(|| anyhow::anyhow!("Missing position params"))?;
        let file_name = Self::uri_to_file_name(&uri);

        let info = match self.project.get_hover(&file_name, position) {
            Some(info) => info,
            None => return Ok(Value::Null),
        };

        // Build LSP Hover response with MarkupContent
        let mut markdown = String::new();
        if !info.display_string.is_empty() {
            markdown.push_str("```typescript\n");
            markdown.push_str(&info.display_string);
            markdown.push_str("\n```");
        }
        if !info.documentation.is_empty() {
            if !markdown.is_empty() {
                markdown.push_str("\n\n---\n\n");
            }
            markdown.push_str(&info.documentation);
        }
        // Append JSDoc tags
        for tag in &info.tags {
            markdown.push_str("\n\n");
            markdown.push_str(&format!("*@{}*", tag.name));
            if !tag.text.is_empty() {
                markdown.push(' ');
                markdown.push_str(&tag.text);
            }
        }

        let mut hover = serde_json::json!({
            "contents": {
                "kind": "markdown",
                "value": markdown,
            }
        });
        if let Some(ref range) = info.range {
            hover["range"] = Self::range_to_json(range);
        }

        Ok(hover)
    }

    // ─── Completion ─────────────────────────────────────────────────────

    fn handle_completion(&mut self, params: Option<Value>) -> Result<Value> {
        let (uri, position) = Self::extract_position(&params)
            .ok_or_else(|| anyhow::anyhow!("Missing position params"))?;
        let file_name = Self::uri_to_file_name(&uri);

        let items = self
            .project
            .get_completions(&file_name, position)
            .unwrap_or_default();

        let lsp_items: Vec<Value> = items
            .iter()
            .map(|item| {
                let mut ci = serde_json::json!({
                    "label": item.label,
                    "kind": Self::completion_kind_to_lsp(item.kind),
                });
                if let Some(ref detail) = item.detail {
                    ci["detail"] = Value::from(detail.as_str());
                }
                if let Some(ref doc) = item.documentation {
                    ci["documentation"] = serde_json::json!({
                        "kind": "markdown",
                        "value": doc,
                    });
                }
                if let Some(ref sort_text) = item.sort_text {
                    ci["sortText"] = Value::from(sort_text.as_str());
                }
                if let Some(ref insert_text) = item.insert_text {
                    ci["insertText"] = Value::from(insert_text.as_str());
                    if item.is_snippet {
                        ci["insertTextFormat"] = Value::from(2); // Snippet format
                    }
                }
                // Auto-import: include additional text edits (e.g., import statements)
                if let Some(ref edits) = item.additional_text_edits {
                    let lsp_edits: Vec<Value> = edits
                        .iter()
                        .map(|edit| {
                            serde_json::json!({
                                "range": Self::range_to_json(&edit.range),
                                "newText": edit.new_text,
                            })
                        })
                        .collect();
                    ci["additionalTextEdits"] = Value::Array(lsp_edits);
                }
                if let Some(ref source) = item.source
                    && item.detail.is_none()
                {
                    ci["detail"] = Value::from(format!("Auto import from '{source}'"));
                }
                // Attach typed resolve data so completionItem/resolve can look up docs
                if let Some(ref data) = item.data {
                    ci["data"] = serde_json::to_value(data).unwrap_or(Value::Null);
                }
                ci
            })
            .collect();

        Ok(serde_json::json!({
            "isIncomplete": false,
            "items": lsp_items,
        }))
    }

    fn handle_completion_resolve(&mut self, params: Option<Value>) -> Result<Value> {
        // The params IS the completion item itself
        let mut item = params.unwrap_or(Value::Null);

        if let Some(data_val) = item.get("data").cloned() {
            let label = item
                .get("label")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();

            // Deserialize the typed CompletionItemData
            if let Ok(data) = serde_json::from_value::<CompletionItemData>(data_val) {
                if let Some((detail, documentation)) =
                    self.project.resolve_completion_with_data(&data, &label)
                {
                    if let Some(detail) = detail {
                        item["detail"] = Value::from(detail);
                    }
                    if let Some(doc) = documentation {
                        item["documentation"] = serde_json::json!({
                            "kind": "markdown",
                            "value": doc,
                        });
                    }
                }
            }
        }

        Ok(item)
    }

    // ─── Definition ─────────────────────────────────────────────────────

    fn handle_definition(&mut self, params: Option<Value>) -> Result<Value> {
        let (uri, position) = Self::extract_position(&params)
            .ok_or_else(|| anyhow::anyhow!("Missing position params"))?;
        let file_name = Self::uri_to_file_name(&uri);

        match self.project.get_definition(&file_name, position) {
            Some(locations) => {
                let locs: Vec<Value> = locations.iter().map(Self::location_to_json).collect();
                Ok(Value::Array(locs))
            }
            None => Ok(Value::Null),
        }
    }

    // ─── Type Definition ────────────────────────────────────────────────

    fn handle_type_definition(&mut self, params: Option<Value>) -> Result<Value> {
        let (uri, position) = Self::extract_position(&params)
            .ok_or_else(|| anyhow::anyhow!("Missing position params"))?;
        let file_name = Self::uri_to_file_name(&uri);

        // Project doesn't wrap type definition yet; use the ProjectFile accessor
        if let Some(file) = self.project.file(&file_name) {
            let provider = tsz::lsp::TypeDefinitionProvider::new(
                file.arena(),
                file.binder(),
                file.line_map(),
                file_name,
                file.source_text(),
            );
            if let Some(locations) = provider.get_type_definition(file.root(), position) {
                let locs: Vec<Value> = locations.iter().map(Self::location_to_json).collect();
                return Ok(Value::Array(locs));
            }
        }
        Ok(Value::Null)
    }

    // ─── References ─────────────────────────────────────────────────────

    fn handle_references(&mut self, params: Option<Value>) -> Result<Value> {
        let (uri, position) = Self::extract_position(&params)
            .ok_or_else(|| anyhow::anyhow!("Missing position params"))?;
        let file_name = Self::uri_to_file_name(&uri);

        match self.project.find_references(&file_name, position) {
            Some(locations) => {
                let locs: Vec<Value> = locations.iter().map(Self::location_to_json).collect();
                Ok(Value::Array(locs))
            }
            None => Ok(Value::Array(vec![])),
        }
    }

    // ─── Implementation ─────────────────────────────────────────────────

    fn handle_implementation(&mut self, params: Option<Value>) -> Result<Value> {
        let (uri, position) = Self::extract_position(&params)
            .ok_or_else(|| anyhow::anyhow!("Missing position params"))?;
        let file_name = Self::uri_to_file_name(&uri);

        match self.project.get_implementations(&file_name, position) {
            Some(locations) => {
                let locs: Vec<Value> = locations.iter().map(Self::location_to_json).collect();
                Ok(Value::Array(locs))
            }
            None => Ok(Value::Array(vec![])),
        }
    }

    // ─── Document Symbols ───────────────────────────────────────────────

    fn handle_document_symbol(&mut self, params: Option<Value>) -> Result<Value> {
        let uri = Self::extract_uri(&params).ok_or_else(|| anyhow::anyhow!("Missing uri"))?;
        let file_name = Self::uri_to_file_name(&uri);

        match self.project.get_document_symbols(&file_name) {
            Some(symbols) => {
                let lsp_syms: Vec<Value> =
                    symbols.iter().map(Self::document_symbol_to_json).collect();
                Ok(Value::Array(lsp_syms))
            }
            None => Ok(Value::Array(vec![])),
        }
    }

    fn document_symbol_to_json(sym: &tsz::lsp::DocumentSymbol) -> Value {
        let mut s = serde_json::json!({
            "name": sym.name,
            "kind": Self::symbol_kind_to_lsp(sym.kind),
            "range": Self::range_to_json(&sym.range),
            "selectionRange": Self::range_to_json(&sym.selection_range),
        });
        if let Some(ref detail) = sym.detail {
            s["detail"] = Value::from(detail.as_str());
        }
        if !sym.children.is_empty() {
            let children: Vec<Value> = sym
                .children
                .iter()
                .map(Self::document_symbol_to_json)
                .collect();
            s["children"] = Value::Array(children);
        }
        s
    }

    // ─── Formatting ─────────────────────────────────────────────────────

    fn handle_formatting(&mut self, params: Option<Value>) -> Result<Value> {
        let uri = Self::extract_uri(&params).ok_or_else(|| anyhow::anyhow!("Missing uri"))?;
        let file_name = Self::uri_to_file_name(&uri);

        let options = params
            .as_ref()
            .and_then(|p| p.get("options"))
            .map(|opts| FormattingOptions {
                tab_size: opts.get("tabSize").and_then(|v| v.as_u64()).unwrap_or(4) as u32,
                insert_spaces: opts
                    .get("insertSpaces")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true),
                trim_trailing_whitespace: opts
                    .get("trimTrailingWhitespace")
                    .and_then(|v| v.as_bool()),
                insert_final_newline: opts.get("insertFinalNewline").and_then(|v| v.as_bool()),
                trim_final_newlines: opts.get("trimFinalNewlines").and_then(|v| v.as_bool()),
                semicolons: opts
                    .get("semicolons")
                    .and_then(|v| v.as_str())
                    .map(String::from),
            })
            .unwrap_or_default();

        match self.project.format_document(&file_name, &options) {
            Some(Ok(edits)) => {
                let lsp_edits: Vec<Value> = edits
                    .iter()
                    .map(|edit| {
                        serde_json::json!({
                            "range": Self::range_to_json(&edit.range),
                            "newText": edit.new_text,
                        })
                    })
                    .collect();
                Ok(Value::Array(lsp_edits))
            }
            Some(Err(e)) => {
                debug!("Formatting error: {}", e);
                Ok(Value::Array(vec![]))
            }
            None => Ok(Value::Array(vec![])),
        }
    }

    // ─── Rename ─────────────────────────────────────────────────────────

    fn handle_rename(&mut self, params: Option<Value>) -> Result<Value> {
        let (uri, position) = Self::extract_position(&params)
            .ok_or_else(|| anyhow::anyhow!("Missing position params"))?;
        let file_name = Self::uri_to_file_name(&uri);
        let new_name = params
            .as_ref()
            .and_then(|p| p.get("newName"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing newName"))?
            .to_string();

        match self
            .project
            .get_rename_edits(&file_name, position, new_name)
        {
            Ok(workspace_edit) => {
                let mut changes: serde_json::Map<String, Value> = serde_json::Map::new();
                for (file, edits) in &workspace_edit.changes {
                    let lsp_edits: Vec<Value> = edits
                        .iter()
                        .map(|edit| {
                            serde_json::json!({
                                "range": Self::range_to_json(&edit.range),
                                "newText": edit.new_text,
                            })
                        })
                        .collect();
                    changes.insert(Self::file_name_to_uri(file), Value::Array(lsp_edits));
                }
                Ok(serde_json::json!({ "changes": changes }))
            }
            Err(msg) => {
                // Return error response for rename failures
                Err(anyhow::anyhow!("{msg}"))
            }
        }
    }

    fn handle_prepare_rename(&mut self, params: Option<Value>) -> Result<Value> {
        let (uri, position) = Self::extract_position(&params)
            .ok_or_else(|| anyhow::anyhow!("Missing position params"))?;
        let file_name = Self::uri_to_file_name(&uri);

        // Try a rename with a placeholder to check if rename is valid at this position
        if let Some(file) = self.project.file(&file_name) {
            let provider = tsz::lsp::FindReferences::new(
                file.arena(),
                file.binder(),
                file.line_map(),
                file_name,
                file.source_text(),
            );
            if let Some(refs) = provider.find_references(file.root(), position)
                && let Some(first) = refs.first()
            {
                return Ok(serde_json::json!({
                    "range": Self::range_to_json(&first.range),
                }));
            }
        }
        Ok(Value::Null)
    }

    // ─── Code Actions ───────────────────────────────────────────────────

    fn handle_code_action(&mut self, params: Option<Value>) -> Result<Value> {
        let uri = Self::extract_uri(&params).ok_or_else(|| anyhow::anyhow!("Missing uri"))?;
        let file_name = Self::uri_to_file_name(&uri);

        let range = Self::extract_range(&params, "range")
            .unwrap_or_else(|| Range::new(Position::new(0, 0), Position::new(0, 0)));

        // Extract diagnostics from context
        let diagnostics = params
            .as_ref()
            .and_then(|p| p.get("context"))
            .and_then(|ctx| ctx.get("diagnostics"))
            .and_then(|d| d.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|d| {
                        Some(tsz::lsp::LspDiagnostic {
                            range: {
                                let r = d.get("range")?;
                                let s = r.get("start")?;
                                let e = r.get("end")?;
                                Range::new(
                                    Position::new(
                                        s.get("line")?.as_u64()? as u32,
                                        s.get("character")?.as_u64()? as u32,
                                    ),
                                    Position::new(
                                        e.get("line")?.as_u64()? as u32,
                                        e.get("character")?.as_u64()? as u32,
                                    ),
                                )
                            },
                            severity: d
                                .get("severity")
                                .and_then(|s| s.as_u64())
                                .and_then(|s| (s as u8).try_into().ok()),
                            code: d.get("code").and_then(|c| c.as_u64()).map(|c| c as u32),
                            source: d.get("source").and_then(|s| s.as_str()).map(String::from),
                            message: d
                                .get("message")
                                .and_then(|m| m.as_str())
                                .unwrap_or("")
                                .to_string(),
                            related_information: None,
                            reports_unnecessary: None,
                            reports_deprecated: None,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        match self
            .project
            .get_code_actions(&file_name, range, diagnostics, None)
        {
            Some(actions) => {
                let lsp_actions: Vec<Value> = actions
                    .iter()
                    .map(|action| {
                        let mut a = serde_json::json!({
                            "title": action.title,
                        });
                        // Serialize kind using its serde rename
                        if let Ok(kind_val) = serde_json::to_value(&action.kind) {
                            a["kind"] = kind_val;
                        }
                        if let Some(ref edit) = action.edit {
                            let mut changes: serde_json::Map<String, Value> =
                                serde_json::Map::new();
                            for (file, edits) in &edit.changes {
                                let lsp_edits: Vec<Value> = edits
                                    .iter()
                                    .map(|e| {
                                        serde_json::json!({
                                            "range": Self::range_to_json(&e.range),
                                            "newText": e.new_text,
                                        })
                                    })
                                    .collect();
                                changes
                                    .insert(Self::file_name_to_uri(file), Value::Array(lsp_edits));
                            }
                            a["edit"] = serde_json::json!({ "changes": changes });
                        }
                        if action.is_preferred {
                            a["isPreferred"] = Value::from(true);
                        }
                        a
                    })
                    .collect();
                Ok(Value::Array(lsp_actions))
            }
            None => Ok(Value::Array(vec![])),
        }
    }

    // ─── Code Lens ──────────────────────────────────────────────────────

    fn handle_code_lens(&mut self, params: Option<Value>) -> Result<Value> {
        let uri = Self::extract_uri(&params).ok_or_else(|| anyhow::anyhow!("Missing uri"))?;
        let file_name = Self::uri_to_file_name(&uri);

        match self.project.get_code_lenses(&file_name) {
            Some(lenses) => {
                let lsp_lenses: Vec<Value> = lenses
                    .iter()
                    .map(|lens| {
                        let mut l = serde_json::json!({
                            "range": Self::range_to_json(&lens.range),
                        });
                        if let Some(ref cmd) = lens.command {
                            l["command"] = serde_json::json!({
                                "title": cmd.title,
                                "command": cmd.command,
                            });
                        }
                        if let Some(ref data) = lens.data {
                            l["data"] = serde_json::to_value(data).unwrap_or(Value::Null);
                        }
                        l
                    })
                    .collect();
                Ok(Value::Array(lsp_lenses))
            }
            None => Ok(Value::Array(vec![])),
        }
    }

    fn handle_code_lens_resolve(&mut self, params: Option<Value>) -> Result<Value> {
        let p = params
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Missing params"))?;

        let range = Self::extract_range(&params, "range")
            .unwrap_or_else(|| Range::new(Position::new(0, 0), Position::new(0, 0)));

        // Deserialize the data field to reconstruct the CodeLens
        let data: Option<tsz::lsp::CodeLensData> = p
            .get("data")
            .and_then(|d| serde_json::from_value(d.clone()).ok());

        let lens = tsz::lsp::CodeLens {
            range,
            command: None,
            data,
        };

        if let Some(ref data) = lens.data {
            let file_name = Self::uri_to_file_name(&data.file_path);
            if let Some(resolved) = self.project.resolve_code_lens(&file_name, &lens) {
                let mut l = serde_json::json!({
                    "range": Self::range_to_json(&resolved.range),
                });
                if let Some(ref cmd) = resolved.command {
                    let mut cmd_json = serde_json::json!({
                        "title": cmd.title,
                        "command": cmd.command,
                    });
                    if let Some(ref args) = cmd.arguments {
                        cmd_json["arguments"] = Value::Array(args.clone());
                    }
                    l["command"] = cmd_json;
                }
                return Ok(l);
            }
        }

        // Fallback: return as-is
        Ok(params.unwrap_or(Value::Null))
    }

    // ─── Selection Range ────────────────────────────────────────────────

    fn handle_selection_range(&mut self, params: Option<Value>) -> Result<Value> {
        let uri = Self::extract_uri(&params).ok_or_else(|| anyhow::anyhow!("Missing uri"))?;
        let file_name = Self::uri_to_file_name(&uri);

        let positions: Vec<Position> = params
            .as_ref()
            .and_then(|p| p.get("positions"))
            .and_then(|pos| pos.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|p| {
                        let line = p.get("line")?.as_u64()? as u32;
                        let character = p.get("character")?.as_u64()? as u32;
                        Some(Position::new(line, character))
                    })
                    .collect()
            })
            .unwrap_or_default();

        match self.project.get_selection_ranges(&file_name, &positions) {
            Some(ranges) => {
                let lsp_ranges: Vec<Value> = ranges
                    .iter()
                    .map(|r| match r {
                        Some(sr) => Self::selection_range_to_json(sr),
                        None => Value::Null,
                    })
                    .collect();
                Ok(Value::Array(lsp_ranges))
            }
            None => Ok(Value::Array(vec![])),
        }
    }

    fn selection_range_to_json(sr: &tsz::lsp::SelectionRange) -> Value {
        let mut result = serde_json::json!({
            "range": Self::range_to_json(&sr.range),
        });
        if let Some(ref parent) = sr.parent {
            result["parent"] = Self::selection_range_to_json(parent);
        }
        result
    }

    // ─── Folding Range ──────────────────────────────────────────────────

    fn handle_folding_range(&mut self, params: Option<Value>) -> Result<Value> {
        let uri = Self::extract_uri(&params).ok_or_else(|| anyhow::anyhow!("Missing uri"))?;
        let file_name = Self::uri_to_file_name(&uri);

        match self.project.get_folding_ranges(&file_name) {
            Some(ranges) => {
                let lsp_ranges: Vec<Value> = ranges
                    .iter()
                    .map(|r| {
                        let mut fr = serde_json::json!({
                            "startLine": r.start_line,
                            "endLine": r.end_line,
                        });
                        if let Some(ref kind) = r.kind {
                            fr["kind"] = Value::from(kind.as_str());
                        }
                        fr
                    })
                    .collect();
                Ok(Value::Array(lsp_ranges))
            }
            None => Ok(Value::Array(vec![])),
        }
    }

    // ─── Signature Help ─────────────────────────────────────────────────

    fn handle_signature_help(&mut self, params: Option<Value>) -> Result<Value> {
        let (uri, position) = Self::extract_position(&params)
            .ok_or_else(|| anyhow::anyhow!("Missing position params"))?;
        let file_name = Self::uri_to_file_name(&uri);

        match self.project.get_signature_help(&file_name, position) {
            Some(help) => {
                let signatures: Vec<Value> = help
                    .signatures
                    .iter()
                    .map(|sig| {
                        let params: Vec<Value> = sig
                            .parameters
                            .iter()
                            .map(|p| {
                                let mut param = serde_json::json!({
                                    "label": p.label.clone(),
                                });
                                if let Some(ref doc) = p.documentation {
                                    param["documentation"] = Value::from(doc.as_str());
                                }
                                param
                            })
                            .collect();
                        let mut s = serde_json::json!({
                            "label": sig.label,
                            "parameters": params,
                        });
                        if let Some(ref doc) = sig.documentation {
                            s["documentation"] = Value::from(doc.as_str());
                        }
                        s
                    })
                    .collect();

                Ok(serde_json::json!({
                    "signatures": signatures,
                    "activeSignature": help.active_signature,
                    "activeParameter": help.active_parameter,
                }))
            }
            None => Ok(Value::Null),
        }
    }

    // ─── Semantic Tokens ────────────────────────────────────────────────

    fn handle_semantic_tokens_full(&mut self, params: Option<Value>) -> Result<Value> {
        let uri = Self::extract_uri(&params).ok_or_else(|| anyhow::anyhow!("Missing uri"))?;
        let file_name = Self::uri_to_file_name(&uri);

        match self.project.get_semantic_tokens_full(&file_name) {
            Some(data) => Ok(serde_json::json!({ "data": data })),
            None => Ok(serde_json::json!({ "data": [] })),
        }
    }

    fn handle_semantic_tokens_range(&mut self, params: Option<Value>) -> Result<Value> {
        let uri = Self::extract_uri(&params).ok_or_else(|| anyhow::anyhow!("Missing uri"))?;
        let file_name = Self::uri_to_file_name(&uri);
        let range = Self::extract_range(&params, "range")
            .ok_or_else(|| anyhow::anyhow!("Missing range"))?;

        match self.project.get_semantic_tokens_range(&file_name, range) {
            Some(data) => Ok(serde_json::json!({ "data": data })),
            None => Ok(serde_json::json!({ "data": [] })),
        }
    }

    // ─── Document Highlight ─────────────────────────────────────────────

    fn handle_document_highlight(&mut self, params: Option<Value>) -> Result<Value> {
        let (uri, position) = Self::extract_position(&params)
            .ok_or_else(|| anyhow::anyhow!("Missing position params"))?;
        let file_name = Self::uri_to_file_name(&uri);

        match self.project.get_document_highlighting(&file_name, position) {
            Some(highlights) => {
                let lsp_highlights: Vec<Value> = highlights
                    .iter()
                    .map(|h| {
                        let kind = match h.kind {
                            Some(tsz::lsp::DocumentHighlightKind::Text) | None => 1,
                            Some(tsz::lsp::DocumentHighlightKind::Read) => 2,
                            Some(tsz::lsp::DocumentHighlightKind::Write) => 3,
                        };
                        serde_json::json!({
                            "range": Self::range_to_json(&h.range),
                            "kind": kind,
                        })
                    })
                    .collect();
                Ok(Value::Array(lsp_highlights))
            }
            None => Ok(Value::Array(vec![])),
        }
    }

    // ─── Inlay Hints ────────────────────────────────────────────────────

    fn handle_inlay_hint(&mut self, params: Option<Value>) -> Result<Value> {
        let uri = Self::extract_uri(&params).ok_or_else(|| anyhow::anyhow!("Missing uri"))?;
        let file_name = Self::uri_to_file_name(&uri);

        let range = Self::extract_range(&params, "range")
            .unwrap_or_else(|| Range::new(Position::new(0, 0), Position::new(u32::MAX, 0)));

        match self.project.get_inlay_hints(&file_name, range) {
            Some(hints) => {
                let lsp_hints: Vec<Value> = hints
                    .iter()
                    .map(|h| {
                        let kind = match h.kind {
                            tsz::lsp::InlayHintKind::Type | tsz::lsp::InlayHintKind::Generic => 1,
                            tsz::lsp::InlayHintKind::Parameter => 2,
                        };
                        let mut hint = serde_json::json!({
                            "position": Self::position_to_json(&h.position),
                            "label": h.label,
                            "kind": kind,
                        });
                        if let Some(ref tooltip) = h.tooltip {
                            hint["tooltip"] = Value::from(tooltip.as_str());
                        }
                        hint
                    })
                    .collect();
                Ok(Value::Array(lsp_hints))
            }
            None => Ok(Value::Array(vec![])),
        }
    }

    fn handle_inlay_hint_resolve(&mut self, params: Option<Value>) -> Result<Value> {
        // The params IS the inlay hint itself - just add a tooltip if missing
        let mut hint = params.unwrap_or(Value::Null);

        // If no tooltip yet, add a description based on the kind
        if hint.get("tooltip").is_none() {
            let kind = hint.get("kind").and_then(|k| k.as_u64()).unwrap_or(0);
            let label = hint
                .get("label")
                .and_then(|l| l.as_str())
                .unwrap_or_default();

            let tooltip = match kind {
                1 => {
                    // Type hint
                    format!("Inferred type{label}")
                }
                2 => {
                    // Parameter hint
                    format!("Parameter name{label}")
                }
                _ => String::new(),
            };

            if !tooltip.is_empty() {
                hint["tooltip"] = serde_json::json!({
                    "kind": "markdown",
                    "value": format!("```typescript\n{tooltip}\n```"),
                });
            }
        }

        Ok(hint)
    }

    // ─── Document Colors ─────────────────────────────────────────────────

    fn handle_document_color(&mut self, params: Option<Value>) -> Result<Value> {
        let uri = Self::extract_uri(&params).ok_or_else(|| anyhow::anyhow!("Missing uri"))?;
        let file_name = Self::uri_to_file_name(&uri);

        match self.project.get_document_colors(&file_name) {
            Some(colors) => {
                let lsp_colors: Vec<Value> = colors
                    .iter()
                    .map(|c| {
                        serde_json::json!({
                            "range": Self::range_to_json(&c.range),
                            "color": {
                                "red": c.color.red,
                                "green": c.color.green,
                                "blue": c.color.blue,
                                "alpha": c.color.alpha,
                            },
                        })
                    })
                    .collect();
                Ok(Value::Array(lsp_colors))
            }
            None => Ok(Value::Array(vec![])),
        }
    }

    fn handle_color_presentation(&mut self, params: Option<Value>) -> Result<Value> {
        // Convert a color back to a text representation
        let color = params
            .as_ref()
            .and_then(|p| p.get("color"))
            .ok_or_else(|| anyhow::anyhow!("Missing color"))?;

        let r = color.get("red").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let g = color.get("green").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let b = color.get("blue").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let a = color.get("alpha").and_then(|v| v.as_f64()).unwrap_or(1.0);

        let ri = (r * 255.0).round() as u8;
        let gi = (g * 255.0).round() as u8;
        let bi = (b * 255.0).round() as u8;

        let mut presentations = Vec::new();

        // #rrggbb format
        presentations.push(serde_json::json!({
            "label": format!("#{ri:02x}{gi:02x}{bi:02x}"),
        }));

        // #rrggbbaa format (only if alpha != 1.0)
        if (a - 1.0).abs() > f64::EPSILON {
            let ai = (a * 255.0).round() as u8;
            presentations.push(serde_json::json!({
                "label": format!("#{ri:02x}{gi:02x}{bi:02x}{ai:02x}"),
            }));
        }

        // rgb() format
        presentations.push(serde_json::json!({
            "label": if (a - 1.0).abs() > f64::EPSILON {
                format!("rgba({ri}, {gi}, {bi}, {a:.2})")
            } else {
                format!("rgb({ri}, {gi}, {bi})")
            },
        }));

        Ok(Value::Array(presentations))
    }

    // ─── Document Links ─────────────────────────────────────────────────

    fn handle_document_link(&mut self, params: Option<Value>) -> Result<Value> {
        let uri = Self::extract_uri(&params).ok_or_else(|| anyhow::anyhow!("Missing uri"))?;
        let file_name = Self::uri_to_file_name(&uri);

        match self.project.get_document_links(&file_name) {
            Some(links) => {
                let lsp_links: Vec<Value> = links
                    .iter()
                    .map(|link| {
                        let mut l = serde_json::json!({
                            "range": Self::range_to_json(&link.range),
                        });
                        if let Some(ref target) = link.target {
                            l["target"] = Value::from(target.as_str());
                        }
                        if let Some(ref tooltip) = link.tooltip {
                            l["tooltip"] = Value::from(tooltip.as_str());
                        }
                        l
                    })
                    .collect();
                Ok(Value::Array(lsp_links))
            }
            None => Ok(Value::Array(vec![])),
        }
    }

    // ─── Linked Editing Range ───────────────────────────────────────────

    fn handle_linked_editing_range(&mut self, params: Option<Value>) -> Result<Value> {
        let (uri, position) = Self::extract_position(&params)
            .ok_or_else(|| anyhow::anyhow!("Missing position params"))?;
        let file_name = Self::uri_to_file_name(&uri);

        match self.project.get_linked_editing_ranges(&file_name, position) {
            Some(result) => {
                let ranges: Vec<Value> = result.ranges.iter().map(Self::range_to_json).collect();
                let mut response = serde_json::json!({ "ranges": ranges });
                if let Some(ref pattern) = result.word_pattern {
                    response["wordPattern"] = Value::from(pattern.as_str());
                }
                Ok(response)
            }
            None => Ok(Value::Null),
        }
    }

    // ─── Call Hierarchy ─────────────────────────────────────────────────

    fn handle_prepare_call_hierarchy(&mut self, params: Option<Value>) -> Result<Value> {
        let (uri, position) = Self::extract_position(&params)
            .ok_or_else(|| anyhow::anyhow!("Missing position params"))?;
        let file_name = Self::uri_to_file_name(&uri);

        match self.project.prepare_call_hierarchy(&file_name, position) {
            Some(item) => Ok(Value::Array(vec![Self::call_hierarchy_item_to_json(&item)])),
            None => Ok(Value::Array(vec![])),
        }
    }

    fn handle_incoming_calls(&mut self, params: Option<Value>) -> Result<Value> {
        let item = params
            .as_ref()
            .and_then(|p| p.get("item"))
            .ok_or_else(|| anyhow::anyhow!("Missing item"))?;
        let uri = item
            .get("uri")
            .and_then(|u| u.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing uri"))?;
        let file_name = Self::uri_to_file_name(uri);
        let position = Self::extract_range_start(item.get("selectionRange"));

        let calls = self.project.get_incoming_calls(&file_name, position);
        let lsp_calls: Vec<Value> = calls
            .iter()
            .map(|call| {
                let from_ranges: Vec<Value> =
                    call.from_ranges.iter().map(Self::range_to_json).collect();
                serde_json::json!({
                    "from": Self::call_hierarchy_item_to_json(&call.from),
                    "fromRanges": from_ranges,
                })
            })
            .collect();
        Ok(Value::Array(lsp_calls))
    }

    fn handle_outgoing_calls(&mut self, params: Option<Value>) -> Result<Value> {
        let item = params
            .as_ref()
            .and_then(|p| p.get("item"))
            .ok_or_else(|| anyhow::anyhow!("Missing item"))?;
        let uri = item
            .get("uri")
            .and_then(|u| u.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing uri"))?;
        let file_name = Self::uri_to_file_name(uri);
        let position = Self::extract_range_start(item.get("selectionRange"));

        let calls = self.project.get_outgoing_calls(&file_name, position);
        let lsp_calls: Vec<Value> = calls
            .iter()
            .map(|call| {
                let from_ranges: Vec<Value> =
                    call.from_ranges.iter().map(Self::range_to_json).collect();
                serde_json::json!({
                    "to": Self::call_hierarchy_item_to_json(&call.to),
                    "fromRanges": from_ranges,
                })
            })
            .collect();
        Ok(Value::Array(lsp_calls))
    }

    fn call_hierarchy_item_to_json(item: &tsz::lsp::CallHierarchyItem) -> Value {
        serde_json::json!({
            "name": item.name,
            "kind": Self::symbol_kind_to_lsp(item.kind),
            "uri": Self::file_name_to_uri(&item.uri),
            "range": Self::range_to_json(&item.range),
            "selectionRange": Self::range_to_json(&item.selection_range),
        })
    }

    fn extract_range_start(range: Option<&Value>) -> Position {
        range
            .and_then(|r| {
                let start = r.get("start")?;
                let line = start.get("line")?.as_u64()? as u32;
                let character = start.get("character")?.as_u64()? as u32;
                Some(Position::new(line, character))
            })
            .unwrap_or_else(|| Position::new(0, 0))
    }

    // ─── Type Hierarchy ─────────────────────────────────────────────────

    fn handle_prepare_type_hierarchy(&mut self, params: Option<Value>) -> Result<Value> {
        let (uri, position) = Self::extract_position(&params)
            .ok_or_else(|| anyhow::anyhow!("Missing position params"))?;
        let file_name = Self::uri_to_file_name(&uri);

        match self.project.prepare_type_hierarchy(&file_name, position) {
            Some(item) => Ok(Value::Array(vec![Self::type_hierarchy_item_to_json(&item)])),
            None => Ok(Value::Array(vec![])),
        }
    }

    fn handle_supertypes(&mut self, params: Option<Value>) -> Result<Value> {
        let item = params
            .as_ref()
            .and_then(|p| p.get("item"))
            .ok_or_else(|| anyhow::anyhow!("Missing item"))?;
        let uri = item
            .get("uri")
            .and_then(|u| u.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing uri"))?;
        let file_name = Self::uri_to_file_name(uri);
        let position = Self::extract_range_start(item.get("selectionRange"));

        let items = self.project.supertypes(&file_name, position);
        let lsp_items: Vec<Value> = items
            .iter()
            .map(Self::type_hierarchy_item_to_json)
            .collect();
        Ok(Value::Array(lsp_items))
    }

    fn handle_subtypes(&mut self, params: Option<Value>) -> Result<Value> {
        let item = params
            .as_ref()
            .and_then(|p| p.get("item"))
            .ok_or_else(|| anyhow::anyhow!("Missing item"))?;
        let uri = item
            .get("uri")
            .and_then(|u| u.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing uri"))?;
        let file_name = Self::uri_to_file_name(uri);
        let position = Self::extract_range_start(item.get("selectionRange"));

        let items = self.project.subtypes(&file_name, position);
        let lsp_items: Vec<Value> = items
            .iter()
            .map(Self::type_hierarchy_item_to_json)
            .collect();
        Ok(Value::Array(lsp_items))
    }

    fn type_hierarchy_item_to_json(item: &tsz::lsp::TypeHierarchyItem) -> Value {
        serde_json::json!({
            "name": item.name,
            "kind": Self::symbol_kind_to_lsp(item.kind),
            "uri": Self::file_name_to_uri(&item.uri),
            "range": Self::range_to_json(&item.range),
            "selectionRange": Self::range_to_json(&item.selection_range),
        })
    }

    // ─── Workspace Symbols ──────────────────────────────────────────────

    fn handle_workspace_symbol(&mut self, params: Option<Value>) -> Result<Value> {
        let query = params
            .as_ref()
            .and_then(|p| p.get("query"))
            .and_then(|q| q.as_str())
            .unwrap_or("");

        let symbols = self.project.get_workspace_symbols(query);
        let lsp_symbols: Vec<Value> = symbols
            .iter()
            .map(|sym| {
                serde_json::json!({
                    "name": sym.name,
                    "kind": Self::symbol_kind_to_lsp(sym.kind),
                    "location": Self::location_to_json(&sym.location),
                })
            })
            .collect();
        Ok(Value::Array(lsp_symbols))
    }

    // ─── Range Formatting ──────────────────────────────────────────────

    fn handle_range_formatting(&mut self, params: Option<Value>) -> Result<Value> {
        let uri = Self::extract_uri(&params).ok_or_else(|| anyhow::anyhow!("Missing uri"))?;
        let file_name = Self::uri_to_file_name(&uri);

        let range = Self::extract_range(&params, "range")
            .ok_or_else(|| anyhow::anyhow!("Missing range"))?;

        let options = params
            .as_ref()
            .and_then(|p| p.get("options"))
            .map(|opts| FormattingOptions {
                tab_size: opts.get("tabSize").and_then(|v| v.as_u64()).unwrap_or(4) as u32,
                insert_spaces: opts
                    .get("insertSpaces")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true),
                trim_trailing_whitespace: opts
                    .get("trimTrailingWhitespace")
                    .and_then(|v| v.as_bool()),
                insert_final_newline: None,
                trim_final_newlines: None,
                semicolons: opts
                    .get("semicolons")
                    .and_then(|v| v.as_str())
                    .map(String::from),
            })
            .unwrap_or_default();

        // Format the whole document and filter edits to only those within the range
        match self.project.format_document(&file_name, &options) {
            Some(Ok(edits)) => {
                let lsp_edits: Vec<Value> = edits
                    .iter()
                    .filter(|edit| {
                        // Include edits that overlap with the requested range
                        edit.range.start.line <= range.end.line
                            && edit.range.end.line >= range.start.line
                    })
                    .map(|edit| {
                        serde_json::json!({
                            "range": Self::range_to_json(&edit.range),
                            "newText": edit.new_text,
                        })
                    })
                    .collect();
                Ok(Value::Array(lsp_edits))
            }
            Some(Err(e)) => {
                debug!("Range formatting error: {}", e);
                Ok(Value::Array(vec![]))
            }
            None => Ok(Value::Array(vec![])),
        }
    }

    // ─── On-Type Formatting ────────────────────────────────────────────

    fn handle_on_type_formatting(&mut self, params: Option<Value>) -> Result<Value> {
        let uri = Self::extract_uri(&params).ok_or_else(|| anyhow::anyhow!("Missing uri"))?;
        let file_name = Self::uri_to_file_name(&uri);

        let position = params
            .as_ref()
            .and_then(|p| p.get("position"))
            .and_then(|pos| {
                let line = pos.get("line")?.as_u64()? as u32;
                let character = pos.get("character")?.as_u64()? as u32;
                Some(Position::new(line, character))
            })
            .ok_or_else(|| anyhow::anyhow!("Missing position"))?;

        let ch = params
            .as_ref()
            .and_then(|p| p.get("ch"))
            .and_then(|c| c.as_str())
            .unwrap_or(";");

        let options = params
            .as_ref()
            .and_then(|p| p.get("options"))
            .map(|opts| FormattingOptions {
                tab_size: opts.get("tabSize").and_then(|v| v.as_u64()).unwrap_or(4) as u32,
                insert_spaces: opts
                    .get("insertSpaces")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true),
                trim_trailing_whitespace: None,
                insert_final_newline: None,
                trim_final_newlines: None,
                semicolons: None,
            })
            .unwrap_or_default();

        // Use the format_on_key API from the formatting provider
        if let Some(file) = self.project.file(&file_name) {
            let source = file.source_text();
            let offset = file
                .line_map()
                .position_to_offset(position, source)
                .unwrap_or(0);

            match tsz::lsp::DocumentFormattingProvider::format_on_key(
                source,
                position.line,
                offset,
                ch,
                &options,
            ) {
                Ok(edits) => {
                    let lsp_edits: Vec<Value> = edits
                        .iter()
                        .map(|edit| {
                            serde_json::json!({
                                "range": Self::range_to_json(&edit.range),
                                "newText": edit.new_text,
                            })
                        })
                        .collect();
                    return Ok(Value::Array(lsp_edits));
                }
                Err(e) => {
                    debug!("On-type formatting error: {}", e);
                    return Ok(Value::Array(vec![]));
                }
            }
        }

        Ok(Value::Array(vec![]))
    }

    // ─── Workspace Configuration ───────────────────────────────────────

    fn handle_did_change_configuration(&mut self, params: Option<Value>) {
        // Extract settings if provided
        if let Some(settings) = params
            .as_ref()
            .and_then(|p| p.get("settings"))
            .and_then(|s| s.get("tsz").or_else(|| s.get("typescript")))
        {
            // Apply strict mode setting if present
            if let Some(strict) = settings.get("strict").and_then(|v| v.as_bool()) {
                self.project.set_strict(strict);
            }

            debug!("Configuration updated: {:?}", settings);
        }
    }

    // ─── Watched File Changes ──────────────────────────────────────────

    fn handle_did_change_watched_files(&mut self, params: Option<Value>) {
        let changes = match params
            .as_ref()
            .and_then(|p| p.get("changes"))
            .and_then(|c| c.as_array())
        {
            Some(c) => c,
            None => return,
        };

        for change in changes {
            let uri = match change.get("uri").and_then(|u| u.as_str()) {
                Some(u) => u,
                None => continue,
            };
            let change_type = change.get("type").and_then(|t| t.as_u64()).unwrap_or(0);
            let file_name = Self::uri_to_file_name(uri);

            match change_type {
                1 => {
                    // Created: read and add if it's a TS/JS file
                    if Self::is_ts_file(&file_name)
                        && let Ok(content) = std::fs::read_to_string(&file_name)
                    {
                        self.project.set_file(file_name, content);
                    }
                }
                2 => {
                    // Changed: update if we're tracking it
                    if self.project.file(&file_name).is_some()
                        && let Ok(content) = std::fs::read_to_string(&file_name)
                    {
                        self.project.set_file(file_name, content);
                    }
                }
                3 => {
                    // Deleted: remove from project
                    self.project.remove_file(&file_name);
                    // Clear diagnostics for deleted file
                    self.pending_notifications.push(JsonRpcNotification {
                        jsonrpc: "2.0".to_string(),
                        method: "textDocument/publishDiagnostics".to_string(),
                        params: serde_json::json!({
                            "uri": uri,
                            "diagnostics": [],
                        }),
                    });
                }
                _ => {}
            }
        }
    }

    fn is_ts_file(path: &str) -> bool {
        let extensions = [".ts", ".tsx", ".js", ".jsx", ".mts", ".cts", ".mjs", ".cjs"];
        extensions.iter().any(|ext| path.ends_with(ext))
    }

    // ─── File Rename ───────────────────────────────────────────────────

    fn handle_will_rename_files(&mut self, params: Option<Value>) -> Result<Value> {
        let files = params
            .as_ref()
            .and_then(|p| p.get("files"))
            .and_then(|f| f.as_array())
            .cloned()
            .unwrap_or_default();

        let mut all_changes = serde_json::Map::new();

        for file_entry in &files {
            let old_uri = match file_entry.get("oldUri").and_then(|u| u.as_str()) {
                Some(u) => u,
                None => continue,
            };
            let new_uri = match file_entry.get("newUri").and_then(|u| u.as_str()) {
                Some(u) => u,
                None => continue,
            };

            let old_path = Self::uri_to_file_name(old_uri);
            let new_path = Self::uri_to_file_name(new_uri);

            let edits = self.project.get_file_rename_edits(&old_path, &new_path);

            for (file_name, file_edits) in edits {
                let uri = Self::file_name_to_uri(&file_name);
                let lsp_edits: Vec<Value> = file_edits
                    .iter()
                    .map(|edit| {
                        serde_json::json!({
                            "range": Self::range_to_json(&edit.range),
                            "newText": edit.new_text,
                        })
                    })
                    .collect();

                // Merge with existing edits for this file
                all_changes
                    .entry(uri)
                    .or_insert_with(|| Value::Array(vec![]))
                    .as_array_mut()
                    .unwrap()
                    .extend(lsp_edits);
            }
        }

        if all_changes.is_empty() {
            Ok(Value::Null)
        } else {
            Ok(serde_json::json!({
                "changes": all_changes,
            }))
        }
    }

    fn handle_did_rename_files(&mut self, params: Option<Value>) {
        let files = match params
            .as_ref()
            .and_then(|p| p.get("files"))
            .and_then(|f| f.as_array())
        {
            Some(f) => f.clone(),
            None => return,
        };

        for file_entry in &files {
            let old_uri = match file_entry.get("oldUri").and_then(|u| u.as_str()) {
                Some(u) => u,
                None => continue,
            };
            let new_uri = match file_entry.get("newUri").and_then(|u| u.as_str()) {
                Some(u) => u,
                None => continue,
            };

            let old_path = Self::uri_to_file_name(old_uri);
            let new_path = Self::uri_to_file_name(new_uri);

            // Compute workspace edits to update import paths
            let edits = self.project.get_file_rename_edits(&old_path, &new_path);

            // Apply edits via workspace/applyEdit request
            if !edits.is_empty() {
                let mut changes = serde_json::Map::new();
                for (file_name, file_edits) in edits {
                    let uri = Self::file_name_to_uri(&file_name);
                    let lsp_edits: Vec<Value> = file_edits
                        .iter()
                        .map(|edit| {
                            serde_json::json!({
                                "range": Self::range_to_json(&edit.range),
                                "newText": edit.new_text,
                            })
                        })
                        .collect();
                    changes.insert(uri, Value::Array(lsp_edits));
                }

                self.pending_notifications.push(JsonRpcNotification {
                    jsonrpc: "2.0".to_string(),
                    method: "workspace/applyEdit".to_string(),
                    params: serde_json::json!({
                        "label": "Update imports for renamed file",
                        "edit": {
                            "changes": changes,
                        },
                    }),
                });
            }

            // Update project state: remove old file, add new
            if let Some(file) = self.project.file(&old_path) {
                let source = file.source_text().to_string();
                self.project.remove_file(&old_path);
                self.project.set_file(new_path.clone(), source);
            }
        }
    }

    // ─── Execute Command ───────────────────────────────────────────────

    fn handle_execute_command(&mut self, params: Option<Value>) -> Result<Value> {
        let command = params
            .as_ref()
            .and_then(|p| p.get("command"))
            .and_then(|c| c.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing command"))?;

        let arguments = params
            .as_ref()
            .and_then(|p| p.get("arguments"))
            .and_then(|a| a.as_array());

        match command {
            "tsz.organizeImports" => {
                // Extract file URI from arguments
                if let Some(args) = arguments
                    && let Some(uri) = args.first().and_then(|a| a.as_str())
                {
                    let file_name = Self::uri_to_file_name(uri);
                    let range = Range::new(Position::new(0, 0), Position::new(0, 0));
                    let context = tsz::lsp::CodeActionContext {
                        diagnostics: vec![],
                        only: Some(vec![tsz::lsp::CodeActionKind::SourceOrganizeImports]),
                        import_candidates: vec![],
                    };

                    if let Some(file) = self.project.file(&file_name) {
                        let provider = tsz::lsp::CodeActionProvider::new(
                            file.arena(),
                            file.binder(),
                            file.line_map(),
                            file_name,
                            file.source_text(),
                        );
                        let actions = provider.provide_code_actions(file.root(), range, context);
                        if let Some(action) = actions.first()
                            && let Some(ref edit) = action.edit
                        {
                            // Apply the workspace edit
                            return Ok(serde_json::to_value(edit).unwrap_or(Value::Null));
                        }
                    }
                }
                Ok(Value::Null)
            }
            _ => {
                debug!("Unknown command: {}", command);
                Ok(Value::Null)
            }
        }
    }
}

// =============================================================================
// Main
// =============================================================================

fn main() -> Result<()> {
    // Initialize tracing (always stderr — stdout carries LSP JSON-RPC).
    tsz_cli::tracing_config::init_tracing();

    let args = Args::parse();

    info!("tsz-lsp: Starting Language Server Protocol server");
    info!("tsz-lsp: Mode: {}", args.mode);

    let mut server = LspServer::new();
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();

    // Use a single BufReader for all reads to avoid losing buffered data
    let mut reader = BufReader::new(stdin.lock());
    let mut header_line = String::new();

    loop {
        // Read headers
        let mut content_length: Option<usize> = None;

        loop {
            header_line.clear();
            let bytes_read = reader
                .read_line(&mut header_line)
                .context("Failed to read header line")?;

            if bytes_read == 0 {
                debug!("tsz-lsp: EOF reached");
                return Ok(());
            }

            let line = header_line.trim_end_matches(['\r', '\n']);

            if line.is_empty() {
                break;
            }

            if let Some(len) = line.strip_prefix("Content-Length: ") {
                content_length = Some(len.trim().parse().context("Invalid content length")?);
            }
        }

        let content_length =
            content_length.ok_or_else(|| anyhow::anyhow!("Missing Content-Length header"))?;

        // Read content from the same BufReader to preserve buffered data
        let mut content = vec![0u8; content_length];
        reader
            .read_exact(&mut content)
            .context("Failed to read content")?;

        let content_str = String::from_utf8(content).context("Invalid UTF-8")?;

        if args.verbose {
            trace!("tsz-lsp: Received: {}", content_str);
        }

        // Parse JSON-RPC message
        let msg: JsonRpcMessage =
            serde_json::from_str(&content_str).context("Failed to parse JSON-RPC message")?;

        // Handle message
        if let Some(response) = server.handle_message(msg) {
            let response_str = serde_json::to_string(&response)?;
            let response_bytes = response_str.as_bytes();

            if args.verbose {
                trace!("tsz-lsp: Sending response: {}", response_str);
            }

            write!(
                stdout,
                "Content-Length: {}\r\n\r\n{}",
                response_bytes.len(),
                response_str
            )?;
            stdout.flush()?;
        }

        // Send any pending notifications (e.g., diagnostics)
        for notification in server.pending_notifications.drain(..) {
            let notification_str = serde_json::to_string(&notification)?;
            let notification_bytes = notification_str.as_bytes();

            if args.verbose {
                trace!("tsz-lsp: Sending notification: {}", notification_str);
            }

            write!(
                stdout,
                "Content-Length: {}\r\n\r\n{}",
                notification_bytes.len(),
                notification_str
            )?;
            stdout.flush()?;
        }
    }
}
