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
use rustc_hash::FxHashSet;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::{debug, info, trace};
use tsz_common::limits;

use tsz::lsp::{
    CompletionItemData, CompletionItemKind, FormattingOptions, Position, Project, Range,
};

#[path = "tsz_lsp/tsz_lsp_dispatch.rs"]
mod tsz_lsp_dispatch;
#[path = "tsz_lsp/tsz_lsp_notification_handlers.rs"]
mod tsz_lsp_notification_handlers;
#[path = "tsz_lsp/tsz_lsp_request_handlers.rs"]
mod tsz_lsp_request_handlers;

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

/// A JSON-RPC server-to-client request. Issue #3545: methods like
/// `workspace/applyEdit` are LSP requests, not notifications, so they
/// must include an `id` and the client is expected to respond.
#[derive(Debug, Serialize)]
struct JsonRpcServerRequest {
    jsonrpc: String,
    id: Value,
    method: String,
    params: Value,
}

// =============================================================================
// LSP Server State
// =============================================================================

/// Counter for generating unique progress tokens.
static PROGRESS_TOKEN_COUNTER: AtomicU64 = AtomicU64::new(1);

struct LspServer {
    /// Multi-file project state backed by the tsz-lsp infrastructure.
    project: Project,
    /// Whether the server has been initialized.
    initialized: bool,
    /// Shutdown requested.
    shutdown_requested: bool,
    /// Pending diagnostics notifications to send after handling a request.
    pending_notifications: Vec<JsonRpcNotification>,
    /// Pending server-to-client requests (e.g. `workspace/applyEdit`).
    /// Drained alongside notifications. Issue #3545.
    pending_server_requests: Vec<JsonRpcServerRequest>,
    /// Counter for generating unique server-side request ids. Issue #3545.
    next_server_request_id: u64,
    /// Request IDs that have been cancelled by the client.
    cancelled_requests: FxHashSet<String>,
    /// Workspace folder URIs.
    workspace_folders: Vec<WorkspaceFolder>,
    /// Whether the client supports workspace folder change notifications.
    client_supports_workspace_folders: bool,
    /// Whether the client supports progress reporting.
    client_supports_progress: bool,
}

#[derive(Debug, Clone)]
struct WorkspaceFolder {
    uri: String,
}

impl LspServer {
    fn new() -> Self {
        Self {
            project: Project::new(),
            initialized: false,
            shutdown_requested: false,
            pending_notifications: Vec::new(),
            pending_server_requests: Vec::new(),
            next_server_request_id: 1,
            cancelled_requests: FxHashSet::default(),
            workspace_folders: Vec::new(),
            client_supports_workspace_folders: false,
            client_supports_progress: false,
        }
    }

    /// Check if a request has been cancelled.
    fn is_cancelled(&self, id: &Option<Value>) -> bool {
        if let Some(id) = id {
            let id_str = match id {
                Value::Number(n) => n.to_string(),
                Value::String(s) => s.clone(),
                _ => return false,
            };
            self.cancelled_requests.contains(&id_str)
        } else {
            false
        }
    }

    /// Generate a unique progress token.
    fn next_progress_token() -> String {
        format!(
            "tsz-progress-{}",
            PROGRESS_TOKEN_COUNTER.fetch_add(1, Ordering::Relaxed)
        )
    }

    /// Send a progress begin notification.
    fn begin_progress(&mut self, token: &str, title: &str, message: Option<&str>) {
        if !self.client_supports_progress {
            return;
        }
        self.pending_notifications.push(JsonRpcNotification {
            jsonrpc: "2.0".to_string(),
            method: "$/progress".to_string(),
            params: serde_json::json!({
                "token": token,
                "value": {
                    "kind": "begin",
                    "title": title,
                    "message": message,
                    "cancellable": false,
                }
            }),
        });
    }

    /// Send a progress end notification.
    fn end_progress(&mut self, token: &str, message: Option<&str>) {
        if !self.client_supports_progress {
            return;
        }
        self.pending_notifications.push(JsonRpcNotification {
            jsonrpc: "2.0".to_string(),
            method: "$/progress".to_string(),
            params: serde_json::json!({
                "token": token,
                "value": {
                    "kind": "end",
                    "message": message,
                }
            }),
        });
    }

    /// Send a window/showMessage notification.
    fn show_message(&mut self, typ: u32, message: &str) {
        self.pending_notifications.push(JsonRpcNotification {
            jsonrpc: "2.0".to_string(),
            method: "window/showMessage".to_string(),
            params: serde_json::json!({
                "type": typ,
                "message": message,
            }),
        });
    }

    /// Send a window/logMessage notification.
    fn log_message(&mut self, typ: u32, message: &str) {
        self.pending_notifications.push(JsonRpcNotification {
            jsonrpc: "2.0".to_string(),
            method: "window/logMessage".to_string(),
            params: serde_json::json!({
                "type": typ,
                "message": message,
            }),
        });
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
        let Some(path) = uri.strip_prefix("file://") else {
            return uri.to_string();
        };

        Self::file_uri_path_to_file_name(path)
    }

    /// Convert an internal file name back to a URI.
    fn file_name_to_uri(file_name: &str) -> String {
        let Some(uri_path) = Self::file_name_to_uri_path(file_name) else {
            return file_name.to_string();
        };

        format!("file://{}", Self::percent_encode_file_uri_path(&uri_path))
    }

    fn file_name_to_uri_path(file_name: &str) -> Option<String> {
        if let Some(unc_path) = file_name.strip_prefix("//")
            && !unc_path.is_empty()
            && !unc_path.starts_with('/')
        {
            return Some(unc_path.to_string());
        }

        if file_name.starts_with('/') {
            Some(file_name.to_string())
        } else {
            None
        }
    }

    fn file_uri_path_to_file_name(path: &str) -> String {
        let path = if path.starts_with('/') {
            path.to_string()
        } else if let Some(path) = path.strip_prefix("localhost/") {
            format!("/{path}")
        } else if path == "localhost" {
            "/".to_string()
        } else if path.is_empty() {
            String::new()
        } else {
            format!("//{path}")
        };

        Self::percent_decode_file_uri_path(&path)
    }

    fn percent_decode_file_uri_path(path: &str) -> String {
        let bytes = path.as_bytes();
        let mut decoded = Vec::with_capacity(bytes.len());
        let mut index = 0;

        while index < bytes.len() {
            if bytes[index] == b'%'
                && index + 2 < bytes.len()
                && let (Some(high), Some(low)) = (
                    Self::hex_digit_value(bytes[index + 1]),
                    Self::hex_digit_value(bytes[index + 2]),
                )
            {
                decoded.push((high << 4) | low);
                index += 3;
                continue;
            }

            decoded.push(bytes[index]);
            index += 1;
        }

        String::from_utf8(decoded).unwrap_or_else(|_| path.to_string())
    }

    fn percent_encode_file_uri_path(path: &str) -> String {
        let mut encoded = String::with_capacity(path.len());

        for byte in path.bytes() {
            if Self::is_file_uri_path_byte_allowed(byte) {
                encoded.push(char::from(byte));
            } else {
                const HEX: &[u8; 16] = b"0123456789ABCDEF";
                encoded.push('%');
                encoded.push(char::from(HEX[(byte >> 4) as usize]));
                encoded.push(char::from(HEX[(byte & 0x0F) as usize]));
            }
        }

        encoded
    }

    const fn hex_digit_value(byte: u8) -> Option<u8> {
        match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'A'..=b'F' => Some(byte - b'A' + 10),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            _ => None,
        }
    }

    const fn is_file_uri_path_byte_allowed(byte: u8) -> bool {
        matches!(
            byte,
            b'A'..=b'Z'
                | b'a'..=b'z'
                | b'0'..=b'9'
                | b'-'
                | b'.'
                | b'_'
                | b'~'
                | b'/'
                | b':'
                | b'@'
                | b'!'
                | b'$'
                | b'&'
                | b'\''
                | b'('
                | b')'
                | b'*'
                | b'+'
                | b','
                | b';'
                | b'='
        )
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

    // ─── Cancel Request ─────────────────────────────────────────────────

    fn handle_cancel_request(&mut self, params: Option<Value>) {
        if let Some(id) = params.as_ref().and_then(|p| p.get("id")) {
            let id_str = match id {
                Value::Number(n) => n.to_string(),
                Value::String(s) => s.clone(),
                _ => return,
            };
            self.cancelled_requests.insert(id_str);
        }
    }

    // ─── Initialize ─────────────────────────────────────────────────────

    fn handle_initialize(&mut self, params: Option<&Value>) -> Value {
        // Extract workspace folders from initialization params
        if let Some(p) = params {
            // Extract workspace folders
            if let Some(folders) = p.get("workspaceFolders").and_then(|f| f.as_array()) {
                for folder in folders {
                    if let Some(uri) = folder.get("uri").and_then(|u| u.as_str()) {
                        let folder_path = Self::uri_to_file_name(uri);
                        self.project.add_workspace_root(folder_path);
                        self.workspace_folders.push(WorkspaceFolder {
                            uri: uri.to_string(),
                        });
                    }
                }
            } else if let Some(root_uri) = p.get("rootUri").and_then(|u| u.as_str()) {
                // Fallback: use rootUri if no workspace folders
                let folder_path = Self::uri_to_file_name(root_uri);
                self.project.add_workspace_root(folder_path);
                self.workspace_folders.push(WorkspaceFolder {
                    uri: root_uri.to_string(),
                });
            }

            // Detect client capabilities
            if let Some(caps) = p.get("capabilities") {
                // Check workspace folder support
                self.client_supports_workspace_folders = caps
                    .pointer("/workspace/workspaceFolders")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                // Check progress support
                self.client_supports_progress = caps
                    .pointer("/window/workDoneProgress")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
            }
        }

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
                    "commands": [
                        "tsz.organizeImports",
                        "tsz.applyCodeAction",
                        "tsz.reloadProject"
                    ]
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
                        },
                        "didCreate": {
                            "filters": [{
                                "scheme": "file",
                                "pattern": { "glob": "**/*.{ts,tsx,js,jsx,mts,cts,mjs,cjs}" }
                            }]
                        },
                        "didDelete": {
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
            self.project.mark_file_open(&file_name);
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
            self.project.set_focused_file(&file_name);
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

    /// Called after initialization to discover workspace files and tsconfig.
    fn handle_initialized(&mut self) {
        let roots: Vec<String> = self.project.workspace_roots().to_vec();

        if roots.is_empty() {
            self.log_message(3, "tsz-lsp: No workspace roots configured");
            return;
        }

        // Load tsconfig.json from each workspace root
        for root in &roots {
            self.project.load_tsconfig(root);
            self.log_message(3, &format!("tsz-lsp: Loaded configuration from {root}"));
        }

        // Discover files from workspace roots (with progress reporting)
        let token = Self::next_progress_token();
        self.begin_progress(&token, "Indexing workspace", Some("Discovering files..."));

        let discovered = self.project.discover_files(&roots);
        let count = discovered.len();

        self.end_progress(&token, Some(&format!("Indexed {count} files")));

        self.log_message(
            3,
            &format!(
                "tsz-lsp: Discovered and indexed {count} files from {} workspace root(s)",
                roots.len()
            ),
        );

        // Publish diagnostics for all discovered files
        for file_name in &discovered {
            let uri = Self::file_name_to_uri(file_name);
            self.publish_diagnostics(&uri);
        }
    }

    /// Handle workspace folder changes.
    fn handle_did_change_workspace_folders(&mut self, params: Option<Value>) {
        let event = match params.as_ref().and_then(|p| p.get("event")) {
            Some(e) => e,
            None => return,
        };

        // Process removed folders
        if let Some(removed) = event.get("removed").and_then(|r| r.as_array()) {
            for folder in removed {
                if let Some(uri) = folder.get("uri").and_then(|u| u.as_str()) {
                    let path = Self::uri_to_file_name(uri);
                    self.project.remove_workspace_root(&path);
                    self.workspace_folders.retain(|f| f.uri != uri);
                    self.log_message(3, &format!("tsz-lsp: Removed workspace folder: {uri}"));
                }
            }
        }

        // Process added folders
        if let Some(added) = event.get("added").and_then(|a| a.as_array()) {
            let mut new_roots = Vec::new();
            for folder in added {
                if let Some(uri) = folder.get("uri").and_then(|u| u.as_str()) {
                    let path = Self::uri_to_file_name(uri);
                    self.project.add_workspace_root(path.clone());
                    self.workspace_folders.push(WorkspaceFolder {
                        uri: uri.to_string(),
                    });
                    new_roots.push(path.clone());

                    // Load tsconfig for new root
                    self.project.load_tsconfig(&path);
                    self.log_message(3, &format!("tsz-lsp: Added workspace folder: {uri}"));
                }
            }

            // Discover files from new workspace roots
            if !new_roots.is_empty() {
                let discovered = self.project.discover_files(&new_roots);
                self.log_message(
                    3,
                    &format!(
                        "tsz-lsp: Discovered {} files from new workspace folders",
                        discovered.len()
                    ),
                );
            }
        }
    }

    /// Handle file creation notifications.
    fn handle_did_create_files(&mut self, params: Option<Value>) {
        let files = match params
            .as_ref()
            .and_then(|p| p.get("files"))
            .and_then(|f| f.as_array())
        {
            Some(f) => f.clone(),
            None => return,
        };

        for file_entry in &files {
            if let Some(uri) = file_entry.get("uri").and_then(|u| u.as_str()) {
                let file_name = Self::uri_to_file_name(uri);
                if Self::is_ts_file(&file_name)
                    && let Ok(content) = std::fs::read_to_string(&file_name)
                {
                    self.project.set_file(file_name, content);
                    self.publish_diagnostics(uri);
                }
            }
        }
    }

    /// Handle file deletion notifications.
    fn handle_did_delete_files(&mut self, params: Option<Value>) {
        let files = match params
            .as_ref()
            .and_then(|p| p.get("files"))
            .and_then(|f| f.as_array())
        {
            Some(f) => f.clone(),
            None => return,
        };

        for file_entry in &files {
            if let Some(uri) = file_entry.get("uri").and_then(|u| u.as_str()) {
                let file_name = Self::uri_to_file_name(uri);
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
            if let Ok(data) = serde_json::from_value::<CompletionItemData>(data_val)
                && let Some((detail, documentation)) =
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
}

// =============================================================================
// Main
// =============================================================================

fn main() -> Result<()> {
    // Initialize tracing (always stderr — stdout carries LSP JSON-RPC).
    tsz_cli::tracing_config::init_tracing();

    let args = Args::parse();

    // Run on a large stack to prevent overflows in recursive AST traversals
    // (document highlights, find-references, narrowing) on deeply-nested code.
    // Matches the 128 MiB stack used by the tsz CLI for project-sized workloads.
    std::thread::Builder::new()
        .stack_size(limits::THREAD_STACK_SIZE_BYTES)
        .spawn(move || lsp_main(args))
        .expect("failed to spawn LSP thread")
        .join()
        .expect("LSP thread panicked")
}

fn lsp_main(args: Args) -> Result<()> {
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

        // Issue #3545: send any pending server-to-client requests (with id).
        for request in server.pending_server_requests.drain(..) {
            let request_str = serde_json::to_string(&request)?;
            let request_bytes = request_str.as_bytes();

            if args.verbose {
                trace!("tsz-lsp: Sending server request: {}", request_str);
            }

            write!(
                stdout,
                "Content-Length: {}\r\n\r\n{}",
                request_bytes.len(),
                request_str
            )?;
            stdout.flush()?;
        }
    }
}

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
