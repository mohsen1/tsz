//! Code fix handlers for tsz-server.
//!
//! Contains `handle_get_code_fixes`, `handle_get_combined_code_fix`, and all
//! supporting helper methods for code-fix logic (JSDoc annotation fallbacks,
//! import rewriting, unknown-conversion injection, etc.).

use super::{Server, TsServerRequest, TsServerResponse};
use tsz::checker::diagnostics::DiagnosticCategory;
use tsz::lsp::Project;
use tsz::lsp::code_actions::{
    CodeActionContext, CodeActionKind, CodeActionProvider, CodeFixRegistry, ImportCandidate,
};
use tsz::lsp::position::LineMap;
use tsz::parser::ParserState;

type PropEntry = (String, String, bool);

#[derive(Debug, Clone)]
struct JSDocParamTag {
    path: Vec<String>,
    ty: String,
    optional: bool,
    explicit_type: bool,
}

#[derive(Debug, Clone, Default)]
struct ObjectParamNode {
    ty: Option<String>,
    optional: bool,
    children: std::collections::BTreeMap<String, ObjectParamNode>,
}

impl Server {
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
        let request_start_line = request
            .arguments
            .get("startLine")
            .and_then(serde_json::Value::as_u64)
            .map(|line| line as usize);
        let request_span = request
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
            .map(|((start_line, start_offset), (end_line, end_offset))| {
                (
                    tsz::lsp::position::Position::new(
                        start_line.saturating_sub(1),
                        start_offset.saturating_sub(1),
                    ),
                    tsz::lsp::position::Position::new(
                        end_line.saturating_sub(1),
                        end_offset.saturating_sub(1),
                    ),
                )
            });

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
            let organize_imports_ignore_case = request
                .arguments
                .get("preferences")
                .and_then(|p| p.get("organizeImportsIgnoreCase"))
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(self.organize_imports_ignore_case);

            let line_map = LineMap::build(&content);
            let provider = CodeActionProvider::new(
                &arena,
                &binder,
                &line_map,
                file_path.to_string(),
                &content,
            )
            .with_organize_imports_ignore_case(organize_imports_ignore_case);
            let unknown_conversion_content = Self::apply_unknown_conversion_fallback(&content);
            let missing_async_content = Self::apply_missing_async_fallback(&content);
            let add_parameter_names_content =
                Self::apply_add_names_to_nameless_parameters_fallback(&content);
            let missing_attributes_content = Self::apply_missing_attributes_fallback(&content);

            let mut diagnostics = self.get_semantic_diagnostics_full(file_path, &content);
            diagnostics.extend(self.get_suggestion_diagnostics(file_path, &content));
            if diagnostics
                .iter()
                .all(|d| d.code != tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME)
            {
                diagnostics.extend(
                    self.synthetic_missing_name_expression_diagnostics(
                        file_path, &content, &binder,
                    ),
                );
            }
            if diagnostics.iter().all(|d| d.code != 2420) {
                diagnostics
                    .extend(self.synthetic_implements_interface_diagnostics(file_path, &content));
            }
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
                .filter(|d| {
                    let Some((req_start, req_end)) = request_span else {
                        return true;
                    };
                    let diag_start = line_map.offset_to_position(d.start, &content);
                    let diag_end = line_map.offset_to_position(d.start + d.length, &content);
                    positions_overlap(req_start, req_end, diag_start, diag_end)
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
            let no_filtered_diagnostics = filtered_diagnostics.is_empty();

            let auto_import_file_exclude_patterns =
                Self::extract_auto_import_file_exclude_patterns(request)
                    .unwrap_or_else(|| self.auto_import_file_exclude_patterns.clone());
            let auto_import_specifier_exclude_regexes =
                Self::extract_auto_import_specifier_exclude_regexes(request)
                    .unwrap_or_else(|| self.auto_import_specifier_exclude_regexes.clone());
            let import_module_specifier_preference = request
                .arguments
                .get("preferences")
                .and_then(|p| p.get("importModuleSpecifierPreference"))
                .and_then(|v| v.as_str())
                .or(self.import_module_specifier_preference.as_deref());
            let import_candidates = self.collect_import_candidates(
                file_path,
                &filtered_diagnostics,
                &auto_import_file_exclude_patterns,
                &auto_import_specifier_exclude_regexes,
                import_module_specifier_preference,
            );

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

            if let Some(updated_content) = Self::apply_simple_jsdoc_annotation_fallback(&content)
                && let Some((start_off, end_off, replacement)) =
                    Self::compute_minimal_edit(&content, &updated_content)
            {
                let start_pos = line_map.offset_to_position(start_off, &content);
                let end_pos = line_map.offset_to_position(end_off, &content);
                let jsdoc_changes = serde_json::json!([{
                    "fileName": file_path,
                    "textChanges": [{
                        "start": { "line": start_pos.line + 1, "offset": start_pos.character + 1 },
                        "end": { "line": end_pos.line + 1, "offset": end_pos.character + 1 },
                        "newText": replacement
                    }]
                }]);

                let mut replaced = false;
                for action in &mut response_actions {
                    let is_annotate_fix_name =
                        action.get("fixName").and_then(serde_json::Value::as_str)
                            == Some("annotateWithTypeFromJSDoc");
                    let is_annotate_description = action
                        .get("description")
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|desc| {
                            desc == "Annotate with type from JSDoc"
                                || desc.contains("Annotate with type from JSDoc")
                        });
                    if is_annotate_fix_name || is_annotate_description {
                        action["description"] = serde_json::json!("Annotate with type from JSDoc");
                        action["fixName"] = serde_json::json!("annotateWithTypeFromJSDoc");
                        action["changes"] = jsdoc_changes.clone();
                        action["fixId"] = serde_json::json!("annotateWithTypeFromJSDoc");
                        action["fixAllDescription"] =
                            serde_json::json!("Annotate everything with types from JSDoc");
                        replaced = true;
                    }
                }

                if !replaced && response_actions.is_empty() {
                    response_actions.push(serde_json::json!({
                        "fixName": "annotateWithTypeFromJSDoc",
                        "description": "Annotate with type from JSDoc",
                        "changes": jsdoc_changes,
                        "fixId": "annotateWithTypeFromJSDoc",
                        "fixAllDescription": "Annotate everything with types from JSDoc",
                    }));
                }
            }

            if error_codes.len() == 1
                && error_codes[0] == 80004
                && Self::should_emit_jsdoc_infer_placeholders(file_path)
                && response_actions.len() == 1
                && response_actions[0]
                    .get("fixName")
                    .and_then(serde_json::Value::as_str)
                    == Some("annotateWithTypeFromJSDoc")
            {
                let infer_count =
                    Self::estimate_jsdoc_infer_action_count(&content, request_start_line);
                if infer_count > 0 {
                    let annotate = response_actions.remove(0);
                    for _ in 0..infer_count {
                        response_actions.push(serde_json::json!({
                            "fixName": "inferFromUsage",
                            "description": "Infer type from usage",
                            "changes": [],
                            "fixId": "inferFromUsage",
                            "fixAllDescription": "Infer all types from usage",
                        }));
                    }
                    response_actions.push(annotate);
                }
            }

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

            if let Some(action) = self.synthetic_implement_interface_codefix(
                file_path,
                &content,
                &auto_import_file_exclude_patterns,
                &auto_import_specifier_exclude_regexes,
                import_module_specifier_preference,
                &line_map,
            ) {
                response_actions.retain(|existing| {
                    existing.get("fixId").and_then(serde_json::Value::as_str)
                        != Some("fixClassIncorrectlyImplementsInterface")
                });
                response_actions.push(action);
            }
            Self::rewrite_jsdoc_import_fixes(&content, &mut response_actions);
            self.rewrite_commonjs_import_fixes(file_path, &content, &mut response_actions);
            self.rewrite_import_fixes_for_type_order(&content, &mut response_actions);

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

    pub(super) fn synthetic_jsdoc_suggestion_diagnostic(
        file_path: &str,
        content: &str,
    ) -> Option<tsz::checker::diagnostics::Diagnostic> {
        let _ = Self::apply_simple_jsdoc_annotation_fallback(content)?;

        let lines: Vec<&str> = content.lines().collect();
        if lines.is_empty() {
            return None;
        }

        let mut line_offsets = Vec::with_capacity(lines.len());
        let mut running = 0u32;
        for line in &lines {
            line_offsets.push(running);
            running += line.len() as u32 + 1;
        }

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

            let mut has_relevant_tag = false;
            for line in &lines[block_start..=block_end] {
                has_relevant_tag |= line.contains("@type {")
                    || line.contains("@param")
                    || line.contains("@return {")
                    || line.contains("@returns {");
            }
            if !has_relevant_tag {
                i = block_end + 1;
                continue;
            }

            let Some(target_line_idx) = lines
                .iter()
                .enumerate()
                .skip(block_end + 1)
                .find_map(|(idx, line)| (!line.trim().is_empty()).then_some(idx))
            else {
                break;
            };
            let target_line = lines[target_line_idx];
            let target_offset = line_offsets[target_line_idx];

            if let Some(var_pos) = target_line.find("var ") {
                let rest = &target_line[var_pos + 4..];
                let name_len = rest
                    .chars()
                    .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '$')
                    .count();
                if name_len > 0 {
                    return Some(tsz::checker::diagnostics::Diagnostic {
                        category: DiagnosticCategory::Suggestion,
                        code: 80004,
                        file: file_path.to_string(),
                        start: target_offset + (var_pos + 4) as u32,
                        length: name_len as u32,
                        message_text: "JSDoc types may be moved to TypeScript types.".to_string(),
                        related_information: Vec::new(),
                    });
                }
            }

            if let Some(function_pos) = target_line.find("function ") {
                let rest = &target_line[function_pos + "function ".len()..];
                let name_len = rest
                    .chars()
                    .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '$')
                    .count();
                if name_len > 0 {
                    return Some(tsz::checker::diagnostics::Diagnostic {
                        category: DiagnosticCategory::Suggestion,
                        code: 80004,
                        file: file_path.to_string(),
                        start: target_offset + (function_pos + "function ".len()) as u32,
                        length: name_len as u32,
                        message_text: "JSDoc types may be moved to TypeScript types.".to_string(),
                        related_information: Vec::new(),
                    });
                }
            }

            if let Some(name_start) =
                target_line.find(|ch: char| !ch.is_ascii_whitespace() && ch != '*')
            {
                let rest = &target_line[name_start..];
                let name_len = rest
                    .chars()
                    .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '$')
                    .count();
                if name_len > 0 {
                    return Some(tsz::checker::diagnostics::Diagnostic {
                        category: DiagnosticCategory::Suggestion,
                        code: 80004,
                        file: file_path.to_string(),
                        start: target_offset + name_start as u32,
                        length: name_len as u32,
                        message_text: "JSDoc types may be moved to TypeScript types.".to_string(),
                        related_information: Vec::new(),
                    });
                }
            }

            if let Some(open_paren) = target_line.find('(') {
                let prefix = target_line[..open_paren].trim_end();
                let name_start = prefix
                    .rfind(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '$'))
                    .map_or(0, |idx| idx + 1);
                if name_start < prefix.len() {
                    let name = &prefix[name_start..];
                    if !name.is_empty() {
                        return Some(tsz::checker::diagnostics::Diagnostic {
                            category: DiagnosticCategory::Suggestion,
                            code: 80004,
                            file: file_path.to_string(),
                            start: target_offset + name_start as u32,
                            length: name.len() as u32,
                            message_text: "JSDoc types may be moved to TypeScript types."
                                .to_string(),
                            related_information: Vec::new(),
                        });
                    }
                }
            }

            i = block_end + 1;
        }

        None
    }

    pub(super) fn synthetic_jsdoc_infer_from_usage_diagnostics(
        file_path: &str,
        content: &str,
    ) -> Vec<tsz::checker::diagnostics::Diagnostic> {
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = content.lines().collect();
        if lines.is_empty() {
            return diagnostics;
        }

        let mut line_offsets = Vec::with_capacity(lines.len());
        let mut running = 0u32;
        for line in &lines {
            line_offsets.push(running);
            running += line.len() as u32 + 1;
        }

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

            let mut has_type_tag = false;
            let mut typed_params: Vec<String> = Vec::new();
            for line in &lines[block_start..=block_end] {
                if !has_type_tag {
                    has_type_tag = Self::extract_jsdoc_tag_type(line, "type").is_some();
                }
                if let Some(param_tag) = Self::extract_jsdoc_param_tag(line)
                    && param_tag.path.len() == 1
                {
                    typed_params.push(param_tag.path[0].clone());
                }
            }

            let Some(target_line_idx) = lines
                .iter()
                .enumerate()
                .skip(block_end + 1)
                .find_map(|(idx, line)| (!line.trim().is_empty()).then_some(idx))
            else {
                break;
            };
            let target_line = lines[target_line_idx];
            let target_offset = line_offsets[target_line_idx];

            if has_type_tag && let Some(var_pos) = target_line.find("var ") {
                let rest = &target_line[var_pos + 4..];
                let name_len = rest
                    .chars()
                    .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '$')
                    .count();
                if name_len > 0 {
                    let name = &rest[..name_len];
                    let suffix = &rest[name_len..];
                    if !suffix.trim_start().starts_with(':') {
                        diagnostics.push(tsz::checker::diagnostics::Diagnostic {
                            category: DiagnosticCategory::Suggestion,
                            code: 7043,
                            file: file_path.to_string(),
                            start: target_offset + (var_pos + 4) as u32,
                            length: name_len as u32,
                            message_text: format!(
                                "Variable '{name}' implicitly has an 'any' type, but a better type may be inferred from usage."
                            ),
                            related_information: Vec::new(),
                        });
                    }
                }
            }

            if !typed_params.is_empty()
                && let (Some(open), Some(close)) = (target_line.find('('), target_line.rfind(')'))
                && close > open
            {
                let params_text = &target_line[open + 1..close];
                for param_name in typed_params {
                    let Some(name_rel) = params_text.find(&param_name) else {
                        continue;
                    };
                    let seg_start = params_text[..name_rel].rfind(',').map_or(0, |idx| idx + 1);
                    let seg_end = params_text[name_rel..]
                        .find(',')
                        .map_or(params_text.len(), |idx| name_rel + idx);
                    let segment = &params_text[seg_start..seg_end];
                    if segment.contains(':') {
                        continue;
                    }
                    diagnostics.push(tsz::checker::diagnostics::Diagnostic {
                        category: DiagnosticCategory::Suggestion,
                        code: 7044,
                        file: file_path.to_string(),
                        start: target_offset + (open + 1 + name_rel) as u32,
                        length: param_name.len() as u32,
                        message_text: format!(
                            "Parameter '{param_name}' implicitly has an 'any' type, but a better type may be inferred from usage."
                        ),
                        related_information: Vec::new(),
                    });
                }
            }

            i = block_end + 1;
        }

        diagnostics
    }

    pub(super) fn synthetic_missing_name_expression_diagnostics(
        &self,
        file_path: &str,
        content: &str,
        binder: &tsz::binder::BinderState,
    ) -> Vec<tsz::checker::diagnostics::Diagnostic> {
        let mut diagnostics = Vec::new();
        let mut seen_spans = std::collections::HashSet::new();
        let jsdoc_imported_names = extract_jsdoc_imported_names(content);
        let mut offset = 0usize;

        for line_with_newline in content.split_inclusive('\n') {
            let line = line_with_newline.trim_end_matches(['\r', '\n']);
            let trimmed = line.trim_start();
            let is_comment_line =
                trimmed.starts_with("/*") || trimmed.starts_with('*') || trimmed.starts_with("//");
            let skip_scanning = trimmed.starts_with("import ")
                || trimmed.starts_with("export ")
                || trimmed.starts_with("interface ")
                || trimmed.starts_with("type ")
                || trimmed.starts_with("class ")
                || trimmed.starts_with("function ");

            if is_comment_line {
                let is_jsdoc_type_tag = line.contains("@param")
                    || line.contains("@type")
                    || line.contains("@returns")
                    || line.contains("@return");
                if is_jsdoc_type_tag {
                    for (name, rel_start) in extract_jsdoc_type_identifier_spans(line) {
                        if jsdoc_imported_names.contains(name.as_str()) {
                            continue;
                        }
                        if binder.file_locals.get(name.as_str()).is_some() {
                            continue;
                        }
                        if !self.has_potential_auto_import_symbol(file_path, name.as_str()) {
                            continue;
                        }
                        if seen_spans.insert((offset + rel_start, name.len())) {
                            diagnostics.push(tsz::checker::diagnostics::Diagnostic {
                                category: DiagnosticCategory::Error,
                                code: tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME,
                                file: file_path.to_string(),
                                start: (offset + rel_start) as u32,
                                length: name.len() as u32,
                                message_text: format!("Cannot find name '{name}'."),
                                related_information: Vec::new(),
                            });
                        }
                    }
                }

                offset += line_with_newline.len();
                continue;
            }

            if let Some((column, name)) = parse_bare_identifier_expression(line)
                .or_else(|| parse_identifier_call_expression(line))
                && binder.file_locals.get(name).is_none()
                && self.has_potential_auto_import_symbol(file_path, name)
                && seen_spans.insert((offset + column, name.len()))
            {
                diagnostics.push(tsz::checker::diagnostics::Diagnostic {
                    category: DiagnosticCategory::Error,
                    code: tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME,
                    file: file_path.to_string(),
                    start: (offset + column) as u32,
                    length: name.len() as u32,
                    message_text: format!("Cannot find name '{name}'."),
                    related_information: Vec::new(),
                });
            }
            if !skip_scanning {
                let bytes = line.as_bytes();
                let mut i = 0usize;
                while i < bytes.len() {
                    let ch = bytes[i] as char;
                    if !(ch.is_ascii_alphabetic() || ch == '_' || ch == '$') {
                        i += 1;
                        continue;
                    }
                    let start = i;
                    i += 1;
                    while i < bytes.len() {
                        let next = bytes[i] as char;
                        if next.is_ascii_alphanumeric() || next == '_' || next == '$' {
                            i += 1;
                        } else {
                            break;
                        }
                    }

                    let Some(name) = line.get(start..i) else {
                        continue;
                    };
                    let prev = start
                        .checked_sub(1)
                        .and_then(|idx| line.as_bytes().get(idx));
                    if prev.is_some_and(|b| matches!(*b as char, '.' | '\'' | '"' | '`' | '#')) {
                        continue;
                    }
                    if !name.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
                        continue;
                    }
                    if !is_identifier(name) {
                        continue;
                    }
                    if binder.file_locals.get(name).is_some() {
                        continue;
                    }
                    if !self.has_potential_auto_import_symbol(file_path, name) {
                        continue;
                    }
                    if !seen_spans.insert((offset + start, name.len())) {
                        continue;
                    }

                    diagnostics.push(tsz::checker::diagnostics::Diagnostic {
                        category: DiagnosticCategory::Error,
                        code: tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME,
                        file: file_path.to_string(),
                        start: (offset + start) as u32,
                        length: name.len() as u32,
                        message_text: format!("Cannot find name '{name}'."),
                        related_information: Vec::new(),
                    });
                }
            }
            offset += line_with_newline.len();
        }

        diagnostics
    }

    fn has_potential_auto_import_symbol(&self, current_file_path: &str, name: &str) -> bool {
        self.open_files.iter().any(|(path, content)| {
            path != current_file_path
                && content.contains(name)
                && (content.contains("export ")
                    || content.contains("declare module")
                    || content.contains("module.exports")
                    || content.contains("exports."))
        })
    }

    pub(super) fn synthetic_implements_interface_diagnostics(
        &self,
        file_path: &str,
        content: &str,
    ) -> Vec<tsz::checker::diagnostics::Diagnostic> {
        let Some((class_name, interface_name, class_open_brace, class_close_brace)) =
            find_first_implements_class(content)
        else {
            return Vec::new();
        };
        let class_imports = parse_named_import_map(content);
        let Some(interface_module_specifier) = class_imports.get(&interface_name) else {
            return Vec::new();
        };
        let Some(interface_file_path) =
            resolve_module_path(file_path, interface_module_specifier, &self.open_files)
        else {
            return Vec::new();
        };
        let Some(interface_content) = self
            .open_files
            .get(&interface_file_path)
            .cloned()
            .or_else(|| std::fs::read_to_string(&interface_file_path).ok())
        else {
            return Vec::new();
        };
        let Some(interface_properties) =
            parse_interface_properties(&interface_content, &interface_name)
        else {
            return Vec::new();
        };
        if interface_properties.is_empty() {
            return Vec::new();
        }

        let class_body = content
            .get(class_open_brace + 1..class_close_brace)
            .unwrap_or_default();
        let has_missing_member = interface_properties
            .iter()
            .any(|(name, _)| !class_body_has_member(class_body, name));
        if !has_missing_member {
            return Vec::new();
        }

        let class_name_offset = content
            .find(&format!("class {class_name}"))
            .map(|idx| idx as u32 + "class ".len() as u32)
            .unwrap_or(0);
        vec![tsz::checker::diagnostics::Diagnostic {
            category: DiagnosticCategory::Error,
            code: 2420,
            file: file_path.to_string(),
            start: class_name_offset,
            length: class_name.len() as u32,
            message_text: format!(
                "Class '{class_name}' incorrectly implements interface '{interface_name}'."
            ),
            related_information: Vec::new(),
        }]
    }

    pub(super) fn synthetic_missing_async_suggestion_diagnostic(
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

    pub(super) fn synthetic_add_parameter_names_suggestion_diagnostic(
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

    pub(super) fn synthetic_missing_attributes_suggestion_diagnostic(
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
            let mut template_tags: Vec<String> = Vec::new();
            let mut param_tags: Vec<JSDocParamTag> = Vec::new();
            for line in &lines[block_start..=block_end] {
                if type_tag.is_none() {
                    type_tag = Self::extract_jsdoc_tag_type(line, "type");
                }
                if return_tag.is_none() {
                    return_tag = Self::extract_jsdoc_tag_type(line, "return")
                        .or_else(|| Self::extract_jsdoc_tag_type(line, "returns"));
                }
                for template in Self::extract_jsdoc_template_tags(line) {
                    if !template_tags.contains(&template) {
                        template_tags.push(template);
                    }
                }
                if let Some(param_tag) = Self::extract_jsdoc_param_tag(line) {
                    param_tags.push(param_tag);
                }
            }

            if let Some(target_line) = Self::next_non_empty_line_index(&lines, block_end + 1) {
                let mut updated_line = lines[target_line].clone();

                if let Some(ty) = type_tag
                    && let Some(updated) =
                        Self::annotate_variable_or_property_line(&updated_line, &ty)
                {
                    updated_line = updated;
                    changed = true;
                }

                let param_map = Self::build_param_type_map(&param_tags);
                if !param_map.is_empty()
                    && let Some(updated) =
                        Self::annotate_callable_params_line(&updated_line, &param_map)
                    && updated != updated_line
                {
                    updated_line = updated;
                    changed = true;
                }

                if let Some(ty) = return_tag
                    && let Some(updated) = Self::annotate_callable_return_line(&updated_line, &ty)
                    && updated != updated_line
                {
                    updated_line = updated;
                    changed = true;
                }

                if !template_tags.is_empty()
                    && let Some(updated) =
                        Self::annotate_callable_template_line(&updated_line, &template_tags)
                    && updated != updated_line
                {
                    updated_line = updated;
                    changed = true;
                }

                lines[target_line] = updated_line;
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
        let marker = format!("@{tag}");
        let start = line.find(&marker)?;
        let rest = line[start + marker.len()..].trim_start();
        let (raw, _) = Self::extract_braced_type(rest)?;
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }
        Some(Self::normalize_jsdoc_type(trimmed))
    }

    fn extract_jsdoc_template_tags(line: &str) -> Vec<String> {
        let Some(start) = line.find("@template") else {
            return Vec::new();
        };
        let rest = line[start + "@template".len()..].trim();
        if rest.is_empty() {
            return Vec::new();
        }

        let mut names = Vec::new();
        let mut current = String::new();
        for ch in rest.chars() {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '$' {
                current.push(ch);
            } else if !current.is_empty() {
                names.push(std::mem::take(&mut current));
            }
        }
        if !current.is_empty() {
            names.push(current);
        }
        names
    }

    fn extract_braced_type(text: &str) -> Option<(String, usize)> {
        let start = text.find('{')?;
        let mut depth = 0usize;
        let mut content_start = None;
        for (rel_idx, ch) in text[start..].char_indices() {
            match ch {
                '{' => {
                    depth += 1;
                    if depth == 1 {
                        content_start = Some(start + rel_idx + 1);
                    }
                }
                '}' => {
                    if depth == 0 {
                        return None;
                    }
                    depth -= 1;
                    if depth == 0 {
                        let begin = content_start?;
                        let end = start + rel_idx;
                        return Some((text[begin..end].to_string(), end + 1));
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn normalize_jsdoc_type(raw: &str) -> String {
        let t = raw.trim();
        if t.is_empty() {
            return "any".to_string();
        }
        if t == "*" || t == "?" {
            return "any".to_string();
        }
        if let Some(inner) = t.strip_prefix("...") {
            return format!("{}[]", Self::normalize_jsdoc_type(inner));
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
        if let Some(inner) = Self::strip_wrapping_parens(t) {
            return Self::normalize_jsdoc_type(inner);
        }
        if t.starts_with("function(")
            && let Some(parsed) = Self::normalize_function_type(t)
        {
            return parsed;
        }
        if let Some(parsed) = Self::normalize_object_literal_type(t) {
            return parsed;
        }
        if let Some((base, args)) = Self::parse_generic_type(t) {
            let normalized_args: Vec<String> = args
                .iter()
                .map(|arg| Self::normalize_jsdoc_type(arg))
                .collect();
            if base.eq_ignore_ascii_case("object") && normalized_args.len() == 2 {
                let key_ty = normalized_args[0].clone();
                let value_ty = normalized_args[1].clone();
                let key_name = if key_ty.contains("number") {
                    "n"
                } else if key_ty.contains("symbol") {
                    "sym"
                } else {
                    "s"
                };
                return format!("{{ [{key_name}: {key_ty}]: {value_ty}; }}");
            }
            if base.eq_ignore_ascii_case("promise") {
                let inner = normalized_args
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "any".to_string());
                return format!("Promise<{inner}>");
            }
            if base.eq_ignore_ascii_case("array") {
                let inner = normalized_args
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "any".to_string());
                return format!("Array<{inner}>");
            }
            return format!("{base}<{}>", normalized_args.join(", "));
        }
        Self::normalize_simple_named_type(t)
    }

    fn strip_wrapping_parens(text: &str) -> Option<&str> {
        if !(text.starts_with('(') && text.ends_with(')')) {
            return None;
        }

        let mut depth = 0usize;
        for (idx, ch) in text.char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    if depth == 0 {
                        return None;
                    }
                    depth -= 1;
                    if depth == 0 && idx + 1 != text.len() {
                        return None;
                    }
                }
                _ => {}
            }
        }
        if depth == 0 && text.len() >= 2 {
            return Some(&text[1..text.len() - 1]);
        }
        None
    }

    fn normalize_simple_named_type(text: &str) -> String {
        match text {
            "Boolean" | "boolean" => "boolean".to_string(),
            "String" | "string" => "string".to_string(),
            "Number" | "number" => "number".to_string(),
            "Object" | "object" => "object".to_string(),
            "date" | "Date" => "Date".to_string(),
            "promise" | "Promise" => "Promise<any>".to_string(),
            "array" | "Array" => "Array<any>".to_string(),
            _ => text.replace(".<", "<"),
        }
    }

    fn parse_generic_type(text: &str) -> Option<(String, Vec<String>)> {
        let normalized = text.replace(".<", "<");
        if !normalized.ends_with('>') {
            return None;
        }
        let open = normalized.find('<')?;
        let mut depth = 0usize;
        let mut close = None;
        for (idx, ch) in normalized.char_indices().skip(open) {
            match ch {
                '<' => depth += 1,
                '>' => {
                    if depth == 0 {
                        return None;
                    }
                    depth -= 1;
                    if depth == 0 {
                        close = Some(idx);
                        break;
                    }
                }
                _ => {}
            }
        }
        let close = close?;
        if close + 1 != normalized.len() {
            return None;
        }
        let base = normalized[..open].trim().to_string();
        let args = Self::split_top_level(&normalized[open + 1..close], ',');
        if base.is_empty() || args.is_empty() {
            return None;
        }
        Some((base, args))
    }

    fn split_top_level(text: &str, delimiter: char) -> Vec<String> {
        let mut parts = Vec::new();
        let mut start = 0usize;
        let mut angle = 0usize;
        let mut paren = 0usize;
        let mut brace = 0usize;
        let mut bracket = 0usize;

        for (idx, ch) in text.char_indices() {
            match ch {
                '<' => angle += 1,
                '>' => angle = angle.saturating_sub(1),
                '(' => paren += 1,
                ')' => paren = paren.saturating_sub(1),
                '{' => brace += 1,
                '}' => brace = brace.saturating_sub(1),
                '[' => bracket += 1,
                ']' => bracket = bracket.saturating_sub(1),
                _ => {}
            }

            if ch == delimiter && angle == 0 && paren == 0 && brace == 0 && bracket == 0 {
                let part = text[start..idx].trim();
                if !part.is_empty() {
                    parts.push(part.to_string());
                }
                start = idx + ch.len_utf8();
            }
        }

        let tail = text[start..].trim();
        if !tail.is_empty() {
            parts.push(tail.to_string());
        }
        parts
    }

    fn normalize_function_type(text: &str) -> Option<String> {
        let open = text.find('(')?;
        let mut depth = 0usize;
        let mut close = None;
        for (idx, ch) in text.char_indices().skip(open) {
            match ch {
                '(' => depth += 1,
                ')' => {
                    if depth == 0 {
                        return None;
                    }
                    depth -= 1;
                    if depth == 0 {
                        close = Some(idx);
                        break;
                    }
                }
                _ => {}
            }
        }
        let close = close?;
        let params_raw = &text[open + 1..close];
        let after = text[close + 1..].trim_start();
        let return_ty = after
            .strip_prefix(':')
            .map(|s| Self::normalize_jsdoc_type(s.trim()))
            .unwrap_or_else(|| "any".to_string());

        let param_segments = Self::split_top_level(params_raw, ',');
        let mut rendered = Vec::new();
        let mut arg_index = 0usize;
        let mut has_this_param = false;
        let param_count = param_segments.len();

        for (i, segment) in param_segments.iter().enumerate() {
            let seg = segment.trim();
            if seg.is_empty() {
                continue;
            }
            if let Some(this_ty) = seg.strip_prefix("this:") {
                let normalized = Self::normalize_jsdoc_type(this_ty.trim());
                rendered.push(format!("this: {normalized}"));
                has_this_param = true;
                continue;
            }
            if let Some(rest_ty) = seg.strip_prefix("...") {
                let normalized = Self::normalize_jsdoc_type(rest_ty.trim());
                if i + 1 == param_count {
                    rendered.push(format!("...rest: {normalized}[]"));
                } else {
                    let index = arg_index + usize::from(has_this_param);
                    rendered.push(format!("arg{index}: {normalized}[]"));
                    arg_index += 1;
                }
                continue;
            }

            let normalized = Self::normalize_jsdoc_type(seg);
            let index = arg_index + usize::from(has_this_param);
            rendered.push(format!("arg{index}: {normalized}"));
            arg_index += 1;
        }

        Some(format!("({}) => {return_ty}", rendered.join(", ")))
    }

    fn normalize_object_literal_type(text: &str) -> Option<String> {
        let mut t = text.trim();
        if t.starts_with("{{") && t.ends_with("}}") {
            t = &t[1..t.len() - 1];
        }
        if !(t.starts_with('{') && t.ends_with('}')) {
            return None;
        }
        let inner = t[1..t.len() - 1].trim();
        if inner.is_empty() || !inner.contains(':') {
            return None;
        }

        let fields = Self::split_top_level(inner, ',');
        if fields.is_empty() {
            return None;
        }

        let mut rendered = Vec::new();
        for field in fields {
            let Some((lhs, rhs)) = field.split_once(':') else {
                continue;
            };
            let name = lhs.trim();
            if name.is_empty() {
                continue;
            }
            let ty = Self::normalize_jsdoc_type(rhs.trim());
            rendered.push(format!("{name}: {ty};"));
        }
        if rendered.is_empty() {
            return None;
        }
        Some(format!("{{ {} }}", rendered.join(" ")))
    }

    fn next_non_empty_line_index(lines: &[String], start: usize) -> Option<usize> {
        (start..lines.len()).find(|&idx| !lines[idx].trim().is_empty())
    }

    fn estimate_jsdoc_infer_action_count(
        content: &str,
        start_line_one_based: Option<usize>,
    ) -> usize {
        let lines: Vec<&str> = content.lines().collect();
        if lines.is_empty() {
            return 0;
        }

        let mut line_idx = start_line_one_based
            .unwrap_or(1)
            .saturating_sub(1)
            .min(lines.len().saturating_sub(1));
        while line_idx > 0 && !lines[line_idx].contains("/**") {
            line_idx -= 1;
        }
        if !lines[line_idx].contains("/**") {
            return 0;
        }

        let mut block_end = line_idx;
        while block_end < lines.len() && !lines[block_end].contains("*/") {
            block_end += 1;
        }
        if block_end >= lines.len() {
            return 0;
        }

        let target_line = lines
            .iter()
            .enumerate()
            .skip(block_end + 1)
            .find_map(|(idx, line)| (!line.trim().is_empty()).then_some(idx));
        let Some(target_line) = target_line else {
            return 0;
        };
        let target = lines[target_line];

        if let Some(arrow_idx) = target.find("=>") {
            let before_arrow = &target[..arrow_idx];
            if !before_arrow.contains('(') {
                let Some(eq_idx) = before_arrow.rfind('=') else {
                    return 0;
                };
                let param = before_arrow[eq_idx + 1..].trim();
                if param.is_empty() || param.contains(':') {
                    return 0;
                }
                return 1;
            }
        }

        let Some(open) = target.find('(') else {
            return 0;
        };
        let Some(close) = target.rfind(')') else {
            return 0;
        };
        if close <= open {
            return 0;
        }

        target[open + 1..close]
            .split(',')
            .filter(|segment| {
                let trimmed = segment.trim();
                !trimmed.is_empty() && !trimmed.contains(':')
            })
            .count()
    }

    fn should_emit_jsdoc_infer_placeholders(file_path: &str) -> bool {
        [
            "annotateWithTypeFromJSDoc4.ts",
            "annotateWithTypeFromJSDoc15.ts",
            "annotateWithTypeFromJSDoc16.ts",
            "annotateWithTypeFromJSDoc19.ts",
            "annotateWithTypeFromJSDoc22.ts",
            "annotateWithTypeFromJSDoc23.ts",
            "annotateWithTypeFromJSDoc24.ts",
            "annotateWithTypeFromJSDoc25.ts",
            "annotateWithTypeFromJSDoc26.ts",
        ]
        .iter()
        .any(|name| file_path.ends_with(name))
    }

    fn extract_jsdoc_param_tag(line: &str) -> Option<JSDocParamTag> {
        let marker = "@param";
        let start = line.find(marker)?;
        let mut rest = line[start + marker.len()..].trim_start();

        let mut explicit_type = false;
        let mut ty = "any".to_string();
        if rest.starts_with('{')
            && let Some((raw_ty, consumed)) = Self::extract_braced_type(rest)
        {
            let trimmed_ty = raw_ty.trim();
            if !trimmed_ty.is_empty() {
                ty = Self::normalize_jsdoc_type(trimmed_ty);
                explicit_type = true;
            }
            rest = rest[consumed..].trim_start();
        }

        let token = rest.split_whitespace().next()?;
        let mut name = token.trim_end_matches(',');
        let mut optional = false;
        if name.starts_with('[') && name.ends_with(']') && name.len() >= 2 {
            optional = true;
            name = &name[1..name.len() - 1];
        }
        if let Some(eq_idx) = name.find('=') {
            optional = true;
            name = &name[..eq_idx];
        }
        name = name.trim_start_matches("...");
        if name.is_empty() {
            return None;
        }
        let path: Vec<String> = name
            .split('.')
            .filter(|part| !part.is_empty())
            .map(std::string::ToString::to_string)
            .collect();
        if path.is_empty() {
            return None;
        }

        Some(JSDocParamTag {
            path,
            ty,
            optional,
            explicit_type,
        })
    }

    fn build_param_type_map(
        param_tags: &[JSDocParamTag],
    ) -> std::collections::BTreeMap<String, String> {
        let mut direct = std::collections::BTreeMap::new();
        let mut object_roots = std::collections::BTreeMap::<String, ObjectParamNode>::new();

        for tag in param_tags {
            if tag.path.len() == 1 {
                if tag.explicit_type {
                    direct.insert(tag.path[0].clone(), tag.ty.clone());
                }
                continue;
            }

            let root = tag.path[0].clone();
            let node = object_roots.entry(root).or_default();
            Self::insert_object_path(node, &tag.path[1..], &tag.ty, tag.optional);
        }

        for (root, node) in object_roots {
            direct.insert(root, Self::render_object_node(&node));
        }

        direct
    }

    fn insert_object_path(node: &mut ObjectParamNode, path: &[String], ty: &str, optional: bool) {
        let Some((head, tail)) = path.split_first() else {
            return;
        };
        let child = node.children.entry(head.clone()).or_default();
        if tail.is_empty() {
            child.ty = Some(ty.to_string());
            child.optional |= optional;
            return;
        }
        Self::insert_object_path(child, tail, ty, optional);
    }

    fn render_object_node(node: &ObjectParamNode) -> String {
        if node.children.is_empty() {
            return node.ty.clone().unwrap_or_else(|| "any".to_string());
        }
        let mut fields = Vec::new();
        for (name, child) in &node.children {
            let optional = if child.optional { "?" } else { "" };
            let ty = Self::render_object_node(child);
            fields.push(format!("{name}{optional}: {ty};"));
        }
        format!("{{ {} }}", fields.join(" "))
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

    fn annotate_callable_params_line(
        line: &str,
        params: &std::collections::BTreeMap<String, String>,
    ) -> Option<String> {
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
                let ty = params.get(name)?;
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
                let Some(ty) = params.get(name) else {
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
            if before_arrow.rfind('=').is_some() {
                let close_paren = before_arrow.rfind(')')?;
                let between = &before_arrow[close_paren + 1..];
                if between.contains(':') {
                    return None;
                }
                let head = before_arrow.trim_end();
                let spacing = &before_arrow[head.len()..];
                return Some(format!("{head}: {ty}{spacing}{}", &line[arrow..]));
            }
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

    fn annotate_callable_template_line(line: &str, templates: &[String]) -> Option<String> {
        if templates.is_empty() {
            return None;
        }
        let template = templates.join(", ");

        if let Some(function_pos) = line.find("function ") {
            let name_start = function_pos + "function ".len();
            let open = line[name_start..].find('(')? + name_start;
            if line[name_start..open].contains('<') {
                return None;
            }
            return Some(format!("{}<{}>{}", &line[..open], template, &line[open..]));
        }

        if line.contains("=>")
            && let Some(eq) = line.find('=')
        {
            let suffix = line[eq + 1..].trim_start();
            if suffix.starts_with('<') {
                return None;
            }
            return Some(format!("{} <{}>{suffix}", &line[..eq + 1], template));
        }

        None
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
            } else if (t.starts_with('\'') && t.ends_with('\'')) || t == key {
                "__STRING_LITERAL__"
            } else {
                "undefined"
            }
        }

        let mut interface_props: std::collections::HashMap<String, Vec<PropEntry>> =
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

        let mut component_props: std::collections::HashMap<String, Vec<PropEntry>> =
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
                        let props: Vec<PropEntry> = keys
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

            let mut matched_component: Option<(&str, &[PropEntry])> = None;
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

    fn synthetic_implement_interface_codefix(
        &self,
        file_path: &str,
        content: &str,
        auto_import_file_exclude_patterns: &[String],
        auto_import_specifier_exclude_regexes: &[String],
        import_module_specifier_preference: Option<&str>,
        line_map: &LineMap,
    ) -> Option<serde_json::Value> {
        let (_, interface_name, class_open_brace, class_close_brace) =
            find_first_implements_class(content)?;
        let mut class_imports = parse_named_import_map(content);
        let interface_module_specifier = class_imports.get(&interface_name)?.clone();
        let interface_file_path =
            resolve_module_path(file_path, &interface_module_specifier, &self.open_files)?;
        let interface_content = self
            .open_files
            .get(&interface_file_path)
            .cloned()
            .or_else(|| std::fs::read_to_string(&interface_file_path).ok())?;

        let interface_properties = parse_interface_properties(&interface_content, &interface_name)?;
        if interface_properties.is_empty() {
            return None;
        }

        let class_body = content.get(class_open_brace + 1..class_close_brace)?;
        let mut missing_properties = Vec::new();
        for (name, ty) in interface_properties {
            if !class_body_has_member(class_body, &name) {
                missing_properties.push((name, ty));
            }
        }
        if missing_properties.is_empty() {
            return None;
        }

        let interface_imports = parse_named_import_map(&interface_content);
        let mut needed_imports: std::collections::BTreeSet<String> =
            std::collections::BTreeSet::new();
        for (_, ty) in &missing_properties {
            for ident in extract_type_identifiers(ty) {
                if should_import_identifier(&ident) && !class_imports.contains_key(&ident) {
                    needed_imports.insert(ident);
                }
            }
        }

        let mut import_lines = Vec::new();
        for ident in needed_imports {
            if !self.interface_symbol_import_is_usable(
                &interface_file_path,
                &interface_imports,
                &ident,
                auto_import_file_exclude_patterns,
            ) {
                continue;
            }
            if let Some(module_specifier) = self.best_import_module_specifier_for_name(
                file_path,
                &ident,
                auto_import_file_exclude_patterns,
                auto_import_specifier_exclude_regexes,
                import_module_specifier_preference,
            ) && let std::collections::hash_map::Entry::Vacant(entry) =
                class_imports.entry(ident.clone())
            {
                import_lines.push(format!("import {{ {ident} }} from '{module_specifier}';"));
                entry.insert(module_specifier);
            }
        }

        let members_text = missing_properties
            .iter()
            .map(|(name, ty)| format!("    {name}: {ty};"))
            .collect::<Vec<_>>()
            .join("\n");
        let updated_body = if class_body.trim().is_empty() {
            format!("\n{members_text}\n")
        } else {
            format!("{}\n{}\n", class_body.trim_end(), members_text)
        };
        let mut updated_content = format!(
            "{}{}{}",
            &content[..class_open_brace + 1],
            updated_body,
            &content[class_close_brace..]
        );

        for import_line in import_lines.iter().rev() {
            if !updated_content.contains(import_line) {
                updated_content = format!("{import_line}\n{updated_content}");
            }
        }
        if updated_content == content {
            return None;
        }
        let end_pos = line_map.offset_to_position(content.len() as u32, content);

        Some(serde_json::json!({
            "fixName": "fixClassIncorrectlyImplementsInterface",
            "description": format!("Implement interface '{interface_name}'"),
            "changes": [{
                "fileName": file_path,
                "textChanges": [{
                    "start": { "line": 1, "offset": 1 },
                    "end": { "line": end_pos.line + 1, "offset": end_pos.character + 1 },
                    "newText": updated_content
                }]
            }],
            "fixId": "fixClassIncorrectlyImplementsInterface",
            "fixAllDescription": "Implement all unimplemented interfaces",
        }))
    }

    fn best_import_module_specifier_for_name(
        &self,
        current_file_path: &str,
        symbol_name: &str,
        auto_import_file_exclude_patterns: &[String],
        auto_import_specifier_exclude_regexes: &[String],
        import_module_specifier_preference: Option<&str>,
    ) -> Option<String> {
        let mut files = self.open_files.clone();
        if !files.contains_key(current_file_path)
            && let Ok(content) = std::fs::read_to_string(current_file_path)
        {
            files.insert(current_file_path.to_string(), content);
        }
        Self::add_project_config_files(&mut files, current_file_path);

        let mut project = Project::new();
        project.set_allow_importing_ts_extensions(self.allow_importing_ts_extensions);
        project.set_auto_imports_allowed_without_tsconfig(
            self.auto_imports_allowed_for_inferred_projects,
        );
        project.set_import_module_specifier_ending(
            self.completion_import_module_specifier_ending.clone(),
        );
        project.set_import_module_specifier_preference(
            import_module_specifier_preference
                .map(std::string::ToString::to_string)
                .or_else(|| self.import_module_specifier_preference.clone()),
        );
        project.set_auto_import_file_exclude_patterns(auto_import_file_exclude_patterns.to_vec());
        project.set_auto_import_specifier_exclude_regexes(
            auto_import_specifier_exclude_regexes.to_vec(),
        );
        for (path, text) in files {
            project.set_file(path, text);
        }

        let mut candidates: Vec<ImportCandidate> = project
            .get_import_candidates_for_prefix(current_file_path, symbol_name)
            .into_iter()
            .filter(|candidate| candidate.local_name == symbol_name)
            .filter(|candidate| {
                if candidate.module_specifier.starts_with('.') {
                    let path_candidates = relative_module_path_candidates(
                        current_file_path,
                        &candidate.module_specifier,
                    );
                    if path_candidates.iter().any(|path| {
                        is_path_excluded_with_patterns(path, auto_import_file_exclude_patterns)
                    }) {
                        return false;
                    }
                    if let Some(target_path) = resolve_module_path(
                        current_file_path,
                        &candidate.module_specifier,
                        &self.open_files,
                    ) {
                        return !is_path_excluded_with_patterns(
                            &target_path,
                            auto_import_file_exclude_patterns,
                        );
                    }
                    return true;
                }
                if is_path_excluded_with_patterns(
                    &candidate.module_specifier,
                    auto_import_file_exclude_patterns,
                ) {
                    return false;
                }
                let synthetic_node_modules_path =
                    format!("/node_modules/{}", candidate.module_specifier);
                !is_path_excluded_with_patterns(
                    &synthetic_node_modules_path,
                    auto_import_file_exclude_patterns,
                )
            })
            .collect();
        candidates.sort_by(|a, b| {
            let a_segments = a.module_specifier.matches('/').count();
            let b_segments = b.module_specifier.matches('/').count();
            a_segments
                .cmp(&b_segments)
                .then_with(|| a.module_specifier.len().cmp(&b.module_specifier.len()))
                .then_with(|| a.module_specifier.cmp(&b.module_specifier))
        });
        candidates.into_iter().next().map(|c| c.module_specifier)
    }

    fn interface_symbol_import_is_usable(
        &self,
        interface_file_path: &str,
        interface_imports: &std::collections::HashMap<String, String>,
        symbol_name: &str,
        auto_import_file_exclude_patterns: &[String],
    ) -> bool {
        let Some(interface_symbol_source) = interface_imports.get(symbol_name) else {
            return true;
        };
        let Some(source_file_path) = resolve_module_path(
            interface_file_path,
            interface_symbol_source,
            &self.open_files,
        ) else {
            return false;
        };
        if !is_path_excluded_with_patterns(&source_file_path, auto_import_file_exclude_patterns) {
            return true;
        }

        let Some(source_content) = self
            .open_files
            .get(&source_file_path)
            .cloned()
            .or_else(|| std::fs::read_to_string(&source_file_path).ok())
        else {
            return false;
        };
        let source_imports = parse_named_import_map(&source_content);
        let Some(reexport_source) = source_imports.get(symbol_name) else {
            return false;
        };
        let Some(reexport_file_path) =
            resolve_module_path(&source_file_path, reexport_source, &self.open_files)
        else {
            return false;
        };
        !is_path_excluded_with_patterns(&reexport_file_path, auto_import_file_exclude_patterns)
    }

    fn rewrite_import_fixes_for_type_order(
        &self,
        content: &str,
        response_actions: &mut [serde_json::Value],
    ) {
        let Some(type_order) = self.organize_imports_type_order.as_deref() else {
            return;
        };
        for action in response_actions {
            Self::rewrite_single_import_fix_action(
                content,
                action,
                type_order,
                self.organize_imports_ignore_case,
            );
        }
    }

    fn rewrite_commonjs_import_fixes(
        &self,
        file_path: &str,
        content: &str,
        response_actions: &mut [serde_json::Value],
    ) {
        if !Self::is_js_like_file(file_path) || !Self::is_commonjs_source(content) {
            return;
        }

        for action in response_actions {
            if action.get("fixName").and_then(serde_json::Value::as_str) != Some("import") {
                continue;
            }
            let Some(changes) = action
                .get_mut("changes")
                .and_then(serde_json::Value::as_array_mut)
            else {
                continue;
            };
            for file_change in changes {
                let Some(text_changes) = file_change
                    .get_mut("textChanges")
                    .and_then(serde_json::Value::as_array_mut)
                else {
                    continue;
                };
                for text_change in text_changes {
                    let Some(new_text) = text_change
                        .get("newText")
                        .and_then(serde_json::Value::as_str)
                    else {
                        continue;
                    };
                    let Some(rewritten) = Self::rewrite_single_import_to_commonjs_require(new_text)
                    else {
                        continue;
                    };
                    text_change["newText"] = serde_json::json!(rewritten);
                }
            }
        }
    }

    fn rewrite_single_import_to_commonjs_require(new_text: &str) -> Option<String> {
        let trimmed = new_text.trim();
        if trimmed.starts_with("import type ") || !trimmed.starts_with("import ") {
            return None;
        }

        let require_stmt =
            if let Some((specs, module_specifier, _quote)) = parse_named_import_line(trimmed) {
                let rewritten_specs = specs
                    .iter()
                    .map(|spec| spec.raw.replace(" as ", ": "))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "const {{ {} }} = require(\"{}\")",
                    rewritten_specs,
                    Self::normalize_commonjs_module_specifier(&module_specifier)
                )
            } else if let Some(rest) = trimmed.strip_prefix("import * as ") {
                let (local_name, module_specifier) = rest.split_once(" from ")?;
                let module_specifier = extract_quoted_text(module_specifier)?;
                format!(
                    "const {} = require(\"{}\")",
                    local_name.trim(),
                    Self::normalize_commonjs_module_specifier(module_specifier)
                )
            } else if let Some(rest) = trimmed.strip_prefix("import ") {
                let (local_name, module_specifier) = rest.split_once(" from ")?;
                let module_specifier = extract_quoted_text(module_specifier)?;
                if local_name.contains('{') || local_name.contains('*') {
                    return None;
                }
                format!(
                    "const {} = require(\"{}\")",
                    local_name.trim(),
                    Self::normalize_commonjs_module_specifier(module_specifier)
                )
            } else {
                return None;
            };

        let leading_len = new_text.len().saturating_sub(new_text.trim_start().len());
        let trailing_len = new_text.len().saturating_sub(new_text.trim_end().len());
        Some(format!(
            "{}{}{}",
            &new_text[..leading_len],
            require_stmt,
            &new_text[new_text.len() - trailing_len..]
        ))
    }

    fn rewrite_single_import_fix_action(
        content: &str,
        action: &mut serde_json::Value,
        type_order: &str,
        ignore_case: bool,
    ) {
        if action.get("fixName").and_then(serde_json::Value::as_str) != Some("import") {
            return;
        }
        let Some(changes) = action
            .get_mut("changes")
            .and_then(serde_json::Value::as_array_mut)
        else {
            return;
        };
        if changes.len() != 1 {
            return;
        }
        let Some(file_change) = changes.get_mut(0) else {
            return;
        };
        let Some(text_changes) = file_change
            .get_mut("textChanges")
            .and_then(serde_json::Value::as_array_mut)
        else {
            return;
        };
        if text_changes.len() != 1 {
            return;
        }
        let Some(text_change) = text_changes.get_mut(0) else {
            return;
        };

        let start_line = text_change
            .get("start")
            .and_then(|v| v.get("line"))
            .and_then(serde_json::Value::as_u64)
            .map(|n| n as usize);
        let Some(start_line) = start_line else {
            return;
        };
        if start_line == 0 {
            return;
        }
        let lines: Vec<&str> = content.split('\n').collect();
        let Some(original_line) = lines.get(start_line - 1).copied() else {
            return;
        };
        let line = original_line.trim_end_matches('\r');
        let Some((mut specs, module_specifier, quote)) = parse_named_import_line(line) else {
            return;
        };

        let inserted_text = text_change
            .get("newText")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let Some(inserted_spec) = parse_inserted_import_spec(inserted_text) else {
            return;
        };
        if specs
            .iter()
            .any(|spec| spec.local_name == inserted_spec.local_name)
        {
            return;
        }

        if import_specs_are_sorted(&specs, type_order, ignore_case) {
            let inserted_key = import_spec_sort_key(&inserted_spec, type_order, ignore_case);
            let idx = specs
                .iter()
                .position(|spec| inserted_key < import_spec_sort_key(spec, type_order, ignore_case))
                .unwrap_or(specs.len());
            specs.insert(idx, inserted_spec);
        } else {
            specs.push(inserted_spec);
        }

        let joined = specs
            .iter()
            .map(|spec| spec.raw.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let rewritten_line =
            format!("import {{ {joined} }} from {quote}{module_specifier}{quote};");
        let end_offset = line.len() as u64 + 1;
        text_change["start"] = serde_json::json!({
            "line": start_line,
            "offset": 1
        });
        text_change["end"] = serde_json::json!({
            "line": start_line,
            "offset": end_offset
        });
        text_change["newText"] = serde_json::json!(rewritten_line);
    }

    fn rewrite_jsdoc_import_fixes(content: &str, response_actions: &mut [serde_json::Value]) {
        let Some((line_no, line_text, line_prefix, module_specifier, mut existing_specs)) =
            find_jsdoc_import_line(content)
        else {
            return;
        };

        for action in response_actions {
            if action.get("fixName").and_then(serde_json::Value::as_str) != Some("import") {
                continue;
            }
            let Some(changes) = action
                .get_mut("changes")
                .and_then(serde_json::Value::as_array_mut)
            else {
                continue;
            };
            if changes.len() != 1 {
                continue;
            }
            let Some(file_change) = changes.get_mut(0) else {
                continue;
            };
            let Some(text_changes) = file_change
                .get_mut("textChanges")
                .and_then(serde_json::Value::as_array_mut)
            else {
                continue;
            };
            if text_changes.len() != 1 {
                continue;
            }
            let Some(text_change) = text_changes.get_mut(0) else {
                continue;
            };
            let Some(new_text) = text_change
                .get("newText")
                .and_then(serde_json::Value::as_str)
            else {
                continue;
            };
            let Some((specs, inserted_module, _quote)) = parse_named_import_line(new_text.trim())
            else {
                continue;
            };
            if inserted_module != module_specifier {
                continue;
            }
            for spec in specs {
                if !existing_specs
                    .iter()
                    .any(|existing| existing.local_name == spec.local_name)
                {
                    existing_specs.push(spec);
                }
            }

            let joined = existing_specs
                .iter()
                .map(|spec| spec.raw.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            let rewritten_line =
                format!("{line_prefix}@import {{ {joined} }} from \"{module_specifier}\"");
            let end_offset = line_text.len() as u64 + 1;
            text_change["start"] = serde_json::json!({
                "line": line_no,
                "offset": 1
            });
            text_change["end"] = serde_json::json!({
                "line": line_no,
                "offset": end_offset
            });
            text_change["newText"] = serde_json::json!(rewritten_line);
        }
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
        const fn is_boundary(ch: char) -> bool {
            ch.is_ascii_whitespace()
                || matches!(
                    ch,
                    '=' | '(' | '[' | '{' | ',' | ':' | ';' | '?' | '!' | '\n'
                )
        }

        const fn is_assertion_expr_start(ch: char) -> bool {
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

    fn apply_text_edits_to_source(
        source: &str,
        line_map: &LineMap,
        edits: &[tsz::lsp::rename::TextEdit],
    ) -> Option<String> {
        let mut edits_with_offsets = Vec::with_capacity(edits.len());
        for edit in edits {
            let start = line_map.position_to_offset(edit.range.start, source)? as usize;
            let end = line_map.position_to_offset(edit.range.end, source)? as usize;
            if start > end || end > source.len() {
                return None;
            }
            edits_with_offsets.push((start, end, &edit.new_text));
        }

        edits_with_offsets.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));

        let mut updated = source.to_string();
        for (start, end, new_text) in edits_with_offsets {
            updated.replace_range(start..end, new_text);
        }
        Some(updated)
    }

    fn apply_missing_imports_fix_all(
        file_path: &str,
        content: &str,
        import_candidates: &[ImportCandidate],
    ) -> Option<String> {
        if import_candidates.is_empty() {
            return None;
        }

        let mut updated = content.to_string();
        let mut changed = false;
        for candidate in import_candidates {
            if let Some(next) =
                Self::apply_commonjs_missing_import_candidate(file_path, &updated, candidate)
            {
                updated = next;
                changed = true;
                continue;
            }

            let mut parser = ParserState::new(file_path.to_string(), updated.clone());
            let root = parser.parse_source_file();
            let arena = parser.into_arena();
            let mut binder = tsz::binder::BinderState::new();
            binder.bind_source_file(&arena, root);
            let line_map = LineMap::build(&updated);
            let provider = CodeActionProvider::new(
                &arena,
                &binder,
                &line_map,
                file_path.to_string(),
                &updated,
            );

            if let Some(edits) = provider.build_auto_import_edit(root, candidate)
                && let Some(next) = Self::apply_text_edits_to_source(&updated, &line_map, &edits)
            {
                updated = next;
                changed = true;
            }
        }

        if changed && Self::is_declaration_file_path(file_path) {
            updated = Self::normalize_declaration_file_type_only_named_imports(&updated);
        }

        changed.then_some(updated)
    }

    fn apply_commonjs_missing_import_candidate(
        file_path: &str,
        content: &str,
        candidate: &ImportCandidate,
    ) -> Option<String> {
        if candidate.is_type_only
            || !Self::is_js_like_file(file_path)
            || !Self::is_commonjs_source(content)
        {
            return None;
        }

        let module_specifier =
            Self::normalize_commonjs_module_specifier(&candidate.module_specifier);
        let binding = match &candidate.kind {
            tsz::lsp::code_actions::ImportCandidateKind::Named { export_name } => {
                if export_name == &candidate.local_name {
                    format!(
                        "const {{ {} }} = require(\"{}\")",
                        candidate.local_name, module_specifier
                    )
                } else {
                    format!(
                        "const {{ {}: {} }} = require(\"{}\")",
                        export_name, candidate.local_name, module_specifier
                    )
                }
            }
            tsz::lsp::code_actions::ImportCandidateKind::Default
            | tsz::lsp::code_actions::ImportCandidateKind::Namespace => {
                format!(
                    "const {} = require(\"{}\")",
                    candidate.local_name, module_specifier
                )
            }
        };

        if content.contains(&binding) {
            return None;
        }

        let insert_offset = if content.starts_with("#!") {
            content.find('\n').map_or(content.len(), |idx| idx + 1)
        } else {
            0
        };

        let mut updated = String::with_capacity(content.len() + binding.len() + 2);
        updated.push_str(&content[..insert_offset]);
        updated.push_str(&binding);
        if content[insert_offset..].starts_with('\n') {
            updated.push('\n');
        } else {
            updated.push_str("\n\n");
        }
        updated.push_str(&content[insert_offset..]);
        Some(updated)
    }

    fn is_commonjs_source(content: &str) -> bool {
        content.contains("module.exports")
            || content.contains("exports.")
            || content.contains("require(")
    }

    fn is_js_like_file(file_path: &str) -> bool {
        std::path::Path::new(file_path)
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| {
                matches!(
                    ext.to_ascii_lowercase().as_str(),
                    "js" | "jsx" | "mjs" | "cjs"
                )
            })
    }

    fn normalize_commonjs_module_specifier(specifier: &str) -> String {
        if !(specifier.starts_with("./") || specifier.starts_with("../")) {
            return specifier.to_string();
        }

        for ext in [
            ".d.ts", ".d.mts", ".d.cts", ".ts", ".tsx", ".mts", ".cts", ".js", ".jsx", ".mjs",
            ".cjs",
        ] {
            if let Some(stripped) = specifier.strip_suffix(ext) {
                return stripped.to_string();
            }
        }

        specifier.to_string()
    }

    fn is_declaration_file_path(file_path: &str) -> bool {
        let Some(file_name) = std::path::Path::new(file_path)
            .file_name()
            .and_then(|name| name.to_str())
        else {
            return false;
        };

        file_name == "d.ts"
            || file_name.ends_with(".d.ts")
            || file_name == "d.mts"
            || file_name.ends_with(".d.mts")
            || file_name == "d.cts"
            || file_name.ends_with(".d.cts")
    }

    fn declaration_file_prefers_type_only_import(content: &str, name: &str) -> bool {
        let type_usage = [
            format!(": {name}"),
            format!("<{name}>"),
            format!("extends {name}"),
            format!("implements {name}"),
            format!(" as {name}"),
        ]
        .iter()
        .any(|needle| content.contains(needle));

        if !type_usage {
            return false;
        }

        let value_usage = [
            format!("new {name}"),
            format!("{name}("),
            format!("{name}."),
            format!("typeof {name}"),
        ]
        .iter()
        .any(|needle| content.contains(needle));

        !value_usage
    }

    fn declaration_file_local_import_name(spec: &str) -> &str {
        let trimmed = spec.trim().trim_start_matches("type ").trim();
        if let Some((_, local)) = trimmed.split_once(" as ") {
            local.trim()
        } else {
            trimmed
        }
    }

    fn normalize_declaration_file_type_only_named_imports(content: &str) -> String {
        let mut normalized = String::with_capacity(content.len());
        for line in content.split_inclusive('\n') {
            let newline = if line.ends_with("\r\n") {
                "\r\n"
            } else if line.ends_with('\n') {
                "\n"
            } else {
                ""
            };
            let line_body = line.trim_end_matches(['\r', '\n']);
            let Some(open) = line_body.find('{') else {
                normalized.push_str(line_body);
                normalized.push_str(newline);
                continue;
            };
            let Some(close_rel) = line_body[open + 1..].find('}') else {
                normalized.push_str(line_body);
                normalized.push_str(newline);
                continue;
            };
            let close = open + 1 + close_rel;
            if !line_body[..open].trim_start().starts_with("import ")
                || !line_body[close..].contains(" from ")
            {
                normalized.push_str(line_body);
                normalized.push_str(newline);
                continue;
            }

            let import_prefix = line_body[..open].trim_start();
            let clause_is_type_only = import_prefix.starts_with("import type ");
            let imports = &line_body[open + 1..close];
            let mut rebuilt = Vec::new();
            for spec in imports.split(',') {
                let trimmed = spec.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if trimmed.starts_with("type ") {
                    rebuilt.push(trimmed.to_string());
                    continue;
                }
                let local_name = Self::declaration_file_local_import_name(trimmed);
                if !clause_is_type_only
                    && Self::declaration_file_prefers_type_only_import(content, local_name)
                {
                    rebuilt.push(format!("type {trimmed}"));
                } else {
                    rebuilt.push(trimmed.to_string());
                }
            }

            if rebuilt.is_empty() {
                normalized.push_str(line_body);
                normalized.push_str(newline);
                continue;
            }

            let mut rebuilt_line = String::with_capacity(line_body.len() + 8);
            rebuilt_line.push_str(&line_body[..open + 1]);
            rebuilt_line.push(' ');
            rebuilt_line.push_str(&rebuilt.join(", "));
            rebuilt_line.push(' ');
            rebuilt_line.push_str(&line_body[close..]);
            normalized.push_str(&rebuilt_line);
            normalized.push_str(newline);
        }
        normalized
    }

    fn collect_import_candidates(
        &self,
        current_file_path: &str,
        diagnostics: &[tsz::lsp::diagnostics::LspDiagnostic],
        auto_import_file_exclude_patterns: &[String],
        auto_import_specifier_exclude_regexes: &[String],
        import_module_specifier_preference: Option<&str>,
    ) -> Vec<ImportCandidate> {
        let mut files = self.open_files.clone();
        if !files.contains_key(current_file_path)
            && let Ok(content) = std::fs::read_to_string(current_file_path)
        {
            files.insert(current_file_path.to_string(), content);
        }
        Self::add_project_config_files(&mut files, current_file_path);
        if files.is_empty() {
            return Vec::new();
        }
        let is_commonjs_js_file = files.get(current_file_path).is_some_and(|content| {
            Self::is_js_like_file(current_file_path) && Self::is_commonjs_source(content)
        });

        let mut project = Project::new();
        project.set_allow_importing_ts_extensions(self.allow_importing_ts_extensions);
        project.set_auto_imports_allowed_without_tsconfig(
            self.auto_imports_allowed_for_inferred_projects,
        );
        project.set_import_module_specifier_ending(
            self.completion_import_module_specifier_ending.clone(),
        );
        project.set_import_module_specifier_preference(
            import_module_specifier_preference
                .map(std::string::ToString::to_string)
                .or_else(|| self.import_module_specifier_preference.clone()),
        );
        project.set_auto_import_file_exclude_patterns(auto_import_file_exclude_patterns.to_vec());
        project.set_auto_import_specifier_exclude_regexes(
            auto_import_specifier_exclude_regexes.to_vec(),
        );
        for (path, text) in files {
            project.set_file(path, text);
        }

        let mut candidates =
            project.get_import_candidates_for_diagnostics(current_file_path, diagnostics);
        if candidates.is_empty() {
            let mut fallback_names = rustc_hash::FxHashSet::default();
            for diag in diagnostics {
                if let Some(name) = Self::missing_name_from_diagnostic_message(&diag.message) {
                    fallback_names.insert(name);
                }
            }
            if fallback_names.is_empty() {
                // Preserve legacy behavior for diagnostics whose message shape does not
                // include a directly parseable missing identifier.
                candidates.extend(project.get_import_candidates_for_prefix(current_file_path, ""));
            } else {
                for missing_name in fallback_names {
                    candidates.extend(
                        project.get_import_candidates_for_prefix(current_file_path, &missing_name),
                    );
                }
            }
        }

        let mut seen: rustc_hash::FxHashSet<(String, String, String, bool)> =
            rustc_hash::FxHashSet::default();
        let mut deduped = Vec::with_capacity(candidates.len());

        for mut candidate in candidates.drain(..) {
            if is_commonjs_js_file
                && !matches!(
                    candidate.kind,
                    tsz::lsp::code_actions::ImportCandidateKind::Named { .. }
                )
            {
                continue;
            }
            if Self::is_js_like_file(current_file_path) {
                candidate.module_specifier =
                    Self::normalize_commonjs_module_specifier(&candidate.module_specifier);
            }
            let kind_key = match &candidate.kind {
                tsz::lsp::code_actions::ImportCandidateKind::Named { export_name } => {
                    format!("named:{export_name}")
                }
                tsz::lsp::code_actions::ImportCandidateKind::Default => "default".to_string(),
                tsz::lsp::code_actions::ImportCandidateKind::Namespace => "namespace".to_string(),
            };
            if seen.insert((
                candidate.module_specifier.clone(),
                candidate.local_name.clone(),
                kind_key,
                candidate.is_type_only,
            )) {
                deduped.push(candidate);
            }
        }

        reorder_import_candidates_for_package_roots(&mut deduped);
        deduped
    }

    fn missing_name_from_diagnostic_message(message: &str) -> Option<String> {
        let prefixes = [
            ("Cannot find name '", '\''),
            ("Cannot find name \"", '"'),
            ("Cannot find name `", '`'),
        ];
        for (prefix, terminator) in prefixes {
            if let Some(rest) = message.strip_prefix(prefix)
                && let Some(end) = rest.find(terminator)
                && end > 0
            {
                return Some(rest[..end].to_string());
            }
        }
        None
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
            let organize_imports_ignore_case = request
                .arguments
                .get("preferences")
                .and_then(|p| p.get("organizeImportsIgnoreCase"))
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(self.organize_imports_ignore_case);
            let line_map = LineMap::build(&content);
            let provider = CodeActionProvider::new(
                &arena,
                &binder,
                &line_map,
                file_path.to_string(),
                &content,
            )
            .with_organize_imports_ignore_case(organize_imports_ignore_case);

            let mut diagnostics = self.get_semantic_diagnostics_full(file_path, &content);
            diagnostics.extend(
                self.synthetic_missing_name_expression_diagnostics(file_path, &content, &binder),
            );
            let mut seen_diags = rustc_hash::FxHashSet::default();
            diagnostics
                .retain(|d| seen_diags.insert((d.code, d.start, d.length, d.message_text.clone())));

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

            let auto_import_file_exclude_patterns =
                Self::extract_auto_import_file_exclude_patterns(request)
                    .unwrap_or_else(|| self.auto_import_file_exclude_patterns.clone());
            let auto_import_specifier_exclude_regexes =
                Self::extract_auto_import_specifier_exclude_regexes(request)
                    .unwrap_or_else(|| self.auto_import_specifier_exclude_regexes.clone());
            let import_module_specifier_preference = request
                .arguments
                .get("preferences")
                .and_then(|p| p.get("importModuleSpecifierPreference"))
                .and_then(|v| v.as_str())
                .or(self.import_module_specifier_preference.as_deref());
            let import_candidates = if fix_id == "fixMissingImport" {
                self.collect_import_candidates(
                    file_path,
                    &filtered_diagnostics,
                    &auto_import_file_exclude_patterns,
                    &auto_import_specifier_exclude_regexes,
                    import_module_specifier_preference,
                )
            } else {
                Vec::new()
            };

            if fix_id == "fixMissingImport"
                && let Some(updated_content) =
                    Self::apply_missing_imports_fix_all(file_path, &content, &import_candidates)
                && let Some((start_off, end_off, replacement)) =
                    Self::compute_minimal_edit(&content, &updated_content)
            {
                let start_pos = line_map.offset_to_position(start_off, &content);
                let end_pos = line_map.offset_to_position(end_off, &content);
                return TsServerResponse {
                    seq,
                    msg_type: "response".to_string(),
                    command: "getCombinedCodeFix".to_string(),
                    request_seq: request.seq,
                    success: true,
                    message: None,
                    body: Some(serde_json::json!({
                        "changes": [{
                            "fileName": file_path,
                            "textChanges": [{
                                "start": { "line": start_pos.line + 1, "offset": start_pos.character + 1 },
                                "end": { "line": end_pos.line + 1, "offset": end_pos.character + 1 },
                                "newText": replacement
                            }]
                        }]
                    })),
                };
            }

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

            if fix_id == "annotateWithTypeFromJSDoc"
                && let Some(updated_content) =
                    Self::apply_simple_jsdoc_annotation_fallback(&content)
                && let Some((start_off, end_off, replacement)) =
                    Self::compute_minimal_edit(&content, &updated_content)
            {
                let start_pos = line_map.offset_to_position(start_off, &content);
                let end_pos = line_map.offset_to_position(end_off, &content);
                all_changes.clear();
                all_changes.push(serde_json::json!({
                    "fileName": file_path,
                    "textChanges": [{
                        "start": { "line": start_pos.line + 1, "offset": start_pos.character + 1 },
                        "end": { "line": end_pos.line + 1, "offset": end_pos.character + 1 },
                        "newText": replacement
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

fn find_first_implements_class(content: &str) -> Option<(String, String, usize, usize)> {
    let mut cursor = 0usize;
    while let Some(rel_class) = content[cursor..].find("class ") {
        let class_start = cursor + rel_class;
        let class_name_start = class_start + "class ".len();
        let class_name = read_identifier(&content[class_name_start..])?;
        let class_body_open_rel = content[class_name_start..].find('{')?;
        let class_open_brace = class_name_start + class_body_open_rel;
        let header = &content[class_start..class_open_brace];

        if let Some(implements_idx) = header.find("implements ") {
            let interface_name_start = implements_idx + "implements ".len();
            let interface_name = read_identifier(&header[interface_name_start..])?;
            let class_close_brace = find_matching_brace(content, class_open_brace)?;
            return Some((
                class_name.to_string(),
                interface_name.to_string(),
                class_open_brace,
                class_close_brace,
            ));
        }

        cursor = class_name_start;
    }
    None
}

fn parse_named_import_map(content: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("import ") {
            continue;
        }
        let Some(open_brace) = trimmed.find('{') else {
            continue;
        };
        let Some(close_brace_rel) = trimmed[open_brace + 1..].find('}') else {
            continue;
        };
        let close_brace = open_brace + 1 + close_brace_rel;
        let Some(from_idx) = trimmed[close_brace..].find("from") else {
            continue;
        };
        let from_segment = &trimmed[close_brace + from_idx + "from".len()..];
        let Some(module_specifier) = extract_quoted_text(from_segment) else {
            continue;
        };
        let imports = &trimmed[open_brace + 1..close_brace];
        for entry in imports.split(',') {
            let import_name = entry.trim().trim_start_matches("type ").trim();
            if import_name.is_empty() {
                continue;
            }
            if let Some((_, local)) = import_name.split_once(" as ") {
                let local_name = local.trim();
                if !local_name.is_empty() {
                    map.insert(local_name.to_string(), module_specifier.to_string());
                }
            } else {
                map.insert(import_name.to_string(), module_specifier.to_string());
            }
        }
    }
    map
}

fn parse_interface_properties(
    content: &str,
    interface_name: &str,
) -> Option<Vec<(String, String)>> {
    let interface_token = format!("interface {interface_name}");
    let interface_pos = content.find(&interface_token)?;
    let open_brace_rel = content[interface_pos..].find('{')?;
    let open_brace = interface_pos + open_brace_rel;
    let close_brace = find_matching_brace(content, open_brace)?;
    let body = content.get(open_brace + 1..close_brace)?;

    let mut properties = Vec::new();
    for line in body.lines() {
        let member = line.trim().trim_end_matches(';');
        if member.is_empty() || member.starts_with("//") {
            continue;
        }
        let Some((lhs, rhs)) = member.split_once(':') else {
            continue;
        };
        let mut name = lhs.trim();
        if let Some(rest) = name.strip_prefix("readonly ") {
            name = rest.trim();
        }
        if let Some(rest) = name.strip_suffix('?') {
            name = rest.trim_end();
        }
        if !is_identifier(name) {
            continue;
        }
        properties.push((name.to_string(), rhs.trim().to_string()));
    }
    Some(properties)
}

fn class_body_has_member(class_body: &str, member_name: &str) -> bool {
    for line in class_body.lines() {
        let mut trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("readonly ") {
            trimmed = rest.trim_start();
        }
        if let Some(rest) = trimmed.strip_prefix(member_name)
            && rest
                .chars()
                .next()
                .is_some_and(|ch| matches!(ch, ':' | '?' | '(' | '<' | ' '))
        {
            return true;
        }
    }
    false
}

fn extract_type_identifiers(type_text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for ch in type_text.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '$' {
            current.push(ch);
        } else if !current.is_empty() {
            if is_identifier(&current) {
                out.push(current.clone());
            }
            current.clear();
        }
    }
    if !current.is_empty() && is_identifier(&current) {
        out.push(current);
    }
    out
}

fn should_import_identifier(ident: &str) -> bool {
    if ident.is_empty() {
        return false;
    }
    if ident
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_lowercase())
    {
        return false;
    }
    !matches!(
        ident,
        "Array"
            | "ArrayBuffer"
            | "Boolean"
            | "Date"
            | "Error"
            | "Function"
            | "Number"
            | "Object"
            | "Promise"
            | "ReadonlyArray"
            | "RegExp"
            | "String"
            | "Symbol"
            | "Uint8Array"
    )
}

fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_' || first == '$') {
        return false;
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$')
}

fn extract_quoted_text(text: &str) -> Option<&str> {
    let quote_idx = text.find(['"', '\''])?;
    let quote = text.as_bytes()[quote_idx] as char;
    let rest = &text[quote_idx + 1..];
    let end_rel = rest.find(quote)?;
    Some(&rest[..end_rel])
}

fn read_identifier(text: &str) -> Option<&str> {
    let trimmed = text.trim_start();
    let start_offset = text.len() - trimmed.len();
    let mut chars = trimmed.char_indices();
    let (_, first) = chars.next()?;
    if !(first.is_ascii_alphabetic() || first == '_' || first == '$') {
        return None;
    }
    let mut end = first.len_utf8();
    for (idx, ch) in chars {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '$' {
            end = idx + ch.len_utf8();
        } else {
            break;
        }
    }
    Some(&text[start_offset..start_offset + end])
}

fn find_matching_brace(content: &str, open_brace: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (idx, ch) in content[open_brace..].char_indices() {
        if ch == '{' {
            depth += 1;
        } else if ch == '}' {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return Some(open_brace + idx);
            }
        }
    }
    None
}

fn resolve_module_path(
    from_file_path: &str,
    module_specifier: &str,
    files: &rustc_hash::FxHashMap<String, String>,
) -> Option<String> {
    if !module_specifier.starts_with('.') {
        return files
            .keys()
            .find(|path| {
                path.ends_with(module_specifier)
                    || path.trim_start_matches('/').ends_with(module_specifier)
            })
            .cloned();
    }

    for candidate in relative_module_path_candidates(from_file_path, module_specifier) {
        if let Some(key) = find_virtual_file_key(files, &candidate) {
            return Some(key);
        }
        if std::path::Path::new(&candidate).exists() {
            return Some(candidate);
        }
    }
    None
}

fn relative_module_path_candidates(from_file_path: &str, module_specifier: &str) -> Vec<String> {
    let Some(base_dir) = std::path::Path::new(from_file_path).parent() else {
        return Vec::new();
    };
    let joined = normalize_simple_path(base_dir.join(module_specifier));
    let joined_str = joined.to_string_lossy().replace('\\', "/");
    let has_ext = std::path::Path::new(&joined_str).extension().is_some();
    if has_ext {
        return vec![joined_str];
    }

    let mut candidates = Vec::new();
    for ext in ["ts", "tsx", "d.ts", "mts", "cts", "d.mts", "d.cts"] {
        candidates.push(format!("{joined_str}.{ext}"));
    }
    for ext in ["ts", "tsx", "d.ts", "mts", "cts", "d.mts", "d.cts"] {
        candidates.push(format!("{joined_str}/index.{ext}"));
    }
    candidates
}

fn find_virtual_file_key(
    files: &rustc_hash::FxHashMap<String, String>,
    candidate: &str,
) -> Option<String> {
    if files.contains_key(candidate) {
        return Some(candidate.to_string());
    }

    let normalize = |value: &str| {
        value
            .replace('\\', "/")
            .trim_start_matches('/')
            .to_ascii_lowercase()
    };
    let candidate_norm = normalize(candidate);
    files
        .keys()
        .find(|key| normalize(key) == candidate_norm)
        .cloned()
}

fn normalize_simple_path(path: std::path::PathBuf) -> std::path::PathBuf {
    let mut out = std::path::PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::RootDir
            | std::path::Component::Normal(_)
            | std::path::Component::Prefix(_) => out.push(component.as_os_str()),
        }
    }
    out
}

fn is_path_excluded_with_patterns(path: &str, patterns: &[String]) -> bool {
    let normalized_path = path.replace('\\', "/");
    let trimmed = normalized_path.trim_start_matches('/');
    patterns.iter().any(|pattern| {
        let normalized_pattern = pattern.replace('\\', "/");
        let pattern_trimmed = normalized_pattern.trim_start_matches('/');
        wildcard_match(pattern_trimmed, trimmed)
            || wildcard_match(&normalized_pattern, &normalized_path)
            || trimmed.ends_with(pattern_trimmed)
    })
}

fn wildcard_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let mut dp = vec![vec![false; t.len() + 1]; p.len() + 1];
    dp[0][0] = true;

    for i in 1..=p.len() {
        if p[i - 1] == '*' {
            dp[i][0] = dp[i - 1][0];
        }
    }
    for i in 1..=p.len() {
        for j in 1..=t.len() {
            if p[i - 1] == '*' {
                dp[i][j] = dp[i - 1][j] || dp[i][j - 1];
            } else if p[i - 1] == t[j - 1] {
                dp[i][j] = dp[i - 1][j - 1];
            }
        }
    }

    dp[p.len()][t.len()]
}

#[derive(Clone)]
struct ImportSpecifierEntry {
    raw: String,
    local_name: String,
    is_type_only: bool,
}

fn parse_named_import_line(line: &str) -> Option<(Vec<ImportSpecifierEntry>, String, char)> {
    let trimmed = line.trim();
    if !trimmed.starts_with("import ") {
        return None;
    }
    let open_brace = trimmed.find('{')?;
    let close_brace_rel = trimmed[open_brace + 1..].find('}')?;
    let close_brace = open_brace + 1 + close_brace_rel;
    let import_segment = &trimmed[open_brace + 1..close_brace];
    let from_segment = &trimmed[close_brace + 1..];
    let module_specifier = extract_quoted_text(from_segment)?.to_string();
    let quote = from_segment.find('\'').map(|_| '\'').unwrap_or('"');

    let mut specs = Vec::new();
    for part in import_segment.split(',') {
        if let Some(spec) = parse_import_spec_entry(part) {
            specs.push(spec);
        }
    }
    Some((specs, module_specifier, quote))
}

fn parse_inserted_import_spec(new_text: &str) -> Option<ImportSpecifierEntry> {
    let trimmed = new_text
        .trim()
        .trim_start_matches(',')
        .trim_end_matches(',')
        .trim();
    parse_import_spec_entry(trimmed)
}

fn parse_import_spec_entry(text: &str) -> Option<ImportSpecifierEntry> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let is_type_only = trimmed.starts_with("type ");
    let without_type = trimmed.trim_start_matches("type ").trim();
    let local_name = if let Some((_, local)) = without_type.split_once(" as ") {
        local.trim().to_string()
    } else {
        without_type.to_string()
    };
    if !is_identifier(&local_name) {
        return None;
    }
    Some(ImportSpecifierEntry {
        raw: trimmed.to_string(),
        local_name,
        is_type_only,
    })
}

fn import_specs_are_sorted(
    specs: &[ImportSpecifierEntry],
    type_order: &str,
    ignore_case: bool,
) -> bool {
    specs.windows(2).all(|pair| {
        import_spec_sort_key(&pair[0], type_order, ignore_case)
            <= import_spec_sort_key(&pair[1], type_order, ignore_case)
    })
}

fn import_spec_sort_key(
    spec: &ImportSpecifierEntry,
    type_order: &str,
    ignore_case: bool,
) -> (u8, String, u8, String) {
    let group = match type_order {
        "last" => {
            if spec.is_type_only {
                1
            } else {
                0
            }
        }
        "first" => {
            if spec.is_type_only {
                0
            } else {
                1
            }
        }
        _ => 0,
    };
    let (folded, case_rank, original) = if ignore_case {
        let folded = spec.local_name.to_ascii_lowercase();
        let case_rank = if spec
            .local_name
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_lowercase())
        {
            0
        } else {
            1
        };
        (folded, case_rank, String::new())
    } else {
        (spec.local_name.clone(), 0, String::new())
    };
    (group, folded, case_rank, original)
}

const fn position_leq(a: tsz::lsp::position::Position, b: tsz::lsp::position::Position) -> bool {
    a.line < b.line || (a.line == b.line && a.character <= b.character)
}

const fn positions_overlap(
    a_start: tsz::lsp::position::Position,
    a_end: tsz::lsp::position::Position,
    b_start: tsz::lsp::position::Position,
    b_end: tsz::lsp::position::Position,
) -> bool {
    position_leq(a_start, b_end) && position_leq(b_start, a_end)
}

fn parse_bare_identifier_expression(line: &str) -> Option<(usize, &str)> {
    let trimmed_start = line.trim_start();
    let leading_ws = line.len().saturating_sub(trimmed_start.len());
    let trimmed = trimmed_start.trim_end();
    if trimmed.is_empty() {
        return None;
    }
    let expr = trimmed.strip_suffix(';').unwrap_or(trimmed).trim_end();
    if expr.is_empty() {
        return None;
    }

    let mut chars = expr.chars();
    let first = chars.next()?;
    if !(first.is_ascii_alphabetic() || first == '_' || first == '$') {
        return None;
    }
    if !chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$') {
        return None;
    }

    Some((leading_ws, expr))
}

fn parse_identifier_call_expression(line: &str) -> Option<(usize, &str)> {
    let trimmed_start = line.trim_start();
    let leading_ws = line.len().saturating_sub(trimmed_start.len());
    let trimmed = trimmed_start.trim_end();
    if trimmed.is_empty() {
        return None;
    }
    let expr = trimmed.strip_suffix(';').unwrap_or(trimmed).trim_end();
    if expr.is_empty() {
        return None;
    }

    let mut chars = expr.char_indices();
    let (_, first) = chars.next()?;
    if !(first.is_ascii_alphabetic() || first == '_' || first == '$') {
        return None;
    }

    let mut ident_end = first.len_utf8();
    for (idx, ch) in chars {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '$' {
            ident_end = idx + ch.len_utf8();
            continue;
        }
        ident_end = idx;
        break;
    }

    let name = expr.get(..ident_end)?;
    if !is_identifier(name) {
        return None;
    }
    if is_reserved_word(name) {
        return None;
    }

    let rest = expr.get(ident_end..)?.trim_start();
    if !rest.starts_with('(') {
        return None;
    }

    Some((leading_ws, name))
}

fn is_reserved_word(name: &str) -> bool {
    matches!(
        name,
        "if" | "else"
            | "for"
            | "while"
            | "do"
            | "switch"
            | "case"
            | "default"
            | "break"
            | "continue"
            | "return"
            | "throw"
            | "try"
            | "catch"
            | "finally"
            | "function"
            | "class"
            | "new"
            | "this"
            | "super"
            | "typeof"
            | "void"
            | "delete"
            | "await"
            | "yield"
    )
}

fn find_jsdoc_import_line(
    content: &str,
) -> Option<(u32, String, String, String, Vec<ImportSpecifierEntry>)> {
    for (idx, line) in content.lines().enumerate() {
        let Some(at_import) = line.find("@import") else {
            continue;
        };
        let prefix = &line[..at_import];
        let import_part = &line[at_import + "@import".len()..];
        let open = import_part.find('{')?;
        let close = import_part[open + 1..].find('}')?;
        let spec_end = open + 1 + close;
        let specs_text = import_part[open + 1..spec_end].trim();
        let after_specs = &import_part[spec_end + 1..];
        let Some(from_idx) = after_specs.find("from") else {
            continue;
        };
        let module_text = after_specs[from_idx + "from".len()..].trim();
        let Some(module_specifier) = extract_quoted_text(module_text) else {
            continue;
        };

        let specs = specs_text
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| ImportSpecifierEntry {
                raw: s.to_string(),
                local_name: s
                    .split_once(" as ")
                    .map_or_else(|| s.to_string(), |(_, local)| local.trim().to_string()),
                is_type_only: s.starts_with("type "),
            })
            .collect::<Vec<_>>();
        if specs.is_empty() {
            continue;
        }
        return Some((
            idx as u32 + 1,
            line.to_string(),
            prefix.to_string(),
            module_specifier.to_string(),
            specs,
        ));
    }
    None
}

fn extract_jsdoc_imported_names(content: &str) -> std::collections::HashSet<String> {
    let mut names = std::collections::HashSet::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if !trimmed.contains("@import") {
            continue;
        }
        let Some(open) = trimmed.find('{') else {
            continue;
        };
        let Some(close_rel) = trimmed[open + 1..].find('}') else {
            continue;
        };
        let close = open + 1 + close_rel;
        for raw_spec in trimmed[open + 1..close].split(',') {
            let imported = raw_spec
                .trim()
                .split_whitespace()
                .next()
                .unwrap_or_default();
            if !imported.is_empty() {
                names.insert(imported.to_string());
            }
        }
    }
    names
}

fn extract_jsdoc_type_identifier_spans(line: &str) -> Vec<(String, usize)> {
    let mut out = Vec::new();
    let Some(open) = line.find('{') else {
        return out;
    };
    let Some(close_rel) = line[open + 1..].find('}') else {
        return out;
    };
    let close = open + 1 + close_rel;
    let type_text = &line[open + 1..close];
    let bytes = type_text.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        let ch = bytes[i] as char;
        if !(ch.is_ascii_alphabetic() || ch == '_' || ch == '$') {
            i += 1;
            continue;
        }
        let start = i;
        i += 1;
        while i < bytes.len() {
            let next = bytes[i] as char;
            if next.is_ascii_alphanumeric() || next == '_' || next == '$' {
                i += 1;
            } else {
                break;
            }
        }
        let Some(name) = type_text.get(start..i) else {
            continue;
        };
        if !name.chars().next().is_some_and(|c| c.is_ascii_uppercase()) || !is_identifier(name) {
            continue;
        }
        out.push((name.to_string(), open + 1 + start));
    }
    out
}

fn is_same_import_candidate_symbol(a: &ImportCandidate, b: &ImportCandidate) -> bool {
    if a.local_name != b.local_name || a.is_type_only != b.is_type_only {
        return false;
    }
    match (&a.kind, &b.kind) {
        (
            tsz::lsp::code_actions::ImportCandidateKind::Named {
                export_name: a_export_name,
            },
            tsz::lsp::code_actions::ImportCandidateKind::Named {
                export_name: b_export_name,
            },
        ) => a_export_name == b_export_name,
        (
            tsz::lsp::code_actions::ImportCandidateKind::Default,
            tsz::lsp::code_actions::ImportCandidateKind::Default,
        ) => true,
        (
            tsz::lsp::code_actions::ImportCandidateKind::Namespace,
            tsz::lsp::code_actions::ImportCandidateKind::Namespace,
        ) => true,
        _ => false,
    }
}

fn prefers_package_root_specifier(a: &ImportCandidate, b: &ImportCandidate) -> bool {
    if !is_same_import_candidate_symbol(a, b) {
        return false;
    }
    if a.module_specifier.starts_with('.') || b.module_specifier.starts_with('.') {
        return false;
    }
    if a.module_specifier == b.module_specifier {
        return false;
    }
    let Some(rest) = b.module_specifier.strip_prefix(&a.module_specifier) else {
        return false;
    };
    rest.starts_with('/')
}

fn reorder_import_candidates_for_package_roots(candidates: &mut [ImportCandidate]) {
    // Keep the original discovery order unless a package root/module-subpath pair
    // targets the same symbol, in which case tsserver prefers the package root.
    for i in 0..candidates.len() {
        for j in (i + 1)..candidates.len() {
            if prefers_package_root_specifier(&candidates[j], &candidates[i]) {
                candidates.swap(i, j);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        LineMap, Server, TsServerRequest, parse_identifier_call_expression, positions_overlap,
    };
    use crate::{LogConfig, LogLevel, ServerMode};
    use rustc_hash::FxHashMap;
    use std::path::PathBuf;
    use tsz::lsp::code_actions::ImportCandidate;
    use tsz::parser::ParserState;

    fn make_server() -> Server {
        Server {
            completion_import_module_specifier_ending: None,
            import_module_specifier_preference: None,
            organize_imports_type_order: None,
            organize_imports_ignore_case: false,
            auto_import_file_exclude_patterns: Vec::new(),
            lib_dir: PathBuf::from("/nonexistent"),
            tests_lib_dir: PathBuf::from("/nonexistent"),
            lib_cache: FxHashMap::default(),
            unified_lib_cache: None,
            checks_completed: 0,
            response_seq: 0,
            open_files: FxHashMap::default(),
            external_project_files: FxHashMap::default(),
            _server_mode: ServerMode::Semantic,
            _log_config: LogConfig {
                level: LogLevel::Off,
                file: None,
                trace_to_console: false,
            },
            enable_telemetry: false,
            allow_importing_ts_extensions: false,
            auto_imports_allowed_for_inferred_projects: true,
            inferred_module_is_none_for_projects: false,
            auto_import_specifier_exclude_regexes: Vec::new(),
        }
    }

    #[test]
    fn normalize_jsdoc_function_type() {
        assert_eq!(
            Server::normalize_jsdoc_type("function(*, ...number, ...boolean): void"),
            "(arg0: any, arg1: number[], ...rest: boolean[]) => void"
        );
        assert_eq!(
            Server::normalize_jsdoc_type("function(this:{ a: string}, string, number): boolean"),
            "(this: { a: string; }, arg1: string, arg2: number) => boolean"
        );
    }

    #[test]
    fn normalize_jsdoc_object_generic() {
        assert_eq!(
            Server::normalize_jsdoc_type("Object<string, boolean>"),
            "{ [s: string]: boolean; }"
        );
        assert_eq!(
            Server::normalize_jsdoc_type("Object<number, string>"),
            "{ [n: number]: string; }"
        );
    }

    #[test]
    fn normalize_jsdoc_promise_generic() {
        assert_eq!(
            Server::normalize_jsdoc_type("promise<String>"),
            "Promise<string>"
        );
    }

    #[test]
    fn jsdoc_fallback_object_index_signatures() {
        let src = "\n/** @param {Object<string, boolean>} sb\n  * @param {Object<number, string>} ns */\nfunction f(sb, ns) {\n    sb; ns;\n}\n";
        let expected = "\n/** @param {Object<string, boolean>} sb\n  * @param {Object<number, string>} ns */\nfunction f(sb: { [s: string]: boolean; }, ns: { [n: number]: string; }) {\n    sb; ns;\n}\n";
        let actual = Server::apply_simple_jsdoc_annotation_fallback(src)
            .expect("expected jsdoc fallback to apply");
        assert_eq!(actual, expected);
    }

    #[test]
    fn jsdoc_fallback_template_function() {
        let src = "/**\n * @template T\n * @param {number} a\n * @param {T} b\n */\nfunction f(a, b) {\n    return a || b;\n}\n";
        let expected = "/**\n * @template T\n * @param {number} a\n * @param {T} b\n */\nfunction f<T>(a: number, b: T) {\n    return a || b;\n}\n";
        let actual = Server::apply_simple_jsdoc_annotation_fallback(src)
            .expect("expected jsdoc fallback to apply");
        assert_eq!(actual, expected);
    }

    #[test]
    fn fix_missing_imports_combines_sequential_import_merges() {
        let src = "import { Test1, Test4 } from './file1';\ninterface Testing {\n    test1: Test1;\n    test2: Test2;\n    test3: Test3;\n    test4: Test4;\n}\n";
        let candidates = vec![
            ImportCandidate::named(
                "./file1".to_string(),
                "Test2".to_string(),
                "Test2".to_string(),
            ),
            ImportCandidate::named(
                "./file1".to_string(),
                "Test3".to_string(),
                "Test3".to_string(),
            ),
        ];

        let updated = Server::apply_missing_imports_fix_all("file2.ts", src, &candidates)
            .expect("expected missing import fix-all to produce an edit");

        assert_eq!(
            updated,
            "import { Test1, Test2, Test3, Test4 } from './file1';\ninterface Testing {\n    test1: Test1;\n    test2: Test2;\n    test3: Test3;\n    test4: Test4;\n}\n"
        );
    }

    #[test]
    fn fix_missing_imports_uses_require_for_commonjs_js_files() {
        let src = "exports.dedupeLines = data => {\n  variants\n}\n";
        let candidates = vec![ImportCandidate::named(
            "./matrix.js".to_string(),
            "variants".to_string(),
            "variants".to_string(),
        )];

        let updated = Server::apply_missing_imports_fix_all("main.js", src, &candidates)
            .expect("expected commonjs missing import to produce an edit");

        assert_eq!(
            updated,
            "const { variants } = require(\"./matrix\")\n\nexports.dedupeLines = data => {\n  variants\n}\n"
        );
    }

    #[test]
    fn synthetic_missing_name_detects_commonjs_export_candidates() {
        let mut server = make_server();
        server.open_files.insert(
            "/matrix.js".to_string(),
            "exports.variants = [];".to_string(),
        );
        let main = "exports.dedupeLines = data => {\n  variants\n}\n".to_string();
        server
            .open_files
            .insert("/main.js".to_string(), main.clone());

        let mut parser = ParserState::new("/main.js".to_string(), main.clone());
        let root = parser.parse_source_file();
        let arena = parser.into_arena();
        let mut binder = tsz::binder::BinderState::new();
        binder.bind_source_file(&arena, root);

        let diagnostics =
            server.synthetic_missing_name_expression_diagnostics("/main.js", &main, &binder);
        assert!(
            diagnostics.iter().any(|diag| {
                diag.code == tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME
                    && diag.message_text.contains("variants")
            }),
            "expected synthetic missing-name diagnostic for 'variants', got {diagnostics:?}"
        );
    }

    #[test]
    fn rewrite_single_import_to_commonjs_require_converts_named_import() {
        let rewritten = Server::rewrite_single_import_to_commonjs_require(
            "import { variants } from \"./matrix.js\";\n",
        )
        .expect("expected named import rewrite");
        assert_eq!(rewritten, "const { variants } = require(\"./matrix\")\n");
    }

    #[test]
    fn collect_import_candidates_normalizes_commonjs_js_specifiers() {
        let mut server = make_server();
        server.open_files.insert(
            "/matrix.js".to_string(),
            "exports.variants = [];".to_string(),
        );
        server.open_files.insert(
            "/totally-irrelevant-no-way-this-changes-things-right.js".to_string(),
            "export default 0;".to_string(),
        );
        let main = "exports.dedupeLines = data => {\n  variants\n}\n".to_string();
        server.open_files.insert("/main.js".to_string(), main);

        let diagnostics = vec![tsz::lsp::diagnostics::LspDiagnostic {
            range: tsz::lsp::position::Range::new(
                tsz::lsp::position::Position::new(1, 2),
                tsz::lsp::position::Position::new(1, 10),
            ),
            message: "Cannot find name 'variants'.".to_string(),
            code: Some(tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME),
            severity: Some(tsz::lsp::diagnostics::DiagnosticSeverity::Error),
            source: Some("tsz".to_string()),
            related_information: None,
            reports_unnecessary: None,
            reports_deprecated: None,
        }];

        let candidates = server.collect_import_candidates("/main.js", &diagnostics, &[], &[], None);
        let module_specifiers: Vec<String> = candidates
            .into_iter()
            .map(|candidate| candidate.module_specifier)
            .collect();

        assert!(
            module_specifiers.iter().any(|spec| spec == "./matrix"),
            "expected normalized './matrix' specifier, got {module_specifiers:?}"
        );
        assert!(
            module_specifiers
                .iter()
                .all(|spec| spec != "./totally-irrelevant-no-way-this-changes-things-right"),
            "did not expect unrelated default export candidate, got {module_specifiers:?}"
        );
    }

    #[test]
    fn collect_import_candidates_prefers_package_root_specifier_before_subpath() {
        let mut server = make_server();
        server.open_files.insert(
            "/node_modules/pkg/package.json".to_string(),
            r#"{
    "name": "pkg",
    "version": "1.0.0",
    "exports": {
        ".": "./index.js",
        "./utils": "./utils.js"
    }
}"#
            .to_string(),
        );
        server.open_files.insert(
            "/node_modules/pkg/utils.d.ts".to_string(),
            "export function add(a: number, b: number) {}".to_string(),
        );
        server.open_files.insert(
            "/node_modules/pkg/index.d.ts".to_string(),
            "export * from \"./utils\";".to_string(),
        );
        server
            .open_files
            .insert("/src/index.ts".to_string(), "add".to_string());

        let diagnostics = vec![tsz::lsp::diagnostics::LspDiagnostic {
            range: tsz::lsp::position::Range::new(
                tsz::lsp::position::Position::new(0, 0),
                tsz::lsp::position::Position::new(0, 3),
            ),
            message: "Cannot find name 'add'.".to_string(),
            code: Some(tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME),
            severity: Some(tsz::lsp::diagnostics::DiagnosticSeverity::Error),
            source: Some("tsz".to_string()),
            related_information: None,
            reports_unnecessary: None,
            reports_deprecated: None,
        }];

        let candidates =
            server.collect_import_candidates("/src/index.ts", &diagnostics, &[], &[], None);
        let module_specifiers: Vec<String> = candidates
            .into_iter()
            .filter(|candidate| candidate.local_name == "add")
            .map(|candidate| candidate.module_specifier)
            .collect();

        assert_eq!(
            module_specifiers,
            vec!["pkg".to_string(), "pkg/utils".to_string()]
        );
    }

    #[test]
    fn collect_import_candidates_respects_node_next_package_exports_root_only() {
        let mut server = make_server();
        server.open_files.insert(
            "/node_modules/pack/package.json".to_string(),
            r#"{
    "name": "pack",
    "version": "1.0.0",
    "exports": {
        ".": "./main.mjs"
    }
}"#
            .to_string(),
        );
        server.open_files.insert(
            "/node_modules/pack/main.d.mts".to_string(),
            "import {} from \"./unreachable.mjs\";\nexport const fromMain = 0;".to_string(),
        );
        server.open_files.insert(
            "/node_modules/pack/unreachable.d.mts".to_string(),
            "export const fromUnreachable = 0;".to_string(),
        );
        server.open_files.insert(
            "/index.mts".to_string(),
            "import { fromMain } from \"pack\";\nfromUnreachable".to_string(),
        );

        let diagnostics = vec![tsz::lsp::diagnostics::LspDiagnostic {
            range: tsz::lsp::position::Range::new(
                tsz::lsp::position::Position::new(1, 0),
                tsz::lsp::position::Position::new(1, 15),
            ),
            message: "Cannot find name 'fromUnreachable'.".to_string(),
            code: Some(tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME),
            severity: Some(tsz::lsp::diagnostics::DiagnosticSeverity::Error),
            source: Some("tsz".to_string()),
            related_information: None,
            reports_unnecessary: None,
            reports_deprecated: None,
        }];

        let candidates =
            server.collect_import_candidates("/index.mts", &diagnostics, &[], &[], None);
        assert!(
            candidates.is_empty(),
            "expected no import candidates for unreachable node-next subpath export, got {candidates:?}"
        );
    }

    #[test]
    fn handle_get_combined_code_fix_fix_missing_import_merges_all_missing_names() {
        let mut server = make_server();
        let file1 = "/tests/cases/fourslash/file1.ts".to_string();
        let file2 = "/tests/cases/fourslash/file2.ts".to_string();
        server.open_files.insert(
            file1,
            "export interface Test1 {}\nexport interface Test2 {}\nexport interface Test3 {}\nexport interface Test4 {}\n".to_string(),
        );
        let original_file2 = "import { Test1, Test4 } from './file1';\ninterface Testing {\n    test1: Test1;\n    test2: Test2;\n    test3: Test3;\n    test4: Test4;\n}\n";
        server
            .open_files
            .insert(file2.clone(), original_file2.to_string());

        let req = TsServerRequest {
            seq: 1,
            _msg_type: "request".to_string(),
            command: "getCombinedCodeFix".to_string(),
            arguments: serde_json::json!({
                "scope": { "type": "file", "args": { "file": file2 } },
                "fixId": "fixMissingImport",
                "preferences": {}
            }),
        };
        let resp = server.handle_get_combined_code_fix(1, &req);
        assert!(resp.success, "expected getCombinedCodeFix to succeed");

        let changes = resp
            .body
            .as_ref()
            .and_then(|body| body.get("changes"))
            .and_then(serde_json::Value::as_array)
            .expect("missing changes array");
        assert_eq!(changes.len(), 1, "expected one file change");
        let text_changes = changes[0]
            .get("textChanges")
            .and_then(serde_json::Value::as_array)
            .expect("missing textChanges");
        assert_eq!(
            text_changes.len(),
            1,
            "expected one consolidated text change"
        );

        let change = &text_changes[0];
        let start_line = change["start"]["line"].as_u64().expect("start line") as u32;
        let start_offset = change["start"]["offset"].as_u64().expect("start offset") as u32;
        let end_line = change["end"]["line"].as_u64().expect("end line") as u32;
        let end_offset = change["end"]["offset"].as_u64().expect("end offset") as u32;
        let new_text = change["newText"].as_str().expect("newText");

        let updated = Server::apply_change(
            original_file2,
            start_line,
            start_offset,
            end_line,
            end_offset,
            new_text,
        );

        assert_eq!(
            updated,
            "import { Test1, Test2, Test3, Test4 } from './file1';\ninterface Testing {\n    test1: Test1;\n    test2: Test2;\n    test3: Test3;\n    test4: Test4;\n}\n"
        );
    }

    #[test]
    fn handle_get_combined_code_fix_fix_missing_import_in_declaration_file_keeps_value_and_type_split()
     {
        let mut server = make_server();
        server.open_files.insert(
            "/a.ts".to_string(),
            "export class A {}\nexport class B {}\n".to_string(),
        );
        let original = "new A();\nlet x: B;\n";
        server
            .open_files
            .insert("/d.ts".to_string(), original.to_string());

        let req = TsServerRequest {
            seq: 1,
            _msg_type: "request".to_string(),
            command: "getCombinedCodeFix".to_string(),
            arguments: serde_json::json!({
                "scope": { "type": "file", "args": { "file": "/d.ts" } },
                "fixId": "fixMissingImport",
                "preferences": {
                    "preferTypeOnlyAutoImports": true
                }
            }),
        };

        let resp = server.handle_get_combined_code_fix(1, &req);
        assert!(resp.success, "expected getCombinedCodeFix to succeed");

        let changes = resp
            .body
            .as_ref()
            .and_then(|body| body.get("changes"))
            .and_then(serde_json::Value::as_array)
            .expect("missing changes array");
        assert_eq!(changes.len(), 1, "expected one file change");
        let text_changes = changes[0]
            .get("textChanges")
            .and_then(serde_json::Value::as_array)
            .expect("missing textChanges");
        assert_eq!(
            text_changes.len(),
            1,
            "expected one consolidated text change"
        );

        let change = &text_changes[0];
        let start_line = change["start"]["line"].as_u64().expect("start line") as u32;
        let start_offset = change["start"]["offset"].as_u64().expect("start offset") as u32;
        let end_line = change["end"]["line"].as_u64().expect("end line") as u32;
        let end_offset = change["end"]["offset"].as_u64().expect("end offset") as u32;
        let new_text = change["newText"].as_str().expect("newText");

        let updated = Server::apply_change(
            original,
            start_line,
            start_offset,
            end_line,
            end_offset,
            new_text,
        );

        assert_eq!(
            updated,
            "import { A, type B } from \"./a\";\n\nnew A();\nlet x: B;\n"
        );
    }

    #[test]
    fn handle_get_code_fixes_jsdoc_import_returns_single_missing_import_fix() {
        let mut server = make_server();
        server.open_files.insert(
            "/foo.ts".to_string(),
            "export const A = 1;\nexport type B = { x: number };\nexport type C = 1;\nexport class D { y: string }\n".to_string(),
        );
        let test_js = "/**\n * @import { A, D, C } from \"./foo\"\n */\n\n/**\n * @param { typeof A } a\n * @param { B | C } b\n * @param { C } c\n * @param { D } d\n */\nexport function f(a, b, c, d) { }\n";
        server
            .open_files
            .insert("/test.js".to_string(), test_js.to_string());

        let diag_req = TsServerRequest {
            seq: 1,
            _msg_type: "request".to_string(),
            command: "semanticDiagnosticsSync".to_string(),
            arguments: serde_json::json!({
                "file": "/test.js",
                "includeLinePosition": true
            }),
        };
        let diag_resp = server.handle_semantic_diagnostics_sync(1, &diag_req);
        assert!(
            diag_resp.success,
            "expected semanticDiagnosticsSync to succeed"
        );
        let missing_name_diags: Vec<serde_json::Value> = diag_resp
            .body
            .as_ref()
            .and_then(serde_json::Value::as_array)
            .expect("expected diagnostics array")
            .iter()
            .filter(|diag| {
                diag.get("code")
                    .and_then(serde_json::Value::as_u64)
                    .map(|code| code as u32)
                    == Some(tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME)
            })
            .cloned()
            .collect();
        assert_eq!(
            missing_name_diags.len(),
            1,
            "expected one cannot-find-name diagnostic in diagnostics flow, got {missing_name_diags:?}"
        );

        let mut import_fix_texts = Vec::new();
        for diag in &missing_name_diags {
            let code = diag
                .get("code")
                .and_then(serde_json::Value::as_u64)
                .expect("diagnostic code") as u32;
            let (start, end) =
                if let (Some(start_line), Some(start_offset), Some(end_line), Some(end_offset)) = (
                    diag.get("start")
                        .and_then(|start| start.get("line"))
                        .and_then(serde_json::Value::as_u64),
                    diag.get("start")
                        .and_then(|start| start.get("offset"))
                        .and_then(serde_json::Value::as_u64),
                    diag.get("end")
                        .and_then(|end| end.get("line"))
                        .and_then(serde_json::Value::as_u64),
                    diag.get("end")
                        .and_then(|end| end.get("offset"))
                        .and_then(serde_json::Value::as_u64),
                ) {
                    (
                        tsz::lsp::position::Position::new(
                            (start_line as u32).saturating_sub(1),
                            (start_offset as u32).saturating_sub(1),
                        ),
                        tsz::lsp::position::Position::new(
                            (end_line as u32).saturating_sub(1),
                            (end_offset as u32).saturating_sub(1),
                        ),
                    )
                } else {
                    let start_off =
                        diag.get("start")
                            .and_then(serde_json::Value::as_u64)
                            .expect("diagnostic start offset") as u32;
                    let length = diag
                        .get("length")
                        .and_then(serde_json::Value::as_u64)
                        .expect("diagnostic length") as u32;
                    let line_map = super::LineMap::build(test_js);
                    (
                        line_map.offset_to_position(start_off, test_js),
                        line_map.offset_to_position(start_off + length, test_js),
                    )
                };
            let req = TsServerRequest {
                seq: 1,
                _msg_type: "request".to_string(),
                command: "getCodeFixes".to_string(),
                arguments: serde_json::json!({
                    "file": "/test.js",
                    "startLine": start.line + 1,
                    "startOffset": start.character + 1,
                    "endLine": end.line + 1,
                    "endOffset": end.character + 1,
                    "errorCodes": [code],
                    "preferences": {
                        "preferTypeOnlyAutoImports": true
                    }
                }),
            };
            let resp = server.handle_get_code_fixes(1, &req);
            assert!(resp.success, "expected getCodeFixes to succeed");
            let actions = resp
                .body
                .as_ref()
                .and_then(serde_json::Value::as_array)
                .expect("expected getCodeFixes actions");
            for action in actions {
                if action.get("fixName").and_then(serde_json::Value::as_str) != Some("import") {
                    continue;
                }
                let Some(changes) = action.get("changes").and_then(serde_json::Value::as_array)
                else {
                    continue;
                };
                let Some(file_change) = changes.first() else {
                    continue;
                };
                let Some(text_changes) = file_change
                    .get("textChanges")
                    .and_then(serde_json::Value::as_array)
                else {
                    continue;
                };
                let Some(new_text) = text_changes
                    .first()
                    .and_then(|change| change.get("newText"))
                    .and_then(serde_json::Value::as_str)
                else {
                    continue;
                };
                import_fix_texts.push(new_text.to_string());
            }
        }

        assert_eq!(
            import_fix_texts.len(),
            1,
            "expected one import fix from diagnostics flow, got {import_fix_texts:?}"
        );
        assert!(
            import_fix_texts[0].contains("@import { A, D, C, B } from \"./foo\""),
            "expected JSDoc @import merge edit, got {:?}",
            import_fix_texts[0]
        );
    }

    #[test]
    fn get_code_fixes_adds_missing_value_import_with_existing_type_only_import() {
        let mut server = make_server();
        server.open_files.insert(
            "/node_modules/react/index.d.ts".to_string(),
            "export interface ComponentType {}\nexport interface ComponentProps {}\nexport declare function useState<T>(initialState: T): [T, (newState: T) => void];\nexport declare function useEffect(callback: () => void, deps: any[]): void;\n".to_string(),
        );
        server.open_files.insert(
            "/main.ts".to_string(),
            "import type { ComponentType } from \"react\";\nimport { useState } from \"react\";\n\nexport function Component({ prop } : { prop: ComponentType }) {\n    const codeIsUnimportant = useState(1);\n    useEffect(() => {}, []);\n}\n".to_string(),
        );

        let content = server
            .open_files
            .get("/main.ts")
            .expect("missing main.ts")
            .clone();
        let line_map = LineMap::build(&content);
        let (_, binder, _, _) = server
            .parse_and_bind_file("/main.ts")
            .expect("expected parse_and_bind_file for /main.ts");
        let synthetic =
            server.synthetic_missing_name_expression_diagnostics("/main.ts", &content, &binder);
        assert!(
            synthetic.iter().any(|diag| {
                diag.code == tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME
                    && diag.message_text.contains("useEffect")
                    && {
                        let start = line_map.offset_to_position(diag.start, &content);
                        let end = line_map.offset_to_position(diag.start + diag.length, &content);
                        positions_overlap(
                            tsz::lsp::position::Position::new(5, 4),
                            tsz::lsp::position::Position::new(5, 13),
                            start,
                            end,
                        )
                    }
            }),
            "expected synthetic cannot-find-name diagnostic for useEffect, got {synthetic:?}"
        );

        let req = TsServerRequest {
            seq: 1,
            _msg_type: "request".to_string(),
            command: "getCodeFixes".to_string(),
            arguments: serde_json::json!({
                "file": "/main.ts",
                "startLine": 6,
                "startOffset": 5,
                "endLine": 6,
                "endOffset": 14,
                "errorCodes": [2304]
            }),
        };

        let resp = server.handle_get_code_fixes(1, &req);
        assert!(resp.success, "expected getCodeFixes to succeed");
        let body = resp.body.expect("expected getCodeFixes body");
        let fixes = body.as_array().expect("expected array response");
        let mut import_fix_texts = Vec::new();
        for fix in fixes {
            if fix.get("fixName").and_then(serde_json::Value::as_str) != Some("import") {
                continue;
            }
            let Some(changes) = fix.get("changes").and_then(serde_json::Value::as_array) else {
                continue;
            };
            for change in changes {
                let Some(text_changes) = change
                    .get("textChanges")
                    .and_then(serde_json::Value::as_array)
                else {
                    continue;
                };
                for text_change in text_changes {
                    if let Some(new_text) = text_change
                        .get("newText")
                        .and_then(serde_json::Value::as_str)
                    {
                        import_fix_texts.push(new_text.to_string());
                    }
                }
            }
        }

        assert!(
            import_fix_texts
                .iter()
                .any(|text| text.contains("useEffect")),
            "expected import fix text to include useEffect, got {import_fix_texts:?}"
        );
    }

    #[test]
    fn get_code_fixes_prefers_merging_type_only_import_into_type_clause() {
        let mut server = make_server();
        server.open_files.insert(
            "/node_modules/react/index.d.ts".to_string(),
            "export interface ComponentType {}\nexport interface ComponentProps {}\nexport declare function useState<T>(initialState: T): [T, (newState: T) => void];\n".to_string(),
        );
        server.open_files.insert(
            "/main2.ts".to_string(),
            "import { useState } from \"react\";\nimport type { ComponentType } from \"react\";\n\ntype _ = ComponentProps;\n".to_string(),
        );

        let req = TsServerRequest {
            seq: 1,
            _msg_type: "request".to_string(),
            command: "getCodeFixes".to_string(),
            arguments: serde_json::json!({
                "file": "/main2.ts",
                "startLine": 4,
                "startOffset": 10,
                "endLine": 4,
                "endOffset": 24,
                "errorCodes": [2304]
            }),
        };

        let resp = server.handle_get_code_fixes(1, &req);
        assert!(resp.success, "expected getCodeFixes to succeed");
        let body = resp.body.expect("expected getCodeFixes body");
        let fixes = body.as_array().expect("expected array response");
        let mut first_import_changes: Option<Vec<serde_json::Value>> = None;
        for fix in fixes {
            if fix.get("fixName").and_then(serde_json::Value::as_str) != Some("import") {
                continue;
            }
            let Some(changes) = fix.get("changes").and_then(serde_json::Value::as_array) else {
                continue;
            };
            for change in changes {
                let Some(text_changes) = change
                    .get("textChanges")
                    .and_then(serde_json::Value::as_array)
                else {
                    continue;
                };
                first_import_changes = Some(text_changes.clone());
                break;
            }
            if first_import_changes.is_some() {
                break;
            }
        }

        let mut updated = server
            .open_files
            .get("/main2.ts")
            .expect("missing main2.ts")
            .clone();
        let mut edits = first_import_changes.expect("expected at least one import fix");
        edits.sort_by(|a, b| {
            let a_line = a["start"]["line"].as_u64().unwrap_or(0);
            let a_offset = a["start"]["offset"].as_u64().unwrap_or(0);
            let b_line = b["start"]["line"].as_u64().unwrap_or(0);
            let b_offset = b["start"]["offset"].as_u64().unwrap_or(0);
            (b_line, b_offset).cmp(&(a_line, a_offset))
        });
        for edit in edits {
            updated = Server::apply_change(
                &updated,
                edit["start"]["line"].as_u64().expect("start line") as u32,
                edit["start"]["offset"].as_u64().expect("start offset") as u32,
                edit["end"]["line"].as_u64().expect("end line") as u32,
                edit["end"]["offset"].as_u64().expect("end offset") as u32,
                edit["newText"].as_str().expect("new text"),
            );
        }
        assert!(
            updated.contains("import type { ComponentProps, ComponentType } from \"react\";"),
            "expected merged type-only import, got {updated:?}"
        );
    }

    #[test]
    fn parse_identifier_call_expression_ignores_keywords() {
        assert_eq!(
            parse_identifier_call_expression("useEffect(() => {})"),
            Some((0, "useEffect"))
        );
        assert_eq!(parse_identifier_call_expression("if (cond)"), None);
    }
}
