//! Diagnostic handlers for tsz-server.
//!
//! Handles semantic, syntactic, and suggestion diagnostic commands,
//! plus code fix related handlers.

use super::{Server, TsServerRequest, TsServerResponse};
use tsz::checker::diagnostics::DiagnosticCategory;
use tsz::lsp::code_actions::{
    CodeActionContext, CodeActionKind, CodeActionProvider, CodeFixRegistry, ImportCandidate,
    ImportCandidateKind,
};
use tsz::lsp::position::LineMap;
use tsz::parser::ParserState;
use tsz_parser::syntax_kind_ext;

pub(crate) struct DiagnosticFormatInput<'a> {
    pub(crate) start_offset: u32,
    pub(crate) length: u32,
    pub(crate) message: &'a str,
    pub(crate) code: u32,
    pub(crate) category: DiagnosticCategory,
    pub(crate) line_map: &'a LineMap,
    pub(crate) content: &'a str,
    pub(crate) include_line_position: bool,
}

impl Server {
    pub(crate) fn handle_configure(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        // Accept configuration but most options are not yet wired
        TsServerResponse {
            seq,
            msg_type: "response".to_string(),
            command: "configure".to_string(),
            request_seq: request.seq,
            success: true,
            message: None,
            body: None,
        }
    }

    pub(crate) fn handle_semantic_diagnostics_sync(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let file = request.arguments.get("file").and_then(|v| v.as_str());
        let include_line_position = request
            .arguments
            .get("includeLinePosition")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let diagnostics: Vec<serde_json::Value> = if let Some(file_path) = file {
            if let Some(content) = self.open_files.get(file_path).cloned() {
                let line_map = LineMap::build(&content);
                let full_diags = self.get_semantic_diagnostics_full(file_path, &content);
                full_diags
                    .iter()
                    .map(|diag| {
                        Self::format_diagnostic(DiagnosticFormatInput {
                            start_offset: diag.start,
                            length: diag.length,
                            message: &diag.message_text,
                            code: diag.code,
                            category: diag.category,
                            line_map: &line_map,
                            content: &content,
                            include_line_position,
                        })
                    })
                    .collect()
            } else {
                vec![]
            }
        } else {
            vec![]
        };

        TsServerResponse {
            seq,
            msg_type: "response".to_string(),
            command: "semanticDiagnosticsSync".to_string(),
            request_seq: request.seq,
            success: true,
            message: None,
            body: Some(serde_json::json!(diagnostics)),
        }
    }

    pub(crate) fn handle_syntactic_diagnostics_sync(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let file = request.arguments.get("file").and_then(|v| v.as_str());
        let include_line_position = request
            .arguments
            .get("includeLinePosition")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let diagnostics: Vec<serde_json::Value> = if let Some(file_path) = file {
            if let Some(content) = self.open_files.get(file_path).cloned() {
                let line_map = LineMap::build(&content);
                let mut parser = ParserState::new(file_path.to_string(), content.clone());
                let _root = parser.parse_source_file();
                parser
                    .get_diagnostics()
                    .iter()
                    .map(|d| {
                        Self::format_diagnostic(DiagnosticFormatInput {
                            start_offset: d.start,
                            length: d.length,
                            message: &d.message,
                            code: d.code,
                            category: DiagnosticCategory::Error,
                            line_map: &line_map,
                            content: &content,
                            include_line_position,
                        })
                    })
                    .collect()
            } else {
                vec![]
            }
        } else {
            vec![]
        };

        TsServerResponse {
            seq,
            msg_type: "response".to_string(),
            command: "syntacticDiagnosticsSync".to_string(),
            request_seq: request.seq,
            success: true,
            message: None,
            body: Some(serde_json::json!(diagnostics)),
        }
    }

    /// Format a diagnostic for the tsserver protocol.
    ///
    /// When `include_line_position` is true (the `SessionClient` always sets this),
    /// the response includes 0-based `start`/`length` fields plus `startLocation`/
    /// `endLocation` with 1-based line/offset. When false, uses `start`/`end` as
    /// 1-based line/offset objects (the traditional tsserver format).
    pub(crate) fn format_diagnostic(input: DiagnosticFormatInput<'_>) -> serde_json::Value {
        let start_pos = input
            .line_map
            .offset_to_position(input.start_offset, input.content);
        let end_pos = input
            .line_map
            .offset_to_position(input.start_offset + input.length, input.content);
        let cat_str = match input.category {
            DiagnosticCategory::Error => "error",
            DiagnosticCategory::Warning => "warning",
            _ => "suggestion",
        };

        if input.include_line_position {
            // When includeLinePosition is true, the harness expects:
            // - start: 0-based byte offset (number)
            // - length: byte length (number)
            // - startLocation: {line, offset} (1-based)
            // - endLocation: {line, offset} (1-based)
            // - message: the diagnostic text
            // - category: category string
            // - code: error code
            serde_json::json!({
                "start": input.start_offset,
                "length": input.length,
                "startLocation": {
                    "line": start_pos.line + 1,
                    "offset": start_pos.character + 1,
                },
                "endLocation": {
                    "line": end_pos.line + 1,
                    "offset": end_pos.character + 1,
                },
                "message": input.message,
                "code": input.code,
                "category": cat_str,
            })
        } else {
            // Traditional tsserver format: start/end as {line, offset}
            serde_json::json!({
                "start": {
                    "line": start_pos.line + 1,
                    "offset": start_pos.character + 1,
                },
                "end": {
                    "line": end_pos.line + 1,
                    "offset": end_pos.character + 1,
                },
                "text": input.message,
                "code": input.code,
                "category": cat_str,
            })
        }
    }

    pub(crate) fn handle_suggestion_diagnostics_sync(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        self.stub_response(seq, request, Some(serde_json::json!([])))
    }

    pub(crate) fn handle_geterr(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        // geterr is async in tsserver - it fires diagnostic events
        // For now, just acknowledge the request
        self.stub_response(seq, request, None)
    }

    pub(crate) fn handle_geterr_for_project(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        self.stub_response(seq, request, None)
    }

    pub(crate) fn handle_get_code_fixes(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let file = request.arguments.get("file").and_then(|v| v.as_str());
        let error_codes: Vec<u32> = request
            .arguments
            .get("errorCodes")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_u64().map(|n| n as u32))
                    .collect()
            })
            .unwrap_or_default();

        if let Some(file_path) = file {
            if let Some((arena, binder, root, content)) = self.parse_and_bind_file(file_path) {
                let line_map = LineMap::build(&content);
                let provider = CodeActionProvider::new(
                    &arena,
                    &binder,
                    &line_map,
                    file_path.to_string(),
                    &content,
                );

                let diagnostics = self.get_semantic_diagnostics_full(file_path, &content);
                
                let filtered_diagnostics: Vec<tsz::lsp::diagnostics::LspDiagnostic> = diagnostics.into_iter()
                    .filter(|d| error_codes.is_empty() || error_codes.contains(&d.code))
                    .map(|d| tsz::lsp::diagnostics::LspDiagnostic {
                        range: tsz::lsp::position::Range::new(
                            line_map.offset_to_position(d.start, &content),
                            line_map.offset_to_position(d.start + d.length, &content),
                        ),
                        message: d.message_text,
                        code: Some(d.code),
                        severity: Some(tsz::lsp::diagnostics::DiagnosticSeverity::Error),
                        source: Some("tsz".to_string()),
                        related_information: None,
                        reports_unnecessary: tsz::lsp::diagnostics::is_unnecessary_code(d.code).then_some(true),
                        reports_deprecated: tsz::lsp::diagnostics::is_deprecated_code(d.code).then_some(true),
                    })
                    .collect();

                let import_candidates = self.collect_import_candidates(file_path);

                let context = CodeActionContext {
                    diagnostics: filtered_diagnostics,
                    only: Some(vec![CodeActionKind::QuickFix]),
                    import_candidates,
                };
                
                let range = tsz::lsp::position::Range::new(
                    tsz::lsp::position::Position::new(0, 0),
                    line_map.offset_to_position(content.len() as u32, &content),
                );
                
                let actions = provider.provide_code_actions(root, range, context);
                
                let response_actions: Vec<serde_json::Value> = actions.into_iter().map(|action| {
                    let mut changes = Vec::new();
                    if let Some(edit) = action.edit {
                        for (fname, edits) in edit.changes {
                            let mut text_changes = Vec::new();
                            for edit in edits {
                                text_changes.push(serde_json::json!({
                                    "start": {
                                        "line": edit.range.start.line + 1,
                                        "offset": edit.range.start.character + 1
                                    },
                                    "end": {
                                        "line": edit.range.end.line + 1,
                                        "offset": edit.range.end.character + 1
                                    },
                                    "newText": edit.new_text
                                }));
                            }
                            changes.push(serde_json::json!({
                                "fileName": fname,
                                "textChanges": text_changes
                            }));
                        }
                    }
                    
                    let (fix_name, fix_id, fix_all_desc) = if let Some(data) = &action.data {
                        (
                            data.get("fixName").and_then(|v| v.as_str()).unwrap_or("quickfix"),
                            data.get("fixId").and_then(|v| v.as_str()),
                            data.get("fixAllDescription").and_then(|v| v.as_str())
                        )
                    } else {
                        ("quickfix", None, None)
                    };
                    
                    let mut json_obj = serde_json::json!({
                        "fixName": fix_name,
                        "description": action.title,
                        "changes": changes,
                    });
                    
                    if let Some(id) = fix_id {
                        json_obj["fixId"] = serde_json::json!(id);
                    }
                    if let Some(desc) = fix_all_desc {
                        json_obj["fixAllDescription"] = serde_json::json!(desc);
                    }
                    
                    json_obj
                }).collect();
                
                return TsServerResponse {
                    seq,
                    msg_type: "response".to_string(),
                    command: "getCodeFixes".to_string(),
                    request_seq: request.seq,
                    success: true,
                    message: None,
                    body: Some(serde_json::json!(response_actions)),
                };
            }
        }
        
        self.stub_response(seq, request, Some(serde_json::json!([])))
    }

    fn collect_import_candidates(&self, current_file_path: &str) -> Vec<ImportCandidate> {
        let mut candidates = Vec::new();
        let current_path = std::path::Path::new(current_file_path);
        let current_dir = current_path.parent().unwrap_or(std::path::Path::new("."));

        for (path, content) in &self.open_files {
            if path == current_file_path {
                continue;
            }

            let mut parser = ParserState::new(path.clone(), content.clone());
            let root = parser.parse_source_file();
            let arena = parser.into_arena();
            
            let mut binder = tsz::binder::BinderState::new();
            binder.bind_source_file(&arena, root);
            
            let mut exports = Vec::new();
            // file_locals is SymbolTable. iter() returns iterator of (&String, &SymbolId).
            for (name, &sym_id) in binder.file_locals.iter() {
                if let Some(sym) = binder.symbols.get(sym_id) {
                    // Check if exported
                    // Simple heuristic: if it's in file_locals and marked exported
                    // Or if it's explicitly exported via export declaration
                    if sym.is_exported {
                        // Check if type only
                        let is_type_only = sym.is_type_only;
                        exports.push((name.clone(), is_type_only));
                    }
                }
            }

            // Compute module specifier relative to current file
            let other_path = std::path::Path::new(path);
            let rel_path = if let Ok(p) = other_path.strip_prefix(current_dir) {
                let s = p.to_string_lossy();
                if !s.starts_with('.') {
                    format!("./{}", s)
                } else {
                    s.into_owned()
                }
            } else {
                path.clone()
            };
            
            // Remove extension for import specifier
            let module_specifier = if rel_path.ends_with(".ts") {
                rel_path[..rel_path.len()-3].to_string()
            } else if rel_path.ends_with(".d.ts") {
                rel_path[..rel_path.len()-5].to_string()
            } else if rel_path.ends_with(".tsx") {
                rel_path[..rel_path.len()-4].to_string()
            } else {
                rel_path
            };

            for (name, is_type_only) in exports {
                candidates.push(ImportCandidate {
                    module_specifier: module_specifier.clone(),
                    local_name: name.clone(),
                    kind: ImportCandidateKind::Named { export_name: name },
                    is_type_only,
                });
            }
        }
        candidates
    }

    pub(crate) fn handle_get_combined_code_fix(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let file = request.arguments.get("scope")
            .and_then(|scope| scope.get("args"))
            .and_then(|args| args.get("file"))
            .and_then(|v| v.as_str());
            
        let fix_id = request.arguments.get("fixId").and_then(|v| v.as_str());

        if let (Some(file_path), Some(fix_id)) = (file, fix_id) {
            if let Some((arena, binder, root, content)) = self.parse_and_bind_file(file_path) {
                let line_map = LineMap::build(&content);
                let provider = CodeActionProvider::new(
                    &arena,
                    &binder,
                    &line_map,
                    file_path.to_string(),
                    &content,
                );

                let diagnostics = self.get_semantic_diagnostics_full(file_path, &content);
                
                let filtered_diagnostics: Vec<tsz::lsp::diagnostics::LspDiagnostic> = diagnostics.into_iter()
                    .filter(|d| {
                        CodeFixRegistry::fixes_for_error_code(d.code).iter().any(|(_, id, _, _)| *id == fix_id)
                    })
                    .map(|d| tsz::lsp::diagnostics::LspDiagnostic {
                        range: tsz::lsp::position::Range::new(
                            line_map.offset_to_position(d.start, &content),
                            line_map.offset_to_position(d.start + d.length, &content),
                        ),
                        message: d.message_text,
                        code: Some(d.code),
                        severity: Some(tsz::lsp::diagnostics::DiagnosticSeverity::Error),
                        source: Some("tsz".to_string()),
                        related_information: None,
                        reports_unnecessary: tsz::lsp::diagnostics::is_unnecessary_code(d.code).then_some(true),
                        reports_deprecated: tsz::lsp::diagnostics::is_deprecated_code(d.code).then_some(true),
                    })
                    .collect();

                let import_candidates = if fix_id == "fixMissingImport" {
                    self.collect_import_candidates(file_path)
                } else {
                    Vec::new()
                };

                let context = CodeActionContext {
                    diagnostics: filtered_diagnostics,
                    only: Some(vec![CodeActionKind::QuickFix]),
                    import_candidates,
                };
                
                let range = tsz::lsp::position::Range::new(
                    tsz::lsp::position::Position::new(0, 0),
                    line_map.offset_to_position(content.len() as u32, &content),
                );
                
                let actions = provider.provide_code_actions(root, range, context);
                
                let mut file_changes_map: rustc_hash::FxHashMap<String, Vec<tsz::lsp::rename::TextEdit>> = rustc_hash::FxHashMap::default();
                
                for action in actions {
                    if let Some(edit) = action.edit {
                        for (fname, edits) in edit.changes {
                            file_changes_map.entry(fname).or_default().extend(edits);
                        }
                    }
                }
                
                let mut all_changes: Vec<serde_json::Value> = Vec::new();
                for (fname, edits) in file_changes_map {
                    let mut text_changes = Vec::new();
                    for edit in edits {
                        text_changes.push(serde_json::json!({
                            "start": {
                                "line": edit.range.start.line + 1,
                                "offset": edit.range.start.character + 1
                            },
                            "end": {
                                "line": edit.range.end.line + 1,
                                "offset": edit.range.end.character + 1
                            },
                            "newText": edit.new_text
                        }));
                    }
                    
                    all_changes.push(serde_json::json!({
                        "fileName": fname,
                        "textChanges": text_changes
                    }));
                }
                
                return TsServerResponse {
                    seq,
                    msg_type: "response".to_string(),
                    command: "getCombinedCodeFix".to_string(),
                    request_seq: request.seq,
                    success: true,
                    message: None,
                    body: Some(serde_json::json!({
                        "changes": all_changes
                    })),
                };
            }
        }

        self.stub_response(seq, request, Some(serde_json::json!({"changes": []})))
    }
}
