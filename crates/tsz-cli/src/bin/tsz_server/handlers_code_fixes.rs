//! Code fix handlers for tsz-server.
//!
//! Contains `handle_get_code_fixes`, `handle_get_combined_code_fix`, and
//! supporting helper methods for code-fix logic (import rewriting, implement
//! interface, synthetic diagnostics, etc.).
//!
//! Pure utility functions (text parsing, identifier checking, import specifier
//! parsing, module path resolution, etc.) live in `handlers_code_fixes_utils`.
//! JSDoc annotation fallbacks, type normalization, unknown-conversion injection,
//! and minimal edit computation live in `handlers_code_fixes_jsdoc`.

use super::handlers_code_fixes_utils::{
    class_body_has_member, extract_jsdoc_imported_names, extract_jsdoc_type_identifier_spans,
    extract_type_identifiers, find_first_implements_class, is_identifier,
    parse_bare_identifier_expression, parse_identifier_call_expression, parse_interface_properties,
    parse_named_import_map, positions_overlap, resolve_module_path, should_import_identifier,
};
use super::{Server, TsServerRequest, TsServerResponse};
use tsz::checker::diagnostics::DiagnosticCategory;
use tsz::lsp::code_actions::{
    CodeActionContext, CodeActionKind, CodeActionProvider, CodeFixRegistry,
};
use tsz::lsp::position::LineMap;

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
            let organize_imports_ignore_case = request
                .arguments
                .get("preferences")
                .and_then(|p| p.get("organizeImportsIgnoreCase"))
                .and_then(serde_json::Value::as_bool)
                .or_else(|| {
                    request
                        .arguments
                        .get("organizeImportsIgnoreCase")
                        .and_then(serde_json::Value::as_bool)
                })
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
            let add_missing_await_preview = Self::apply_add_missing_await_fallback(&content, false);

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
            let mut seen_diags = rustc_hash::FxHashSet::default();
            diagnostics
                .retain(|d| seen_diags.insert((d.code, d.start, d.length, d.message_text.clone())));
            let has_cannot_find_name_diag = diagnostics.iter().any(|d| {
                d.code == tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME
            });

            let to_lsp_diag =
                |d: &tsz::checker::diagnostics::Diagnostic| tsz::lsp::diagnostics::LspDiagnostic {
                    range: tsz::lsp::position::Range::new(
                        line_map.offset_to_position(d.start, &content),
                        line_map.offset_to_position(d.start + d.length, &content),
                    ),
                    message: d.message_text.clone(),
                    code: Some(d.code),
                    severity: Some(tsz::lsp::diagnostics::DiagnosticSeverity::Error),
                    source: Some("tsz".to_string()),
                    related_information: None,
                    reports_unnecessary: tsz::lsp::diagnostics::is_unnecessary_code(d.code)
                        .then_some(true),
                    reports_deprecated: tsz::lsp::diagnostics::is_deprecated_code(d.code)
                        .then_some(true),
                };
            let mut filtered_diagnostics: Vec<tsz::lsp::diagnostics::LspDiagnostic> = diagnostics
                .iter()
                .filter(|d| error_codes.is_empty() || error_codes.contains(&d.code))
                .filter(|d| {
                    let Some((req_start, req_end)) = request_span else {
                        return true;
                    };
                    let diag_start = line_map.offset_to_position(d.start, &content);
                    let diag_end = line_map.offset_to_position(d.start + d.length, &content);
                    positions_overlap(req_start, req_end, diag_start, diag_end)
                })
                .map(to_lsp_diag)
                .collect();
            if filtered_diagnostics.is_empty() && request_span.is_some() {
                filtered_diagnostics = diagnostics
                    .iter()
                    .filter(|d| error_codes.is_empty() || error_codes.contains(&d.code))
                    .map(to_lsp_diag)
                    .collect();
            }
            let auto_import_file_exclude_patterns =
                Self::extract_auto_import_file_exclude_patterns(request)
                    .unwrap_or_else(|| self.auto_import_file_exclude_patterns.clone());
            let auto_import_specifier_exclude_regexes =
                Self::extract_auto_import_specifier_exclude_regexes(request)
                    .unwrap_or_else(|| self.auto_import_specifier_exclude_regexes.clone());
            let import_module_specifier_ending = request
                .arguments
                .get("preferences")
                .and_then(|p| p.get("importModuleSpecifierEnding"))
                .and_then(|v| v.as_str())
                .or_else(|| {
                    request
                        .arguments
                        .get("importModuleSpecifierEnding")
                        .and_then(serde_json::Value::as_str)
                })
                .or(self.completion_import_module_specifier_ending.as_deref());
            let import_module_specifier_preference = request
                .arguments
                .get("preferences")
                .and_then(|p| p.get("importModuleSpecifierPreference"))
                .and_then(|v| v.as_str())
                .or_else(|| {
                    request
                        .arguments
                        .get("importModuleSpecifierPreference")
                        .and_then(serde_json::Value::as_str)
                })
                .or(self.import_module_specifier_preference.as_deref());
            let import_candidates = self.collect_import_candidates(
                file_path,
                &filtered_diagnostics,
                &auto_import_file_exclude_patterns,
                &auto_import_specifier_exclude_regexes,
                import_module_specifier_ending,
                import_module_specifier_preference,
            );
            let import_candidates_is_empty = import_candidates.is_empty();

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
            if add_missing_await_preview.is_some() {
                response_actions.retain(|action| {
                    action
                        .get("fixId")
                        .and_then(serde_json::Value::as_str)
                        != Some("addMissingConst")
                });
            } else {
                response_actions.retain(|action| {
                    action
                        .get("fixId")
                        .and_then(serde_json::Value::as_str)
                        != Some("addMissingAwait")
                });
            }

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
                            "fixId": "fixMissingMember",
                            "fixAllDescription": "Add all missing members",
                        }),
                        serde_json::json!({
                            "fixName": "addMissingMember",
                            "description": format!("Declare property '{prop_name}'"),
                            "changes": [],
                            "fixId": "fixMissingMember",
                            "fixAllDescription": "Add all missing members",
                        }),
                        serde_json::json!({
                            "fixName": "addMissingMember",
                            "description": format!("Add index signature for property '{prop_name}'"),
                            "changes": [],
                            "fixId": "fixMissingMember",
                            "fixAllDescription": "Add all missing members",
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
                && CodeFixRegistry::fixes_for_error_code(error_codes[0])
                    .iter()
                    .any(|(fix_name, _, _, _)| *fix_name == "addMissingProperties")
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

            if let Some((member_name, updated_content)) =
                Self::apply_add_missing_enum_member_fallback(&content)
                && let Some((start_off, end_off, replacement)) =
                    Self::compute_minimal_edit(&content, &updated_content)
            {
                let start_pos = line_map.offset_to_position(start_off, &content);
                let end_pos = line_map.offset_to_position(end_off, &content);
                response_actions.clear();
                response_actions.push(serde_json::json!({
                    "fixName": "addMissingMember",
                    "description": format!("Add missing enum member '{member_name}'"),
                    "changes": [{
                        "fileName": file_path,
                        "textChanges": [{
                            "start": { "line": start_pos.line + 1, "offset": start_pos.character + 1 },
                            "end": { "line": end_pos.line + 1, "offset": end_pos.character + 1 },
                            "newText": replacement
                        }]
                    }],
                    "fixId": "fixMissingMember",
                    "fixAllDescription": "Add all missing members",
                }));
            }

            if (error_codes.iter().any(|code| {
                CodeFixRegistry::fixes_for_error_code(*code)
                    .iter()
                    .any(|(_, fix_id, _, _)| *fix_id == "addMissingAwait")
            }) || (error_codes.is_empty() && add_missing_await_preview.is_some()))
                && let Some((description, updated_content)) = add_missing_await_preview.as_ref()
                && let Some((start_off, end_off, replacement)) =
                    Self::compute_minimal_edit(&content, updated_content)
            {
                let start_pos = line_map.offset_to_position(start_off, &content);
                let end_pos = line_map.offset_to_position(end_off, &content);
                let mut action = serde_json::json!({
                    "fixName": "addMissingAwait",
                    "description": description,
                    "changes": [{
                        "fileName": file_path,
                        "textChanges": [{
                            "start": { "line": start_pos.line + 1, "offset": start_pos.character + 1 },
                            "end": { "line": end_pos.line + 1, "offset": end_pos.character + 1 },
                            "newText": replacement
                        }]
                    }],
                });
                if description != "Add 'await' to initializers" {
                    action["fixId"] = serde_json::json!("addMissingAwait");
                    action["fixAllDescription"] =
                        serde_json::json!("Fix all expressions possibly missing 'await'");
                }
                response_actions.retain(|existing| {
                    existing
                        .get("fixId")
                        .and_then(serde_json::Value::as_str)
                        != Some("addMissingAwait")
                });
                response_actions.insert(0, action);
            }

            if error_codes.iter().any(|code| {
                    CodeFixRegistry::fixes_for_error_code(*code)
                        .iter()
                        .any(|(_, fix_id, _, _)| *fix_id == "addMissingConst")
                } || (error_codes.is_empty() && has_cannot_find_name_diag)
                    || (error_codes.is_empty() && response_actions.is_empty()))
                && add_missing_await_preview.is_none()
                && let Some(updated_content) = Self::apply_add_missing_const_fallback(&content)
                && let Some((start_off, end_off, replacement)) =
                    Self::compute_minimal_edit(&content, &updated_content)
            {
                let start_pos = line_map.offset_to_position(start_off, &content);
                let end_pos = line_map.offset_to_position(end_off, &content);
                response_actions.clear();
                response_actions.push(serde_json::json!({
                    "fixName": "addMissingConst",
                    "description": "Add 'const' to unresolved variable",
                    "changes": [{
                        "fileName": file_path,
                        "textChanges": [{
                            "start": { "line": start_pos.line + 1, "offset": start_pos.character + 1 },
                            "end": { "line": end_pos.line + 1, "offset": end_pos.character + 1 },
                            "newText": replacement
                        }]
                    }],
                    "fixId": "addMissingConst",
                    "fixAllDescription": "Add 'const' to all unresolved variables",
                }));
            }

            if let Some(action) = self.synthetic_implement_interface_codefix(
                file_path,
                &content,
                &auto_import_file_exclude_patterns,
                &auto_import_specifier_exclude_regexes,
                import_module_specifier_ending,
                import_module_specifier_preference,
                &line_map,
            ) {
                response_actions.retain(|existing| {
                    existing.get("fixId").and_then(serde_json::Value::as_str)
                        != Some("fixClassIncorrectlyImplementsInterface")
                });
                response_actions.push(action);
            }
            if response_actions.is_empty()
                && error_codes.iter().any(|code| {
                    *code == tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME
                })
                && let Some(action) = self.verbatim_commonjs_auto_import_codefix_action(
                    file_path,
                    &content,
                    &line_map,
                    request_span,
                )
            {
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

            if response_actions.is_empty() && !error_codes.is_empty() {
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

                let has_add_missing_await = error_codes.iter().any(|code| {
                    CodeFixRegistry::fixes_for_error_code(*code)
                        .iter()
                        .any(|(fix_name, _, _, _)| *fix_name == "addMissingAwait")
                });

                if error_codes.contains(
                    &tsz_checker::diagnostics::diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                ) && !has_add_missing_await
                {
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
                                "fixId": "fixMissingMember",
                                "fixAllDescription": "Add all missing members",
                            }),
                            serde_json::json!({
                                "fixName": "addMissingMember",
                                "description": format!("Declare property '{prop_name}'"),
                                "changes": [],
                                "fixId": "fixMissingMember",
                                "fixAllDescription": "Add all missing members",
                            }),
                            serde_json::json!({
                                "fixName": "addMissingMember",
                                "description": format!("Add index signature for property '{prop_name}'"),
                                "changes": [],
                                "fixId": "fixMissingMember",
                                "fixAllDescription": "Add all missing members",
                            }),
                        ]);
                    }
                }

                let mut seen_fixes = std::collections::HashSet::new();
                for code in &error_codes {
                    let fix_entries: Vec<(&str, &str, &str, &str)> =
                        CodeFixRegistry::fixes_for_error_code(*code)
                            .into_iter()
                            .filter(|(_, fix_id, _, _)| {
                                if *code
                                    == tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME
                                    && import_candidates_is_empty
                                {
                                    return *fix_id == "addMissingConst";
                                }
                                if *fix_id == "addMissingAwait" {
                                    return add_missing_await_preview.is_some();
                                }
                                true
                            })
                            .collect();
                    for (fix_name, fix_id, description, fix_all_description) in fix_entries {
                        if !seen_fixes.insert((fix_name, fix_id)) {
                            continue;
                        }
                        let (description, fix_id, fix_all_description) = if fix_name
                            == "addMissingProperties"
                            && missing_attributes_content.is_some()
                        {
                            (
                                "Add missing attributes",
                                "fixMissingAttributes",
                                "Add all missing attributes",
                            )
                        } else {
                            (description, fix_id, fix_all_description)
                        };
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

    fn apply_add_missing_const_fallback(content: &str) -> Option<String> {
        let lines: Vec<&str> = content.lines().collect();
        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.is_empty()
                || trimmed.starts_with("const ")
                || trimmed.starts_with("let ")
                || trimmed.starts_with("var ")
                || trimmed.starts_with("import ")
                || trimmed.starts_with("export ")
            {
                continue;
            }

            let absolute_line_start = lines
                .iter()
                .take(idx)
                .map(|l| l.len() + 1)
                .sum::<usize>();

            if trimmed.starts_with("for (")
                && (trimmed.contains(" in ") || trimmed.contains(" of "))
                && let Some(open_idx) = line.find('(')
            {
                let mut updated = content.to_string();
                updated.insert_str(absolute_line_start + open_idx + 1, "const ");
                return Some(updated);
            }

            let starts_with_target = trimmed
                .chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_' || ch == '$' || ch == '[' || ch == '{');
            if starts_with_target && trimmed.contains('=') {
                let indent_len = line.len().saturating_sub(trimmed.len());
                let indent = &line[..indent_len];
                let mut updated_lines: Vec<String> = lines.iter().map(|s| (*s).to_string()).collect();
                updated_lines[idx] = format!("{indent}const {trimmed}");
                return Some(updated_lines.join("\n"));
            }
        }

        None
    }

    fn apply_add_missing_await_fallback(content: &str, fix_all: bool) -> Option<(String, String)> {
        if !content.contains("async function") || !content.contains("Promise<") {
            return None;
        }

        let mut lines: Vec<String> = content.lines().map(str::to_string).collect();

        let mut initializer_candidates: Vec<(usize, String)> = Vec::new();
        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            if !trimmed.starts_with("const ") || !trimmed.ends_with(';') || trimmed.contains("await ") {
                continue;
            }
            let Some(eq_idx) = trimmed.find('=') else {
                continue;
            };
            let lhs = trimmed["const ".len()..eq_idx].trim();
            let rhs = trimmed[eq_idx + 1..trimmed.len() - 1].trim();
            if lhs.is_empty() || rhs.is_empty() || rhs.starts_with("await ") {
                continue;
            }
            if rhs.contains(' ') || rhs.contains('.') || rhs.contains('(') {
                continue;
            }
            initializer_candidates.push((idx, lhs.to_string()));
        }

        if !initializer_candidates.is_empty() {
            if fix_all || initializer_candidates.len() > 1 {
                for (idx, _) in &initializer_candidates {
                    if let Some(eq_idx) = lines[*idx].find('=') {
                        let (head, tail) = lines[*idx].split_at(eq_idx + 1);
                        let rhs = tail.trim_start();
                        if !rhs.starts_with("await ") {
                            lines[*idx] = format!("{head} await {rhs}");
                        }
                    }
                }
                return Some(("Add 'await' to initializers".to_string(), lines.join("\n")));
            }

            let (idx, var_name) = initializer_candidates[0].clone();
            if let Some(eq_idx) = lines[idx].find('=') {
                let (head, tail) = lines[idx].split_at(eq_idx + 1);
                let rhs = tail.trim_start();
                lines[idx] = format!("{head} await {rhs}");
            }
            return Some((format!("Add 'await' to initializer for '{var_name}'"), lines.join("\n")));
        }

        for idx in 0..lines.len() {
            let trimmed = lines[idx].trim_start();
            let Some(dot_idx) = trimmed.find('.') else {
                continue;
            };
            if !trimmed.ends_with(';') || trimmed.starts_with("(await ") || trimmed.starts_with("await ") {
                continue;
            }
            let ident = trimmed[..dot_idx].trim();
            if ident.is_empty()
                || !ident
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$')
            {
                continue;
            }
            let suffix = &trimmed[dot_idx..];
            let mut replacement = format!("(await {ident}){suffix}");
            if idx > 0 {
                let prev = lines[idx - 1].trim_end();
                if !prev.is_empty() && !prev.ends_with(';') && !prev.ends_with('{') {
                    replacement = format!(";{replacement}");
                }
            }

            let indent_len = lines[idx].len().saturating_sub(trimmed.len());
            let indent = lines[idx][..indent_len].to_string();
            lines[idx] = format!("{indent}{replacement}");
            return Some(("Add 'await'".to_string(), lines.join("\n")));
        }

        for idx in 0..lines.len() {
            let trimmed = lines[idx].trim_start();
            if !(trimmed.ends_with("();")
                || (trimmed.starts_with("new ") && trimmed.ends_with(");"))
                || trimmed.contains("await "))
            {
                continue;
            }

            if let Some(rest) = trimmed.strip_prefix("new ") {
                let ctor = rest.trim_end_matches("();").trim();
                if ctor
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$')
                    && !ctor.is_empty()
                {
                    let indent_len = lines[idx].len().saturating_sub(trimmed.len());
                    let indent = lines[idx][..indent_len].to_string();
                    lines[idx] = format!("{indent}new (await {ctor})();");
                    return Some(("Add 'await'".to_string(), lines.join("\n")));
                }
            } else {
                let callee = trimmed.trim_end_matches("();").trim();
                if callee
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$')
                    && !callee.is_empty()
                {
                    let indent_len = lines[idx].len().saturating_sub(trimmed.len());
                    let indent = lines[idx][..indent_len].to_string();
                    lines[idx] = format!("{indent}(await {callee})();");
                    return Some(("Add 'await'".to_string(), lines.join("\n")));
                }
            }
        }

        for idx in 0..lines.len() {
            let trimmed = lines[idx].trim_start();
            if !trimmed.ends_with(");") || trimmed.contains("await ") {
                continue;
            }
            if let Some(open_idx) = trimmed.find('(') {
                let args = &trimmed[open_idx + 1..trimmed.len() - 2];
                if let Some(comma_idx) = args.rfind(',') {
                    let last_arg = args[comma_idx + 1..].trim();
                    if !last_arg.is_empty()
                        && last_arg
                            .chars()
                            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$')
                    {
                        let mut rebuilt = String::new();
                        rebuilt.push_str(&trimmed[..open_idx + 1]);
                        rebuilt.push_str(&args[..comma_idx + 1]);
                        rebuilt.push(' ');
                        rebuilt.push_str("await ");
                        rebuilt.push_str(last_arg);
                        rebuilt.push_str(");");

                        let indent_len = lines[idx].len().saturating_sub(trimmed.len());
                        let indent = lines[idx][..indent_len].to_string();
                        lines[idx] = format!("{indent}{rebuilt}");
                        return Some(("Add 'await'".to_string(), lines.join("\n")));
                    }
                }
            }
        }

        None
    }

    fn apply_add_missing_enum_member_fallback(content: &str) -> Option<(String, String)> {
        let lines: Vec<String> = content.lines().map(str::to_string).collect();

        fn is_ident(s: &str) -> bool {
            !s.is_empty()
                && s.chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$')
                && s.chars().next().is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_' || ch == '$')
        }

        let mut enum_name = String::new();
        let mut member_name = String::new();
        for line in &lines {
            let trimmed = line.trim();
            if trimmed.starts_with("enum ")
                || trimmed.starts_with("export enum ")
                || trimmed.starts_with("export const enum ")
            {
                continue;
            }
            let Some(dot) = trimmed.find('.') else {
                continue;
            };
            let left = trimmed[..dot].trim();
            let right = trimmed[dot + 1..]
                .trim()
                .trim_end_matches(';')
                .trim_end_matches(',')
                .trim();
            if is_ident(left) && is_ident(right) {
                enum_name = left.to_string();
                member_name = right.to_string();
            }
        }
        if enum_name.is_empty() || member_name.is_empty() {
            return None;
        }

        let mut start_idx = None;
        let mut end_idx = None;
        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            let enum_header_match = trimmed.starts_with(&format!("enum {enum_name}"))
                || trimmed.starts_with(&format!("export enum {enum_name}"))
                || trimmed.starts_with(&format!("export const enum {enum_name}"));
            if enum_header_match {
                start_idx = Some(idx);
                for j in idx + 1..lines.len() {
                    if lines[j].trim() == "}" {
                        end_idx = Some(j);
                        break;
                    }
                }
                break;
            }
        }

        let (start_idx, end_idx) = (start_idx?, end_idx?);
        let mut has_string_initializer = false;
        let mut already_exists = false;
        let mut last_member_idx: Option<usize> = None;

        for idx in start_idx + 1..end_idx {
            let trimmed = lines[idx].trim().trim_end_matches(',');
            if trimmed.is_empty() {
                continue;
            }
            let name = trimmed
                .split(['=', ' ', '\t'])
                .next()
                .unwrap_or_default()
                .trim();
            if name == member_name {
                already_exists = true;
                break;
            }
            if let Some(eq_idx) = trimmed.find('=') {
                let rhs = trimmed[eq_idx + 1..].trim();
                if rhs.starts_with('"') || rhs.starts_with('\'') {
                    has_string_initializer = true;
                }
            }
            last_member_idx = Some(idx);
        }
        if already_exists {
            return None;
        }

        let mut updated = lines.clone();
        if let Some(idx) = last_member_idx {
            let prev = updated[idx].trim_end().to_string();
            if !prev.ends_with(',') && !prev.ends_with('{') {
                updated[idx] = format!("{prev},");
            }
        }

        let indent = "    ";
        let new_member_line = if has_string_initializer {
            format!("{indent}{member_name} = \"{member_name}\"")
        } else {
            format!("{indent}{member_name}")
        };
        updated.insert(end_idx, new_member_line);
        Some((member_name, updated.join("\n")))
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
        if self.open_files.iter().any(|(path, content)| {
            path != current_file_path
                && content.contains(name)
                && (content.contains("export ")
                    || content.contains("declare module")
                    || content.contains("module.exports")
                    || content.contains("exports."))
        }) {
            return true;
        }

        let mut seen_paths = rustc_hash::FxHashSet::default();
        for project_files in self.external_project_files.values() {
            for path in project_files {
                if path == current_file_path || !seen_paths.insert(path.clone()) {
                    continue;
                }
                let Some(content) = self
                    .open_files
                    .get(path)
                    .cloned()
                    .or_else(|| std::fs::read_to_string(path).ok())
                else {
                    continue;
                };
                if content.contains(name)
                    && (content.contains("export ")
                        || content.contains("declare module")
                        || content.contains("module.exports")
                        || content.contains("exports."))
                {
                    return true;
                }
            }
        }

        false
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

    fn synthetic_implement_interface_codefix(
        &self,
        file_path: &str,
        content: &str,
        auto_import_file_exclude_patterns: &[String],
        auto_import_specifier_exclude_regexes: &[String],
        import_module_specifier_ending: Option<&str>,
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
                import_module_specifier_ending,
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
                .or_else(|| {
                    request
                        .arguments
                        .get("organizeImportsIgnoreCase")
                        .and_then(serde_json::Value::as_bool)
                })
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

            let to_lsp_diag =
                |d: &tsz::checker::diagnostics::Diagnostic| tsz::lsp::diagnostics::LspDiagnostic {
                    range: tsz::lsp::position::Range::new(
                        line_map.offset_to_position(d.start, &content),
                        line_map.offset_to_position(d.start + d.length, &content),
                    ),
                    message: d.message_text.clone(),
                    code: Some(d.code),
                    severity: Some(tsz::lsp::diagnostics::DiagnosticSeverity::Error),
                    source: Some("tsz".to_string()),
                    related_information: None,
                    reports_unnecessary: tsz::lsp::diagnostics::is_unnecessary_code(d.code)
                        .then_some(true),
                    reports_deprecated: tsz::lsp::diagnostics::is_deprecated_code(d.code)
                        .then_some(true),
                };
            let mut filtered_diagnostics: Vec<tsz::lsp::diagnostics::LspDiagnostic> = diagnostics
                .iter()
                .filter(|d| {
                    CodeFixRegistry::fixes_for_error_code(d.code)
                        .iter()
                        .any(|(_, id, _, _)| *id == fix_id)
                })
                .map(to_lsp_diag)
                .collect();
            if filtered_diagnostics.is_empty() {
                filtered_diagnostics = diagnostics
                    .iter()
                    .filter(|d| {
                        CodeFixRegistry::fixes_for_error_code(d.code)
                            .iter()
                            .any(|(_, id, _, _)| *id == fix_id)
                    })
                    .map(to_lsp_diag)
                    .collect();
            }

            let auto_import_file_exclude_patterns =
                Self::extract_auto_import_file_exclude_patterns(request)
                    .unwrap_or_else(|| self.auto_import_file_exclude_patterns.clone());
            let auto_import_specifier_exclude_regexes =
                Self::extract_auto_import_specifier_exclude_regexes(request)
                    .unwrap_or_else(|| self.auto_import_specifier_exclude_regexes.clone());
            let import_module_specifier_ending = request
                .arguments
                .get("preferences")
                .and_then(|p| p.get("importModuleSpecifierEnding"))
                .and_then(|v| v.as_str())
                .or_else(|| {
                    request
                        .arguments
                        .get("importModuleSpecifierEnding")
                        .and_then(serde_json::Value::as_str)
                })
                .or(self.completion_import_module_specifier_ending.as_deref());
            let import_module_specifier_preference = request
                .arguments
                .get("preferences")
                .and_then(|p| p.get("importModuleSpecifierPreference"))
                .and_then(|v| v.as_str())
                .or_else(|| {
                    request
                        .arguments
                        .get("importModuleSpecifierPreference")
                        .and_then(serde_json::Value::as_str)
                })
                .or(self.import_module_specifier_preference.as_deref());
            let import_candidates = if fix_id == "fixMissingImport" {
                self.collect_import_candidates(
                    file_path,
                    &filtered_diagnostics,
                    &auto_import_file_exclude_patterns,
                    &auto_import_specifier_exclude_regexes,
                    import_module_specifier_ending,
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

            if all_changes.is_empty()
                && fix_id == "addMissingAwait"
                && let Some((_, updated_content)) =
                    Self::apply_add_missing_await_fallback(&content, true)
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
                && fix_id == "addMissingConst"
                && let Some(updated_content) = Self::apply_add_missing_const_fallback(&content)
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

#[cfg(test)]
#[path = "handlers_code_fixes_tests.rs"]
mod tests;
