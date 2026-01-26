//! TSZ Language Server Protocol (LSP) Server
//!
//! This binary provides a Language Server Protocol server for TypeScript/JavaScript.
//! It communicates via stdin/stdout using JSON-RPC messages.
//!
//! Usage:
//!   tsz-lsp              # Start LSP server (default: stdio)
//!   tsz-lsp --version    # Show version
//!   tsz-lsp --help       # Show help
//!
//! The server supports the following LSP features:
//! - textDocument/hover
//! - textDocument/completion
//! - textDocument/definition
//! - textDocument/references
//! - textDocument/documentSymbol
//! - textDocument/formatting
//! - textDocument/rename
//! - textDocument/codeAction
//! - textDocument/codeLens
//! - textDocument/selectionRange
//! - textDocument/semanticTokens
//! - textDocument/foldingRange
//! - textDocument/signatureHelp

use anyhow::{Context, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};

use wasm::binder::BinderState;
use wasm::lsp::position::LineMap;
use wasm::lsp::{
    CodeLensProvider, DocumentSymbolProvider, FindReferences, GoToDefinition, Position,
    SelectionRangeProvider, TypeDefinitionProvider,
};
use wasm::parser::ParserState;

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

/// LSP Server State
struct LspServer {
    /// Open documents (uri -> content)
    documents: HashMap<String, DocumentState>,
    /// Server capabilities
    capabilities: ServerCapabilities,
    /// Whether the server is initialized
    initialized: bool,
    /// Shutdown requested
    shutdown_requested: bool,
}

/// State for an open document
#[allow(dead_code)]
struct DocumentState {
    content: String,
    version: i32,
    // Cached parser state
    parser: Option<ParserState>,
    // Cached binder state
    binder: Option<BinderState>,
    // Cached line map
    line_map: Option<LineMap>,
    // Root node index
    root: Option<wasm::parser::NodeIndex>,
}

impl DocumentState {
    fn new(content: String, version: i32) -> Self {
        Self {
            content,
            version,
            parser: None,
            binder: None,
            line_map: None,
            root: None,
        }
    }

    fn ensure_parsed(&mut self, uri: &str) {
        if self.parser.is_none() {
            let mut parser = ParserState::new(uri.to_string(), self.content.clone());
            let root = parser.parse_source_file();

            let mut binder = BinderState::new();
            binder.bind_source_file(parser.get_arena(), root);

            let line_map = LineMap::build(&self.content);

            self.root = Some(root);
            self.parser = Some(parser);
            self.binder = Some(binder);
            self.line_map = Some(line_map);
        }
    }

    fn get_root(&self) -> wasm::parser::NodeIndex {
        self.root.unwrap_or(wasm::parser::NodeIndex::NONE)
    }
}

/// Server capabilities advertised during initialization
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ServerCapabilities {
    text_document_sync: TextDocumentSyncOptions,
    hover_provider: bool,
    completion_provider: Option<CompletionOptions>,
    definition_provider: bool,
    type_definition_provider: bool,
    references_provider: bool,
    document_symbol_provider: bool,
    document_formatting_provider: bool,
    rename_provider: Option<RenameOptions>,
    code_action_provider: bool,
    code_lens_provider: Option<CodeLensOptions>,
    selection_range_provider: bool,
    folding_range_provider: bool,
    signature_help_provider: Option<SignatureHelpOptions>,
    semantic_tokens_provider: Option<SemanticTokensOptions>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TextDocumentSyncOptions {
    open_close: bool,
    change: i32, // 1 = Full, 2 = Incremental
    save: Option<SaveOptions>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SaveOptions {
    include_text: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CompletionOptions {
    trigger_characters: Vec<String>,
    resolve_provider: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RenameOptions {
    prepare_provider: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CodeLensOptions {
    resolve_provider: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SignatureHelpOptions {
    trigger_characters: Vec<String>,
    retrigger_characters: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SemanticTokensOptions {
    legend: SemanticTokensLegend,
    full: bool,
    range: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SemanticTokensLegend {
    token_types: Vec<String>,
    token_modifiers: Vec<String>,
}

impl Default for ServerCapabilities {
    fn default() -> Self {
        Self {
            text_document_sync: TextDocumentSyncOptions {
                open_close: true,
                change: 1, // Full sync
                save: Some(SaveOptions { include_text: true }),
            },
            hover_provider: true,
            completion_provider: Some(CompletionOptions {
                trigger_characters: vec![".".to_string(), "<".to_string(), "/".to_string()],
                resolve_provider: false,
            }),
            definition_provider: true,
            type_definition_provider: true,
            references_provider: true,
            document_symbol_provider: true,
            document_formatting_provider: true,
            rename_provider: Some(RenameOptions {
                prepare_provider: true,
            }),
            code_action_provider: true,
            code_lens_provider: Some(CodeLensOptions {
                resolve_provider: true,
            }),
            selection_range_provider: true,
            folding_range_provider: true,
            signature_help_provider: Some(SignatureHelpOptions {
                trigger_characters: vec!["(".to_string(), ",".to_string()],
                retrigger_characters: vec![")".to_string()],
            }),
            semantic_tokens_provider: Some(SemanticTokensOptions {
                legend: SemanticTokensLegend {
                    token_types: vec![
                        "namespace".to_string(),
                        "type".to_string(),
                        "class".to_string(),
                        "enum".to_string(),
                        "interface".to_string(),
                        "struct".to_string(),
                        "typeParameter".to_string(),
                        "parameter".to_string(),
                        "variable".to_string(),
                        "property".to_string(),
                        "enumMember".to_string(),
                        "event".to_string(),
                        "function".to_string(),
                        "method".to_string(),
                        "macro".to_string(),
                        "keyword".to_string(),
                        "modifier".to_string(),
                        "comment".to_string(),
                        "string".to_string(),
                        "number".to_string(),
                        "regexp".to_string(),
                        "operator".to_string(),
                    ],
                    token_modifiers: vec![
                        "declaration".to_string(),
                        "definition".to_string(),
                        "readonly".to_string(),
                        "static".to_string(),
                        "deprecated".to_string(),
                        "abstract".to_string(),
                        "async".to_string(),
                        "modification".to_string(),
                        "documentation".to_string(),
                        "defaultLibrary".to_string(),
                    ],
                },
                full: true,
                range: false,
            }),
        }
    }
}

/// JSON-RPC message structures
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct JsonRpcMessage {
    jsonrpc: String,
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

impl LspServer {
    fn new() -> Self {
        Self {
            documents: HashMap::new(),
            capabilities: ServerCapabilities::default(),
            initialized: false,
            shutdown_requested: false,
        }
    }

    fn handle_message(&mut self, msg: JsonRpcMessage) -> Option<JsonRpcResponse> {
        let method = msg.method.as_deref();
        let id = msg.id.clone();

        match method {
            Some("initialize") => {
                let result = self.handle_initialize(msg.params);
                Some(self.make_response(id, result))
            }
            Some("initialized") => {
                self.initialized = true;
                None // Notification, no response
            }
            Some("shutdown") => {
                self.shutdown_requested = true;
                Some(self.make_response(id, Ok(Value::Null)))
            }
            Some("exit") => {
                std::process::exit(if self.shutdown_requested { 0 } else { 1 });
            }
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
            Some("textDocument/hover") => {
                let result = self.handle_hover(msg.params);
                Some(self.make_response(id, result))
            }
            Some("textDocument/completion") => {
                let result = self.handle_completion(msg.params);
                Some(self.make_response(id, result))
            }
            Some("textDocument/definition") => {
                let result = self.handle_definition(msg.params);
                Some(self.make_response(id, result))
            }
            Some("textDocument/typeDefinition") => {
                let result = self.handle_type_definition(msg.params);
                Some(self.make_response(id, result))
            }
            Some("textDocument/references") => {
                let result = self.handle_references(msg.params);
                Some(self.make_response(id, result))
            }
            Some("textDocument/documentSymbol") => {
                let result = self.handle_document_symbol(msg.params);
                Some(self.make_response(id, result))
            }
            Some("textDocument/selectionRange") => {
                let result = self.handle_selection_range(msg.params);
                Some(self.make_response(id, result))
            }
            Some("textDocument/codeLens") => {
                let result = self.handle_code_lens(msg.params);
                Some(self.make_response(id, result))
            }
            Some("codeLens/resolve") => {
                let result = self.handle_code_lens_resolve(msg.params);
                Some(self.make_response(id, result))
            }
            Some(method) if id.is_some() => {
                // Unknown request - return method not found error
                Some(self.make_error_response(id, -32601, format!("Method not found: {}", method)))
            }
            _ => None, // Unknown notification or malformed message
        }
    }

    fn make_response(&self, id: Option<Value>, result: Result<Value>) -> JsonRpcResponse {
        match result {
            Ok(value) => JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: id.unwrap_or(Value::Null),
                result: Some(value),
                error: None,
            },
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

    fn make_error_response(
        &self,
        id: Option<Value>,
        code: i32,
        message: String,
    ) -> JsonRpcResponse {
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

    fn handle_initialize(&mut self, _params: Option<Value>) -> Result<Value> {
        Ok(serde_json::json!({
            "capabilities": self.capabilities,
            "serverInfo": {
                "name": "tsz-lsp",
                "version": env!("CARGO_PKG_VERSION")
            }
        }))
    }

    fn handle_did_open(&mut self, params: Option<Value>) {
        if let Some(params) = params {
            if let (Some(uri), Some(text), Some(version)) = (
                params
                    .get("textDocument")
                    .and_then(|td| td.get("uri"))
                    .and_then(|u| u.as_str()),
                params
                    .get("textDocument")
                    .and_then(|td| td.get("text"))
                    .and_then(|t| t.as_str()),
                params
                    .get("textDocument")
                    .and_then(|td| td.get("version"))
                    .and_then(|v| v.as_i64()),
            ) {
                self.documents.insert(
                    uri.to_string(),
                    DocumentState::new(text.to_string(), version as i32),
                );
            }
        }
    }

    fn handle_did_change(&mut self, params: Option<Value>) {
        if let Some(params) = params {
            if let (Some(uri), Some(changes), Some(version)) = (
                params
                    .get("textDocument")
                    .and_then(|td| td.get("uri"))
                    .and_then(|u| u.as_str()),
                params.get("contentChanges").and_then(|c| c.as_array()),
                params
                    .get("textDocument")
                    .and_then(|td| td.get("version"))
                    .and_then(|v| v.as_i64()),
            ) {
                // Full sync mode - take the last change
                if let Some(change) = changes.last() {
                    if let Some(text) = change.get("text").and_then(|t| t.as_str()) {
                        self.documents.insert(
                            uri.to_string(),
                            DocumentState::new(text.to_string(), version as i32),
                        );
                    }
                }
            }
        }
    }

    fn handle_did_close(&mut self, params: Option<Value>) {
        if let Some(params) = params {
            if let Some(uri) = params
                .get("textDocument")
                .and_then(|td| td.get("uri"))
                .and_then(|u| u.as_str())
            {
                self.documents.remove(uri);
            }
        }
    }

    fn handle_hover(&mut self, params: Option<Value>) -> Result<Value> {
        // Hover requires full type checking infrastructure (TypeInterner)
        // which is not yet ready. Return null for now.
        // TODO: Implement when type checker is complete
        let _ = params;
        Ok(Value::Null)
    }

    fn handle_completion(&mut self, params: Option<Value>) -> Result<Value> {
        // Completions require full type checking for type-aware suggestions.
        // Return basic keyword completions for now.
        // TODO: Implement full completions when type checker is complete
        let _ = params;

        let keywords = vec![
            "const",
            "let",
            "var",
            "function",
            "class",
            "interface",
            "type",
            "enum",
            "import",
            "export",
            "return",
            "if",
            "else",
            "for",
            "while",
            "switch",
            "case",
            "break",
            "continue",
            "try",
            "catch",
            "finally",
            "throw",
            "new",
            "this",
            "super",
            "extends",
            "implements",
            "public",
            "private",
            "protected",
            "static",
            "readonly",
            "abstract",
            "async",
            "await",
            "yield",
            "typeof",
            "instanceof",
            "in",
            "of",
            "as",
            "is",
            "true",
            "false",
            "null",
            "undefined",
            "void",
            "never",
            "any",
            "unknown",
            "string",
            "number",
            "boolean",
            "object",
            "symbol",
            "bigint",
        ];

        Ok(serde_json::json!({
            "isIncomplete": true,
            "items": keywords.iter().map(|kw| {
                serde_json::json!({
                    "label": kw,
                    "kind": 14, // Keyword
                    "detail": "TypeScript keyword"
                })
            }).collect::<Vec<_>>()
        }))
    }

    fn handle_definition(&mut self, params: Option<Value>) -> Result<Value> {
        let (uri, line, character) = self.extract_position(&params)?;

        let doc = self
            .documents
            .get_mut(&uri)
            .ok_or_else(|| anyhow::anyhow!("Document not found"))?;
        doc.ensure_parsed(&uri);

        let parser = doc.parser.as_ref().unwrap();
        let binder = doc.binder.as_ref().unwrap();
        let line_map = doc.line_map.as_ref().unwrap();

        let provider = GoToDefinition::new(
            parser.get_arena(),
            binder,
            line_map,
            uri.clone(),
            &doc.content,
        );

        let position = Position::new(line, character);
        let root = doc.get_root();

        if let Some(locations) = provider.get_definition(root, position) {
            Ok(serde_json::to_value(&locations)?)
        } else {
            Ok(Value::Null)
        }
    }

    fn handle_type_definition(&mut self, params: Option<Value>) -> Result<Value> {
        let (uri, line, character) = self.extract_position(&params)?;

        let doc = self
            .documents
            .get_mut(&uri)
            .ok_or_else(|| anyhow::anyhow!("Document not found"))?;
        doc.ensure_parsed(&uri);

        let parser = doc.parser.as_ref().unwrap();
        let binder = doc.binder.as_ref().unwrap();
        let line_map = doc.line_map.as_ref().unwrap();

        let provider = TypeDefinitionProvider::new(
            parser.get_arena(),
            binder,
            line_map,
            uri.clone(),
            &doc.content,
        );

        let position = Position::new(line, character);
        let root = doc.get_root();

        if let Some(locations) = provider.get_type_definition(root, position) {
            Ok(serde_json::to_value(&locations)?)
        } else {
            Ok(Value::Null)
        }
    }

    fn handle_references(&mut self, params: Option<Value>) -> Result<Value> {
        let (uri, line, character) = self.extract_position(&params)?;

        let doc = self
            .documents
            .get_mut(&uri)
            .ok_or_else(|| anyhow::anyhow!("Document not found"))?;
        doc.ensure_parsed(&uri);

        let parser = doc.parser.as_ref().unwrap();
        let binder = doc.binder.as_ref().unwrap();
        let line_map = doc.line_map.as_ref().unwrap();

        let finder = FindReferences::new(
            parser.get_arena(),
            binder,
            line_map,
            uri.clone(),
            &doc.content,
        );

        let position = Position::new(line, character);
        let root = doc.get_root();

        if let Some(locations) = finder.find_references(root, position) {
            Ok(serde_json::to_value(&locations)?)
        } else {
            Ok(Value::Array(vec![]))
        }
    }

    fn handle_document_symbol(&mut self, params: Option<Value>) -> Result<Value> {
        let uri = params
            .as_ref()
            .and_then(|p| p.get("textDocument"))
            .and_then(|td| td.get("uri"))
            .and_then(|u| u.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing uri"))?
            .to_string();

        let doc = self
            .documents
            .get_mut(&uri)
            .ok_or_else(|| anyhow::anyhow!("Document not found"))?;
        doc.ensure_parsed(&uri);

        let parser = doc.parser.as_ref().unwrap();
        let line_map = doc.line_map.as_ref().unwrap();

        let provider = DocumentSymbolProvider::new(parser.get_arena(), line_map, &doc.content);

        let root = doc.get_root();
        let symbols = provider.get_document_symbols(root);

        Ok(serde_json::to_value(&symbols)?)
    }

    fn handle_selection_range(&mut self, params: Option<Value>) -> Result<Value> {
        let uri = params
            .as_ref()
            .and_then(|p| p.get("textDocument"))
            .and_then(|td| td.get("uri"))
            .and_then(|u| u.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing uri"))?
            .to_string();

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

        let doc = self
            .documents
            .get_mut(&uri)
            .ok_or_else(|| anyhow::anyhow!("Document not found"))?;
        doc.ensure_parsed(&uri);

        let parser = doc.parser.as_ref().unwrap();
        let line_map = doc.line_map.as_ref().unwrap();

        let provider = SelectionRangeProvider::new(parser.get_arena(), line_map, &doc.content);

        let ranges = provider.get_selection_ranges(&positions);
        Ok(serde_json::to_value(&ranges)?)
    }

    fn handle_code_lens(&mut self, params: Option<Value>) -> Result<Value> {
        let uri = params
            .as_ref()
            .and_then(|p| p.get("textDocument"))
            .and_then(|td| td.get("uri"))
            .and_then(|u| u.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing uri"))?
            .to_string();

        let doc = self
            .documents
            .get_mut(&uri)
            .ok_or_else(|| anyhow::anyhow!("Document not found"))?;
        doc.ensure_parsed(&uri);

        let parser = doc.parser.as_ref().unwrap();
        let binder = doc.binder.as_ref().unwrap();
        let line_map = doc.line_map.as_ref().unwrap();

        let provider = CodeLensProvider::new(
            parser.get_arena(),
            binder,
            line_map,
            uri.clone(),
            &doc.content,
        );

        let root = doc.get_root();
        let lenses = provider.provide_code_lenses(root);

        Ok(serde_json::to_value(&lenses)?)
    }

    fn handle_code_lens_resolve(&mut self, params: Option<Value>) -> Result<Value> {
        // For now, just return the lens as-is
        // Full resolution would require re-parsing the document
        Ok(params.unwrap_or(Value::Null))
    }

    fn extract_position(&self, params: &Option<Value>) -> Result<(String, u32, u32)> {
        let uri = params
            .as_ref()
            .and_then(|p| p.get("textDocument"))
            .and_then(|td| td.get("uri"))
            .and_then(|u| u.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing uri"))?
            .to_string();

        let line = params
            .as_ref()
            .and_then(|p| p.get("position"))
            .and_then(|pos| pos.get("line"))
            .and_then(|l| l.as_u64())
            .ok_or_else(|| anyhow::anyhow!("Missing line"))? as u32;

        let character = params
            .as_ref()
            .and_then(|p| p.get("position"))
            .and_then(|pos| pos.get("character"))
            .and_then(|c| c.as_u64())
            .ok_or_else(|| anyhow::anyhow!("Missing character"))? as u32;

        Ok((uri, line, character))
    }
}

fn main() -> Result<()> {
    let args = Args::parse();

    eprintln!("tsz-lsp: Starting Language Server Protocol server");
    eprintln!("tsz-lsp: Mode: {}", args.mode);

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
                eprintln!("tsz-lsp: EOF reached");
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
            eprintln!("tsz-lsp: Received: {}", content_str);
        }

        // Parse JSON-RPC message
        let msg: JsonRpcMessage =
            serde_json::from_str(&content_str).context("Failed to parse JSON-RPC message")?;

        // Handle message
        if let Some(response) = server.handle_message(msg) {
            let response_str = serde_json::to_string(&response)?;
            let response_bytes = response_str.as_bytes();

            if args.verbose {
                eprintln!("tsz-lsp: Sending: {}", response_str);
            }

            write!(
                stdout,
                "Content-Length: {}\r\n\r\n{}",
                response_bytes.len(),
                response_str
            )?;
            stdout.flush()?;
        }
    }
}
