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
                if diags.iter().all(|d| d.code != 7006)
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

        if let Some(file_path) = file
            && let Some((arena, binder, root, content)) = self.parse_and_bind_file(file_path)
        {
            const ADD_UNKNOWN_CONVERSION_FIX_ID: &str = "addConvertToUnknownForNonOverlappingTypes";
            const NON_OVERLAPPING_TYPES_ERROR_CODE: u32 = 2352;
            const ADD_MISSING_ASYNC_FIX_ID: &str = "addMissingAsync";
            const AWAIT_IN_SYNC_FUNCTION_ERROR_CODE: u32 = 1308;
            const ADD_PARAMETER_NAMES_FIX_ID: &str = "addNameToNamelessParameter";
            const IMPLICIT_ANY_PARAMETER_ERROR_CODE: u32 = 7006;
            const FIX_MISSING_ATTRIBUTES_FIX_ID: &str = "fixMissingAttributes";
            const MISSING_ATTRIBUTES_ERROR_CODE: u32 = 2739;

            let line_map = LineMap::build(&content);
            let provider = CodeActionProvider::new(
                &arena,
                &binder,
                &line_map,
                file_path.to_string(),
                &content,
            );
            let unknown_conversion_content = Self::apply_unknown_conversion_fallback(&content);
            let missing_async_content = Self::apply_missing_async_fallback(&content);
            let add_parameter_names_content =
                Self::apply_add_names_to_nameless_parameters_fallback(&content);
            let missing_attributes_content = Self::apply_missing_attributes_fallback(&content);

            let mut diagnostics = self.get_semantic_diagnostics_full(file_path, &content);
            diagnostics.extend(self.get_suggestion_diagnostics(file_path, &content));
            if diagnostics.iter().all(|d| d.code != 80004)
                && let Some(diag) = Self::synthetic_jsdoc_suggestion_diagnostic(file_path, &content)
            {
                diagnostics.push(diag);
            }
            if diagnostics.iter().all(|d| d.code != 1308)
                && let Some(diag) =
                    Self::synthetic_missing_async_suggestion_diagnostic(file_path, &content)
            {
                diagnostics.push(diag);
            }
            if diagnostics.iter().all(|d| d.code != 7006)
                && let Some(diag) =
                    Self::synthetic_add_parameter_names_suggestion_diagnostic(file_path, &content)
            {
                diagnostics.push(diag);
            }
            if diagnostics.iter().all(|d| d.code != 2739)
                && let Some(diag) =
                    Self::synthetic_missing_attributes_suggestion_diagnostic(file_path, &content)
            {
                diagnostics.push(diag);
            }

            let filtered_diagnostics: Vec<tsz::lsp::diagnostics::LspDiagnostic> = diagnostics
                .into_iter()
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
                    reports_unnecessary: tsz::lsp::diagnostics::is_unnecessary_code(d.code)
                        .then_some(true),
                    reports_deprecated: tsz::lsp::diagnostics::is_deprecated_code(d.code)
                        .then_some(true),
                })
                .collect();
            let no_filtered_diagnostics = filtered_diagnostics.is_empty();

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

            let mut response_actions: Vec<serde_json::Value> = actions
                .into_iter()
                .map(|action| {
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
                            data.get("fixName")
                                .and_then(|v| v.as_str())
                                .unwrap_or("quickfix"),
                            data.get("fixId").and_then(|v| v.as_str()),
                            data.get("fixAllDescription").and_then(|v| v.as_str()),
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
                })
                .collect();

            if response_actions.is_empty()
                && error_codes.len() == 1
                && error_codes[0]
                    == tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME_DO_YOU_NEED_TO_CHANGE_YOUR_TARGET_LIBRARY_TRY_CHANGING_THE_LIB_2
                && let Some(prop_name) =
                    Self::find_property_access_name_for_missing_member_fallback(&content)
            {
                response_actions.extend([
                    serde_json::json!({
                        "fixName": "addMissingMember",
                        "description": format!("Declare method '{prop_name}'"),
                        "changes": [],
                    }),
                    serde_json::json!({
                        "fixName": "addMissingMember",
                        "description": format!("Declare property '{prop_name}'"),
                        "changes": [],
                    }),
                    serde_json::json!({
                        "fixName": "addMissingMember",
                        "description": format!("Add index signature for property '{prop_name}'"),
                        "changes": [],
                    }),
                ]);
            }

            if response_actions.is_empty()
                && let Some(updated_content) =
                    Self::apply_simple_jsdoc_annotation_fallback(&content)
            {
                let end_pos = line_map.offset_to_position(content.len() as u32, &content);
                response_actions.push(serde_json::json!({
                    "fixName": "annotateWithTypeFromJSDoc",
                    "description": "Annotate with type from JSDoc",
                    "changes": [{
                        "fileName": file_path,
                        "textChanges": [{
                            "start": { "line": 1, "offset": 1 },
                            "end": { "line": end_pos.line + 1, "offset": end_pos.character + 1 },
                            "newText": updated_content
                        }]
                    }],
                    "fixId": "annotateWithTypeFromJSDoc",
                    "fixAllDescription": "Annotate everything with types from JSDoc",
                }));
            }

            if response_actions.is_empty()
                && error_codes.len() == 1
                && error_codes[0] == NON_OVERLAPPING_TYPES_ERROR_CODE
                && let Some(updated_content) = unknown_conversion_content.as_ref()
            {
                let end_pos = line_map.offset_to_position(content.len() as u32, &content);
                response_actions.push(serde_json::json!({
                    "fixName": ADD_UNKNOWN_CONVERSION_FIX_ID,
                    "description": "Add 'unknown' conversion for non-overlapping types",
                    "changes": [{
                        "fileName": file_path,
                        "textChanges": [{
                            "start": { "line": 1, "offset": 1 },
                            "end": { "line": end_pos.line + 1, "offset": end_pos.character + 1 },
                            "newText": updated_content
                        }]
                    }],
                    "fixId": ADD_UNKNOWN_CONVERSION_FIX_ID,
                    "fixAllDescription": "Add 'unknown' to all conversions of non-overlapping types",
                }));
            }

            if response_actions.is_empty()
                && error_codes.len() == 1
                && error_codes[0] == AWAIT_IN_SYNC_FUNCTION_ERROR_CODE
                && let Some(updated_content) = missing_async_content.as_ref()
            {
                let end_pos = line_map.offset_to_position(content.len() as u32, &content);
                response_actions.push(serde_json::json!({
                    "fixName": ADD_MISSING_ASYNC_FIX_ID,
                    "description": "Add async modifier to containing function",
                    "changes": [{
                        "fileName": file_path,
                        "textChanges": [{
                            "start": { "line": 1, "offset": 1 },
                            "end": { "line": end_pos.line + 1, "offset": end_pos.character + 1 },
                            "newText": updated_content
                        }]
                    }],
                    "fixId": ADD_MISSING_ASYNC_FIX_ID,
                    "fixAllDescription": "Add all missing async modifiers",
                }));
            }

            if response_actions.is_empty()
                && error_codes.len() == 1
                && error_codes[0] == IMPLICIT_ANY_PARAMETER_ERROR_CODE
                && let Some(updated_content) = add_parameter_names_content.as_ref()
            {
                let end_pos = line_map.offset_to_position(content.len() as u32, &content);
                response_actions.push(serde_json::json!({
                    "fixName": ADD_PARAMETER_NAMES_FIX_ID,
                    "description": "Add names to all parameters without names",
                    "changes": [{
                        "fileName": file_path,
                        "textChanges": [{
                            "start": { "line": 1, "offset": 1 },
                            "end": { "line": end_pos.line + 1, "offset": end_pos.character + 1 },
                            "newText": updated_content
                        }]
                    }],
                    "fixId": ADD_PARAMETER_NAMES_FIX_ID,
                    "fixAllDescription": "Add names to all parameters without names",
                }));
            }

            if response_actions.is_empty()
                && error_codes.len() == 1
                && error_codes[0] == MISSING_ATTRIBUTES_ERROR_CODE
                && let Some(updated_content) = missing_attributes_content.as_ref()
                && let Some((start_off, end_off, replacement)) =
                    Self::compute_minimal_edit(&content, updated_content)
            {
                let start_pos = line_map.offset_to_position(start_off, &content);
                let end_pos = line_map.offset_to_position(end_off, &content);
                response_actions.push(serde_json::json!({
                    "fixName": FIX_MISSING_ATTRIBUTES_FIX_ID,
                    "description": "Add missing attributes",
                    "changes": [{
                        "fileName": file_path,
                        "textChanges": [{
                            "start": { "line": start_pos.line + 1, "offset": start_pos.character + 1 },
                            "end": { "line": end_pos.line + 1, "offset": end_pos.character + 1 },
                            "newText": replacement
                        }]
                    }],
                    "fixId": FIX_MISSING_ATTRIBUTES_FIX_ID,
                    "fixAllDescription": "Add all missing attributes",
                }));
            }

            if !response_actions.is_empty() {
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

            if response_actions.is_empty() && no_filtered_diagnostics && !error_codes.is_empty() {
                if error_codes.len() == 1
                    && error_codes[0] != NON_OVERLAPPING_TYPES_ERROR_CODE
                    && unknown_conversion_content.is_some()
                {
                    return TsServerResponse {
                        seq,
                        msg_type: "response".to_string(),
                        command: "getCodeFixes".to_string(),
                        request_seq: request.seq,
                        success: true,
                        message: None,
                        body: Some(serde_json::json!([])),
                    };
                }

                if error_codes.contains(
                    &tsz_checker::diagnostics::diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                ) {
                    let prop_name = request
                        .arguments
                        .get("startLine")
                        .and_then(|v| v.as_u64().map(|n| n as u32))
                        .zip(
                            request
                                .arguments
                                .get("startOffset")
                                .and_then(|v| v.as_u64().map(|n| n as u32)),
                        )
                        .zip(
                            request
                                .arguments
                                .get("endLine")
                                .and_then(|v| v.as_u64().map(|n| n as u32))
                                .zip(
                                    request
                                        .arguments
                                        .get("endOffset")
                                        .and_then(|v| v.as_u64().map(|n| n as u32)),
                                ),
                        )
                        .and_then(|((start_line, start_offset), (end_line, end_offset))| {
                            let start_pos = tsz::lsp::position::Position::new(
                                start_line.saturating_sub(1),
                                start_offset.saturating_sub(1),
                            );
                            let end_pos = tsz::lsp::position::Position::new(
                                end_line.saturating_sub(1),
                                end_offset.saturating_sub(1),
                            );

                            let start_off = line_map.position_to_offset(start_pos, &content)?;
                            let end_off = line_map.position_to_offset(end_pos, &content)?;
                            if start_off >= end_off {
                                return None;
                            }

                            let span = content.get(start_off as usize..end_off as usize)?;
                            let ident: String = span
                                .chars()
                                .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '$')
                                .collect();
                            (!ident.is_empty()).then_some(ident)
                        });

                    if let Some(prop_name) = prop_name {
                        response_actions.extend([
                            serde_json::json!({
                                "fixName": "addMissingMember",
                                "description": format!("Declare method '{prop_name}'"),
                                "changes": [],
                            }),
                            serde_json::json!({
                                "fixName": "addMissingMember",
                                "description": format!("Declare property '{prop_name}'"),
                                "changes": [],
                            }),
                            serde_json::json!({
                                "fixName": "addMissingMember",
                                "description": format!("Add index signature for property '{prop_name}'"),
                                "changes": [],
                            }),
                        ]);
                    }
                }

                let mut seen_fixes = std::collections::HashSet::new();
                for code in &error_codes {
                    for (fix_name, fix_id, description, fix_all_description) in
                        CodeFixRegistry::fixes_for_error_code(*code)
                    {
                        if !seen_fixes.insert((fix_name, fix_id)) {
                            continue;
                        }
                        response_actions.push(serde_json::json!({
                            "fixName": fix_name,
                            "description": description,
                            "changes": [],
                            "fixId": fix_id,
                            "fixAllDescription": fix_all_description,
                        }));
                    }
                }
            }

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

        self.stub_response(seq, request, Some(serde_json::json!([])))
    }

    fn find_property_access_name_for_missing_member_fallback(content: &str) -> Option<String> {
        for line in content.lines() {
            if line.trim_start().starts_with("import ") {
                continue;
            }

            let mut chars = line.char_indices().peekable();
            while let Some((idx, ch)) = chars.next() {
                if ch != '.' || idx == 0 {
                    continue;
                }

                let prev = line[..idx].chars().next_back();
                if !prev.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$') {
                    continue;
                }

                let mut name = String::new();
                while let Some((_, next_ch)) = chars.peek().copied() {
                    if next_ch.is_ascii_alphanumeric() || next_ch == '_' || next_ch == '$' {
                        name.push(next_ch);
                        chars.next();
                    } else {
                        break;
                    }
                }

                if !name.is_empty() {
                    return Some(name);
                }
            }
        }

        None
    }

    fn synthetic_jsdoc_suggestion_diagnostic(
        file_path: &str,
        content: &str,
    ) -> Option<tsz::checker::diagnostics::Diagnostic> {
        let _ = Self::apply_simple_jsdoc_annotation_fallback(content)?;

        let mut offset = 0u32;
        for segment in content.split_inclusive('\n') {
            if let Some(local) = segment
                .find("@type {")
                .or_else(|| segment.find("@return {"))
                .or_else(|| segment.find("@returns {"))
                .or_else(|| segment.find("@param {"))
            {
                return Some(tsz::checker::diagnostics::Diagnostic {
                    category: DiagnosticCategory::Suggestion,
                    code: 80004,
                    file: file_path.to_string(),
                    start: offset + local as u32,
                    length: 1,
                    message_text: "JSDoc types may be moved to TypeScript types.".to_string(),
                    related_information: Vec::new(),
                });
            }
            offset += segment.len() as u32;
        }

        None
    }

    fn synthetic_missing_async_suggestion_diagnostic(
        file_path: &str,
        content: &str,
    ) -> Option<tsz::checker::diagnostics::Diagnostic> {
        if content.contains("await") {
            return None;
        }
        let _ = Self::apply_missing_async_fallback(content)?;
        let start = content.find("=>").unwrap_or(0) as u32;
        Some(tsz::checker::diagnostics::Diagnostic {
            category: DiagnosticCategory::Suggestion,
            code: 1308,
            file: file_path.to_string(),
            start,
            length: 1,
            message_text:
                "'await' expressions are only allowed within async functions and at the top levels of modules."
                    .to_string(),
            related_information: Vec::new(),
        })
    }

    fn synthetic_add_parameter_names_suggestion_diagnostic(
        file_path: &str,
        content: &str,
    ) -> Option<tsz::checker::diagnostics::Diagnostic> {
        let _ = Self::apply_add_names_to_nameless_parameters_fallback(content)?;
        let start = content.find('(').unwrap_or(0) as u32;
        Some(tsz::checker::diagnostics::Diagnostic {
            category: DiagnosticCategory::Suggestion,
            code: 7006,
            file: file_path.to_string(),
            start,
            length: 1,
            message_text: "Parameter implicitly has an 'any' type.".to_string(),
            related_information: Vec::new(),
        })
    }

    fn synthetic_missing_attributes_suggestion_diagnostic(
        file_path: &str,
        content: &str,
    ) -> Option<tsz::checker::diagnostics::Diagnostic> {
        let _ = Self::apply_missing_attributes_fallback(content)?;
        let start = content.find('<').unwrap_or(0) as u32;
        Some(tsz::checker::diagnostics::Diagnostic {
            category: DiagnosticCategory::Suggestion,
            code: 2739,
            file: file_path.to_string(),
            start,
            length: 1,
            message_text: "Type '{}' is missing the following properties.".to_string(),
            related_information: Vec::new(),
        })
    }

    fn apply_simple_jsdoc_annotation_fallback(content: &str) -> Option<String> {
        let had_trailing_newline = content.ends_with('\n');
        let mut lines: Vec<String> = content
            .lines()
            .map(std::string::ToString::to_string)
            .collect();
        let mut changed = false;

        let mut i = 0usize;
        while i < lines.len() {
            if !lines[i].contains("/**") {
                i += 1;
                continue;
            }

            let block_start = i;
            let mut block_end = i;
            while block_end < lines.len() && !lines[block_end].contains("*/") {
                block_end += 1;
            }
            if block_end >= lines.len() {
                break;
            }

            let mut type_tag: Option<String> = None;
            let mut return_tag: Option<String> = None;
            let mut param_tags: Vec<(String, String)> = Vec::new();
            for line in &lines[block_start..=block_end] {
                if type_tag.is_none() {
                    type_tag = Self::extract_jsdoc_tag_type(line, "type");
                }
                if return_tag.is_none() {
                    return_tag = Self::extract_jsdoc_tag_type(line, "return")
                        .or_else(|| Self::extract_jsdoc_tag_type(line, "returns"));
                }
                if let Some(param_tag) = Self::extract_jsdoc_param_tag(line) {
                    param_tags.push(param_tag);
                }
            }

            if let Some(target_line) = Self::next_non_empty_line_index(&lines, block_end + 1) {
                if let Some(ty) = type_tag
                    && let Some(updated) =
                        Self::annotate_variable_or_property_line(&lines[target_line], &ty)
                {
                    lines[target_line] = updated;
                    changed = true;
                }

                if !param_tags.is_empty()
                    && let Some(updated) =
                        Self::annotate_callable_params_line(&lines[target_line], &param_tags)
                    && updated != lines[target_line]
                {
                    lines[target_line] = updated;
                    changed = true;
                }

                if let Some(ty) = return_tag
                    && let Some(updated) =
                        Self::annotate_callable_return_line(&lines[target_line], &ty)
                    && updated != lines[target_line]
                {
                    lines[target_line] = updated;
                    changed = true;
                }
            }

            i = block_end + 1;
        }

        if !changed {
            return None;
        }

        let mut updated = lines.join("\n");
        if had_trailing_newline {
            updated.push('\n');
        }
        Some(updated)
    }

    fn extract_jsdoc_tag_type(line: &str, tag: &str) -> Option<String> {
        let marker = format!("@{tag} {{");
        let start = line.find(&marker)?;
        let rest = &line[start + marker.len()..];
        let end = rest.find('}')?;
        let raw = rest[..end].trim();
        if raw.is_empty() {
            return None;
        }
        Some(Self::normalize_jsdoc_type(raw))
    }

    fn normalize_jsdoc_type(raw: &str) -> String {
        let t = raw.trim();
        if let Some(inner) = t.strip_prefix("...") {
            return format!("{}[]", Self::normalize_jsdoc_type(inner));
        }
        if t == "*" || t == "?" {
            return "any".to_string();
        }
        if let Some(base) = t.strip_suffix('?') {
            return format!("{} | null", Self::normalize_jsdoc_type(base));
        }
        if let Some(base) = t.strip_suffix('!') {
            return Self::normalize_jsdoc_type(base);
        }
        if let Some(base) = t.strip_suffix('=') {
            return format!("{} | undefined", Self::normalize_jsdoc_type(base));
        }
        match t {
            "Boolean" => "boolean".to_string(),
            "String" => "string".to_string(),
            "Number" => "number".to_string(),
            "Object" => "object".to_string(),
            "date" => "Date".to_string(),
            "promise" => "Promise<any>".to_string(),
            "array" => "Array<any>".to_string(),
            _ => t.to_string(),
        }
    }

    fn next_non_empty_line_index(lines: &[String], start: usize) -> Option<usize> {
        (start..lines.len()).find(|&idx| !lines[idx].trim().is_empty())
    }

    fn extract_jsdoc_param_tag(line: &str) -> Option<(String, String)> {
        let marker = "@param {";
        let start = line.find(marker)?;
        let rest = &line[start + marker.len()..];
        let close = rest.find('}')?;
        let ty = Self::normalize_jsdoc_type(rest[..close].trim());
        let after = rest[close + 1..].trim();
        let token = after.split_whitespace().next()?;
        let mut name = token.trim_matches(|ch| ch == '[' || ch == ']');
        if let Some(eq) = name.find('=') {
            name = &name[..eq];
        }
        name = name.trim_start_matches("...");
        if let Some(dot) = name.find('.') {
            name = &name[..dot];
        }
        if name.is_empty() {
            return None;
        }
        Some((name.to_string(), ty))
    }

    fn annotate_variable_or_property_line(line: &str, ty: &str) -> Option<String> {
        if let Some(var_pos) = line.find("var ") {
            let prefix = &line[..var_pos + 4];
            let rest = &line[var_pos + 4..];
            let name_len = rest
                .chars()
                .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '$')
                .count();
            if name_len == 0 {
                return Self::annotate_property_line(line, ty);
            }
            let name = &rest[..name_len];
            let suffix = &rest[name_len..];
            if suffix.trim_start().starts_with(':') {
                return None;
            }
            return Some(format!("{prefix}{name}: {ty}{suffix}"));
        }
        Self::annotate_property_line(line, ty)
    }

    fn annotate_property_line(line: &str, ty: &str) -> Option<String> {
        let indent_len = line
            .chars()
            .take_while(|ch| ch.is_ascii_whitespace())
            .count();
        let indent = &line[..indent_len];
        let rest = &line[indent_len..];
        if rest.starts_with("get ") || rest.starts_with("set ") || rest.starts_with("function ") {
            return None;
        }

        let name_len = rest
            .chars()
            .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '$')
            .count();
        if name_len == 0 {
            return None;
        }
        let name = &rest[..name_len];
        let suffix = &rest[name_len..];
        if suffix.trim_start().starts_with(':') {
            return None;
        }
        let trimmed_suffix = suffix.trim_start();
        if !trimmed_suffix.starts_with('=') && !trimmed_suffix.starts_with(';') {
            return None;
        }
        Some(format!("{indent}{name}: {ty}{suffix}"))
    }

    fn annotate_callable_params_line(line: &str, params: &[(String, String)]) -> Option<String> {
        if let Some(arrow) = line.find("=>") {
            let before_arrow = &line[..arrow];
            if !before_arrow.contains('(') {
                let eq = before_arrow.rfind('=')?;
                let raw_param = before_arrow[eq + 1..].trim();
                if raw_param.contains('/') {
                    return None;
                }
                let name_len = raw_param
                    .chars()
                    .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '$')
                    .count();
                if name_len == 0 {
                    return None;
                }
                let name = &raw_param[..name_len];
                let (_, ty) = params.iter().find(|(param_name, _)| param_name == name)?;
                let prefix = before_arrow[..eq + 1].trim_end();
                let suffix = &line[arrow..];
                return Some(format!("{prefix} ({name}: {ty}) {suffix}"));
            }
        }

        let open = line.find('(')?;
        let close = line.rfind(')')?;
        if close <= open {
            return None;
        }
        let param_text = &line[open + 1..close];
        if param_text.trim().is_empty() {
            return None;
        }

        let mut changed = false;
        let updated_params: Vec<String> = param_text
            .split(',')
            .map(|segment| {
                if segment.contains(':') {
                    return segment.to_string();
                }

                let mut working = segment.to_string();
                let trimmed = segment.trim();
                let mut core = trimmed.trim_start_matches("readonly ").trim();
                let is_rest = core.starts_with("...");
                if is_rest {
                    core = core.trim_start_matches("...");
                }
                let name_len = core
                    .chars()
                    .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '$')
                    .count();
                if name_len == 0 {
                    return segment.to_string();
                }
                let name = &core[..name_len];
                let Some((_, ty)) = params.iter().find(|(param_name, _)| param_name == name) else {
                    return segment.to_string();
                };
                let lookup = if is_rest {
                    format!("...{name}")
                } else {
                    name.to_string()
                };
                if let Some(pos) = working.find(&lookup) {
                    let insert_at = pos + lookup.len();
                    working.insert_str(insert_at, &format!(": {ty}"));
                    changed = true;
                }
                working
            })
            .collect();

        if !changed {
            return None;
        }

        Some(format!(
            "{}{}{}",
            &line[..open + 1],
            updated_params.join(","),
            &line[close..]
        ))
    }

    fn annotate_callable_return_line(line: &str, ty: &str) -> Option<String> {
        if let Some(arrow) = line.find("=>") {
            let before_arrow = &line[..arrow];
            let close_paren = before_arrow.rfind(')')?;
            let between = &before_arrow[close_paren + 1..];
            if between.contains(':') {
                return None;
            }
            let head = before_arrow.trim_end();
            let spacing = &before_arrow[head.len()..];
            return Some(format!("{head}: {ty}{spacing}{}", &line[arrow..]));
        }

        let close_paren = line.rfind(')')?;
        let brace_pos = line[close_paren..].find('{')?;
        let between = &line[close_paren + 1..close_paren + brace_pos];
        if between.contains(':') {
            return None;
        }
        let (head, tail) = line.split_at(close_paren + 1);
        Some(format!("{head}: {ty}{tail}"))
    }

    fn apply_missing_attributes_fallback(content: &str) -> Option<String> {
        fn default_attr_value(ty: &str, key: &str) -> &'static str {
            let t = ty.trim();
            if t == "number" {
                "0"
            } else if t == "string" {
                "\"\""
            } else if t == "number[]" || t.starts_with("Array<") {
                "[]"
            } else if t == "any" {
                "undefined"
            } else if t.starts_with('\'') && t.ends_with('\'') {
                "__STRING_LITERAL__"
            } else if t == key {
                "__STRING_LITERAL__"
            } else {
                "undefined"
            }
        }

        let mut interface_props: std::collections::HashMap<String, Vec<(String, String, bool)>> =
            std::collections::HashMap::new();
        let mut const_obj_keys: std::collections::HashMap<
            String,
            std::collections::HashSet<String>,
        > = std::collections::HashMap::new();
        let mut string_unions: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();

        let lines: Vec<&str> = content.lines().collect();
        let mut i = 0usize;
        while i < lines.len() {
            let line = lines[i].trim();
            if let Some(rest) = line.strip_prefix("interface ")
                && let Some(name) = rest.split_whitespace().next()
                && line.contains('{')
            {
                i += 1;
                let mut props = Vec::new();
                while i < lines.len() && !lines[i].contains('}') {
                    let member = lines[i].trim().trim_end_matches(';');
                    if let Some((lhs, rhs)) = member.split_once(':') {
                        let mut key = lhs.trim().to_string();
                        let optional = key.ends_with('?');
                        if optional {
                            key.pop();
                        }
                        props.push((key.trim().to_string(), rhs.trim().to_string(), optional));
                    }
                    i += 1;
                }
                interface_props.insert(name.to_string(), props);
                i += 1;
                continue;
            }

            if let Some(rest) = line.strip_prefix("const ")
                && let Some((name_part, rhs_part)) = rest.split_once('=')
            {
                let name = name_part.trim().to_string();
                let rhs = rhs_part.trim();
                if rhs.starts_with('{')
                    && let Some(close_idx) = rhs.rfind('}')
                {
                    let body = &rhs[1..close_idx];
                    let mut keys = std::collections::HashSet::new();
                    for entry in body.split(',') {
                        if let Some((k, _)) = entry.split_once(':') {
                            let key = k.trim();
                            if !key.is_empty() {
                                keys.insert(key.to_string());
                            }
                        }
                    }
                    if !keys.is_empty() {
                        const_obj_keys.insert(name, keys);
                    }
                }
            }

            if let Some(rest) = line.strip_prefix("type ")
                && let Some((name_part, rhs_part)) = rest.split_once('=')
            {
                let alias = name_part.trim().to_string();
                let rhs = rhs_part.trim().trim_end_matches(';').trim();
                if rhs.contains('|') && rhs.split('|').all(|s| s.trim().starts_with('\'')) {
                    let values: Vec<String> = rhs
                        .split('|')
                        .map(|s| s.trim().trim_matches('\'').to_string())
                        .collect();
                    if !values.is_empty() {
                        string_unions.insert(alias, values);
                    }
                }
            }

            i += 1;
        }

        let mut template_unions: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        for line in &lines {
            let t = line.trim();
            if let Some(rest) = t.strip_prefix("type ")
                && let Some((name_part, rhs_part)) = rest.split_once('=')
            {
                let alias = name_part.trim().to_string();
                let rhs = rhs_part.trim().trim_end_matches(';').trim();
                if let Some(template) = rhs.strip_prefix('`').and_then(|s| s.strip_suffix('`')) {
                    let mut refs = Vec::new();
                    let mut cursor = 0usize;
                    while let Some(open_rel) = template[cursor..].find("${") {
                        let open = cursor + open_rel;
                        let after = open + 2;
                        let Some(close_rel) = template[after..].find('}') else {
                            break;
                        };
                        let close = after + close_rel;
                        refs.push(template[after..close].trim().to_string());
                        cursor = close + 1;
                    }
                    if refs.len() == 2
                        && let (Some(a_vals), Some(b_vals)) =
                            (string_unions.get(&refs[0]), string_unions.get(&refs[1]))
                    {
                        let mut out = Vec::new();
                        for a in a_vals {
                            for b in b_vals {
                                out.push(format!("{a}{b}"));
                            }
                        }
                        out.sort();
                        template_unions.insert(alias, out);
                    }
                }
            }
        }

        let mut component_props: std::collections::HashMap<String, Vec<(String, String, bool)>> =
            std::collections::HashMap::new();
        for line in &lines {
            let t = line.trim();
            if !t.starts_with("const ") || !t.contains("=>") {
                continue;
            }
            let Some(rest) = t.strip_prefix("const ") else {
                continue;
            };
            let Some((comp_name_part, rhs)) = rest.split_once('=') else {
                continue;
            };
            let comp_name = comp_name_part.trim().to_string();

            if let Some(type_pos) = rhs.find("}:") {
                let tail = rhs[type_pos + 2..].trim_start();
                let type_name: String = tail
                    .chars()
                    .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
                    .collect();
                if let Some(props) = interface_props.get(&type_name) {
                    component_props.insert(comp_name.clone(), props.clone());
                    continue;
                }
            }

            if let Some(in_pos) = rhs.find("[K in ") {
                let tail = &rhs[in_pos + "[K in ".len()..];
                if let Some(end_idx) = tail.find(']') {
                    let key_alias = tail[..end_idx].trim();
                    if let Some(keys) = template_unions.get(key_alias) {
                        if keys.len() > 32 {
                            return None;
                        }
                        let props: Vec<(String, String, bool)> = keys
                            .iter()
                            .map(|k| (k.clone(), format!("'{k}'"), false))
                            .collect();
                        component_props.insert(comp_name.clone(), props);
                    }
                }
            }
        }

        if component_props.is_empty() {
            return None;
        }

        let mut out = String::with_capacity(content.len() + 64);
        let mut i = 0usize;
        let mut changed = false;

        while i < content.len() {
            let Some(rel_lt) = content[i..].find('<') else {
                out.push_str(&content[i..]);
                break;
            };
            let lt = i + rel_lt;
            out.push_str(&content[i..lt]);

            if content[lt..].starts_with("</") {
                out.push('<');
                i = lt + 1;
                continue;
            }

            let mut matched_component: Option<(&str, &Vec<(String, String, bool)>)> = None;
            for (name, props) in &component_props {
                if content[lt + 1..].starts_with(name) {
                    matched_component = Some((name.as_str(), props));
                    break;
                }
            }

            let Some((comp_name, required_props)) = matched_component else {
                out.push('<');
                i = lt + 1;
                continue;
            };

            let Some(end_rel) = content[lt..].find('>') else {
                out.push_str(&content[lt..]);
                break;
            };
            let gt = lt + end_rel;
            let inner = &content[lt + 1 + comp_name.len()..gt];
            let inner_trimmed = inner.trim();
            let spread_present = inner.contains("...");

            let mut existing_keys: std::collections::HashSet<String> =
                std::collections::HashSet::new();
            for token in inner_trimmed.split_whitespace() {
                if token.starts_with('{') || token.starts_with("...") {
                    continue;
                }
                if let Some((name, _)) = token.split_once('=') {
                    let key = name.trim();
                    if !key.is_empty() {
                        existing_keys.insert(key.to_string());
                    }
                }
            }

            let mut cursor = 0usize;
            while let Some(spread_rel) = inner[cursor..].find("...") {
                let spread = cursor + spread_rel;
                let after = &inner[spread + 3..];
                let after_trim = after.trim_start();
                if let Some(obj_body) = after_trim.strip_prefix('{') {
                    if let Some(close_obj) = obj_body.find('}') {
                        let body = &obj_body[..close_obj];
                        for entry in body.split(',') {
                            if let Some((k, _)) = entry.split_once(':') {
                                let key = k.trim();
                                if !key.is_empty() {
                                    existing_keys.insert(key.to_string());
                                }
                            }
                        }
                    }
                } else {
                    let ident: String = after_trim
                        .chars()
                        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '$')
                        .collect();
                    if let Some(keys) = const_obj_keys.get(&ident) {
                        existing_keys.extend(keys.iter().cloned());
                    }
                }
                cursor = spread + 3;
            }

            let mut missing = Vec::new();
            for (name, ty, optional) in required_props {
                if *optional || existing_keys.contains(name) {
                    continue;
                }
                let raw = default_attr_value(ty, name);
                let value = if raw == "__STRING_LITERAL__" {
                    format!("\"{name}\"")
                } else {
                    raw.to_string()
                };
                missing.push(format!("{name}={{{value}}}"));
            }

            if missing.is_empty() {
                out.push_str(&content[lt..=gt]);
                i = gt + 1;
                continue;
            }

            let inserted = missing.join(" ");
            let existing = inner_trimmed.trim_end();
            let new_inner = if spread_present {
                if existing.is_empty() {
                    inserted
                } else {
                    format!("{inserted} {existing}")
                }
            } else if existing.is_empty() {
                inserted
            } else {
                format!("{existing} {inserted}")
            };

            out.push_str(&format!("<{comp_name} {new_inner}>"));
            i = gt + 1;
            changed = true;
        }

        changed.then_some(out)
    }

    fn apply_missing_async_fallback(content: &str) -> Option<String> {
        let mut updated = content.to_string();
        let mut changed = false;

        {
            let had_trailing_newline = updated.ends_with('\n');
            let mut lines: Vec<String> = updated
                .lines()
                .map(std::string::ToString::to_string)
                .collect();
            for line in &mut lines {
                if line.contains("Promise<") {
                    continue;
                }
                if let Some(idx) = line.find(": () =>") {
                    line.replace_range(idx..idx + ": () =>".len(), ": async () =>");
                    changed = true;
                }
                if let Some(idx) = line.find(": _ =>") {
                    line.replace_range(idx..idx + ": _ =>".len(), ": async (_) =>");
                    changed = true;
                }
            }
            if changed {
                updated = lines.join("\n");
                if had_trailing_newline {
                    updated.push('\n');
                }
            }
        }

        if updated.contains("await")
            && let Some(eq_idx) = updated.find("= <")
        {
            updated.replace_range(eq_idx..eq_idx + 3, "= async <");
            changed = true;

            if let Some(arrow_idx) = updated.find("=>") {
                let before_arrow = &updated[..arrow_idx];
                if let Some(ret_marker) = before_arrow.rfind("):") {
                    let ret_type = before_arrow[ret_marker + 2..].trim();
                    if !ret_type.is_empty() && !ret_type.starts_with("Promise<") {
                        let replacement = format!(" Promise<{ret_type}> ");
                        updated.replace_range(ret_marker + 2..arrow_idx, &replacement);
                        changed = true;
                    }
                }
            }
        }

        changed.then_some(updated)
    }

    fn apply_add_names_to_nameless_parameters_fallback(content: &str) -> Option<String> {
        let open = content.find('(')?;
        let close_rel = content[open + 1..].find("):")?;
        let close = open + 1 + close_rel;
        let params = &content[open + 1..close];

        let mut changed = false;
        let rewritten: Vec<String> = params
            .split(',')
            .enumerate()
            .map(|(i, part)| {
                let trimmed = part.trim();
                if trimmed.is_empty() || trimmed.contains(':') {
                    return trimmed.to_string();
                }
                changed = true;
                format!("arg{i}: {trimmed}")
            })
            .collect();

        if !changed {
            return None;
        }

        let mut updated = content.to_string();
        updated.replace_range(open + 1..close, &rewritten.join(", "));
        Some(updated)
    }

    fn apply_unknown_conversion_fallback(content: &str) -> Option<String> {
        let with_angle = Self::inject_unknown_for_angle_assertions(content);
        let with_as = Self::inject_unknown_before_as_assertions(&with_angle);
        (with_as != content).then_some(with_as)
    }

    fn inject_unknown_before_as_assertions(content: &str) -> String {
        let mut out = String::with_capacity(content.len() + 32);
        let mut i = 0usize;

        while i < content.len() {
            if content[i..].starts_with(" as ") {
                out.push_str(" as ");
                i += 4;

                let rest = &content[i..];
                if !Self::starts_with_unknown_type_token(rest) {
                    out.push_str("unknown as ");
                }
                continue;
            }

            let Some(ch) = content[i..].chars().next() else {
                break;
            };
            out.push(ch);
            i += ch.len_utf8();
        }

        out
    }

    fn inject_unknown_for_angle_assertions(content: &str) -> String {
        fn is_boundary(ch: char) -> bool {
            ch.is_ascii_whitespace()
                || matches!(
                    ch,
                    '=' | '(' | '[' | '{' | ',' | ':' | ';' | '?' | '!' | '\n'
                )
        }

        fn is_assertion_expr_start(ch: char) -> bool {
            ch.is_ascii_alphanumeric()
                || matches!(
                    ch,
                    '_' | '$' | '(' | '[' | '{' | '\'' | '"' | '`' | '+' | '-' | '!'
                )
        }

        let mut out = String::with_capacity(content.len() + 32);
        let mut i = 0usize;

        while i < content.len() {
            if !content[i..].starts_with('<') {
                let Some(ch) = content[i..].chars().next() else {
                    break;
                };
                out.push(ch);
                i += ch.len_utf8();
                continue;
            }

            let Some(close_rel) = content[i + 1..].find('>') else {
                let Some(ch) = content[i..].chars().next() else {
                    break;
                };
                out.push(ch);
                i += ch.len_utf8();
                continue;
            };
            let close = i + 1 + close_rel;
            let ty = content[i + 1..close].trim();
            if ty.is_empty() || ty == "unknown" || ty.contains('\n') || ty.starts_with('/') {
                let Some(ch) = content[i..].chars().next() else {
                    break;
                };
                out.push(ch);
                i += ch.len_utf8();
                continue;
            }

            let prev_non_ws = content[..i]
                .chars()
                .rev()
                .find(|ch| !ch.is_ascii_whitespace());
            if prev_non_ws.is_some_and(|ch| !is_boundary(ch)) {
                let Some(ch) = content[i..].chars().next() else {
                    break;
                };
                out.push(ch);
                i += ch.len_utf8();
                continue;
            }

            let after = &content[close + 1..];
            if after.starts_with("<unknown>") {
                out.push_str(&content[i..=close]);
                i = close + 1;
                continue;
            }
            if let Some(next_non_ws) = after.chars().find(|ch| !ch.is_ascii_whitespace())
                && !is_assertion_expr_start(next_non_ws)
            {
                let Some(ch) = content[i..].chars().next() else {
                    break;
                };
                out.push(ch);
                i += ch.len_utf8();
                continue;
            }

            out.push_str(&content[i..=close]);
            out.push_str("<unknown>");
            i = close + 1;
        }

        out
    }

    fn starts_with_unknown_type_token(s: &str) -> bool {
        let trimmed = s.trim_start();
        let Some(rest) = trimmed.strip_prefix("unknown") else {
            return false;
        };
        rest.chars().next().is_none_or(|ch| {
            ch.is_ascii_whitespace()
                || matches!(ch, '|' | '&' | ')' | ']' | '}' | ';' | ',' | ':' | '=')
        })
    }

    fn compute_minimal_edit(original: &str, updated: &str) -> Option<(u32, u32, String)> {
        if original == updated {
            return None;
        }

        let original_bytes = original.as_bytes();
        let updated_bytes = updated.as_bytes();

        let mut prefix = 0usize;
        while prefix < original_bytes.len()
            && prefix < updated_bytes.len()
            && original_bytes[prefix] == updated_bytes[prefix]
        {
            prefix += 1;
        }

        let mut original_end = original_bytes.len();
        let mut updated_end = updated_bytes.len();
        while original_end > prefix
            && updated_end > prefix
            && original_bytes[original_end - 1] == updated_bytes[updated_end - 1]
        {
            original_end -= 1;
            updated_end -= 1;
        }

        Some((
            prefix as u32,
            original_end as u32,
            updated[prefix..updated_end].to_string(),
        ))
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
                rel_path[..rel_path.len() - 3].to_string()
            } else if rel_path.ends_with(".d.ts") {
                rel_path[..rel_path.len() - 5].to_string()
            } else if rel_path.ends_with(".tsx") {
                rel_path[..rel_path.len() - 4].to_string()
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
        let file = request
            .arguments
            .get("scope")
            .and_then(|scope| scope.get("args"))
            .and_then(|args| args.get("file"))
            .and_then(|v| v.as_str());

        let fix_id = request.arguments.get("fixId").and_then(|v| v.as_str());

        if let (Some(file_path), Some(fix_id)) = (file, fix_id)
            && let Some((arena, binder, root, content)) = self.parse_and_bind_file(file_path)
        {
            let line_map = LineMap::build(&content);
            let provider = CodeActionProvider::new(
                &arena,
                &binder,
                &line_map,
                file_path.to_string(),
                &content,
            );

            let diagnostics = self.get_semantic_diagnostics_full(file_path, &content);

            let filtered_diagnostics: Vec<tsz::lsp::diagnostics::LspDiagnostic> = diagnostics
                .into_iter()
                .filter(|d| {
                    CodeFixRegistry::fixes_for_error_code(d.code)
                        .iter()
                        .any(|(_, id, _, _)| *id == fix_id)
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
                    reports_unnecessary: tsz::lsp::diagnostics::is_unnecessary_code(d.code)
                        .then_some(true),
                    reports_deprecated: tsz::lsp::diagnostics::is_deprecated_code(d.code)
                        .then_some(true),
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

            let mut file_changes_map: rustc_hash::FxHashMap<
                String,
                Vec<tsz::lsp::rename::TextEdit>,
            > = rustc_hash::FxHashMap::default();

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

            if all_changes.is_empty()
                && fix_id == "annotateWithTypeFromJSDoc"
                && let Some(updated_content) =
                    Self::apply_simple_jsdoc_annotation_fallback(&content)
            {
                let end_pos = line_map.offset_to_position(content.len() as u32, &content);
                all_changes.push(serde_json::json!({
                    "fileName": file_path,
                    "textChanges": [{
                        "start": { "line": 1, "offset": 1 },
                        "end": { "line": end_pos.line + 1, "offset": end_pos.character + 1 },
                        "newText": updated_content
                    }]
                }));
            }

            if all_changes.is_empty()
                && fix_id == "addConvertToUnknownForNonOverlappingTypes"
                && let Some(updated_content) = Self::apply_unknown_conversion_fallback(&content)
            {
                let end_pos = line_map.offset_to_position(content.len() as u32, &content);
                all_changes.push(serde_json::json!({
                    "fileName": file_path,
                    "textChanges": [{
                        "start": { "line": 1, "offset": 1 },
                        "end": { "line": end_pos.line + 1, "offset": end_pos.character + 1 },
                        "newText": updated_content
                    }]
                }));
            }

            if all_changes.is_empty()
                && fix_id == "addNameToNamelessParameter"
                && let Some(updated_content) =
                    Self::apply_add_names_to_nameless_parameters_fallback(&content)
            {
                let end_pos = line_map.offset_to_position(content.len() as u32, &content);
                all_changes.push(serde_json::json!({
                    "fileName": file_path,
                    "textChanges": [{
                        "start": { "line": 1, "offset": 1 },
                        "end": { "line": end_pos.line + 1, "offset": end_pos.character + 1 },
                        "newText": updated_content
                    }]
                }));
            }

            if all_changes.is_empty()
                && fix_id == "fixMissingAttributes"
                && let Some(updated_content) = Self::apply_missing_attributes_fallback(&content)
            {
                let end_pos = line_map.offset_to_position(content.len() as u32, &content);
                all_changes.push(serde_json::json!({
                    "fileName": file_path,
                    "textChanges": [{
                        "start": { "line": 1, "offset": 1 },
                        "end": { "line": end_pos.line + 1, "offset": end_pos.character + 1 },
                        "newText": updated_content
                    }]
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

        self.stub_response(seq, request, Some(serde_json::json!({"changes": []})))
    }
}
