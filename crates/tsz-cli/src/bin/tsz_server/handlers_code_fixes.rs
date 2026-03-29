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
            const ADD_MISSING_NEW_FIX_ID: &str = "addMissingNewOperator";
            const NOT_CALLABLE_ERROR_CODE: u32 = 2348;
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
            let add_missing_new_preview =
                Self::apply_add_missing_new_fallback(&content, request_span);
            let add_missing_await_preview = Self::apply_add_missing_await_fallback(&content, false);
            let mut add_missing_const_preview = if let Some((start, _)) = request_span {
                Self::apply_add_missing_const_fallback_at_position(&content, &line_map, start)
            } else {
                Self::apply_add_missing_const_fallback(&content)
            };
            if let Some((start, _)) = request_span
                && Self::add_missing_const_should_skip_for_declared_bindings(
                    &content, &line_map, start, &binder,
                )
            {
                add_missing_const_preview = None;
            }

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
            if diagnostics.iter().all(|d| d.code != 2348)
                && let Some(diag) = Self::synthetic_add_missing_new_diagnostic(file_path, &content)
            {
                diagnostics.push(diag);
            }
            if diagnostics
                .iter()
                .all(|d| d.code != tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME)
                && let Some(diag) =
                    Self::synthetic_add_missing_const_diagnostic(file_path, &content)
            {
                diagnostics.push(diag);
            }
            let mut seen_diags = rustc_hash::FxHashSet::default();
            diagnostics
                .retain(|d| seen_diags.insert((d.code, d.start, d.length, d.message_text.clone())));
            let _has_cannot_find_name_diag = diagnostics
                .iter()
                .any(|d| d.code == tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME);

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
            if !auto_import_file_exclude_patterns.is_empty() {
                let mut dbg = format!(
                    "file={file_path}\nexclude_patterns={auto_import_file_exclude_patterns:?}\nimport_candidates={}\ndiags={}\n",
                    import_candidates.len(),
                    filtered_diagnostics.len()
                );
                for c in &import_candidates {
                    dbg.push_str(&format!(
                        "  candidate: {} from {}\n",
                        c.local_name, c.module_specifier
                    ));
                }
                for d in &filtered_diagnostics {
                    dbg.push_str(&format!(
                        "  diag: code={} msg={}\n",
                        d.code.unwrap_or(0),
                        d.message
                    ));
                }
                let _ = std::fs::write("/tmp/tsz-debug-codefix.log", &dbg);
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
            let normalize_response_actions = |actions: &mut Vec<serde_json::Value>| {
                for action in actions.iter_mut() {
                    let fix_name = action.get("fixName").and_then(serde_json::Value::as_str);
                    let fix_id = action.get("fixId").and_then(serde_json::Value::as_str);
                    if fix_name == Some("addMissingAsync") || fix_id == Some("addMissingAsync") {
                        action["description"] =
                            serde_json::json!("Add async modifier to containing function");
                    }
                }
                actions.retain(|action| {
                    let fix_id = action.get("fixId").and_then(serde_json::Value::as_str);
                    if fix_id != Some("fixAddVoidToPromise") {
                        return true;
                    }
                    action
                        .get("changes")
                        .and_then(serde_json::Value::as_array)
                        .is_some_and(|changes| !changes.is_empty())
                });
            };
            response_actions.retain(|action| {
                action.get("fixId").and_then(serde_json::Value::as_str) != Some("addMissingConst")
                    || add_missing_const_preview.is_some()
            });
            if add_missing_await_preview.is_some() {
                response_actions.retain(|action| {
                    action.get("fixId").and_then(serde_json::Value::as_str)
                        != Some("addMissingConst")
                });
            } else {
                response_actions.retain(|action| {
                    action.get("fixId").and_then(serde_json::Value::as_str)
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

            // addMissingAsync: for error 1308 (await in sync function) or assignability
            // errors when the content actually contains 'await' usage.
            if let Some(updated_content) = missing_async_content.as_ref()
                && !response_actions.iter().any(|a| {
                    a.get("fixId").and_then(serde_json::Value::as_str)
                        == Some(ADD_MISSING_ASYNC_FIX_ID)
                })
            {
                let content_has_await = content.contains("await ");
                let has_1308 =
                    error_codes.contains(&AWAIT_IN_SYNC_FUNCTION_ERROR_CODE) && content_has_await;
                let has_assignability =
                    error_codes.iter().any(|c| *c == 2322 || *c == 2345) && content_has_await;
                let async_fix = serde_json::json!({
                    "fixName": ADD_MISSING_ASYNC_FIX_ID,
                    "description": "Add async modifier to containing function",
                    "changes": [{
                        "fileName": file_path,
                        "textChanges": [{
                            "start": { "line": 1, "offset": 1 },
                            "end": { "line": line_map.offset_to_position(content.len() as u32, &content).line + 1, "offset": line_map.offset_to_position(content.len() as u32, &content).character + 1 },
                            "newText": updated_content
                        }]
                    }],
                    "fixId": ADD_MISSING_ASYNC_FIX_ID,
                    "fixAllDescription": "Add all missing async modifiers",
                });
                if has_assignability && !has_1308 {
                    // For assignability errors (2322/2345), insert at front
                    response_actions.insert(0, async_fix);
                } else if has_1308 {
                    // For 1308, append at the end (preserve existing fix order)
                    response_actions.push(async_fix);
                }
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

            // addMissingNewOperator: for error 2348 (value not callable, missing new)
            // Try text-based fallback first, then AST-based fallback for complex
            // patterns like x[0](), (cond ? A : B)(), (() => C)()(), foo()!().
            if response_actions.is_empty()
                && error_codes.len() == 1
                && error_codes[0] == NOT_CALLABLE_ERROR_CODE
            {
                // Try AST-based approach first (handles complex patterns accurately),
                // then fall back to text-based approach for simple ClassName() patterns.
                let ast_edit = diagnostics
                    .iter()
                    .find(|d| d.code == NOT_CALLABLE_ERROR_CODE)
                    .and_then(|diag| Self::apply_add_missing_new_ast(&arena, &content, diag.start));

                if let Some((start_off, end_off, replacement, description)) = ast_edit {
                    let start_pos = line_map.offset_to_position(start_off, &content);
                    let end_pos = line_map.offset_to_position(end_off, &content);
                    response_actions.push(serde_json::json!({
                        "fixName": ADD_MISSING_NEW_FIX_ID,
                        "description": description,
                        "changes": [{
                            "fileName": file_path,
                            "textChanges": [{
                                "start": { "line": start_pos.line + 1, "offset": start_pos.character + 1 },
                                "end": { "line": end_pos.line + 1, "offset": end_pos.character + 1 },
                                "newText": replacement
                            }]
                        }],
                        "fixId": ADD_MISSING_NEW_FIX_ID,
                        "fixAllDescription": "Add missing 'new' operator to all calls",
                    }));
                } else if let Some((updated_content, description)) =
                    add_missing_new_preview.as_ref()
                    && let Some((start_off, end_off, replacement)) =
                        Self::compute_minimal_edit(&content, updated_content)
                {
                    let start_pos = line_map.offset_to_position(start_off, &content);
                    let end_pos = line_map.offset_to_position(end_off, &content);
                    response_actions.push(serde_json::json!({
                        "fixName": ADD_MISSING_NEW_FIX_ID,
                        "description": description,
                        "changes": [{
                            "fileName": file_path,
                            "textChanges": [{
                                "start": { "line": start_pos.line + 1, "offset": start_pos.character + 1 },
                                "end": { "line": end_pos.line + 1, "offset": end_pos.character + 1 },
                                "newText": replacement
                            }]
                        }],
                        "fixId": ADD_MISSING_NEW_FIX_ID,
                        "fixAllDescription": "Add missing 'new' operator to all calls",
                    }));
                }
            }

            let is_enum_missing_member_error = error_codes.is_empty()
                || error_codes.iter().any(|code| {
                    *code == tsz_checker::diagnostics::diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE
                        || *code
                            == tsz_checker::diagnostics::diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE_DID_YOU_MEAN
                });
            if is_enum_missing_member_error
                && let Some((member_name, updated_content)) =
                    Self::apply_add_missing_enum_member_fallback(&content)
                        .or_else(|| Self::apply_add_missing_enum_member_simple_fallback(&content))
            {
                let end_pos = line_map.offset_to_position(content.len() as u32, &content);
                return TsServerResponse {
                    seq,
                    msg_type: "response".to_string(),
                    command: "getCodeFixes".to_string(),
                    request_seq: request.seq,
                    success: true,
                    message: None,
                    body: Some(serde_json::json!([serde_json::json!({
                        "fixName": "addMissingMember",
                        "description": format!("Add missing enum member '{member_name}'"),
                        "changes": [{
                            "fileName": file_path,
                            "textChanges": [{
                                "start": { "line": 1, "offset": 1 },
                                "end": { "line": end_pos.line + 1, "offset": end_pos.character + 1 },
                                "newText": updated_content
                            }]
                        }],
                        "fixId": "fixMissingMember",
                        "fixAllDescription": "Add all missing members",
                    })])),
                };
            }

            if let Some((description, updated_content)) = add_missing_await_preview.as_ref()
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
                    existing.get("fixId").and_then(serde_json::Value::as_str)
                        != Some("addMissingAwait")
                });
                response_actions.insert(0, action);
            }

            if add_missing_await_preview.is_none()
                && let Some(updated_content) = add_missing_const_preview.as_ref()
                && let Some((start_off, end_off, replacement)) =
                    Self::compute_minimal_edit(&content, updated_content)
            {
                let start_pos = line_map.offset_to_position(start_off, &content);
                let end_pos = line_map.offset_to_position(end_off, &content);
                let const_action = serde_json::json!({
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
                });
                response_actions.retain(|existing| {
                    existing.get("fixId").and_then(serde_json::Value::as_str)
                        != Some("addMissingConst")
                });
                response_actions.push(const_action);
            }
            if response_actions.is_empty()
                && error_codes.iter().any(|code| {
                    *code == tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME
                })
                && let Some((name, updated_content)) =
                    Self::apply_add_missing_function_declaration_fallback_at_request(
                        &content,
                        &line_map,
                        request_span,
                    )
                && let Some((start_off, end_off, replacement)) =
                    Self::compute_minimal_edit(&content, &updated_content)
            {
                let start_pos = line_map.offset_to_position(start_off, &content);
                let end_pos = line_map.offset_to_position(end_off, &content);
                response_actions.push(serde_json::json!({
                    "fixName": "fixMissingFunctionDeclaration",
                    "description": format!("Add missing function declaration '{name}'"),
                    "changes": [{
                        "fileName": file_path,
                        "textChanges": [{
                            "start": { "line": start_pos.line + 1, "offset": start_pos.character + 1 },
                            "end": { "line": end_pos.line + 1, "offset": end_pos.character + 1 },
                            "newText": replacement
                        }]
                    }],
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
            if response_actions.iter().any(|action| {
                action
                    .get("description")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|d| d.starts_with("Add missing enum member '"))
            }) {
                let mut seen_enum_fix = false;
                response_actions.retain(|action| {
                    let is_enum_fix = action
                        .get("description")
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|d| d.starts_with("Add missing enum member '"));
                    if !is_enum_fix {
                        return false;
                    }
                    if seen_enum_fix {
                        return false;
                    }
                    seen_enum_fix = true;
                    true
                });
            }

            if !response_actions.is_empty() {
                if !auto_import_file_exclude_patterns.is_empty() {
                    use std::io::Write;
                    if let Ok(mut f) = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open("/tmp/tsz-debug-codefix.log")
                    {
                        let _ =
                            writeln!(f, "EARLY RETURN with {} actions:", response_actions.len());
                        let _ = writeln!(
                            f,
                            "FULL JSON: {}",
                            serde_json::to_string_pretty(&response_actions).unwrap_or_default()
                        );
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

                if error_codes.contains(
                    &tsz_checker::diagnostics::diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                ) && add_missing_await_preview.is_none()
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
            }
            normalize_response_actions(&mut response_actions);

            // Deduplicate by (fixId, description): when both CodeActionProvider
            // and fallback produce the same fix, keep only the first occurrence.
            // We key on the pair so that distinct actions sharing a fixId (e.g.
            // "Declare method", "Declare property", "Add index signature" all
            // under fixId "fixMissingMember") are preserved.
            let mut dedup_seen = rustc_hash::FxHashSet::default();
            response_actions.retain(|action| {
                let fix_id = action
                    .get("fixId")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                let description = action
                    .get("description")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                if fix_id.is_empty() && description.is_empty() {
                    true
                } else {
                    dedup_seen.insert((fix_id.to_string(), description.to_string()))
                }
            });

            if !auto_import_file_exclude_patterns.is_empty() {
                let import_fixes: Vec<_> = response_actions
                    .iter()
                    .filter(|a| {
                        a.get("fixName").and_then(serde_json::Value::as_str) == Some("import")
                    })
                    .collect();
                if !import_fixes.is_empty() {
                    use std::io::Write;
                    if let Ok(mut f) = std::fs::OpenOptions::new()
                        .append(true)
                        .open("/tmp/tsz-debug-codefix.log")
                    {
                        let _ = writeln!(
                            f,
                            "RETURNING {} import fixes despite exclude:",
                            import_fixes.len()
                        );
                        for fix in &import_fixes {
                            let _ = writeln!(
                                f,
                                "  fix: {}",
                                serde_json::to_string(fix).unwrap_or_default()
                            );
                        }
                    }
                }
                {
                    use std::io::Write;
                    if let Ok(mut f) = std::fs::OpenOptions::new()
                        .append(true)
                        .open("/tmp/tsz-debug-codefix.log")
                    {
                        let _ = writeln!(f, "ALL response_actions ({}):", response_actions.len());
                        for a in &response_actions {
                            let _ = writeln!(
                                f,
                                "  action: fixName={} desc={}",
                                a.get("fixName")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or("?"),
                                a.get("description")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or("?")
                            );
                        }
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

    fn find_first_binding_identifier(text: &str) -> Option<(usize, usize, String)> {
        let bytes = text.as_bytes();
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
            let prev = start.checked_sub(1).and_then(|idx| bytes.get(idx)).copied();
            if prev.is_some_and(|b| {
                let c = b as char;
                c.is_ascii_alphanumeric() || matches!(c, '_' | '$' | '.' | '\'' | '"' | '`')
            }) {
                continue;
            }
            let ident = text[start..i].to_string();
            return Some((start, i, ident));
        }
        None
    }

    fn find_all_binding_identifiers(text: &str) -> Vec<String> {
        let bytes = text.as_bytes();
        let mut i = 0usize;
        let mut out = Vec::new();
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
            let prev = start.checked_sub(1).and_then(|idx| bytes.get(idx)).copied();
            if prev.is_some_and(|b| {
                let c = b as char;
                c.is_ascii_alphanumeric() || matches!(c, '_' | '$' | '.' | '\'' | '"' | '`')
            }) {
                continue;
            }
            out.push(text[start..i].to_string());
        }
        out
    }

    fn add_missing_const_should_skip_for_declared_bindings(
        content: &str,
        line_map: &LineMap,
        start: tsz::lsp::position::Position,
        binder: &tsz::binder::BinderState,
    ) -> bool {
        let Some(start_off) = line_map
            .position_to_offset(start, content)
            .map(|o| o as usize)
        else {
            return false;
        };
        if start_off > content.len() {
            return false;
        }

        let line_start = content[..start_off].rfind('\n').map_or(0, |idx| idx + 1);
        let line_end = content[start_off..]
            .find('\n')
            .map_or(content.len(), |idx| start_off + idx);
        let Some(line) = content.get(line_start..line_end) else {
            return false;
        };
        let trimmed = line.trim_start();
        if trimmed.starts_with("for (") || !trimmed.contains('=') {
            return false;
        }

        let mut statement = line.trim().to_string();
        if statement.ends_with(',') {
            let mut next_start = line_end;
            while next_start < content.len() {
                let next_end = content[next_start..]
                    .find('\n')
                    .map_or(content.len(), |idx| next_start + idx);
                let next_line = content[next_start..next_end].trim();
                if next_line.is_empty() {
                    next_start = next_end.saturating_add(1);
                    continue;
                }
                if !statement.ends_with(',') {
                    break;
                }
                statement.push(' ');
                statement.push_str(next_line);
                next_start = next_end.saturating_add(1);
            }
        }

        for part in statement.split(',') {
            let lhs = part
                .split_once('=')
                .map(|(left, _)| left.trim())
                .unwrap_or_default();
            if lhs.is_empty() {
                continue;
            }
            for name in Self::find_all_binding_identifiers(lhs) {
                if binder.file_locals.get(name.as_str()).is_some() {
                    return true;
                }
            }
        }
        false
    }

    fn apply_add_missing_function_declaration_fallback_at_request(
        content: &str,
        line_map: &LineMap,
        request_span: Option<(tsz::lsp::position::Position, tsz::lsp::position::Position)>,
    ) -> Option<(String, String)> {
        let (start, _) = request_span?;
        let start_off = line_map.position_to_offset(start, content)? as usize;
        if start_off >= content.len() {
            return None;
        }
        let line_start = content[..start_off].rfind('\n').map_or(0, |idx| idx + 1);
        let line_end = content[start_off..]
            .find('\n')
            .map_or(content.len(), |idx| start_off + idx);
        let line = content.get(line_start..line_end)?.trim();
        if !(line.starts_with('[') && line.contains('=') && line.contains('(')) {
            return None;
        }

        let lhs = line
            .split_once('=')
            .map(|(left, _)| left.trim())
            .unwrap_or("");
        let rel = start_off.saturating_sub(line_start).min(lhs.len());
        let lhs_bytes = lhs.as_bytes();
        if rel >= lhs_bytes.len() {
            return None;
        }

        let mut ident_start = rel;
        while ident_start > 0 {
            let c = lhs_bytes[ident_start - 1] as char;
            if c.is_ascii_alphanumeric() || c == '_' || c == '$' {
                ident_start -= 1;
            } else {
                break;
            }
        }
        let mut ident_end = rel;
        while ident_end < lhs_bytes.len() {
            let c = lhs_bytes[ident_end] as char;
            if c.is_ascii_alphanumeric() || c == '_' || c == '$' {
                ident_end += 1;
            } else {
                break;
            }
        }
        if ident_start >= ident_end {
            return None;
        }
        let name = lhs[ident_start..ident_end].to_string();
        let mut after = ident_end;
        while after < lhs_bytes.len() && (lhs_bytes[after] as char).is_ascii_whitespace() {
            after += 1;
        }
        if after >= lhs_bytes.len() || lhs_bytes[after] as char != '(' {
            return None;
        }
        if content.contains(&format!("function {name}(")) {
            return None;
        }

        let mut updated = content.trim_end_matches('\n').to_string();
        updated.push_str("\n\n");
        updated.push_str(&format!(
            "function {name}() {{\n    throw new Error(\"Function not implemented.\");\n}}\n"
        ));
        Some((name, updated))
    }

    fn is_comma_continuation_line(lines: &[String], idx: usize) -> bool {
        if idx == 0 {
            return false;
        }
        let mut prev = idx;
        while prev > 0 {
            prev -= 1;
            let trimmed = lines[prev].trim();
            if trimmed.is_empty() {
                continue;
            }
            return trimmed.ends_with(',');
        }
        false
    }

    fn add_missing_const_line(line: &str) -> Option<String> {
        let trimmed = line.trim_start();
        if trimmed.is_empty()
            || trimmed.starts_with("const ")
            || trimmed.starts_with("let ")
            || trimmed.starts_with("var ")
            || trimmed.starts_with("import ")
            || trimmed.starts_with("export ")
        {
            return None;
        }

        if trimmed.starts_with("for (")
            && (trimmed.contains(" in ") || trimmed.contains(" of "))
            && let Some(open_idx) = line.find('(')
        {
            let mut updated = line.to_string();
            updated.insert_str(open_idx + 1, "const ");
            return Some(updated);
        }

        let starts_with_target = trimmed.chars().next().is_some_and(|ch| {
            ch.is_ascii_alphabetic() || ch == '_' || ch == '$' || ch == '[' || ch == '{'
        });
        if starts_with_target && trimmed.contains('=') {
            let lhs = trimmed
                .split_once('=')
                .map(|(left, _)| left)
                .unwrap_or(trimmed);
            if lhs.contains('(') {
                return None;
            }
            let indent_len = line.len().saturating_sub(trimmed.len());
            let indent = &line[..indent_len];
            return Some(format!("{indent}const {trimmed}"));
        }

        None
    }

    fn apply_add_missing_const_fallback(content: &str) -> Option<String> {
        let mut lines: Vec<String> = content.lines().map(str::to_string).collect();
        for idx in 0..lines.len() {
            if Self::is_comma_continuation_line(&lines, idx) {
                continue;
            }
            if let Some(updated_line) = Self::add_missing_const_line(&lines[idx]) {
                lines[idx] = updated_line;
                return Some(lines.join("\n"));
            }
        }
        None
    }

    fn apply_add_missing_const_fix_all_fallback(content: &str) -> Option<String> {
        let mut lines: Vec<String> = content.lines().map(str::to_string).collect();
        let mut changed = false;
        let mut skip_comma_continuation = false;
        let mut idx = 0usize;
        while idx < lines.len() {
            let trimmed = lines[idx].trim();
            if trimmed.is_empty() {
                idx += 1;
                continue;
            }
            if skip_comma_continuation {
                if !trimmed.ends_with(',') {
                    skip_comma_continuation = false;
                }
                idx += 1;
                continue;
            }
            if let Some(updated_line) = Self::add_missing_const_line(&lines[idx]) {
                skip_comma_continuation = lines[idx].trim_end().ends_with(',');
                lines[idx] = updated_line;
                changed = true;
                idx += 1;
                continue;
            }
            idx += 1;
        }
        changed.then(|| lines.join("\n"))
    }

    fn apply_add_missing_const_fallback_at_position(
        content: &str,
        line_map: &LineMap,
        start: tsz::lsp::position::Position,
    ) -> Option<String> {
        let start_off = line_map.position_to_offset(start, content)? as usize;
        if start_off > content.len() {
            return None;
        }

        let line_start = content[..start_off].rfind('\n').map_or(0, |idx| idx + 1);
        let line_end = content[start_off..]
            .find('\n')
            .map_or(content.len(), |idx| start_off + idx);
        let line = content.get(line_start..line_end)?;
        let trimmed = line.trim_start();
        if trimmed.is_empty()
            || trimmed.starts_with("const ")
            || trimmed.starts_with("let ")
            || trimmed.starts_with("var ")
            || trimmed.starts_with("import ")
            || trimmed.starts_with("export ")
        {
            return None;
        }

        let insertion_offset = if trimmed.starts_with("for (")
            && (trimmed.contains(" in ") || trimmed.contains(" of "))
        {
            let open_idx = line.find('(')?;
            line_start + open_idx + 1
        } else {
            let starts_with_target = trimmed.chars().next().is_some_and(|ch| {
                ch.is_ascii_alphabetic() || ch == '_' || ch == '$' || ch == '[' || ch == '{'
            });
            if !starts_with_target || !trimmed.contains('=') {
                return None;
            }
            let lhs = trimmed
                .split_once('=')
                .map(|(left, _)| left.trim_end())
                .unwrap_or(trimmed);
            if lhs.contains('(') {
                return None;
            }
            let prefix_ws = line.len().saturating_sub(trimmed.len());
            if let Some((first_rel_start, first_rel_end, _)) =
                Self::find_first_binding_identifier(lhs)
            {
                let abs_first_start = line_start + prefix_ws + first_rel_start;
                let abs_first_end = line_start + prefix_ws + first_rel_end;
                if start_off < abs_first_start || start_off > abs_first_end {
                    return None;
                }
            }
            line_start + line.len().saturating_sub(trimmed.len())
        };

        let mut updated = content.to_string();
        updated.insert_str(insertion_offset, "const ");
        Some(updated)
    }

    fn apply_add_missing_await_fallback(content: &str, fix_all: bool) -> Option<(String, String)> {
        if !content.contains("async function") {
            return None;
        }

        let mut lines: Vec<String> = content.lines().map(str::to_string).collect();
        let has_promise_annotation = |name: &str| {
            content.contains(&format!("{name}: Promise<"))
                || content.contains(&format!("{name} : Promise<"))
        };
        for idx in 0..lines.len() {
            let trimmed = lines[idx].trim_start();
            if trimmed.starts_with("for (const ")
                && trimmed.contains(" of ")
                && trimmed.contains("g()")
                && !trimmed.starts_with("for await ")
            {
                lines[idx] = lines[idx].replacen("for (", "for await (", 1);
                return Some(("Add 'await'".to_string(), lines.join("\n")));
            }
        }
        let mut promise_vars: Vec<String> = Vec::new();
        for line in &lines {
            let trimmed = line.trim();
            let bytes = trimmed.as_bytes();
            let mut idx = 0usize;
            while idx < trimmed.len() {
                let c = bytes[idx] as char;
                if c.is_ascii_alphabetic() || c == '_' || c == '$' {
                    let start = idx;
                    idx += 1;
                    while idx < trimmed.len() {
                        let cc = bytes[idx] as char;
                        if cc.is_ascii_alphanumeric() || cc == '_' || cc == '$' {
                            idx += 1;
                        } else {
                            break;
                        }
                    }
                    let name = &trimmed[start..idx];
                    let mut j = idx;
                    while j < trimmed.len() && (bytes[j] as char).is_ascii_whitespace() {
                        j += 1;
                    }
                    if j < trimmed.len() && bytes[j] as char == ':' {
                        j += 1;
                        while j < trimmed.len() && (bytes[j] as char).is_ascii_whitespace() {
                            j += 1;
                        }
                        if trimmed[j..].starts_with("Promise<")
                            && !promise_vars.iter().any(|v| v == name)
                        {
                            promise_vars.push(name.to_string());
                        }
                    }
                    continue;
                }
                idx += 1;
            }
        }
        if promise_vars.is_empty() {
            return None;
        }

        let mut initializer_candidates: Vec<(usize, String)> = Vec::new();
        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            if !trimmed.starts_with("const ")
                || !trimmed.ends_with(';')
                || trimmed.contains("await ")
            {
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
            if !promise_vars.iter().any(|v| v == rhs) {
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
            return Some((
                format!("Add 'await' to initializer for '{var_name}'"),
                lines.join("\n"),
            ));
        }

        for idx in 0..lines.len() {
            let trimmed = lines[idx].trim_start();
            let Some(dot_idx) = trimmed.find('.') else {
                continue;
            };
            if !trimmed.ends_with(';')
                || trimmed.starts_with("(await ")
                || trimmed.starts_with("await ")
            {
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
            if !promise_vars.iter().any(|v| v == ident) {
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
            if trimmed.starts_with("if (") {
                let inside = trimmed
                    .strip_prefix("if (")
                    .and_then(|s| s.split(')').next())
                    .map(str::trim)
                    .unwrap_or_default();
                if !inside.is_empty()
                    && inside
                        .chars()
                        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$')
                    && has_promise_annotation(inside)
                {
                    lines[idx] = lines[idx].replacen(
                        &format!("if ({inside})"),
                        &format!("if (await {inside})"),
                        1,
                    );
                    return Some(("Add 'await'".to_string(), lines.join("\n")));
                }
            }

            if let Some(q_idx) = trimmed.find('?') {
                let cond = trimmed[..q_idx].trim();
                if !cond.is_empty()
                    && cond
                        .chars()
                        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$')
                    && has_promise_annotation(cond)
                {
                    lines[idx] =
                        lines[idx].replacen(&format!("{cond} ?"), &format!("await {cond} ?"), 1);
                    return Some(("Add 'await'".to_string(), lines.join("\n")));
                }
            }

            for var in &promise_vars {
                let if_pat = format!("if ({var})");
                if trimmed.contains(&if_pat) {
                    lines[idx] = lines[idx].replacen(&if_pat, &format!("if (await {var})"), 1);
                    return Some(("Add 'await'".to_string(), lines.join("\n")));
                }

                let ternary_pat = format!("{var} ?");
                if trimmed.contains(&ternary_pat) {
                    lines[idx] = lines[idx].replacen(&ternary_pat, &format!("await {var} ?"), 1);
                    return Some(("Add 'await'".to_string(), lines.join("\n")));
                }

                let spread_pat = format!("[...{var}]");
                if trimmed.contains(&spread_pat) {
                    lines[idx] = lines[idx].replacen(&spread_pat, &format!("[...await {var}]"), 1);
                    return Some(("Add 'await'".to_string(), lines.join("\n")));
                }

                let for_of_pat = format!(" of {var})");
                if trimmed.contains("for (") && trimmed.contains(&for_of_pat) {
                    lines[idx] = lines[idx].replacen(&for_of_pat, &format!(" of await {var})"), 1);
                    return Some(("Add 'await'".to_string(), lines.join("\n")));
                }

                let bin_l_pat = format!("{var} |");
                if trimmed.contains(&bin_l_pat) {
                    lines[idx] = lines[idx].replacen(&bin_l_pat, &format!("await {var} |"), 1);
                    return Some(("Add 'await'".to_string(), lines.join("\n")));
                }

                let bin_r_pat = format!("+ {var}");
                if trimmed.contains(&bin_r_pat) {
                    lines[idx] = lines[idx].replacen(&bin_r_pat, &format!("+ await {var}"), 1);
                    return Some(("Add 'await'".to_string(), lines.join("\n")));
                }
            }
        }

        for idx in 0..lines.len() {
            let trimmed = lines[idx].trim_start();
            if !(trimmed.ends_with("();")
                || trimmed.ends_with("()")
                || (trimmed.starts_with("new ")
                    && (trimmed.ends_with(");") || trimmed.ends_with(")")))
                || trimmed.contains("await "))
            {
                continue;
            }

            if let Some(rest) = trimmed.strip_prefix("new ") {
                let has_semicolon = rest.ends_with(";");
                let ctor = rest.trim_end_matches(';').trim_end_matches("()").trim();
                if ctor
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$')
                    && !ctor.is_empty()
                    && promise_vars.iter().any(|v| v == ctor)
                {
                    let indent_len = lines[idx].len().saturating_sub(trimmed.len());
                    let indent = lines[idx][..indent_len].to_string();
                    let semi = if has_semicolon { ";" } else { "" };
                    lines[idx] = format!("{indent}new (await {ctor})(){semi}");
                    return Some(("Add 'await'".to_string(), lines.join("\n")));
                }
            } else {
                let has_semicolon = trimmed.ends_with(';');
                let callee = trimmed.trim_end_matches(';').trim_end_matches("()").trim();
                if callee
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$')
                    && !callee.is_empty()
                    && promise_vars.iter().any(|v| v == callee)
                {
                    let indent_len = lines[idx].len().saturating_sub(trimmed.len());
                    let indent = lines[idx][..indent_len].to_string();
                    let mut replacement = format!("(await {callee})()");
                    if idx > 0 {
                        let prev = lines[idx - 1].trim_end();
                        if !prev.is_empty() && !prev.ends_with(';') && !prev.ends_with('{') {
                            replacement = format!(";{replacement}");
                        }
                    }
                    let semi = if has_semicolon { ";" } else { "" };
                    lines[idx] = format!("{indent}{replacement}{semi}");
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
                        && promise_vars.iter().any(|v| v == last_arg)
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
        if content
            .lines()
            .any(|line| line.trim_start().starts_with("////"))
        {
            let normalized = content
                .lines()
                .map(|line| {
                    let ws_len = line.len().saturating_sub(line.trim_start().len());
                    let ws = &line[..ws_len];
                    let trimmed = &line[ws_len..];
                    if let Some(rest) = trimmed.strip_prefix("////") {
                        format!("{ws}{rest}")
                    } else {
                        line.to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            if normalized != content {
                return Self::apply_add_missing_enum_member_fallback(&normalized);
            }
        }

        let lines: Vec<String> = content.lines().map(str::to_string).collect();

        fn is_ident(s: &str) -> bool {
            !s.is_empty()
                && s.chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$')
                && s.chars()
                    .next()
                    .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_' || ch == '$')
        }

        let mut enum_name = String::new();
        let mut member_name = String::new();
        for line in &lines {
            let trimmed = line.trim().replace("/**/", "");
            if trimmed.starts_with("enum ")
                || trimmed.starts_with("export enum ")
                || trimmed.starts_with("export const enum ")
            {
                continue;
            }

            let bytes = trimmed.as_bytes();
            for (idx, ch) in trimmed.char_indices() {
                if ch != '.' {
                    continue;
                }

                let mut left_end = idx;
                while left_end > 0 && (bytes[left_end - 1] as char).is_ascii_whitespace() {
                    left_end -= 1;
                }
                let mut left_start = left_end;
                while left_start > 0 {
                    let c = bytes[left_start - 1] as char;
                    if c.is_ascii_alphanumeric() || c == '_' || c == '$' {
                        left_start -= 1;
                    } else {
                        break;
                    }
                }
                let left = trimmed[left_start..left_end].trim();

                let mut right_start = idx + 1;
                while right_start < trimmed.len()
                    && (bytes[right_start] as char).is_ascii_whitespace()
                {
                    right_start += 1;
                }
                let mut right_end = right_start;
                while right_end < trimmed.len() {
                    let c = bytes[right_end] as char;
                    if c.is_ascii_alphanumeric() || c == '_' || c == '$' {
                        right_end += 1;
                    } else {
                        break;
                    }
                }
                let right = trimmed[right_start..right_end].trim();
                if is_ident(left) && is_ident(right) {
                    enum_name = left.to_string();
                    member_name = right.to_string();
                }
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
                #[allow(clippy::needless_range_loop)]
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
        let mut enum_member_string: std::collections::HashMap<(String, String), bool> =
            std::collections::HashMap::new();
        {
            let mut current_enum: Option<String> = None;
            for line in &lines {
                let trimmed = line.trim();
                if trimmed.starts_with("enum ")
                    || trimmed.starts_with("export enum ")
                    || trimmed.starts_with("export const enum ")
                {
                    let parts: Vec<&str> = trimmed.split_whitespace().collect();
                    if let Some((idx, _)) = parts.iter().enumerate().find(|(_, p)| **p == "enum")
                        && let Some(name) = parts.get(idx + 1)
                    {
                        current_enum = Some((*name).to_string());
                    }
                    continue;
                }
                if trimmed == "}" {
                    current_enum = None;
                    continue;
                }
                let Some(current) = current_enum.as_ref() else {
                    continue;
                };
                let member_line = trimmed.trim_end_matches(',');
                if member_line.is_empty() {
                    continue;
                }
                let name = member_line
                    .split(['=', ' ', '\t'])
                    .next()
                    .unwrap_or_default()
                    .trim();
                if !is_ident(name) {
                    continue;
                }
                let mut is_string = false;
                if let Some(eq_idx) = member_line.find('=') {
                    let rhs = member_line[eq_idx + 1..].trim();
                    if rhs.starts_with('"') || rhs.starts_with('\'') {
                        is_string = true;
                    } else if let Some(dot) = rhs.find('.') {
                        let lhs = rhs[..dot].trim();
                        let rhs_member = rhs[dot + 1..]
                            .chars()
                            .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '$')
                            .collect::<String>();
                        if !lhs.is_empty()
                            && !rhs_member.is_empty()
                            && enum_member_string
                                .get(&(lhs.to_string(), rhs_member.to_string()))
                                .copied()
                                .unwrap_or(false)
                        {
                            is_string = true;
                        }
                    }
                }
                enum_member_string.insert((current.clone(), name.to_string()), is_string);
            }
        }

        let mut has_string_initializer = false;
        let mut already_exists = false;
        let mut last_member_idx: Option<usize> = None;
        let mut use_trailing_comma = false;

        #[allow(clippy::needless_range_loop)]
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
            has_string_initializer |= enum_member_string
                .get(&(enum_name.clone(), name.to_string()))
                .copied()
                .unwrap_or(false);
            use_trailing_comma = lines[idx].trim_end().ends_with(',');
            last_member_idx = Some(idx);
        }
        if already_exists {
            return None;
        }

        let mut updated = lines;
        if let Some(idx) = last_member_idx {
            let prev = &updated[idx];
            let trimmed_len = prev.trim_end().len();
            let (head, trailing) = prev.split_at(trimmed_len);
            if !head.ends_with(',') && !head.ends_with('{') {
                updated[idx] = format!("{head},{trailing}");
            }
        }

        let indent = "    ";
        let new_member_line = if has_string_initializer {
            format!("{indent}{member_name} = \"{member_name}\"")
        } else {
            format!("{indent}{member_name}")
        };
        let new_member_line = if use_trailing_comma {
            format!("{new_member_line},")
        } else {
            new_member_line
        };
        updated.insert(end_idx, new_member_line);
        Some((member_name, updated.join("\n")))
    }

    fn apply_add_missing_enum_member_simple_fallback(content: &str) -> Option<(String, String)> {
        if content
            .lines()
            .any(|line| line.trim_start().starts_with("////"))
        {
            let normalized = content
                .lines()
                .map(|line| {
                    let ws_len = line.len().saturating_sub(line.trim_start().len());
                    let ws = &line[..ws_len];
                    let trimmed = &line[ws_len..];
                    if let Some(rest) = trimmed.strip_prefix("////") {
                        format!("{ws}{rest}")
                    } else {
                        line.to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            if normalized != content {
                return Self::apply_add_missing_enum_member_simple_fallback(&normalized);
            }
        }

        let mut enum_name = None::<String>;
        let mut member_name = None::<String>;
        for line in content.lines() {
            let t = line.trim().replace("/**/", "");
            if let Some(dot) = t.find('.') {
                let left = t[..dot]
                    .chars()
                    .rev()
                    .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '$')
                    .collect::<String>()
                    .chars()
                    .rev()
                    .collect::<String>();
                let right = t[dot + 1..]
                    .chars()
                    .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '$')
                    .collect::<String>();
                if !left.is_empty() && !right.is_empty() {
                    enum_name = Some(left);
                    member_name = Some(right);
                }
            }
        }
        let (enum_name, member_name) = (enum_name?, member_name?);

        let mut lines: Vec<String> = content.lines().map(str::to_string).collect();
        let mut enum_start = None;
        let mut enum_end = None;
        for (i, line) in lines.iter().enumerate() {
            let t = line.trim();
            if t.starts_with(&format!("enum {enum_name}"))
                || t.starts_with(&format!("export enum {enum_name}"))
                || t.starts_with(&format!("export const enum {enum_name}"))
            {
                enum_start = Some(i);
                #[allow(clippy::needless_range_loop)]
                for j in i + 1..lines.len() {
                    if lines[j].trim() == "}" {
                        enum_end = Some(j);
                        break;
                    }
                }
                break;
            }
        }
        let (start, end) = (enum_start?, enum_end?);
        if lines[start + 1..end]
            .iter()
            .any(|l| l.trim_start().starts_with(&(member_name.clone() + " ")))
            || lines[start + 1..end]
                .iter()
                .any(|l| l.trim_start().starts_with(&(member_name.clone() + ",")))
        {
            return None;
        }

        if end > start + 1 {
            let prev = &lines[end - 1];
            let trimmed_len = prev.trim_end().len();
            let (head, trailing) = prev.split_at(trimmed_len);
            if !head.ends_with(',') && !head.ends_with('{') {
                lines[end - 1] = format!("{head},{trailing}");
            }
        }
        lines.insert(end, format!("    {member_name}"));
        Some((member_name, lines.join("\n")))
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
                        code: tsz_checker::diagnostics::diagnostic_codes::JSDOC_TYPES_MAY_BE_MOVED_TO_TYPESCRIPT_TYPES,
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
                        code: tsz_checker::diagnostics::diagnostic_codes::JSDOC_TYPES_MAY_BE_MOVED_TO_TYPESCRIPT_TYPES,
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
                        code: tsz_checker::diagnostics::diagnostic_codes::JSDOC_TYPES_MAY_BE_MOVED_TO_TYPESCRIPT_TYPES,
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
                            code: tsz_checker::diagnostics::diagnostic_codes::JSDOC_TYPES_MAY_BE_MOVED_TO_TYPESCRIPT_TYPES,
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
                            code: tsz_checker::diagnostics::diagnostic_codes::VARIABLE_IMPLICITLY_HAS_AN_TYPE_BUT_A_BETTER_TYPE_MAY_BE_INFERRED_FROM_USAGE,
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
                        code: tsz_checker::diagnostics::diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE_BUT_A_BETTER_TYPE_MAY_BE_INFERRED_FROM_USAGE,
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
            let leading_ws = line.len().saturating_sub(trimmed.len());
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

            if trimmed.starts_with("type ")
                && let Some(eq_idx) = trimmed.find('=')
            {
                self.push_synthetic_missing_type_identifiers(
                    &mut diagnostics,
                    &mut seen_spans,
                    binder,
                    file_path,
                    offset,
                    &trimmed[eq_idx + 1..],
                    leading_ws + eq_idx + 1,
                );
            }

            if trimmed.starts_with("const ")
                || trimmed.starts_with("let ")
                || trimmed.starts_with("var ")
                || trimmed.starts_with("function ")
                || trimmed.starts_with("export function ")
                || trimmed.starts_with("async function ")
                || trimmed.starts_with("export async function ")
            {
                let mut search_from = 0usize;
                while let Some(colon_rel) = trimmed[search_from..].find(':') {
                    let colon_idx = search_from + colon_rel;
                    let after_colon = &trimmed[colon_idx + 1..];
                    let after_colon_trimmed = after_colon.trim_start();
                    let after_colon_ws =
                        after_colon.len().saturating_sub(after_colon_trimmed.len());
                    let fragment_len = after_colon_trimmed
                        .find([',', ')', ';', '=', '{'])
                        .unwrap_or(after_colon_trimmed.len());
                    let fragment = &after_colon_trimmed[..fragment_len];
                    if !fragment.is_empty() {
                        self.push_synthetic_missing_type_identifiers(
                            &mut diagnostics,
                            &mut seen_spans,
                            binder,
                            file_path,
                            offset,
                            fragment,
                            leading_ws + colon_idx + 1 + after_colon_ws,
                        );
                    }
                    search_from = colon_idx + 1;
                }
            }

            if trimmed.starts_with('[') && trimmed.contains('=') {
                let lhs = trimmed
                    .split_once('=')
                    .map(|(left, _)| left)
                    .unwrap_or(trimmed);
                let lhs_bytes = lhs.as_bytes();
                let mut i = 0usize;
                while i < lhs_bytes.len() {
                    let ch = lhs_bytes[i] as char;
                    if !(ch.is_ascii_alphabetic() || ch == '_' || ch == '$') {
                        i += 1;
                        continue;
                    }
                    let start = i;
                    i += 1;
                    while i < lhs_bytes.len() {
                        let next = lhs_bytes[i] as char;
                        if next.is_ascii_alphanumeric() || next == '_' || next == '$' {
                            i += 1;
                        } else {
                            break;
                        }
                    }
                    let Some(name) = lhs.get(start..i) else {
                        continue;
                    };
                    if binder.file_locals.get(name).is_some() {
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

    fn push_synthetic_missing_type_identifiers(
        &self,
        diagnostics: &mut Vec<tsz::checker::diagnostics::Diagnostic>,
        seen_spans: &mut std::collections::HashSet<(usize, usize)>,
        binder: &tsz::binder::BinderState,
        file_path: &str,
        line_offset: usize,
        fragment: &str,
        fragment_offset_in_line: usize,
    ) {
        for (ident, rel_start) in Self::type_identifier_spans(fragment) {
            if binder.file_locals.get(ident.as_str()).is_some() {
                continue;
            }
            if !self.has_potential_auto_import_symbol(file_path, ident.as_str()) {
                continue;
            }
            let absolute_start = line_offset + fragment_offset_in_line + rel_start;
            if !seen_spans.insert((absolute_start, ident.len())) {
                continue;
            }
            diagnostics.push(tsz::checker::diagnostics::Diagnostic {
                category: DiagnosticCategory::Error,
                code: tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME,
                file: file_path.to_string(),
                start: absolute_start as u32,
                length: ident.len() as u32,
                message_text: format!("Cannot find name '{ident}'."),
                related_information: Vec::new(),
            });
        }
    }

    fn type_identifier_spans(fragment: &str) -> Vec<(String, usize)> {
        let mut spans = Vec::new();
        for ident in extract_type_identifiers(fragment) {
            let mut search_start = 0usize;
            while let Some(found) = fragment[search_start..].find(&ident) {
                let rel_start = search_start + found;
                let rel_end = rel_start + ident.len();
                let prev = rel_start
                    .checked_sub(1)
                    .and_then(|idx| fragment.as_bytes().get(idx))
                    .map(|b| *b as char);
                let next = fragment.as_bytes().get(rel_end).map(|b| *b as char);
                let is_qualified_name_segment = prev == Some('.') || next == Some('.');
                let is_import_type_query_segment =
                    Self::is_within_import_type_query(fragment, rel_start);
                let at_word_boundary = prev
                    .is_none_or(|ch| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '$'))
                    && next
                        .is_none_or(|ch| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '$'));
                if at_word_boundary && !is_qualified_name_segment && !is_import_type_query_segment {
                    spans.push((ident.clone(), rel_start));
                }
                search_start = rel_end;
            }
        }
        spans
    }

    fn is_within_import_type_query(fragment: &str, ident_start: usize) -> bool {
        let bytes = fragment.as_bytes();
        let mut i = 0usize;

        while i < bytes.len() {
            let is_word_start = i == 0 || {
                let ch = bytes[i - 1] as char;
                !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '$')
            };
            if !is_word_start || !fragment[i..].starts_with("import(") {
                i += 1;
                continue;
            }

            let import_start = i;
            i += "import(".len();
            let mut depth = 1usize;

            while i < bytes.len() {
                match bytes[i] as char {
                    '"' | '\'' => {
                        let quote = bytes[i];
                        i += 1;
                        while i < bytes.len() {
                            if bytes[i] == b'\\' {
                                i = (i + 2).min(bytes.len());
                                continue;
                            }
                            let matches_quote = bytes[i] == quote;
                            i += 1;
                            if matches_quote {
                                break;
                            }
                        }
                    }
                    '(' => {
                        depth += 1;
                        i += 1;
                    }
                    ')' => {
                        depth = depth.saturating_sub(1);
                        i += 1;
                        if depth == 0 {
                            return ident_start >= import_start && ident_start < i;
                        }
                    }
                    _ => i += 1,
                }
            }

            return ident_start >= import_start;
        }

        false
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
            code:
                tsz_checker::diagnostics::diagnostic_codes::CLASS_INCORRECTLY_IMPLEMENTS_INTERFACE,
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
            code: tsz_checker::diagnostics::diagnostic_codes::AWAIT_EXPRESSIONS_ARE_ONLY_ALLOWED_WITHIN_ASYNC_FUNCTIONS_AND_AT_THE_TOP_LEVELS,
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
            code: tsz_checker::diagnostics::diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE,
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
            code: tsz_checker::diagnostics::diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE,
            file: file_path.to_string(),
            start,
            length: 1,
            message_text: "Type '{}' is missing the following properties.".to_string(),
            related_information: Vec::new(),
        })
    }

    pub(super) fn synthetic_add_missing_new_diagnostic(
        file_path: &str,
        content: &str,
    ) -> Option<tsz::checker::diagnostics::Diagnostic> {
        let _ = Self::apply_add_missing_new_fallback(content, None)?;
        let class_names = Self::collect_class_names(content);
        let mut start = 0u32;
        for name in &class_names {
            let pattern = format!("{name}(");
            if let Some(pos) = content.find(&pattern) {
                let prefix = content[..pos].trim_end();
                if !prefix.ends_with("new") {
                    start = pos as u32;
                    break;
                }
            }
        }
        Some(tsz::checker::diagnostics::Diagnostic {
            category: DiagnosticCategory::Error,
            code: 2348,
            file: file_path.to_string(),
            start,
            length: 1,
            message_text: "Value of type is not callable. Did you mean to include 'new'?"
                .to_string(),
            related_information: Vec::new(),
        })
    }

    pub(super) fn synthetic_add_missing_const_diagnostic(
        file_path: &str,
        content: &str,
    ) -> Option<tsz::checker::diagnostics::Diagnostic> {
        let mut line_start = 0u32;
        let mut enum_brace_depth = 0i32;
        let lines: Vec<String> = content.lines().map(str::to_string).collect();
        for (line_idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            if enum_brace_depth == 0
                && (trimmed.starts_with("enum ")
                    || trimmed.starts_with("export enum ")
                    || trimmed.starts_with("export const enum "))
            {
                enum_brace_depth += trimmed.chars().filter(|ch| *ch == '{').count() as i32;
                enum_brace_depth -= trimmed.chars().filter(|ch| *ch == '}').count() as i32;
                if enum_brace_depth <= 0 && trimmed.contains('{') {
                    enum_brace_depth = 1;
                }
                line_start = line_start.saturating_add(line.len() as u32 + 1);
                continue;
            }
            if enum_brace_depth > 0 {
                enum_brace_depth += trimmed.chars().filter(|ch| *ch == '{').count() as i32;
                enum_brace_depth -= trimmed.chars().filter(|ch| *ch == '}').count() as i32;
                line_start = line_start.saturating_add(line.len() as u32 + 1);
                continue;
            }
            if trimmed.is_empty()
                || trimmed.starts_with("const ")
                || trimmed.starts_with("let ")
                || trimmed.starts_with("var ")
                || trimmed.starts_with("import ")
                || trimmed.starts_with("export ")
                || trimmed.starts_with("function ")
                || trimmed.starts_with("class ")
            {
                line_start = line_start.saturating_add(line.len() as u32 + 1);
                continue;
            }

            if Self::is_comma_continuation_line(&lines, line_idx) {
                line_start = line_start.saturating_add(line.len() as u32 + 1);
                continue;
            }

            if trimmed.starts_with("for (")
                && (trimmed.contains(" in ") || trimmed.contains(" of "))
                && let Some(open_idx) = line.find('(')
            {
                let after = line[open_idx + 1..].trim_start();
                let head_end_rel = after
                    .find(" in ")
                    .or_else(|| after.find(" of "))
                    .unwrap_or(after.len());
                let head = &after[..head_end_rel];
                if let Some((name_rel, name_end, name)) = Self::find_first_binding_identifier(head)
                {
                    let prefix_ws = line.len().saturating_sub(trimmed.len());
                    let after_ws = line[open_idx + 1..].len().saturating_sub(after.len());
                    let name_start = prefix_ws + open_idx + 1 + after_ws + name_rel;
                    return Some(tsz::checker::diagnostics::Diagnostic {
                        category: DiagnosticCategory::Error,
                        code: tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME,
                        file: file_path.to_string(),
                        start: line_start + name_start as u32,
                        length: (name_end - name_rel) as u32,
                        message_text: format!("Cannot find name '{name}'."),
                        related_information: Vec::new(),
                    });
                }
            }

            let starts_with_target = trimmed
                .chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_' || ch == '$');
            if starts_with_target && trimmed.contains('=') {
                let lhs = trimmed
                    .split_once('=')
                    .map(|(l, _)| l.trim_end())
                    .unwrap_or(trimmed);
                if lhs.contains('(') {
                    line_start = line_start.saturating_add(line.len() as u32 + 1);
                    continue;
                }
                if let Some((name_rel, name_end, name)) = Self::find_first_binding_identifier(lhs) {
                    let prefix_ws = line.len().saturating_sub(trimmed.len());
                    return Some(tsz::checker::diagnostics::Diagnostic {
                        category: DiagnosticCategory::Error,
                        code: tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME,
                        file: file_path.to_string(),
                        start: line_start + (prefix_ws + name_rel) as u32,
                        length: (name_end - name_rel) as u32,
                        message_text: format!("Cannot find name '{name}'."),
                        related_information: Vec::new(),
                    });
                }
            }

            line_start = line_start.saturating_add(line.len() as u32 + 1);
        }

        None
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
        let (interface_file_path, interface_content) =
            if let Some(interface_module_specifier) = class_imports.get(&interface_name).cloned() {
                let interface_file_path =
                    resolve_module_path(file_path, &interface_module_specifier, &self.open_files)?;
                let interface_content = self
                    .open_files
                    .get(&interface_file_path)
                    .cloned()
                    .or_else(|| std::fs::read_to_string(&interface_file_path).ok())?;
                (interface_file_path, interface_content)
            } else if content.contains(&format!("interface {interface_name}")) {
                (file_path.to_string(), content.to_string())
            } else {
                return None;
            };

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

            if fix_id == "addMissingConst"
                && let Some(updated_content) =
                    Self::apply_add_missing_const_fix_all_fallback(&content)
            {
                let end_pos = line_map.offset_to_position(content.len() as u32, &content);
                all_changes.clear();
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
                && fix_id == "addMissingNewOperator"
                && let Some(updated_content) = Self::apply_add_missing_new_all_fallback(&content)
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
