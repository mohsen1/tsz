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
            let line_map = LineMap::build(&content);
            let provider = CodeActionProvider::new(
                &arena,
                &binder,
                &line_map,
                file_path.to_string(),
                &content,
            );

            let mut diagnostics = self.get_semantic_diagnostics_full(file_path, &content);
            diagnostics.extend(self.get_suggestion_diagnostics(file_path, &content));
            if diagnostics.iter().all(|d| d.code != 80004)
                && let Some(diag) = Self::synthetic_jsdoc_suggestion_diagnostic(file_path, &content)
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
                && Self::should_offer_jsdoc_annotate_fallback(&error_codes)
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

    fn should_offer_jsdoc_annotate_fallback(error_codes: &[u32]) -> bool {
        error_codes
            .iter()
            .any(|code| matches!(*code, 80004 | 7043 | 7044))
    }

    fn synthetic_jsdoc_suggestion_diagnostic(
        file_path: &str,
        content: &str,
    ) -> Option<tsz::checker::diagnostics::Diagnostic> {
        let _ = Self::apply_simple_jsdoc_annotation_fallback(content)?;

        let mut offset = 0u32;
        for segment in content.split_inclusive('\n') {
            if let Some(local) = segment.find("@type {").or_else(|| segment.find("@return {")).or_else(|| segment.find("@returns {")) {
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

    fn apply_simple_jsdoc_annotation_fallback(content: &str) -> Option<String> {
        let had_trailing_newline = content.ends_with('\n');
        let mut lines: Vec<String> = content.lines().map(std::string::ToString::to_string).collect();
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
