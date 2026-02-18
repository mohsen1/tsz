//! Diagnostic handlers for tsz-server.
//!
//! Handles semantic, syntactic, and suggestion diagnostic commands,
//! plus code fix related handlers.

use super::{Server, TsServerRequest, TsServerResponse};
use tsz::checker::diagnostics::DiagnosticCategory;
use tsz::lsp::position::LineMap;
use tsz::parser::ParserState;

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
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?.to_string();
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

            if error_codes.is_empty() {
                // If no specific error codes, get all diagnostics for the file
                // and return fixes for all of them
                let content = self.open_files.get(&file)?.clone();
                let diags = self.get_semantic_diagnostics_full(&file, &content);
                let mut all_fixes = Vec::new();
                let mut seen_codes = rustc_hash::FxHashSet::default();
                for diag in &diags {
                    if !seen_codes.insert(diag.code) {
                        continue;
                    }
                    let fixes =
                        tsz::lsp::code_actions::CodeFixRegistry::fixes_for_error_code(diag.code);
                    for (fix_name, fix_id, description, fix_all_desc) in fixes {
                        all_fixes.push(serde_json::json!({
                            "fixName": fix_name,
                            "description": description,
                            "changes": [{
                                "fileName": file,
                                "textChanges": []
                            }],
                            "fixId": fix_id,
                            "fixAllDescription": fix_all_desc
                        }));
                    }
                }
                return Some(serde_json::json!(all_fixes));
            }

            let mut all_fixes = Vec::new();
            for error_code in &error_codes {
                let fixes =
                    tsz::lsp::code_actions::CodeFixRegistry::fixes_for_error_code(*error_code);
                for (fix_name, fix_id, description, fix_all_desc) in fixes {
                    all_fixes.push(serde_json::json!({
                        "fixName": fix_name,
                        "description": description,
                        "changes": [{
                            "fileName": file,
                            "textChanges": []
                        }],
                        "commands": [],
                        "fixId": fix_id,
                        "fixAllDescription": fix_all_desc
                    }));
                }
            }
            Some(serde_json::json!(all_fixes))
        })();
        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    pub(crate) fn handle_get_combined_code_fix(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        self.stub_response(seq, request, Some(serde_json::json!({"changes": []})))
    }
}
