//! Structural, format, and miscellaneous handlers for tsz-server.
//!
//! Handles formatting, inlay hints, selection ranges, call hierarchy,
//! outlining spans, brace matching, refactoring stubs, and related commands.

use super::{Server, TsServerRequest, TsServerResponse};
use tsz::lsp::call_hierarchy::CallHierarchyProvider;
use tsz::lsp::folding::FoldingRangeProvider;
use tsz::lsp::inlay_hints::InlayHintsProvider;
use tsz::lsp::position::{LineMap, Position, Range};
use tsz::lsp::selection_range::SelectionRangeProvider;
use tsz::lsp::semantic_tokens::SemanticTokensProvider;
use tsz_solver::TypeInterner;

impl Server {
    fn tsserver_call_hierarchy_name_kind(name: &str, kind: &str) -> (String, String) {
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

    pub(crate) fn handle_get_applicable_refactors(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        self.stub_response(seq, request, Some(serde_json::json!([])))
    }

    pub(crate) fn handle_get_edits_for_refactor(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        self.stub_response(seq, request, Some(serde_json::json!({"edits": []})))
    }

    pub(crate) fn handle_organize_imports(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        self.stub_response(seq, request, Some(serde_json::json!([])))
    }

    pub(crate) fn handle_get_edits_for_file_rename(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        self.stub_response(seq, request, Some(serde_json::json!([])))
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
        self.stub_response(
            seq,
            request,
            Some(serde_json::json!({"configFileName": "", "fileNames": []})),
        )
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
                            tracked_files.push(file_name.to_string());
                        }
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
                                    tracked_files.push(file_name.to_string());
                                }
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
                        tsz::lsp::inlay_hints::InlayHintKind::Parameter => "Parameter",
                        tsz::lsp::inlay_hints::InlayHintKind::Type => "Type",
                        tsz::lsp::inlay_hints::InlayHintKind::Generic => "Enum",
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
                sr: &tsz::lsp::selection_range::SelectionRange,
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
        self.stub_response(seq, request, None)
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
            let mut item = provider.prepare(root, position);
            if item.is_none()
                && let Some(base_offset) = line_map.position_to_offset(position, &source_text)
            {
                let len = source_text.len() as u32;
                let mut probes = [base_offset; 2];
                let mut probe_count = 0usize;
                if base_offset < len {
                    probes[probe_count] = base_offset.saturating_add(1).min(len);
                    probe_count += 1;
                }
                if base_offset > 0 {
                    probes[probe_count] = base_offset - 1;
                    probe_count += 1;
                }
                for probe_offset in probes.into_iter().take(probe_count) {
                    let probe = line_map.offset_to_position(probe_offset, &source_text);
                    item = provider.prepare(root, probe);
                    if item.is_some() {
                        break;
                    }
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
            let mut positions = vec![position];
            if let Some(base_offset) = line_map.position_to_offset(position, &source_text) {
                let len = source_text.len() as u32;
                if base_offset < len {
                    positions.push(
                        line_map.offset_to_position(
                            base_offset.saturating_add(1).min(len),
                            &source_text,
                        ),
                    );
                }
                if base_offset > 0 {
                    positions.push(line_map.offset_to_position(base_offset - 1, &source_text));
                }
            }

            if is_incoming {
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
