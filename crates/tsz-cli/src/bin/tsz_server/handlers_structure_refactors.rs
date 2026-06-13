//! Refactor, formatting, project-sync, and editor-range tsserver handlers.

use super::*;

impl Server {
    pub(crate) fn handle_get_applicable_refactors(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            // Issue #3718: tsserver accepts FileLocationOrRangeRequestArgs.
            // The position-only form sends `{ line, offset }` and the range
            // form sends `{ startLine, startOffset, endLine, endOffset }`.
            // Treat a position as a zero-length range that anchors both
            // ends at the same coordinate.
            let (start_line, start_offset, end_line, end_offset) =
                Self::parse_refactor_request_range(request)?;

            let (arena, binder, root, content) = self.parse_and_bind_file(file)?;
            let line_map = LineMap::build(&content);

            let range = Range {
                start: Position {
                    line: start_line.saturating_sub(1),
                    character: start_offset.saturating_sub(1),
                },
                end: Position {
                    line: end_line.saturating_sub(1),
                    character: end_offset.saturating_sub(1),
                },
            };

            let provider =
                CodeActionProvider::new(&arena, &binder, &line_map, file.to_string(), &content);

            let mut refactors = Vec::new();

            // Check if extract variable is applicable
            if provider.extract_variable(root, range).is_some() {
                // Issue #3803: tsc emits one extract action per *applicable*
                // scope and attaches a range. Approximate "applicable scopes"
                // by detecting whether the request's expression has an
                // enclosing function in its ancestor chain.
                let action_range = serde_json::json!({
                    "start": { "line": start_line, "offset": start_offset },
                    "end": { "line": end_line, "offset": end_offset },
                });
                let inside_function =
                    Self::request_is_inside_function(&arena, &line_map, &content, range);
                let function_actions: Vec<serde_json::Value> = if inside_function {
                    vec![
                        serde_json::json!({
                            "name": "function_scope_0",
                            "description": "Extract to function in enclosing scope",
                            "kind": "refactor.extract.function",
                            "range": action_range,
                        }),
                        serde_json::json!({
                            "name": "function_scope_1",
                            "description": "Extract to function in global scope",
                            "kind": "refactor.extract.function",
                            "range": action_range,
                        }),
                    ]
                } else {
                    vec![serde_json::json!({
                        "name": "function_scope_0",
                        "description": "Extract to function in global scope",
                        "kind": "refactor.extract.function",
                        "range": action_range,
                    })]
                };
                let constant_actions: Vec<serde_json::Value> = if inside_function {
                    vec![
                        serde_json::json!({
                            "name": "constant_scope_0",
                            "description": "Extract to constant in enclosing scope",
                            "kind": "refactor.extract.constant",
                            "range": action_range,
                        }),
                        serde_json::json!({
                            "name": "constant_scope_1",
                            "description": "Extract to constant in global scope",
                            "kind": "refactor.extract.constant",
                            "range": action_range,
                        }),
                    ]
                } else {
                    vec![serde_json::json!({
                        "name": "constant_scope_0",
                        "description": "Extract to constant in enclosing scope",
                        "kind": "refactor.extract.constant",
                        "range": action_range,
                    })]
                };
                refactors.push(serde_json::json!({
                    "name": "Extract Symbol",
                    "description": "Extract function",
                    "actions": function_actions,
                }));
                refactors.push(serde_json::json!({
                    "name": "Extract Symbol",
                    "description": "Extract constant",
                    "actions": constant_actions,
                }));
            }

            Some(serde_json::json!(refactors))
        })();

        self.success_or_empty_array(seq, request, result)
    }

    /// Parse the request's range fields, falling back to a position
    /// (`line`/`offset`) when the explicit range fields are absent. tsserver
    /// accepts `FileLocationOrRangeRequestArgs` for refactor commands; a
    /// position is treated as a zero-length range. Issue #3718.
    pub(crate) fn parse_refactor_request_range(
        request: &TsServerRequest,
    ) -> Option<(u32, u32, u32, u32)> {
        let line_only = request
            .arguments
            .get("line")
            .and_then(serde_json::Value::as_u64)
            .map(|line| line as u32);
        let offset_only = request
            .arguments
            .get("offset")
            .and_then(serde_json::Value::as_u64)
            .map(|offset| offset as u32);

        let pick = |range_key: &str, position: Option<u32>| -> Option<u32> {
            request
                .arguments
                .get(range_key)
                .and_then(serde_json::Value::as_u64)
                .map(|n| n as u32)
                .or(position)
        };

        let start_line = pick("startLine", line_only)?;
        let start_offset = pick("startOffset", offset_only)?;
        let end_line = pick("endLine", line_only)?;
        let end_offset = pick("endOffset", offset_only)?;
        Some((start_line, start_offset, end_line, end_offset))
    }

    /// Walk the AST upward from the request range looking for an
    /// enclosing function-like node (function/method/arrow/constructor/
    /// accessor). Returns `true` when one is found, `false` when the
    /// request range is at module level. Used by
    /// `handle_get_applicable_refactors` to decide which extract scopes
    /// to advertise. Issue #3803.
    fn request_is_inside_function(
        arena: &tsz::parser::node::NodeArena,
        line_map: &LineMap,
        source_text: &str,
        range: Range,
    ) -> bool {
        let Some(start_offset) = line_map.position_to_offset(range.start, source_text) else {
            return false;
        };
        let mut current = tsz::lsp::utils::find_node_at_offset(arena, start_offset);
        while current.is_some() {
            let Some(node) = arena.get(current) else {
                return false;
            };
            if node.is_function_like() {
                return true;
            }
            let Some(ext) = arena.get_extended(current) else {
                return false;
            };
            current = ext.parent;
        }
        false
    }

    pub(crate) fn handle_get_edits_for_refactor(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let refactor = request.arguments.get("refactor")?.as_str()?;
            // Issue #3718: accept either the range form (startLine etc.) or
            // a position-only form ({ line, offset }) per
            // FileLocationOrRangeRequestArgs.
            let (start_line, start_offset, end_line, end_offset) =
                Self::parse_refactor_request_range(request)?;

            let (arena, binder, root, content) = self.parse_and_bind_file(file)?;
            let line_map = LineMap::build(&content);

            let range = Range {
                start: Position {
                    line: start_line.saturating_sub(1),
                    character: start_offset.saturating_sub(1),
                },
                end: Position {
                    line: end_line.saturating_sub(1),
                    character: end_offset.saturating_sub(1),
                },
            };

            let provider =
                CodeActionProvider::new(&arena, &binder, &line_map, file.to_string(), &content);

            if refactor == "Extract Symbol" {
                let action = provider.extract_variable(root, range)?;
                let edit = action.edit?;
                let mut file_edits = Vec::new();
                for (fname, edits) in edit.changes {
                    let mut text_changes = Vec::new();
                    for e in edits {
                        text_changes.push(serde_json::json!({
                            "start": {
                                "line": e.range.start.line + 1,
                                "offset": e.range.start.character + 1
                            },
                            "end": {
                                "line": e.range.end.line + 1,
                                "offset": e.range.end.character + 1
                            },
                            "newText": e.new_text
                        }));
                    }
                    file_edits.push(serde_json::json!({
                        "fileName": fname,
                        "textChanges": text_changes
                    }));
                }
                return Some(serde_json::json!({ "edits": file_edits }));
            }

            None
        })();

        self.success_response(
            seq,
            request,
            Some(result.unwrap_or(serde_json::json!({"edits": []}))),
        )
    }

    pub(crate) fn handle_organize_imports(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request
                .arguments
                .get("scope")
                .and_then(|s| s.get("args"))
                .and_then(|a| a.get("file"))
                .and_then(|v| v.as_str())
                .or_else(|| request.arguments.get("file").and_then(|v| v.as_str()))?;

            let (arena, binder, root, content) = self.parse_and_bind_file(file)?;

            let parse_organize_imports_ignore_case = |value: &serde_json::Value| {
                value
                    .as_bool()
                    .or_else(|| value.as_str().and_then(|s| (s == "auto").then_some(true)))
            };
            let organize_imports_ignore_case = request
                .arguments
                .get("preferences")
                .and_then(|p| p.get("organizeImportsIgnoreCase"))
                .and_then(parse_organize_imports_ignore_case)
                .or_else(|| {
                    request
                        .arguments
                        .get("organizeImportsIgnoreCase")
                        .and_then(parse_organize_imports_ignore_case)
                })
                .unwrap_or(self.organize_imports_ignore_case);
            let organize_imports_type_order = request
                .arguments
                .get("preferences")
                .and_then(|p| p.get("organizeImportsTypeOrder"))
                .and_then(serde_json::Value::as_str)
                .or_else(|| {
                    request
                        .arguments
                        .get("organizeImportsTypeOrder")
                        .and_then(serde_json::Value::as_str)
                })
                .map(ToOwned::to_owned)
                .or_else(|| self.organize_imports_type_order.clone());

            let line_map = LineMap::build(&content);
            let provider =
                CodeActionProvider::new(&arena, &binder, &line_map, file.to_string(), &content)
                    .with_organize_imports_ignore_case(organize_imports_ignore_case)
                    .with_organize_imports_type_order(organize_imports_type_order);

            let action = provider.organize_imports(root)?;

            let mut text_changes = Vec::new();
            if let Some(edit) = action.edit {
                for (_fname, edits) in edit.changes {
                    for e in edits {
                        text_changes.push(serde_json::json!({
                            "start": {
                                "line": e.range.start.line + 1,
                                "offset": e.range.start.character + 1
                            },
                            "end": {
                                "line": e.range.end.line + 1,
                                "offset": e.range.end.character + 1
                            },
                            "newText": e.new_text
                        }));
                    }
                }
            }

            Some(serde_json::json!([{
                "fileName": file,
                "textChanges": text_changes
            }]))
        })();

        self.success_or_empty_array(seq, request, result)
    }

    pub(crate) fn handle_get_edits_for_file_rename(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let old_file = request.arguments.get("oldFilePath")?.as_str()?;
            let new_file = request.arguments.get("newFilePath")?.as_str()?;

            let old_path = std::path::Path::new(old_file);
            let new_path = std::path::Path::new(new_file);

            let mut file_changes: Vec<serde_json::Value> = Vec::new();

            // Scan all open files for imports that reference the renamed file
            let open_files: Vec<(String, String)> = self
                .open_files
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();

            for (dep_file, source_text) in &open_files {
                let (arena, _binder, root, _) = self.parse_and_bind_file(dep_file)?;
                let line_map = LineMap::build(source_text);
                let provider = FileRenameProvider::new(&arena, &line_map, source_text);
                let imports = provider.find_import_specifier_nodes(root);

                let dep_dir = std::path::Path::new(dep_file.as_str()).parent()?;
                let mut text_changes: Vec<serde_json::Value> = Vec::new();

                for import in &imports {
                    // Check if this import points to the old file
                    let spec = &import.current_specifier;
                    if !spec.starts_with('.') {
                        continue; // Only relative imports
                    }
                    let resolved = dep_dir.join(spec);
                    let resolved_normalized = Self::normalize_module_path(&resolved);
                    let old_normalized = Self::normalize_module_path(old_path);

                    if resolved_normalized != old_normalized {
                        continue;
                    }

                    // Compute new relative path
                    let new_rel = Self::compute_relative_import(dep_dir, new_path);
                    let quote_char = source_text
                        .get(import.range.start.character as usize..)
                        .and_then(|s| s.chars().next())
                        .unwrap_or('"');

                    text_changes.push(serde_json::json!({
                        "start": Self::lsp_to_tsserver_position(import.range.start),
                        "end": Self::lsp_to_tsserver_position(import.range.end),
                        "newText": format!("{quote_char}{new_rel}{quote_char}"),
                    }));
                }

                if !text_changes.is_empty() {
                    file_changes.push(serde_json::json!({
                        "fileName": dep_file,
                        "textChanges": text_changes,
                    }));
                }
            }

            Some(serde_json::json!(file_changes))
        })();
        self.success_or_empty_array(seq, request, result)
    }

    fn normalize_module_path(path: &std::path::Path) -> String {
        let normalized = Self::normalize_path_string(path);
        let s = normalized.as_str();
        let s = s
            .strip_suffix(".ts")
            .or_else(|| s.strip_suffix(".tsx"))
            .or_else(|| s.strip_suffix(".js"))
            .or_else(|| s.strip_suffix(".jsx"))
            .unwrap_or(s);
        s.to_string()
    }

    pub(super) fn compute_relative_import(
        from_dir: &std::path::Path,
        to_file: &std::path::Path,
    ) -> String {
        let to_stem = to_file.with_extension("");

        // Compute relative path components
        let from_parts: Vec<_> = from_dir.components().collect();
        let to_parts: Vec<_> = to_stem.components().collect();

        let mut common = 0;
        while common < from_parts.len().min(to_parts.len())
            && from_parts[common] == to_parts[common]
        {
            common += 1;
        }

        let ups = from_parts.len() - common;
        let mut parts: Vec<String> = Vec::new();
        for _ in 0..ups {
            parts.push("..".to_string());
        }
        for &comp in &to_parts[common..] {
            parts.push(comp.as_os_str().to_string_lossy().to_string());
        }

        let rel = parts.join("/");
        if rel.starts_with('.') {
            rel
        } else {
            format!("./{rel}")
        }
    }

    pub(crate) fn handle_format(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let source_text = self
                .open_files
                .get(file)
                .cloned()
                .or_else(|| std::fs::read_to_string(file).ok())?;
            let request_options = request
                .arguments
                .get("options")
                .cloned()
                .unwrap_or_default();
            let mut native_open_map = serde_json::Map::new();
            native_open_map.insert(
                file.to_string(),
                serde_json::Value::String(source_text.clone()),
            );
            if let Some(native) = self.try_native_typescript_operation(serde_json::json!({
                "op": "format",
                "file": file,
                "line": request.arguments.get("line").cloned().unwrap_or(serde_json::Value::Null),
                "offset": request.arguments.get("offset").cloned().unwrap_or(serde_json::Value::Null),
                "endLine": request.arguments.get("endLine").cloned().unwrap_or(serde_json::Value::Null),
                "endOffset": request.arguments.get("endOffset").cloned().unwrap_or(serde_json::Value::Null),
                "options": request_options,
                "openFiles": serde_json::Value::Object(native_open_map),
            })) {
                return Some(native);
            }

            let options = tsz::lsp::formatting::FormattingOptions {
                tab_size: request
                    .arguments
                    .get("options")
                    .and_then(|o| o.get("tabSize"))
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(4) as u32,
                insert_spaces: request
                    .arguments
                    .get("options")
                    .and_then(|o| o.get("insertSpaces"))
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(true),
                ..Default::default()
            };

            let range = request
                .arguments
                .get("line")
                .and_then(serde_json::Value::as_u64)
                .zip(
                    request
                        .arguments
                        .get("offset")
                        .and_then(serde_json::Value::as_u64),
                )
                .zip(
                    request
                        .arguments
                        .get("endLine")
                        .and_then(serde_json::Value::as_u64)
                        .zip(
                            request
                                .arguments
                                .get("endOffset")
                                .and_then(serde_json::Value::as_u64),
                        ),
                )
                .map(|((line, offset), (end_line, end_offset))| {
                    Range::new(
                        Position::new(
                            line.saturating_sub(1) as u32,
                            offset.saturating_sub(1) as u32,
                        ),
                        Position::new(
                            end_line.saturating_sub(1) as u32,
                            end_offset.saturating_sub(1) as u32,
                        ),
                    )
                });

            let edits_result = if let Some(range) = range {
                tsz::lsp::formatting::DocumentFormattingProvider::format_range(
                    &source_text,
                    range,
                    &options,
                )
            } else {
                tsz::lsp::formatting::DocumentFormattingProvider::format_document(
                    file,
                    &source_text,
                    &options,
                )
            };

            match edits_result {
                Ok(edits) => {
                    let line_map = LineMap::build(&source_text);
                    let body: Vec<serde_json::Value> = edits
                        .iter()
                        .map(|edit| {
                            let normalized =
                                narrow_indentation_only_edit(&source_text, &line_map, edit);
                            serde_json::json!({
                                "start": Self::lsp_to_tsserver_position(normalized.range.start),
                                "end": Self::lsp_to_tsserver_position(normalized.range.end),
                                "newText": normalized.new_text,
                            })
                        })
                        .collect();
                    Some(serde_json::json!(body))
                }
                Err(_) => Some(serde_json::json!([])),
            }
        })();
        self.success_or_empty_array(seq, request, result)
    }

    pub(crate) fn handle_format_on_key(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let source_text = self
                .open_files
                .get(file)
                .cloned()
                .or_else(|| std::fs::read_to_string(file).ok())?;
            let line = request.arguments.get("line")?.as_u64()? as u32;
            let offset = request.arguments.get("offset")?.as_u64()? as u32;
            let key = request.arguments.get("key")?.as_str()?;
            let request_options = request
                .arguments
                .get("options")
                .cloned()
                .unwrap_or_default();
            let mut native_open_map = serde_json::Map::new();
            native_open_map.insert(
                file.to_string(),
                serde_json::Value::String(source_text.clone()),
            );
            if let Some(native) = self.try_native_typescript_operation(serde_json::json!({
                "op": "formatOnKey",
                "file": file,
                "line": line,
                "offset": offset,
                "key": key,
                "options": request_options,
                "openFiles": serde_json::Value::Object(native_open_map),
            })) {
                return Some(native);
            }

            let options = tsz::lsp::formatting::FormattingOptions {
                tab_size: request
                    .arguments
                    .get("options")
                    .and_then(|o| o.get("tabSize"))
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(4) as u32,
                insert_spaces: request
                    .arguments
                    .get("options")
                    .and_then(|o| o.get("insertSpaces"))
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(true),
                ..Default::default()
            };

            // tsserver protocol uses 1-based line/offset, convert to 0-based
            let lsp_line = line.saturating_sub(1);
            let lsp_offset = offset.saturating_sub(1);

            match tsz::lsp::formatting::DocumentFormattingProvider::format_on_key(
                &source_text,
                lsp_line,
                lsp_offset,
                key,
                &options,
            ) {
                Ok(edits) => {
                    let body: Vec<serde_json::Value> = edits
                        .iter()
                        .map(|edit| {
                            serde_json::json!({
                                "start": Self::lsp_to_tsserver_position(edit.range.start),
                                "end": Self::lsp_to_tsserver_position(edit.range.end),
                                "newText": edit.new_text,
                            })
                        })
                        .collect();
                    Some(serde_json::json!(body))
                }
                Err(_) => Some(serde_json::json!([])),
            }
        })();
        self.success_or_empty_array(seq, request, result)
    }

    pub(crate) fn find_nearest_tsconfig(file: &str) -> Option<String> {
        let mut current = std::path::Path::new(file).parent();
        while let Some(dir) = current {
            for name in ["tsconfig.json", "jsconfig.json"] {
                let config_path = dir.join(name);
                if config_path.exists() {
                    return Some(config_path.to_string_lossy().to_string());
                }
            }
            current = dir.parent();
        }
        None
    }

    pub(crate) fn handle_reload(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        // Clear caches so next request re-parses everything
        self.lib_cache.clear();
        self.unified_lib_cache = None;

        let reload_finished = if let Some(file) = request
            .arguments
            .get("file")
            .and_then(|value| value.as_str())
        {
            let source_path = request
                .arguments
                .get("tmpfile")
                .and_then(|value| value.as_str())
                .unwrap_or(file);
            if let Ok(content) = std::fs::read_to_string(source_path) {
                self.open_files.insert(file.to_string(), content);
                true
            } else {
                false
            }
        } else {
            // Re-read all open files for reload-project style requests.
            let paths: Vec<String> = self.open_files.keys().cloned().collect();
            for path in &paths {
                if let Ok(content) = std::fs::read_to_string(path) {
                    self.open_files.insert(path.clone(), content);
                }
            }
            true
        };

        self.success_response(
            seq,
            request,
            Some(serde_json::json!({ "reloadFinished": reload_finished })),
        )
    }

    pub(crate) fn handle_reload_projects(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        self.lib_cache.clear();
        self.unified_lib_cache = None;

        let paths: Vec<String> = self.open_files.keys().cloned().collect();
        for path in &paths {
            if let Ok(content) = std::fs::read_to_string(path) {
                self.open_files.insert(path.clone(), content);
            }
        }

        self.success_response(seq, request, None)
    }

    pub(crate) fn handle_compiler_options_for_inferred(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let options = request
            .arguments
            .get("options")
            .filter(|value| value.is_object())
            .or_else(|| {
                request
                    .arguments
                    .get("compilerOptions")
                    .filter(|value| value.is_object())
            })
            .or_else(|| request.arguments.is_object().then_some(&request.arguments));
        self.apply_inferred_project_options(options);
        self.success_response(seq, request, Some(serde_json::json!(true)))
    }

    pub(crate) fn handle_external_project(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        match request.command.as_str() {
            "openExternalProject" => {
                self.apply_inferred_project_options(request.arguments.get("options"));
                let project_name = request
                    .arguments
                    .get("projectFileName")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("")
                    .to_string();

                let mut tracked_files = Vec::new();
                if let Some(root_files) = request
                    .arguments
                    .get("rootFiles")
                    .and_then(serde_json::Value::as_array)
                {
                    for entry in root_files {
                        let Some(file_name) = entry.get("fileName").and_then(|v| v.as_str()) else {
                            continue;
                        };
                        let content = entry
                            .get("content")
                            .and_then(serde_json::Value::as_str)
                            .map(std::string::ToString::to_string)
                            .or_else(|| std::fs::read_to_string(file_name).ok());
                        if let Some(content) = content {
                            self.open_files.insert(file_name.to_string(), content);
                        }
                        tracked_files.push(file_name.to_string());
                    }
                }
                if !project_name.is_empty() {
                    self.external_project_files
                        .insert(project_name, tracked_files);
                }
            }
            "openExternalProjects" => {
                if let Some(projects) = request
                    .arguments
                    .get("projects")
                    .and_then(serde_json::Value::as_array)
                {
                    for project in projects {
                        self.apply_inferred_project_options(project.get("options"));
                        let project_name = project
                            .get("projectFileName")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("")
                            .to_string();

                        let mut tracked_files = Vec::new();
                        if let Some(root_files) = project
                            .get("rootFiles")
                            .and_then(serde_json::Value::as_array)
                        {
                            for entry in root_files {
                                let Some(file_name) =
                                    entry.get("fileName").and_then(|v| v.as_str())
                                else {
                                    continue;
                                };
                                let content = entry
                                    .get("content")
                                    .and_then(serde_json::Value::as_str)
                                    .map(std::string::ToString::to_string)
                                    .or_else(|| std::fs::read_to_string(file_name).ok());
                                if let Some(content) = content {
                                    self.open_files.insert(file_name.to_string(), content);
                                }
                                tracked_files.push(file_name.to_string());
                            }
                        }
                        if !project_name.is_empty() {
                            self.external_project_files
                                .insert(project_name, tracked_files);
                        }
                    }
                }
            }
            "closeExternalProject" => {
                if let Some(project_name) = request
                    .arguments
                    .get("projectFileName")
                    .and_then(serde_json::Value::as_str)
                    && let Some(files) = self.external_project_files.remove(project_name)
                {
                    for file in files {
                        let still_owned_elsewhere = self
                            .external_project_files
                            .values()
                            .any(|other_files| other_files.iter().any(|p| p == &file));
                        if !still_owned_elsewhere {
                            self.open_files.remove(&file);
                        }
                    }
                }
            }
            _ => {}
        }

        let body = match request.command.as_str() {
            "openExternalProject" | "openExternalProjects" => Some(serde_json::json!(true)),
            _ => None,
        };
        self.success_response(seq, request, body)
    }

    pub(crate) fn handle_synchronize_project_list(
        &self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let include_redirect_info = request
            .arguments
            .get("includeProjectReferenceRedirectInfo")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let mut body: Vec<serde_json::Value> = Vec::new();

        let mut projects: Vec<(&String, &Vec<String>)> =
            self.external_project_files.iter().collect();
        projects.sort_by_key(|(left, _)| *left);

        for (project_name, files) in projects {
            body.push(Self::synchronize_project_list_entry(
                project_name,
                false,
                serde_json::json!({}),
                files.clone(),
                include_redirect_info,
            ));
        }

        let external_files: rustc_hash::FxHashSet<String> = self
            .external_project_files
            .values()
            .flat_map(|files| files.iter().cloned())
            .collect();
        let mut configured_projects: std::collections::BTreeMap<String, serde_json::Value> =
            std::collections::BTreeMap::new();
        let mut inferred_roots: Vec<String> = Vec::new();

        let mut open_files: Vec<&String> = self.open_files.keys().collect();
        open_files.sort();
        for file in open_files {
            if external_files.contains(file) || !Self::is_supported_project_source_file(file) {
                continue;
            }
            match self.find_project_config_file(file) {
                Some(config_path) => {
                    configured_projects
                        .entry(config_path.clone())
                        .or_insert_with(|| {
                            let options = self
                                .read_config_json(&config_path)
                                .and_then(|config| config.get("compilerOptions").cloned())
                                .unwrap_or_else(|| serde_json::json!({}));
                            let (_, file_names) = self.compute_project_info(file);
                            Self::synchronize_project_list_entry(
                                &config_path,
                                false,
                                options,
                                file_names,
                                include_redirect_info,
                            )
                        });
                }
                None => inferred_roots.push(file.clone()),
            }
        }

        body.extend(configured_projects.into_values());

        if !inferred_roots.is_empty() {
            let mut file_names: Vec<String> = Vec::new();
            let (lib_names, no_lib, _) = self.inferred_project_info(&inferred_roots[0]);
            if !no_lib {
                file_names
                    .extend(self.resolve_virtual_lib_files(&lib_names, Some(&inferred_roots[0])));
            }

            let mut visited: rustc_hash::FxHashSet<String> = rustc_hash::FxHashSet::default();
            let mut project_files = Vec::new();
            for root in inferred_roots {
                self.collect_reachable_files(&root, &mut visited, &mut project_files);
            }
            project_files.sort();
            project_files.dedup();
            file_names.extend(project_files);

            body.push(Self::synchronize_project_list_entry(
                "/dev/null/inferredProject1*",
                true,
                self.inferred_project_options_json(),
                file_names,
                include_redirect_info,
            ));
        }

        self.success_response(seq, request, Some(serde_json::json!(body)))
    }

    fn synchronize_project_list_entry(
        project_name: &str,
        is_inferred: bool,
        options: serde_json::Value,
        files: Vec<String>,
        include_redirect_info: bool,
    ) -> serde_json::Value {
        let files: Vec<serde_json::Value> = if include_redirect_info {
            files
                .iter()
                .map(|file_name| {
                    serde_json::json!({
                        "fileName": file_name,
                        "isSourceOfProjectReferenceRedirect": false,
                    })
                })
                .collect()
        } else {
            files
                .iter()
                .map(|file_name| serde_json::json!(file_name))
                .collect()
        };

        serde_json::json!({
            "info": {
                "projectName": project_name,
                "isInferred": is_inferred,
                "version": 1,
                "options": options,
                "languageServiceDisabled": false,
            },
            "files": files,
            "projectErrors": [],
        })
    }

    fn inferred_project_options_json(&self) -> serde_json::Value {
        let mut options = serde_json::Map::new();
        let (lib, target, no_lib) = match self.inferred_projectinfo_options.as_ref() {
            Some(opts) => (opts.lib.as_ref(), opts.target.as_ref(), opts.no_lib),
            None => (
                self.inferred_check_options.lib.as_ref(),
                self.inferred_check_options.target.as_ref(),
                self.inferred_check_options.no_lib,
            ),
        };

        if let Some(lib) = lib {
            options.insert("lib".to_string(), serde_json::json!(lib));
        }
        if let Some(target) = target {
            options.insert("target".to_string(), serde_json::json!(target));
        }
        if no_lib {
            options.insert("noLib".to_string(), serde_json::json!(true));
        }
        if let Some(module) = self.inferred_check_options.module.as_ref() {
            options.insert("module".to_string(), serde_json::json!(module));
        }
        if self.inferred_check_options.allow_js {
            options.insert("allowJs".to_string(), serde_json::json!(true));
        }
        if self.inferred_check_options.check_js {
            options.insert("checkJs".to_string(), serde_json::json!(true));
        }

        serde_json::Value::Object(options)
    }

    pub(crate) fn handle_inlay_hints(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let (arena, binder, root, source_text) = self.parse_and_bind_file(file)?;
            let line_map = LineMap::build(&source_text);
            let interner = TypeInterner::new();
            let provider = InlayHintsProvider::new(
                &arena,
                &binder,
                &line_map,
                &source_text,
                &interner,
                file.to_string(),
            );

            let protocol_span = request
                .arguments
                .get("start")
                .and_then(serde_json::Value::as_u64)
                .zip(
                    request
                        .arguments
                        .get("length")
                        .and_then(serde_json::Value::as_u64),
                )
                .map(|(start, length)| {
                    let source_len = source_text.len() as u64;
                    let start = start.min(source_len) as u32;
                    let end = start
                        .saturating_add(length.min(u32::MAX as u64) as u32)
                        .min(source_text.len() as u32);
                    (start, end)
                });

            let range = if let Some((start, end)) = protocol_span {
                Range::new(
                    line_map.offset_to_position(start, &source_text),
                    line_map.offset_to_position(end, &source_text),
                )
            } else {
                let start = request
                    .arguments
                    .get("startLine")
                    .and_then(serde_json::Value::as_u64)
                    .zip(
                        request
                            .arguments
                            .get("startOffset")
                            .and_then(serde_json::Value::as_u64),
                    )
                    .map_or(Position::new(0, 0), |(line, offset)| {
                        Self::tsserver_to_lsp_position(line as u32, offset as u32)
                    });
                let end = request
                    .arguments
                    .get("endLine")
                    .and_then(serde_json::Value::as_u64)
                    .zip(
                        request
                            .arguments
                            .get("endOffset")
                            .and_then(serde_json::Value::as_u64),
                    )
                    .map_or(Position::new(u32::MAX, u32::MAX), |(line, offset)| {
                        Self::tsserver_to_lsp_position(line as u32, offset as u32)
                    });
                Range::new(start, end)
            };

            let hints = provider.provide_inlay_hints(root, range);
            // tsserver default for `includeInlayParameterNameHints` is `"none"`:
            // parameter hints are suppressed unless the client explicitly opts
            // in via `configure`. Type/Generic hints are unaffected by this
            // preference. See #3793.
            let parameter_hints_enabled = matches!(
                self.include_inlay_parameter_name_hints.as_deref(),
                Some("literals") | Some("all")
            );
            let body: Vec<serde_json::Value> = hints
                .iter()
                .filter(|hint| {
                    if matches!(hint.kind, InlayHintKind::Parameter) && !parameter_hints_enabled {
                        return false;
                    }
                    protocol_span.is_none_or(|(start, end)| {
                        line_map
                            .position_to_offset(hint.position, &source_text)
                            .is_some_and(|position| position >= start && position < end)
                    })
                })
                .map(|hint| {
                    let kind = match hint.kind {
                        InlayHintKind::Parameter => "Parameter",
                        InlayHintKind::Type => "Type",
                        InlayHintKind::Generic => "Enum",
                    };
                    // tsserver-shape parameter hints carry no trailing space in
                    // `text` and don't include `whitespaceBefore` (the default
                    // is `false`, so the field is omitted). See #3793.
                    let text = if matches!(hint.kind, InlayHintKind::Parameter) {
                        hint.label.trim_end_matches(' ').to_string()
                    } else {
                        hint.label.clone()
                    };
                    serde_json::json!({
                        "text": text,
                        "position": Self::lsp_to_tsserver_position(hint.position),
                        "kind": kind,
                        "whitespaceAfter": true,
                    })
                })
                .collect();
            Some(serde_json::json!(body))
        })();
        self.success_or_empty_array(seq, request, result)
    }

    pub(crate) fn handle_selection_range(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let (arena, _binder, _root, source_text) = self.parse_and_bind_file(file)?;
            let line_map = LineMap::build(&source_text);
            let provider = SelectionRangeProvider::new(&arena, &line_map, &source_text);

            let locations = request.arguments.get("locations")?.as_array()?;
            let positions: Vec<Position> = locations
                .iter()
                .filter_map(|loc| {
                    let line = loc.get("line")?.as_u64()? as u32;
                    let offset = loc.get("offset")?.as_u64()? as u32;
                    Some(Self::tsserver_to_lsp_position(line, offset))
                })
                .collect();

            let ranges = provider.get_selection_ranges(&positions);
            let full_protocol = request.command.ends_with("-full");

            fn selection_range_to_json(
                sr: &tsz::lsp::editor_ranges::selection_range::SelectionRange,
                line_map: &LineMap,
                source_text: &str,
                full_protocol: bool,
            ) -> serde_json::Value {
                let text_span = if full_protocol {
                    let start = line_map
                        .position_to_offset(sr.range.start, source_text)
                        .unwrap_or(0);
                    let end = line_map
                        .position_to_offset(sr.range.end, source_text)
                        .unwrap_or(start);
                    serde_json::json!({
                        "start": start,
                        "length": end.saturating_sub(start),
                    })
                } else {
                    serde_json::json!({
                        "start": {
                            "line": sr.range.start.line + 1,
                            "offset": sr.range.start.character + 1,
                        },
                        "end": {
                            "line": sr.range.end.line + 1,
                            "offset": sr.range.end.character + 1,
                        },
                    })
                };
                if let Some(ref parent) = sr.parent {
                    serde_json::json!({
                        "textSpan": text_span,
                        "parent": selection_range_to_json(parent, line_map, source_text, full_protocol),
                    })
                } else {
                    serde_json::json!({
                        "textSpan": text_span,
                    })
                }
            }

            let body: Vec<serde_json::Value> = ranges
                .iter()
                .map(|opt_sr| {
                    opt_sr
                        .as_ref()
                        .map(|sr| {
                            selection_range_to_json(sr, &line_map, &source_text, full_protocol)
                        })
                        .unwrap_or(serde_json::json!(null))
                })
                .collect();
            Some(serde_json::json!(body))
        })();
        self.success_or_empty_array(seq, request, result)
    }

    pub(crate) fn handle_linked_editing_range(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let (file, line, offset) = Self::extract_file_position(&request.arguments)?;
            let (arena, _binder, _root, source_text) = self.parse_and_bind_file(&file)?;
            let line_map = LineMap::build(&source_text);
            let position = Self::tsserver_to_lsp_position(line, offset);
            let provider = LinkedEditingProvider::new(&arena, &line_map, &source_text);
            let linked = provider.provide_linked_editing_ranges(_root, position)?;
            let ranges: Vec<serde_json::Value> = linked
                .ranges
                .iter()
                .map(|r| {
                    serde_json::json!({
                        "start": Self::lsp_to_tsserver_position(r.start),
                        "end": Self::lsp_to_tsserver_position(r.end),
                    })
                })
                .collect();
            Some(serde_json::json!({
                "ranges": ranges,
                "wordPattern": linked.word_pattern,
            }))
        })();
        self.success_response(seq, request, result)
    }
}
