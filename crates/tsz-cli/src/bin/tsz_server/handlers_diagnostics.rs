//! Diagnostic handlers for tsz-server.
//!
//! Handles semantic, syntactic, and suggestion diagnostic commands.

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
    pub(super) fn extract_auto_import_file_exclude_patterns(
        request: &TsServerRequest,
    ) -> Option<Vec<String>> {
        request
            .arguments
            .get("preferences")
            .and_then(|p| p.get("autoImportFileExcludePatterns"))
            .or_else(|| request.arguments.get("autoImportFileExcludePatterns"))
            .and_then(serde_json::Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(std::string::ToString::to_string))
                    .collect()
            })
    }

    pub(super) fn extract_auto_import_specifier_exclude_regexes(
        request: &TsServerRequest,
    ) -> Option<Vec<String>> {
        request
            .arguments
            .get("preferences")
            .and_then(|p| p.get("autoImportSpecifierExcludeRegexes"))
            .or_else(|| request.arguments.get("autoImportSpecifierExcludeRegexes"))
            .and_then(serde_json::Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(std::string::ToString::to_string))
                    .collect()
            })
    }

    pub(crate) fn handle_configure(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let preferences = request
            .arguments
            .get("preferences")
            .unwrap_or(&request.arguments);

        self.completion_import_module_specifier_ending = preferences
            .get("importModuleSpecifierEnding")
            .and_then(|v| v.as_str())
            .map(std::string::ToString::to_string);
        self.import_module_specifier_preference = preferences
            .get("importModuleSpecifierPreference")
            .and_then(|v| v.as_str())
            .map(std::string::ToString::to_string);
        self.organize_imports_type_order = preferences
            .get("organizeImportsTypeOrder")
            .and_then(|v| v.as_str())
            .map(std::string::ToString::to_string);
        self.organize_imports_ignore_case = preferences
            .get("organizeImportsIgnoreCase")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true);
        self.auto_import_file_exclude_patterns =
            Self::extract_auto_import_file_exclude_patterns(request).unwrap_or_default();
        self.auto_import_specifier_exclude_regexes =
            Self::extract_auto_import_specifier_exclude_regexes(request).unwrap_or_default();
        self.include_completions_with_class_member_snippets = preferences
            .get("includeCompletionsWithClassMemberSnippets")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);

        // Accept configuration; selected completion preferences are wired.
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
                let mut full_diags = self.get_semantic_diagnostics_full(file_path, &content);
                let has_module_none_diagnostic = full_diags.iter().any(|d| {
                    d.code
                        == tsz_checker::diagnostics::diagnostic_codes::CANNOT_USE_IMPORTS_EXPORTS_OR_MODULE_AUGMENTATIONS_WHEN_MODULE_IS_NONE
                });
                if !has_module_none_diagnostic
                    && full_diags.iter().all(|d| {
                        d.code != tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME
                    })
                    && let Some((_, binder, _, _)) = self.parse_and_bind_file(file_path)
                {
                    full_diags.extend(self.synthetic_missing_name_expression_diagnostics(
                        file_path, &content, &binder,
                    ));
                }
                if full_diags.iter().all(|d| d.code != 2420) {
                    full_diags.extend(
                        self.synthetic_implements_interface_diagnostics(file_path, &content),
                    );
                }
                full_diags
                    .iter()
                    .map(|diag| {
                        let (start_offset, length) =
                            Self::normalized_diagnostic_span(diag, &content);
                        Self::format_diagnostic(DiagnosticFormatInput {
                            start_offset,
                            length,
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

    fn normalized_diagnostic_span(
        diag: &tsz::checker::diagnostics::Diagnostic,
        content: &str,
    ) -> (u32, u32) {
        if diag.code
            != tsz_checker::diagnostics::diagnostic_codes::CANNOT_USE_IMPORTS_EXPORTS_OR_MODULE_AUGMENTATIONS_WHEN_MODULE_IS_NONE
        {
            return (diag.start, diag.length);
        }

        let Ok(start) = usize::try_from(diag.start) else {
            return (diag.start, diag.length);
        };
        let Some(mut end) = start.checked_add(diag.length as usize) else {
            return (diag.start, diag.length);
        };
        end = end.min(content.len());
        if start >= end || start >= content.len() {
            return (diag.start, diag.length);
        }

        let bytes = content.as_bytes();
        let slice = &bytes[start..end];
        if let Some(rel) = slice.iter().position(|b| *b == b';') {
            return (diag.start, (rel + 1) as u32);
        }
        if let Some(rel) = slice.iter().position(|b| *b == b'\n') {
            return (diag.start, rel as u32);
        }

        (diag.start, diag.length)
    }

    pub(crate) fn handle_suggestion_diagnostics_sync(
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
                let mut diags = self.get_suggestion_diagnostics(file_path, &content);
                if diags.iter().all(|d| d.code != 80004)
                    && let Some(diag) =
                        Self::synthetic_jsdoc_suggestion_diagnostic(file_path, &content)
                {
                    diags.push(diag);
                }
                if diags.iter().all(|d| d.code != 1308)
                    && let Some(diag) =
                        Self::synthetic_missing_async_suggestion_diagnostic(file_path, &content)
                {
                    diags.push(diag);
                }
                let has_jsdoc_type_tags = content.contains("@type {")
                    || content.contains("@param {")
                    || content.contains("@return {")
                    || content.contains("@returns {");
                if !has_jsdoc_type_tags
                    && diags.iter().all(|d| d.code != 7006)
                    && let Some(diag) = Self::synthetic_add_parameter_names_suggestion_diagnostic(
                        file_path, &content,
                    )
                {
                    diags.push(diag);
                }
                if diags.iter().all(|d| d.code != 2739)
                    && let Some(diag) = Self::synthetic_missing_attributes_suggestion_diagnostic(
                        file_path, &content,
                    )
                {
                    diags.push(diag);
                }
                if diags.iter().all(|d| d.code != 7043 && d.code != 7044) {
                    diags.extend(Self::synthetic_jsdoc_infer_from_usage_diagnostics(
                        file_path, &content,
                    ));
                }
                diags
                    .iter()
                    .map(|d| {
                        Self::format_diagnostic(DiagnosticFormatInput {
                            start_offset: d.start,
                            length: d.length,
                            message: &d.message_text,
                            code: d.code,
                            category: d.category,
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
            command: "suggestionDiagnosticsSync".to_string(),
            request_seq: request.seq,
            success: true,
            message: None,
            body: Some(serde_json::json!(diagnostics)),
        }
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
}
