//! Structural, format, and miscellaneous handlers for tsz-server.
//!
//! Handles formatting, inlay hints, selection ranges, call hierarchy,
//! outlining spans, brace matching, refactoring stubs, and related commands.

use super::{Server, TsServerRequest, TsServerResponse};
use tsz::emitter::Printer;
use tsz::lsp::code_actions::CodeActionProvider;
use tsz::lsp::editor_decorations::inlay_hints::{InlayHintKind, InlayHintsProvider};
use tsz::lsp::editor_ranges::folding::FoldingRangeProvider;
use tsz::lsp::editor_ranges::selection_range::SelectionRangeProvider;
use tsz::lsp::hierarchy::call_hierarchy::CallHierarchyProvider;
use tsz::lsp::highlighting::semantic_tokens::SemanticTokensProvider;
use tsz::lsp::position::{LineMap, Position, Range};
use tsz::lsp::rename::file_rename::FileRenameProvider;
use tsz::lsp::rename::linked_editing::LinkedEditingProvider;
use tsz_solver::TypeInterner;

impl Server {
    fn tsserver_call_hierarchy_name_kind(name: &str, kind: &str) -> (String, String) {
        if kind == "file" {
            return (name.to_string(), "script".to_string());
        }
        if kind == "property" {
            if let Some(stripped) = name.strip_prefix("get ") {
                return (stripped.to_string(), "getter".to_string());
            }
            if let Some(stripped) = name.strip_prefix("set ") {
                return (stripped.to_string(), "setter".to_string());
            }
        }
        (name.to_string(), kind.to_string())
    }

    fn call_hierarchy_probe_positions(
        line_map: &LineMap,
        source_text: &str,
        position: Position,
    ) -> Vec<Position> {
        let Some(base_offset) = line_map.position_to_offset(position, source_text) else {
            return vec![position];
        };

        let len = source_text.len() as u32;
        let bytes = source_text.as_bytes();
        let mut positions = vec![position];

        // Fourslash call-hierarchy markers are often comment-based (`/**/foo`).
        // Probe just after the comment terminator to resolve the intended token.
        if base_offset + 1 < len
            && bytes[base_offset as usize] == b'/'
            && bytes[(base_offset + 1) as usize] == b'*'
        {
            let mut probe = base_offset + 2;
            while probe + 1 < len {
                if bytes[probe as usize] == b'*' && bytes[(probe + 1) as usize] == b'/' {
                    probe += 2;
                    break;
                }
                probe += 1;
            }
            while probe < len && bytes[probe as usize].is_ascii_whitespace() {
                probe += 1;
            }
            if probe < len {
                positions.push(line_map.offset_to_position(probe, source_text));
            }
        }

        if base_offset < len {
            positions.push(
                line_map.offset_to_position(base_offset.saturating_add(1).min(len), source_text),
            );
        }
        if base_offset > 0 {
            positions.push(line_map.offset_to_position(base_offset - 1, source_text));
        }

        positions
    }

    fn apply_inferred_project_options(&mut self, options: Option<&serde_json::Value>) {
        if let Some(options) = options {
            self.allow_importing_ts_extensions = options
                .get("allowImportingTsExtensions")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            self.inferred_module_is_none_for_projects = options
                .get("module")
                .is_some_and(Self::inferred_module_option_is_none);
            self.auto_imports_allowed_for_inferred_projects =
                Self::inferred_auto_imports_allowed(options);
        }
    }

    pub(crate) fn inferred_auto_imports_allowed(options: &serde_json::Value) -> bool {
        let module_none = options
            .get("module")
            .is_some_and(Self::inferred_module_option_is_none);
        if !module_none {
            return true;
        }

        options
            .get("target")
            .is_some_and(Self::inferred_target_supports_import_syntax)
    }

    fn inferred_module_option_is_none(value: &serde_json::Value) -> bool {
        if let Some(v) = value.as_str() {
            return v.eq_ignore_ascii_case("none") || v.parse::<i64>().ok() == Some(0);
        }
        value.as_i64() == Some(0)
    }

    fn inferred_target_supports_import_syntax(value: &serde_json::Value) -> bool {
        if let Some(target) = value.as_str() {
            if let Ok(numeric_target) = target.parse::<i64>() {
                return numeric_target >= 2;
            }

            return target.eq_ignore_ascii_case("es6")
                || target.eq_ignore_ascii_case("es2015")
                || target.eq_ignore_ascii_case("es2016")
                || target.eq_ignore_ascii_case("es2017")
                || target.eq_ignore_ascii_case("es2018")
                || target.eq_ignore_ascii_case("es2019")
                || target.eq_ignore_ascii_case("es2020")
                || target.eq_ignore_ascii_case("es2021")
                || target.eq_ignore_ascii_case("es2022")
                || target.eq_ignore_ascii_case("es2023")
                || target.eq_ignore_ascii_case("es2024")
                || target.eq_ignore_ascii_case("esnext")
                || target.eq_ignore_ascii_case("latest");
        }

        value.as_i64().is_some_and(|target| target >= 2)
    }

    pub(crate) fn handle_get_supported_code_fixes(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let codes: Vec<String> = tsz::lsp::code_actions::CodeFixRegistry::supported_error_codes()
            .iter()
            .map(std::string::ToString::to_string)
            .collect();
        self.stub_response(seq, request, Some(serde_json::json!(codes)))
    }

    pub(crate) fn handle_apply_code_action_command(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let body = if request
            .arguments
            .get("command")
            .is_some_and(serde_json::Value::is_array)
        {
            serde_json::json!([])
        } else {
            serde_json::json!({
                "successMessage": ""
            })
        };
        self.stub_response(seq, request, Some(body))
    }

    pub(crate) fn handle_encoded_semantic_classifications_full(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let (arena, binder, root, source_text) = self.parse_and_bind_file(file)?;
            let line_map = LineMap::build(&source_text);
            let mut provider =
                SemanticTokensProvider::new(&arena, &binder, &line_map, &source_text);
            let tokens = provider.get_semantic_tokens(root);
            Some(serde_json::json!({
                "spans": tokens,
                "endOfLineState": 0,
            }))
        })();
        self.stub_response(
            seq,
            request,
            Some(result.unwrap_or(serde_json::json!({"spans": [], "endOfLineState": 0}))),
        )
    }

    pub(crate) fn handle_emit_output(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let (arena, _binder, root, source_text) = self.parse_and_bind_file(file)?;

            let mut printer = Printer::with_source_text_len(&arena, source_text.len());
            printer.set_source_text(&source_text);
            printer.emit(root);
            let output = printer.take_output();

            let out_name = file
                .strip_suffix(".ts")
                .or_else(|| file.strip_suffix(".tsx"))
                .map(|base| format!("{base}.js"))
                .unwrap_or_else(|| format!("{file}.js"));

            Some(serde_json::json!({
                "outputFiles": [{
                    "name": out_name,
                    "text": output,
                    "writeByteOrderMark": false,
                }],
                "emitSkipped": false,
            }))
        })();
        self.stub_response(
            seq,
            request,
            Some(result.unwrap_or(serde_json::json!({"outputFiles": [], "emitSkipped": true}))),
        )
    }

    pub(crate) fn handle_get_applicable_refactors(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let start_line = request.arguments.get("startLine")?.as_u64()? as u32;
            let start_offset = request.arguments.get("startOffset")?.as_u64()? as u32;
            let end_line = request.arguments.get("endLine")?.as_u64()? as u32;
            let end_offset = request.arguments.get("endOffset")?.as_u64()? as u32;

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
                refactors.push(serde_json::json!({
                    "name": "Extract Symbol",
                    "description": "Extract expression to variable",
                    "actions": [{
                        "name": "constant_extractedConstant",
                        "description": "Extract to constant in enclosing scope"
                    }]
                }));
            }

            Some(serde_json::json!(refactors))
        })();

        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    pub(crate) fn handle_get_edits_for_refactor(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let refactor = request.arguments.get("refactor")?.as_str()?;
            let start_line = request.arguments.get("startLine")?.as_u64()? as u32;
            let start_offset = request.arguments.get("startOffset")?.as_u64()? as u32;
            let end_line = request.arguments.get("endLine")?.as_u64()? as u32;
            let end_offset = request.arguments.get("endOffset")?.as_u64()? as u32;

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

        self.stub_response(
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
            let provider =
                CodeActionProvider::new(&arena, &binder, &line_map, file.to_string(), &content)
                    .with_organize_imports_ignore_case(organize_imports_ignore_case);

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

        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
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
        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    fn normalize_module_path(path: &std::path::Path) -> String {
        let s = path.to_string_lossy();
        let s = s
            .strip_suffix(".ts")
            .or_else(|| s.strip_suffix(".tsx"))
            .or_else(|| s.strip_suffix(".js"))
            .or_else(|| s.strip_suffix(".jsx"))
            .unwrap_or(&s);
        s.to_string()
    }

    fn compute_relative_import(from_dir: &std::path::Path, to_file: &std::path::Path) -> String {
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
            let source_text = self.open_files.get(file)?.clone();

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

            match tsz::lsp::formatting::DocumentFormattingProvider::format_document(
                file,
                &source_text,
                &options,
            ) {
                Ok(edits) => {
                    let line_map = LineMap::build(&source_text);
                    let body: Vec<serde_json::Value> = edits
                        .iter()
                        .map(|edit| {
                            let (normalized_range, normalized_text) =
                                Self::narrow_to_indentation_only_edit_if_possible(
                                    &source_text,
                                    &line_map,
                                    edit,
                                );
                            serde_json::json!({
                                "start": Self::lsp_to_tsserver_position(normalized_range.start),
                                "end": Self::lsp_to_tsserver_position(normalized_range.end),
                                "newText": normalized_text,
                            })
                        })
                        .collect();
                    Some(serde_json::json!(body))
                }
                Err(_) => Some(serde_json::json!([])),
            }
        })();
        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    fn narrow_to_indentation_only_edit_if_possible(
        source_text: &str,
        line_map: &LineMap,
        edit: &tsz::lsp::formatting::TextEdit,
    ) -> (Range, String) {
        let Some(start_off) = line_map.position_to_offset(edit.range.start, source_text) else {
            return (edit.range, edit.new_text.clone());
        };
        let Some(end_off) = line_map.position_to_offset(edit.range.end, source_text) else {
            return (edit.range, edit.new_text.clone());
        };
        if start_off >= end_off {
            return (edit.range, edit.new_text.clone());
        }

        let Some(old_text) = source_text.get(start_off as usize..end_off as usize) else {
            return (edit.range, edit.new_text.clone());
        };
        if old_text.contains('\n') || old_text.contains('\r') {
            return (edit.range, edit.new_text.clone());
        }
        if edit.new_text.contains('\n') || edit.new_text.contains('\r') {
            return (edit.range, edit.new_text.clone());
        }

        let mut prefix = 0usize;
        for ((old_idx, old_ch), (_, new_ch)) in
            old_text.char_indices().zip(edit.new_text.char_indices())
        {
            if old_ch != new_ch {
                break;
            }
            prefix = old_idx + old_ch.len_utf8();
        }

        let old_after_prefix = &old_text[prefix..];
        let new_after_prefix = &edit.new_text[prefix..];

        let mut old_suffix_bytes = 0usize;
        let mut new_suffix_bytes = 0usize;
        let mut old_rev = old_after_prefix.char_indices().rev();
        let mut new_rev = new_after_prefix.char_indices().rev();
        while let (Some((old_idx, old_ch)), Some((new_idx, new_ch))) =
            (old_rev.next(), new_rev.next())
        {
            if old_ch != new_ch {
                break;
            }
            old_suffix_bytes = old_after_prefix.len() - old_idx;
            new_suffix_bytes = new_after_prefix.len() - new_idx;
        }

        let old_mid_end = old_text.len().saturating_sub(old_suffix_bytes);
        let new_mid_end = edit.new_text.len().saturating_sub(new_suffix_bytes);
        let narrowed_start = start_off + prefix as u32;
        let narrowed_end = start_off + old_mid_end as u32;
        let start_pos = line_map.offset_to_position(narrowed_start, source_text);
        let end_pos = line_map.offset_to_position(narrowed_end, source_text);
        let new_text = edit.new_text[prefix..new_mid_end].to_string();

        if narrowed_start == start_off && narrowed_end == end_off && new_text == edit.new_text {
            return (edit.range, edit.new_text.clone());
        }

        (Range::new(start_pos, end_pos), new_text)
    }

    pub(crate) fn handle_format_on_key(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let source_text = self.open_files.get(file)?.clone();
            let line = request.arguments.get("line")?.as_u64()? as u32;
            let offset = request.arguments.get("offset")?.as_u64()? as u32;
            let key = request.arguments.get("key")?.as_str()?;

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
        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    pub(crate) fn handle_project_info(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let config_file_name = Self::find_nearest_tsconfig(file).unwrap_or_default();
            let file_names: Vec<&str> = self
                .open_files
                .keys()
                .map(std::string::String::as_str)
                .collect();
            Some(serde_json::json!({
                "configFileName": config_file_name,
                "fileNames": file_names,
            }))
        })();
        self.stub_response(
            seq,
            request,
            Some(result.unwrap_or(serde_json::json!({"configFileName": "", "fileNames": []}))),
        )
    }

    pub(super) fn find_nearest_tsconfig(file: &str) -> Option<String> {
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

        // Re-read open files from disk
        let paths: Vec<String> = self.open_files.keys().cloned().collect();
        for path in &paths {
            if let Ok(content) = std::fs::read_to_string(path) {
                self.open_files.insert(path.clone(), content);
            }
        }

        self.stub_response(seq, request, Some(serde_json::json!(true)))
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
        self.stub_response(seq, request, Some(serde_json::json!(true)))
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

        self.stub_response(seq, request, None)
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

            // Extract the range from arguments, default to entire file
            let start = match request
                .arguments
                .get("start")
                .and_then(serde_json::Value::as_u64)
            {
                Some(_) => {
                    let line = request
                        .arguments
                        .get("startLine")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(1) as u32;
                    let offset = request
                        .arguments
                        .get("startOffset")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(1) as u32;
                    Self::tsserver_to_lsp_position(line, offset)
                }
                None => Position::new(0, 0),
            };
            let end = match request
                .arguments
                .get("end")
                .and_then(serde_json::Value::as_u64)
            {
                Some(_) => {
                    let line = request
                        .arguments
                        .get("endLine")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(u32::MAX as u64) as u32;
                    let offset = request
                        .arguments
                        .get("endOffset")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(u32::MAX as u64) as u32;
                    Self::tsserver_to_lsp_position(line, offset)
                }
                None => Position::new(u32::MAX, u32::MAX),
            };
            let range = Range::new(start, end);

            let hints = provider.provide_inlay_hints(root, range);
            let body: Vec<serde_json::Value> = hints
                .iter()
                .map(|hint| {
                    let kind = match hint.kind {
                        InlayHintKind::Parameter => "Parameter",
                        InlayHintKind::Type => "Type",
                        InlayHintKind::Generic => "Enum",
                    };
                    serde_json::json!({
                        "text": hint.label,
                        "position": Self::lsp_to_tsserver_position(hint.position),
                        "kind": kind,
                        "whitespaceBefore": false,
                        "whitespaceAfter": true,
                    })
                })
                .collect();
            Some(serde_json::json!(body))
        })();
        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
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

            fn selection_range_to_json(
                sr: &tsz::lsp::editor_ranges::selection_range::SelectionRange,
            ) -> serde_json::Value {
                let text_span = serde_json::json!({
                    "start": {
                        "line": sr.range.start.line + 1,
                        "offset": sr.range.start.character + 1,
                    },
                    "end": {
                        "line": sr.range.end.line + 1,
                        "offset": sr.range.end.character + 1,
                    },
                });
                if let Some(ref parent) = sr.parent {
                    serde_json::json!({
                        "textSpan": text_span,
                        "parent": selection_range_to_json(parent),
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
                        .map(selection_range_to_json)
                        .unwrap_or(serde_json::json!(null))
                })
                .collect();
            Some(serde_json::json!(body))
        })();
        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
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
        self.stub_response(seq, request, result)
    }

    pub(crate) fn handle_prepare_call_hierarchy(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let (file, line, offset) = Self::extract_file_position(&request.arguments)?;
            let (arena, binder, root, source_text) = self.parse_and_bind_file(&file)?;
            let line_map = LineMap::build(&source_text);
            let position = Self::tsserver_to_lsp_position(line, offset);
            let provider =
                CallHierarchyProvider::new(&arena, &binder, &line_map, file, &source_text);
            let mut item = None;
            for probe in Self::call_hierarchy_probe_positions(&line_map, &source_text, position) {
                item = provider.prepare(root, probe);
                if item.is_some() {
                    break;
                }
            }
            let item = item?;
            let raw_kind = format!("{:?}", item.kind).to_lowercase();
            let (name, kind) = Self::tsserver_call_hierarchy_name_kind(&item.name, &raw_kind);
            let mut body_item = serde_json::json!({
                "name": name,
                "kind": kind,
                "file": item.uri,
                "span": {
                    "start": Self::lsp_to_tsserver_position(item.range.start),
                    "end": Self::lsp_to_tsserver_position(item.range.end),
                },
                "selectionSpan": {
                    "start": Self::lsp_to_tsserver_position(item.selection_range.start),
                    "end": Self::lsp_to_tsserver_position(item.selection_range.end),
                },
            });
            if let Some(container_name) = item.container_name {
                body_item["containerName"] = serde_json::json!(container_name);
            }
            Some(serde_json::json!([body_item]))
        })();
        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    pub(crate) fn handle_call_hierarchy(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let (file, line, offset) = Self::extract_file_position(&request.arguments)?;
            let (arena, binder, root, source_text) = self.parse_and_bind_file(&file)?;
            let line_map = LineMap::build(&source_text);
            let position = Self::tsserver_to_lsp_position(line, offset);
            let provider =
                CallHierarchyProvider::new(&arena, &binder, &line_map, file, &source_text);

            let is_incoming = request.command == "provideCallHierarchyIncomingCalls";
            // TypeScript treats absolute position 0 as a source-file call hierarchy query.
            // In tsserver protocol this is line:1/offset:1, and should not probe into
            // adjacent offsets to resolve the first identifier token.
            let is_file_start_query = line == 1 && offset == 1;
            let positions = if is_file_start_query {
                vec![position]
            } else {
                Self::call_hierarchy_probe_positions(&line_map, &source_text, position)
            };

            if is_incoming {
                if is_file_start_query {
                    return Some(serde_json::json!([]));
                }
                let mut calls = Vec::new();
                for probe in &positions {
                    calls = provider.incoming_calls(root, *probe);
                    if !calls.is_empty() {
                        break;
                    }
                }
                let body: Vec<serde_json::Value> = calls
                    .iter()
                    .map(|call| {
                        let raw_kind = format!("{:?}", call.from.kind).to_lowercase();
                        let (name, kind) =
                            Self::tsserver_call_hierarchy_name_kind(&call.from.name, &raw_kind);
                        let from_ranges: Vec<serde_json::Value> = call
                            .from_ranges
                            .iter()
                            .map(|r| {
                                serde_json::json!({
                                    "start": Self::lsp_to_tsserver_position(r.start),
                                    "end": Self::lsp_to_tsserver_position(r.end),
                                })
                            })
                            .collect();
                        let mut from = serde_json::json!({
                            "from": {
                                "name": name,
                                "kind": kind,
                                "file": call.from.uri,
                                "span": {
                                    "start": Self::lsp_to_tsserver_position(call.from.range.start),
                                    "end": Self::lsp_to_tsserver_position(call.from.range.end),
                                },
                                "selectionSpan": {
                                    "start": Self::lsp_to_tsserver_position(call.from.selection_range.start),
                                    "end": Self::lsp_to_tsserver_position(call.from.selection_range.end),
                                },
                            },
                            "fromSpans": from_ranges,
                        });
                        if let Some(container_name) = &call.from.container_name {
                            from["from"]["containerName"] = serde_json::json!(container_name);
                        }
                        from
                    })
                    .collect();
                Some(serde_json::json!(body))
            } else {
                if is_file_start_query {
                    return Some(serde_json::json!([]));
                }
                // Prefer exact-position outgoing calls; if the cursor sits on a
                // token boundary where prepare fails, probe adjacent offsets to
                // recover the same behavior used by prepare/incoming handlers.
                let mut calls = provider.outgoing_calls(root, position);
                if calls.is_empty() && provider.prepare(root, position).is_none() {
                    for probe in positions.iter().skip(1) {
                        if provider.prepare(root, *probe).is_some() {
                            calls = provider.outgoing_calls(root, *probe);
                            break;
                        }
                    }
                }
                let body: Vec<serde_json::Value> = calls
                    .iter()
                    .map(|call| {
                        let raw_kind = format!("{:?}", call.to.kind).to_lowercase();
                        let (name, kind) =
                            Self::tsserver_call_hierarchy_name_kind(&call.to.name, &raw_kind);
                        let from_ranges: Vec<serde_json::Value> = call
                            .from_ranges
                            .iter()
                            .map(|r| {
                                serde_json::json!({
                                    "start": Self::lsp_to_tsserver_position(r.start),
                                    "end": Self::lsp_to_tsserver_position(r.end),
                                })
                            })
                            .collect();
                        let mut to = serde_json::json!({
                            "to": {
                                "name": name,
                                "kind": kind,
                                "file": call.to.uri,
                                "span": {
                                    "start": Self::lsp_to_tsserver_position(call.to.range.start),
                                    "end": Self::lsp_to_tsserver_position(call.to.range.end),
                                },
                                "selectionSpan": {
                                    "start": Self::lsp_to_tsserver_position(call.to.selection_range.start),
                                    "end": Self::lsp_to_tsserver_position(call.to.selection_range.end),
                                },
                            },
                            "fromSpans": from_ranges,
                        });
                        if let Some(container_name) = &call.to.container_name {
                            to["to"]["containerName"] = serde_json::json!(container_name);
                        }
                        to
                    })
                    .collect();
                Some(serde_json::json!(body))
            }
        })();
        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    pub(crate) fn handle_map_code(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        self.stub_response(seq, request, Some(serde_json::json!([])))
    }

    pub(crate) fn handle_outlining_spans(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let (arena, _binder, root, source_text) = self.parse_and_bind_file(file)?;
            let line_map = LineMap::build(&source_text);
            let provider = FoldingRangeProvider::new(&arena, &line_map, &source_text);
            let ranges = provider.get_folding_ranges(root);

            let body: Vec<serde_json::Value> = ranges
                .iter()
                .map(|fr| {
                    // Convert byte offsets to precise line/offset positions
                    let start_pos = line_map.offset_to_position(fr.start_offset, &source_text);
                    let end_pos = line_map.offset_to_position(fr.end_offset, &source_text);
                    let hint_end_pos = line_map
                        .offset_to_position(fr.end_offset.min(fr.start_offset + 200), &source_text);

                    let mut span = serde_json::json!({
                        "textSpan": {
                            "start": Self::lsp_to_tsserver_position(start_pos),
                            "end": Self::lsp_to_tsserver_position(end_pos),
                        },
                        "hintSpan": {
                            "start": Self::lsp_to_tsserver_position(start_pos),
                            "end": Self::lsp_to_tsserver_position(
                                if hint_end_pos.line == start_pos.line {
                                    hint_end_pos
                                } else {
                                    end_pos
                                }
                            ),
                        },
                        "bannerText": "...",
                        "autoCollapse": false,
                    });
                    span["kind"] = serde_json::json!(fr.kind.as_deref().unwrap_or("code"));
                    span
                })
                .collect();
            Some(serde_json::json!(body))
        })();
        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    pub(crate) fn handle_brace(&mut self, seq: u64, request: &TsServerRequest) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let (file, line, offset) = Self::extract_file_position(&request.arguments)?;
            let (arena, _binder, _root, source_text) = self.parse_and_bind_file(&file)?;
            let line_map = LineMap::build(&source_text);
            let position = Self::tsserver_to_lsp_position(line, offset);
            let byte_offset = line_map.position_to_offset(position, &source_text)? as usize;

            let bytes = source_text.as_bytes();
            if byte_offset >= bytes.len() {
                return Some(serde_json::json!([]));
            }

            let ch = bytes[byte_offset];

            // Build a map of which positions are "in code" (not inside strings/comments)
            let code_map = super::build_code_map(bytes);

            if !code_map[byte_offset] {
                return Some(serde_json::json!([]));
            }

            let match_pos = match ch {
                b'{' => super::scan_forward(bytes, &code_map, byte_offset, b'{', b'}'),
                b'(' => super::scan_forward(bytes, &code_map, byte_offset, b'(', b')'),
                b'[' => super::scan_forward(bytes, &code_map, byte_offset, b'[', b']'),
                b'}' => super::scan_backward(bytes, &code_map, byte_offset, b'}', b'{'),
                b')' => super::scan_backward(bytes, &code_map, byte_offset, b')', b'('),
                b']' => super::scan_backward(bytes, &code_map, byte_offset, b']', b'['),
                b'<' | b'>' => {
                    // For angle brackets, use AST-based matching (not text scanning)
                    // because < and > are also comparison operators
                    super::find_angle_bracket_match(&arena, &source_text, byte_offset)
                }
                _ => None,
            };

            if let Some(match_offset) = match_pos {
                let pos1 = line_map.offset_to_position(byte_offset as u32, &source_text);
                let pos1_end = line_map.offset_to_position((byte_offset + 1) as u32, &source_text);
                let pos2 = line_map.offset_to_position(match_offset as u32, &source_text);
                let pos2_end = line_map.offset_to_position((match_offset + 1) as u32, &source_text);

                let span1 = serde_json::json!({
                    "start": {"line": pos1.line + 1, "offset": pos1.character + 1},
                    "end": {"line": pos1_end.line + 1, "offset": pos1_end.character + 1}
                });
                let span2 = serde_json::json!({
                    "start": {"line": pos2.line + 1, "offset": pos2.character + 1},
                    "end": {"line": pos2_end.line + 1, "offset": pos2_end.character + 1}
                });

                // Return sorted by position
                if byte_offset < match_offset {
                    Some(serde_json::json!([span1, span2]))
                } else {
                    Some(serde_json::json!([span2, span1]))
                }
            } else {
                Some(serde_json::json!([]))
            }
        })();
        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }
}
