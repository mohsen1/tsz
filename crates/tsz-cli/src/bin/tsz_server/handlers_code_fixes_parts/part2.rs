impl Server {
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

        // Determine the offset for the variable name. Prefer a server-generated
        // TS9010 diagnostic (exact name position); fall back to the client-supplied
        // request span when the server did not emit TS9010 (e.g. because
        // isolatedDeclarations is not enabled in the server's inferred options but
        // the client reports the diagnostic at a known span).
        let name_offset: u32 = if let Some(diag) = diagnostics.iter().find(|d| {
            d.code == TS9010
                && request_span.is_none_or(|(start, end)| {
                    let diag_pos = line_map.offset_to_position(d.start, content);
                    let diag_end = line_map.offset_to_position(d.start + d.length, content);
                    positions_overlap(start, end, diag_pos, diag_end)
                })
        }) {
            diag.start
        } else if let Some((span_start, _)) = request_span {
            // Client-provided span: convert line/col position to byte offset.
            let Some(offset) = line_map.position_to_offset(span_start, content) else {
                return vec![];
            };
            offset
        } else {
            return vec![];
        };

        // Find the name identifier at the variable name offset
        let name_idx = tsz::lsp::utils::find_node_at_offset(arena, name_offset);
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
