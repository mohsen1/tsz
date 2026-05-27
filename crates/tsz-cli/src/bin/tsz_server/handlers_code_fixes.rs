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
    class_body_has_member, find_first_implements_class, parse_interface_properties,
    parse_named_import_map, positions_overlap, resolve_module_path,
};
use super::{Server, TsServerRequest, TsServerResponse};
use tsz::checker::diagnostics::DiagnosticCategory;
use tsz::lsp::code_actions::{
    CodeActionContext, CodeActionKind, CodeActionProvider, CodeFixRegistry,
};
use tsz::lsp::position::LineMap;

const FIX_MISSING_TYPE_ANNOTATION_FIX_ID: &str = "fixMissingTypeAnnotationOnExports";

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
            .with_organize_imports_ignore_case(organize_imports_ignore_case)
            .with_new_line_override(self.new_line_character.clone());
            let unknown_conversion_content = Self::apply_unknown_conversion_fallback(&content);
            let missing_async_content = Self::apply_missing_async_fallback(&content);
            let add_parameter_names_content =
                Self::apply_add_names_to_nameless_parameters_fallback(&content);
            let missing_attributes_content = Self::apply_missing_attributes_fallback(&content);
            let add_missing_new_preview =
                Self::apply_add_missing_new_fallback(&content, request_span);
            let add_missing_await_preview = Self::apply_add_missing_await_fallback(&content, false);
            let add_missing_function_declaration_preview =
                Self::apply_add_missing_function_declaration_fallback_at_request(
                    &content,
                    &line_map,
                    request_span,
                );
            let add_missing_function_declaration_anywhere =
                Self::apply_add_missing_function_declaration_fallback_anywhere(&content);
            let add_missing_function_declaration_candidate =
                add_missing_function_declaration_preview
                    .or_else(|| add_missing_function_declaration_anywhere.clone());
            let has_mixed_declared_binding_assignment =
                Self::has_mixed_declared_binding_assignment(&content);
            let add_missing_const_preview = if let Some((start, _)) = request_span {
                Self::apply_add_missing_const_fallback_at_position(&content, &line_map, start)
            } else {
                None
            };
            let add_missing_const_anywhere = Self::apply_add_missing_const_fallback(&content);
            // When the client supplies a request span, only honor a fix that
            // covers the cursor position. Falling back to the file-wide
            // anywhere search would surface fixes for diagnostics outside
            // the requested range (issue #3832).
            let mut add_missing_const_candidate = if request_span.is_some() {
                add_missing_const_preview.clone()
            } else {
                add_missing_const_preview
                    .clone()
                    .or_else(|| add_missing_const_anywhere.clone())
            };
            if has_mixed_declared_binding_assignment {
                add_missing_const_candidate = None;
            }
            let mut skip_add_missing_const_for_declared_bindings = false;
            if let Some((start, _)) = request_span
                && Self::add_missing_const_should_skip_for_declared_bindings(
                    &content, &line_map, start,
                )
            {
                add_missing_const_candidate = None;
                skip_add_missing_const_for_declared_bindings = true;
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
            let has_cannot_find_name_diag = diagnostics
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
            // tsc returns no fixes when the request span does not overlap any
            // diagnostic. Don't fall back to all matching diagnostics in the
            // file once the span filter is empty (issue #3832).
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
            if filtered_diagnostics.is_empty()
                && error_codes
                    .contains(&tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME)
                && request_span.is_some_and(|(start, end)| {
                    start == end && start.line == 0 && start.character == 0
                })
            {
                filtered_diagnostics.extend(
                    diagnostics
                        .iter()
                        .filter(|d| {
                            d.code == tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME
                        })
                        .map(to_lsp_diag),
                );
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
            let includes_cannot_find_name = error_codes
                .contains(&tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME)
                || (error_codes.is_empty() && has_cannot_find_name_diag);
            if includes_cannot_find_name {
                let should_prune_binding_fix = |action: &serde_json::Value| {
                    matches!(
                        action.get("fixId").and_then(serde_json::Value::as_str),
                        Some("fixMissingImport")
                            | Some("fixMissingMember")
                            | Some("forgottenThisPropertyAccess")
                    )
                };

                if add_missing_function_declaration_candidate.is_some() {
                    // In destructuring-call contexts (e.g. [x, y()] = ...), tsserver
                    // prefers function-declaration fixes over import/member guesses.
                    response_actions.retain(|action| {
                        !should_prune_binding_fix(action)
                            && action.get("fixId").and_then(serde_json::Value::as_str)
                                != Some("addMissingConst")
                    });
                } else if has_mixed_declared_binding_assignment {
                    // Mixed declared/undeclared LHS assignment patterns (e.g.
                    // `let x; [x, y] = ...`) should not offer import/member/const
                    // quick fixes at this site.
                    response_actions.retain(|action| {
                        !should_prune_binding_fix(action)
                            && action.get("fixId").and_then(serde_json::Value::as_str)
                                != Some("addMissingConst")
                    });
                } else if add_missing_const_preview.is_some()
                    || skip_add_missing_const_for_declared_bindings
                {
                    response_actions.retain(|action| {
                        if should_prune_binding_fix(action) {
                            return false;
                        }
                        if skip_add_missing_const_for_declared_bindings
                            && action.get("fixId").and_then(serde_json::Value::as_str)
                                == Some("addMissingConst")
                        {
                            return false;
                        }
                        true
                    });
                }
            }
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
                    || add_missing_const_candidate.is_some()
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

            if response_actions.is_empty()
                && error_codes.len() == 1
                && error_codes[0]
                    == tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME_DO_YOU_NEED_TO_CHANGE_YOUR_TARGET_LIBRARY_TRY_CHANGING_THE_LIB_2
                && let Some(prop_name) =
                    Self::find_property_access_name_for_missing_member_fallback(&content)
            {
                response_actions.extend(
                    self.missing_member_codefix_actions(file_path, &content, &prop_name),
                );
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

            // addMissingAsync: for error 1308 (await in sync function) or
            // assignability errors (2322/2345) where an async rewrite exists.
            if let Some(updated_content) = missing_async_content.as_ref()
                && !response_actions.iter().any(|a| {
                    a.get("fixId").and_then(serde_json::Value::as_str)
                        == Some(ADD_MISSING_ASYNC_FIX_ID)
                })
            {
                let has_1308 = error_codes.contains(&AWAIT_IN_SYNC_FUNCTION_ERROR_CODE);
                let has_assignability = error_codes.iter().any(|c| *c == 2322 || *c == 2345);
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
                if has_assignability {
                    // For assignability errors (2322/2345), prefer async at
                    // the top.
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
                && let Some(updated_content) = add_missing_const_candidate.as_ref()
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
                && includes_cannot_find_name
                && let Some((name, updated_content)) = add_missing_function_declaration_candidate
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

            // Final fallback for plain unresolved call sites (`foo(1);`).
            // The import-candidate code path emits an empty
            // `fixMissingImport` action when no candidate is available; if no
            // action carries real text changes, fall back to the
            // missing-function-declaration fix. Keeps real import fixes
            // (e.g. `useEffect`) untouched. See #3806.
            if includes_cannot_find_name
                && response_actions
                    .iter()
                    .all(Self::action_has_no_text_changes)
                && let Some((name, updated_content)) =
                    Self::apply_add_missing_function_declaration_for_plain_call_at_request(
                        &content,
                        &line_map,
                        request_span,
                    )
                && let Some((start_off, end_off, replacement)) =
                    Self::compute_minimal_edit(&content, &updated_content)
            {
                let start_pos = line_map.offset_to_position(start_off, &content);
                let end_pos = line_map.offset_to_position(end_off, &content);
                response_actions.retain(|a| {
                    a.get("fixId").and_then(serde_json::Value::as_str) != Some("fixMissingImport")
                });
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
            if !response_actions.iter().any(|action| {
                action.get("fixName").and_then(serde_json::Value::as_str) == Some("import")
            }) && error_codes
                .contains(&tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME)
                && let Some(action) = self.verbatim_commonjs_auto_import_codefix_action(
                    file_path,
                    &content,
                    &line_map,
                    request_span,
                )
            {
                response_actions.push(action);
            }
            if !response_actions.iter().any(|a| {
                a.get("fixId").and_then(serde_json::Value::as_str)
                    == Some(FIX_MISSING_TYPE_ANNOTATION_FIX_ID)
            }) {
                let isolated_fixes = Self::apply_isolated_decl_type_annotation_fix(
                    file_path,
                    &content,
                    &arena,
                    &line_map,
                    &diagnostics,
                    &error_codes,
                    request_span,
                );
                response_actions.extend(isolated_fixes);
            }
            Self::rewrite_jsdoc_import_fixes(&content, &mut response_actions);
            self.rewrite_commonjs_import_fixes(file_path, &content, &mut response_actions);
            self.rewrite_import_fixes_for_type_order(&content, &mut response_actions);
            // tsserver does NOT emit `inferFromUsage` placeholder actions for
            // JSDoc `@type {function(...)}` annotations. tsz used to inject
            // empty-changes placeholders gated on a hardcoded list of
            // conformance fixture filenames (issue #3848). The
            // filename-driven branch was a CLAUDE.md §25 anti-hardcoding
            // pattern and produced different protocol responses for the
            // same source text under different file names; deleted in
            // favor of always matching tsserver.
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

            if response_actions.is_empty()
                && !error_codes.is_empty()
                && error_codes.contains(
                    &tsz_checker::diagnostics::diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                )
                && add_missing_await_preview.is_none()
            {
                let prop_name =
                    Self::find_property_access_name_for_missing_member_fallback(&content);

                if let Some(prop_name) = prop_name {
                    response_actions.extend(
                        self.missing_member_codefix_actions(file_path, &content, &prop_name),
                    );
                }
            }
            normalize_response_actions(&mut response_actions);

            // Deduplicate by (fixId, description): when both CodeActionProvider
            // and fallback produce the same fix, keep only the first occurrence.
            // We key on the pair so that distinct actions sharing a fixId (e.g.
            // "Declare method", "Declare property", "Add index signature" all
            // under fixId "fixMissingMember") are preserved.
            // `inferFromUsage` placeholders are intentionally repeated for
            // `annotateWithTypeFromJSDoc16.ts` and similar fixtures, so keep
            // those duplicates.
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
                if fix_id == "inferFromUsage" || (fix_id.is_empty() && description.is_empty()) {
                    true
                } else {
                    dedup_seen.insert((fix_id.to_string(), description.to_string()))
                }
            });

            if error_codes.len() == 1
                && error_codes[0]
                    == tsz_checker::diagnostics::diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE
                && add_missing_await_preview.is_none()
                && !response_actions.iter().any(|action| {
                    action.get("fixId").and_then(serde_json::Value::as_str)
                        == Some("fixMissingMember")
                })
            {
                let prop_name =
                    Self::find_property_access_name_for_missing_member_fallback(&content);

                if let Some(prop_name) = prop_name {
                    response_actions.extend(
                        self.missing_member_codefix_actions(file_path, &content, &prop_name),
                    );
                }
            }

            if !auto_import_file_exclude_patterns.is_empty() {
                if response_actions.is_empty()
                    && error_codes.len() == 1
                    && error_codes[0]
                        == tsz_checker::diagnostics::diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE
                    && let Some(prop_name) =
                        Self::find_property_access_name_for_missing_member_fallback(&content)
                {
                    response_actions.extend(
                        self.missing_member_codefix_actions(file_path, &content, &prop_name),
                    );
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
        }

        if let Some(file_path) = file
            && let Some((_, _, _, content)) = self.parse_and_bind_file(file_path)
            && self
                .get_semantic_diagnostics_full(file_path, &content)
                .iter()
                .any(|d| {
                    d.code
                        == tsz_checker::diagnostics::diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE
                })
            && let Some(prop_name) =
                Self::find_property_access_name_for_missing_member_fallback(&content)
        {
            let actions = self.missing_member_codefix_actions(file_path, &content, &prop_name);
            if actions.is_empty() {
                return self.success_response(seq, request, Some(serde_json::json!([])));
            }
            return TsServerResponse {
                seq,
                msg_type: "response".to_string(),
                command: "getCodeFixes".to_string(),
                request_seq: request.seq,
                success: true,
                message: None,
                body: Some(serde_json::json!(actions)),
            };
        }

        self.success_response(seq, request, Some(serde_json::json!([])))
    }

    pub(super) fn has_potential_auto_import_symbol(
        &self,
        current_file_path: &str,
        name: &str,
    ) -> bool {
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
            .any(|m| !class_body_has_member(class_body, m.name()));
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
            .with_organize_imports_ignore_case(organize_imports_ignore_case)
            .with_new_line_override(self.new_line_character.clone());

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

        self.success_response(seq, request, Some(serde_json::json!({"changes": []})))
    }

    /// Infer the annotation type for an isolated-declarations initializer using
    /// AST-structural analysis only (no full type-checker run required).
    ///
    /// Rule: When an exported variable's initializer is a JSX expression
    /// (`JsxElement`, `JsxSelfClosingElement`, `JsxFragment`), the annotation type
    /// is `JSX.Element` — this is the standardized JSX element type defined by
    /// the JSX namespace in TypeScript's JSX support.
    fn infer_type_for_isolated_decl_initializer(
        arena: &tsz::parser::node::NodeArena,
        init_idx: tsz::parser::NodeIndex,
    ) -> Option<String> {
        use tsz::parser::syntax_kind_ext::{
            JSX_ELEMENT, JSX_FRAGMENT, JSX_SELF_CLOSING_ELEMENT, PARENTHESIZED_EXPRESSION,
        };
        const JSX_ELEMENT_TYPE: &str = "JSX.Element";

        let node = arena.get(init_idx)?;

        // JSX expressions always have type JSX.Element in TypeScript — structural
        // inspection of the node kind is sufficient; no type-checker needed.
        if node.kind == JSX_ELEMENT
            || node.kind == JSX_SELF_CLOSING_ELEMENT
            || node.kind == JSX_FRAGMENT
        {
            return Some(JSX_ELEMENT_TYPE.to_string());
        }

        // Recurse through parenthesized wrappers: `(<div/>)` → JSX.Element
        if node.kind == PARENTHESIZED_EXPRESSION {
            let paren = arena.get_parenthesized(node)?;
            if paren.expression.is_some() {
                return Self::infer_type_for_isolated_decl_initializer(arena, paren.expression);
            }
        }

        // Other initializer kinds require full type-checker inference.
        None
    }

    /// Generate code fix actions for TS9010 (variable missing type annotation)
    /// under `--isolatedDeclarations`.
    ///
    /// Returns two actions per applicable diagnostic:
    /// 1. Direct annotation: `const x: T = expr;`
    /// 2. Satisfies+cast: `const x = (expr) satisfies T as T;`
    fn apply_isolated_decl_type_annotation_fix(
        file_path: &str,
        content: &str,
        arena: &tsz::parser::node::NodeArena,
        line_map: &tsz::lsp::position::LineMap,
        diagnostics: &[tsz::checker::diagnostics::Diagnostic],
        error_codes: &[u32],
        request_span: Option<(tsz::lsp::position::Position, tsz::lsp::position::Position)>,
    ) -> Vec<serde_json::Value> {
        use tsz::parser::syntax_kind_ext::VARIABLE_DECLARATION;

        const TS9010: u32 = 9010;
        // VariableDeclaration is always within a few AST levels of its name node.
        const MAX_ANCESTOR_WALK: usize = 20;

        if !error_codes.contains(&TS9010) {
            return vec![];
        }

        // Find TS9010 diagnostic at the requested span (if any)
        let Some(diag) = diagnostics.iter().find(|d| {
            d.code == TS9010
                && request_span.is_none_or(|(start, end)| {
                    let diag_pos = line_map.offset_to_position(d.start, content);
                    let diag_end = line_map.offset_to_position(d.start + d.length, content);
                    positions_overlap(start, end, diag_pos, diag_end)
                })
        }) else {
            return vec![];
        };

        // Find the name identifier at the diagnostic start position
        let name_idx = tsz::lsp::utils::find_node_at_offset(arena, diag.start);
        if name_idx.is_none() {
            return vec![];
        }

        // Walk up to the enclosing VariableDeclaration (at most a few levels)
        let mut decl_idx = tsz::parser::NodeIndex::NONE;
        let mut current = name_idx;
        for _ in 0..MAX_ANCESTOR_WALK {
            let Some(node) = arena.get(current) else {
                break;
            };
            if node.kind == VARIABLE_DECLARATION {
                decl_idx = current;
                break;
            }
            let Some(parent) = arena.parent_of(current) else {
                break;
            };
            current = parent;
        }
        if decl_idx.is_none() {
            return vec![];
        }

        let Some(decl_node) = arena.get(decl_idx) else {
            return vec![];
        };
        let Some(decl) = arena.get_variable_declaration(decl_node) else {
            return vec![];
        };

        let init_idx = decl.initializer;
        if init_idx.is_none() {
            return vec![];
        }

        // Infer the annotation type from the initializer's AST shape
        let Some(type_string) = Self::infer_type_for_isolated_decl_initializer(arena, init_idx)
        else {
            return vec![];
        };

        // Position for the direct annotation: insert `: T` right after the name
        let Some(name_node) = arena.get(decl.name) else {
            return vec![];
        };
        let name_end_pos = line_map.offset_to_position(name_node.end, content);

        // Position for the satisfies+cast: wrap the initializer expression
        let Some(init_node) = arena.get(init_idx) else {
            return vec![];
        };

        // Skip leading whitespace/trivia to find the initializer content start
        let init_content_start = {
            let bytes = content.as_bytes();
            let mut pos = init_node.pos as usize;
            while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
                pos += 1;
            }
            pos as u32
        };
        let init_end = init_node.end;

        let init_start_pos = line_map.offset_to_position(init_content_start, content);
        let init_end_pos = line_map.offset_to_position(init_end, content);

        let fix_all_desc =
            "Add annotations of inferred types to all items with missing annotations";

        vec![
            serde_json::json!({
                "fixName": FIX_MISSING_TYPE_ANNOTATION_FIX_ID,
                "description": format!("Add annotation of type '{type_string}'"),
                "changes": [{
                    "fileName": file_path,
                    "textChanges": [{
                        "start": {
                            "line": name_end_pos.line + 1,
                            "offset": name_end_pos.character + 1
                        },
                        "end": {
                            "line": name_end_pos.line + 1,
                            "offset": name_end_pos.character + 1
                        },
                        "newText": format!(": {type_string}")
                    }]
                }],
                "fixId": FIX_MISSING_TYPE_ANNOTATION_FIX_ID,
                "fixAllDescription": fix_all_desc,
            }),
            serde_json::json!({
                "fixName": FIX_MISSING_TYPE_ANNOTATION_FIX_ID,
                "description": format!("Add satisfies and an inline type assertion with '{type_string}'"),
                "changes": [{
                    "fileName": file_path,
                    "textChanges": [
                        {
                            "start": {
                                "line": init_start_pos.line + 1,
                                "offset": init_start_pos.character + 1
                            },
                            "end": {
                                "line": init_start_pos.line + 1,
                                "offset": init_start_pos.character + 1
                            },
                            "newText": "("
                        },
                        {
                            "start": {
                                "line": init_end_pos.line + 1,
                                "offset": init_end_pos.character + 1
                            },
                            "end": {
                                "line": init_end_pos.line + 1,
                                "offset": init_end_pos.character + 1
                            },
                            "newText": format!(") satisfies {type_string} as {type_string}")
                        }
                    ]
                }],
                "fixId": FIX_MISSING_TYPE_ANNOTATION_FIX_ID,
                "fixAllDescription": fix_all_desc,
            }),
        ]
    }
}

#[cfg(test)]
#[path = "handlers_code_fixes_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "handlers_code_fixes_nested_pkg_tests.rs"]
mod nested_pkg_tests;
