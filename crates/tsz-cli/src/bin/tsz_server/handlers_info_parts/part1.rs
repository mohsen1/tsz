impl Server {
    fn build_project_for_file(&self, file_name: &str) -> Option<Project> {
        let mut files = self.open_files.clone();
        for project_files in self.external_project_files.values() {
            for path in project_files {
                if files.contains_key(path) {
                    continue;
                }
                if let Ok(content) = std::fs::read_to_string(path) {
                    files.insert(path.clone(), content);
                }
            }
        }
        if !files.contains_key(file_name)
            && let Ok(content) = std::fs::read_to_string(file_name)
        {
            files.insert(file_name.to_string(), content);
        }
        Self::add_project_config_files(&mut files, file_name);
        if files.is_empty() {
            return None;
        }

        let mut project = Project::new();
        project.set_allow_importing_ts_extensions(self.allow_importing_ts_extensions);
        project.set_auto_imports_allowed_without_tsconfig(
            self.auto_imports_allowed_for_inferred_projects,
        );
        project.set_import_module_specifier_ending(
            self.completion_import_module_specifier_ending.clone(),
        );
        project.set_import_module_specifier_preference(
            self.import_module_specifier_preference.clone(),
        );
        project
            .set_auto_import_file_exclude_patterns(self.auto_import_file_exclude_patterns.clone());
        project.set_auto_import_specifier_exclude_regexes(
            self.auto_import_specifier_exclude_regexes.clone(),
        );
        for (path, text) in files {
            project.set_file(path, text);
        }
        Some(project)
    }

    pub(super) fn find_ancestor_of_kind(
        arena: &tsz::parser::node::NodeArena,
        mut node_idx: tsz::parser::NodeIndex,
        kind: u16,
    ) -> tsz::parser::NodeIndex {
        while node_idx.is_some() {
            let Some(node) = arena.get(node_idx) else {
                break;
            };
            if node.kind == kind {
                return node_idx;
            }
            let Some(ext) = arena.get_extended(node_idx) else {
                break;
            };
            node_idx = ext.parent;
        }
        tsz::parser::NodeIndex::NONE
    }

    pub(super) fn node_text_opt(
        source_text: &str,
        node: &tsz::parser::node::Node,
    ) -> Option<String> {
        if node.end <= node.pos || node.end as usize > source_text.len() {
            return None;
        }
        Some(source_text[node.pos as usize..node.end as usize].to_string())
    }

    pub(crate) fn handle_definition(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let full = request.command.ends_with("-full");
        let result = (|| -> Option<serde_json::Value> {
            let (file, line, offset) = Self::extract_file_position(&request.arguments)?;
            let (arena, binder, root, source_text) = self.parse_and_bind_file(&file)?;
            let line_map = LineMap::build(&source_text);
            let position = Self::tsserver_to_lsp_position(line, offset);
            let raw_offset = line_map.position_to_offset(position, &source_text)?;
            let offset = Self::adjusted_quoted_specifier_offset(&arena, &source_text, raw_offset);
            let position = line_map.offset_to_position(offset, &source_text);
            if Self::is_offset_inside_comment(&source_text, offset) {
                return None;
            }
            if let Some(canonical_loc) =
                self.canonical_definition_for_alias_position(&file, &arena, &source_text, offset)
                && let Some(def) = self.definition_info_from_location(&canonical_loc, full)
            {
                return Some(serde_json::json!([def]));
            }
            let provider =
                GoToDefinition::new(&arena, &binder, &line_map, file.clone(), &source_text);
            let mut infos = provider
                .get_definition_info(root, position)
                .unwrap_or_default();
            let file_ctx = ParsedFileContext {
                arena: &arena,
                binder: &binder,
                line_map: &line_map,
                root,
                source_text: &source_text,
                file: &file,
            };
            if let Some(remapped) =
                self.maybe_remap_alias_to_ambient_module(&file_ctx, position, &infos)
            {
                infos = remapped;
            }
            if infos.is_empty() {
                return None;
            }
            let body: Vec<serde_json::Value> = infos
                .iter()
                .map(|info| {
                    if full {
                        Self::definition_info_to_json_full(info, &file, &line_map, &source_text)
                    } else {
                        Self::definition_info_to_json(info, &file)
                    }
                })
                .collect();
            Some(serde_json::json!(body))
        })();
        self.success_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    pub(crate) fn handle_type_definition(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let full = request.command.ends_with("-full");
        let result = (|| -> Option<serde_json::Value> {
            let (file, line, offset) = Self::extract_file_position(&request.arguments)?;
            let (arena, _binder, _root, source_text) = self.parse_and_bind_file(&file)?;
            let line_map = LineMap::build(&source_text);
            let position = Self::tsserver_to_lsp_position(line, offset);
            let raw_offset = line_map.position_to_offset(position, &source_text)?;
            let offset = Self::adjusted_quoted_specifier_offset(&arena, &source_text, raw_offset);
            let position = line_map.offset_to_position(offset, &source_text);
            if Self::is_offset_inside_comment(&source_text, offset) {
                return None;
            }

            let project = self.build_project_for_file(&file)?;
            let locs = project.get_type_definition(&file, position)?;
            if locs.is_empty() {
                return None;
            }

            let body: Vec<serde_json::Value> = locs
                .iter()
                .filter_map(|loc| {
                    if full {
                        let target_text = if loc.file_path == file {
                            source_text.clone()
                        } else {
                            self.open_files
                                .get(&loc.file_path)
                                .cloned()
                                .or_else(|| std::fs::read_to_string(&loc.file_path).ok())
                                .unwrap_or_else(|| source_text.clone())
                        };
                        let target_map = LineMap::build(&target_text);
                        let span_start =
                            target_map.position_to_offset(loc.range.start, &target_text)?;
                        let span_end =
                            target_map.position_to_offset(loc.range.end, &target_text)?;
                        Some(serde_json::json!({
                            "fileName": loc.file_path,
                            "textSpan": {
                                "start": span_start,
                                "length": span_end.saturating_sub(span_start),
                            },
                        }))
                    } else {
                        Some(serde_json::json!({
                            "file": loc.file_path,
                            "start": Self::lsp_to_tsserver_position(loc.range.start),
                            "end": Self::lsp_to_tsserver_position(loc.range.end),
                        }))
                    }
                })
                .collect();

            if body.is_empty() {
                return None;
            }

            Some(serde_json::json!(body))
        })();
        self.success_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    pub(crate) fn handle_definition_and_bound_span(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let full = request.command.ends_with("-full");
        let result = (|| -> Option<serde_json::Value> {
            let (file, line, offset) = Self::extract_file_position(&request.arguments)?;
            let (arena, binder, root, source_text) = self.parse_and_bind_file(&file)?;
            let line_map = LineMap::build(&source_text);
            let position = Self::tsserver_to_lsp_position(line, offset);
            let raw_offset = line_map.position_to_offset(position, &source_text)?;
            let offset = Self::adjusted_quoted_specifier_offset(&arena, &source_text, raw_offset);
            let position = line_map.offset_to_position(offset, &source_text);
            if Self::is_offset_inside_comment(&source_text, offset) {
                return None;
            }
            if let Some(canonical_loc) =
                self.canonical_definition_for_alias_position(&file, &arena, &source_text, offset)
                && let Some(definition) = self.definition_info_from_location(&canonical_loc, full)
            {
                let bound_range = if Self::is_quoted_import_or_export_specifier_offset(
                    &arena,
                    &source_text,
                    offset,
                ) {
                    let node_idx = tsz::lsp::utils::find_node_at_or_before_offset(
                        &arena,
                        offset,
                        &source_text,
                    );
                    node_idx
                        .into_option()
                        .and_then(|idx| arena.get(idx))
                        .map(|node| {
                            let start = line_map.offset_to_position(node.pos, &source_text);
                            let end = line_map.offset_to_position(node.end, &source_text);
                            (node.pos, node.end, start, end)
                        })
                        .unwrap_or_else(|| {
                            let pos_offset = line_map
                                .position_to_offset(position, &source_text)
                                .unwrap_or(offset);
                            (pos_offset, pos_offset, position, position)
                        })
                } else {
                    let pos_offset = line_map
                        .position_to_offset(position, &source_text)
                        .unwrap_or(offset);
                    (pos_offset, pos_offset, position, position)
                };
                let text_span = Self::bound_text_span_json(full, bound_range);
                return Some(serde_json::json!({
                    "definitions": [definition],
                    "textSpan": text_span,
                }));
            }
            let provider =
                GoToDefinition::new(&arena, &binder, &line_map, file.clone(), &source_text);
            let mut infos = provider
                .get_definition_info(root, position)
                .unwrap_or_default();
            let file_ctx = ParsedFileContext {
                arena: &arena,
                binder: &binder,
                line_map: &line_map,
                root,
                source_text: &source_text,
                file: &file,
            };
            if let Some(remapped) =
                self.maybe_remap_alias_to_ambient_module(&file_ctx, position, &infos)
            {
                infos = remapped;
            }
            if infos.is_empty() {
                return None;
            }

            // Build definitions array using the shape that matches the protocol variant.
            let definitions: Vec<serde_json::Value> = infos
                .iter()
                .map(|info| {
                    if full {
                        Self::definition_info_to_json_full(info, &file, &line_map, &source_text)
                    } else {
                        Self::definition_info_to_json(info, &file)
                    }
                })
                .collect();

            // Compute textSpan from hover range for symbol-accurate bound spans.
            let interner = TypeInterner::new();
            let hover_provider =
                HoverProvider::new(&arena, &binder, &line_map, &interner, &source_text, file);
            let mut type_cache = None;
            let hover_range = hover_provider
                .get_hover(root, position, &mut type_cache)
                .and_then(|info| info.range)
                .filter(|range| range.start != range.end);
            let symbol_range = hover_range.or_else(|| {
                let mut probe = line_map.position_to_offset(position, &source_text)?;
                let max = source_text.len() as u32;
                let mut remaining = 256u32;
                while probe < max && remaining > 0 {
                    let node_idx =
                        tsz::lsp::utils::find_node_at_or_before_offset(&arena, probe, &source_text);
                    if node_idx.is_some()
                        && tsz::lsp::utils::is_symbol_query_node(&arena, node_idx)
                        && let Some(node) = arena.get(node_idx)
                    {
                        let start = line_map.offset_to_position(node.pos, &source_text);
                        let end = line_map.offset_to_position(node.end, &source_text);
                        if start != end {
                            return Some(tsz::lsp::position::Range::new(start, end));
                        }
                    }

                    let ch = source_text.as_bytes()[probe as usize];
                    if ch == b'\n' || ch == b'\r' {
                        break;
                    }
                    probe += 1;
                    remaining -= 1;
                }
                None
            });
            let bound_range = symbol_range
                .map(|range| {
                    let start_off = line_map
                        .position_to_offset(range.start, &source_text)
                        .unwrap_or(0);
                    let end_off = line_map
                        .position_to_offset(range.end, &source_text)
                        .unwrap_or(start_off);
                    (start_off, end_off, range.start, range.end)
                })
                .unwrap_or_else(|| {
                    let pos_offset = line_map
                        .position_to_offset(position, &source_text)
                        .unwrap_or(0);
                    (pos_offset, pos_offset, position, position)
                });
            let text_span = Self::bound_text_span_json(full, bound_range);

            Some(serde_json::json!({
                "definitions": definitions,
                "textSpan": text_span,
            }))
        })();
        self.success_response(seq, request, result)
    }

    /// Build the `textSpan` payload for `definitionAndBoundSpan` /
    /// `definitionAndBoundSpan-full` from a `(start_offset, end_offset,
    /// start_pos, end_pos)` tuple.
    fn bound_text_span_json(
        full: bool,
        bound: (u32, u32, Position, Position),
    ) -> serde_json::Value {
        let (start_off, end_off, start_pos, end_pos) = bound;
        if full {
            serde_json::json!({
                "start": start_off,
                "length": end_off.saturating_sub(start_off),
            })
        } else {
            serde_json::json!({
                "start": Self::lsp_to_tsserver_position(start_pos),
                "end": Self::lsp_to_tsserver_position(end_pos),
            })
        }
    }

    pub(crate) fn handle_references(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let (file, line, offset) = Self::extract_file_position(&request.arguments)?;
            let (arena, binder, root, source_text) = self.parse_and_bind_file(&file)?;
            let line_map = LineMap::build(&source_text);
            let position = Self::tsserver_to_lsp_position(line, offset);
            let raw_query_offset = line_map.position_to_offset(position, &source_text)?;
            let query_offset =
                Self::adjusted_quoted_specifier_offset(&arena, &source_text, raw_query_offset);
            let position = line_map.offset_to_position(query_offset, &source_text);
            let quoted_alias_is_type_only =
                Self::is_type_only_quoted_import_or_export_specifier_offset(
                    &arena,
                    &source_text,
                    query_offset,
                );
            let quoted_symbol_name = if quoted_alias_is_type_only {
                Self::quoted_specifier_literal_at_offset(&arena, &source_text, query_offset)
                    .map(|name| format!("\"{name}\""))
                    .unwrap_or_default()
            } else {
                String::new()
            };
            if let Some(mut project) = self.build_project_for_file(&file)
                && let Some(locs) = self.quoted_alias_chain_references(
                    &mut project,
                    &file,
                    &arena,
                    &source_text,
                    query_offset,
                    position,
                    !quoted_alias_is_type_only,
                )
            {
                let definition_locs = project.get_definition(&file, position).unwrap_or_default();
                let refs: Vec<serde_json::Value> = locs
                    .iter()
                    .filter_map(|loc| {
                        let source = self
                            .open_files
                            .get(&loc.file_path)
                            .cloned()
                            .or_else(|| std::fs::read_to_string(&loc.file_path).ok())?;
                        let loc_line_map = LineMap::build(&source);
                        let line_text = source
                            .lines()
                            .nth(loc.range.start.line as usize)
                            .unwrap_or("")
                            .to_string();
                        let is_query_alias_definition = quoted_alias_is_type_only
                            && loc.file_path == file
                            && loc_line_map
                                .position_to_offset(loc.range.start, &source)
                                .zip(loc_line_map.position_to_offset(loc.range.end, &source))
                                .is_some_and(|(start, end)| {
                                    start <= query_offset && query_offset <= end
                                });
                        let is_definition = definition_locs
                            .iter()
                            .any(|def| def.file_path == loc.file_path && def.range == loc.range);
                        Some(serde_json::json!({
                            "file": loc.file_path,
                            "start": Self::lsp_to_tsserver_position(loc.range.start),
                            "end": Self::lsp_to_tsserver_position(loc.range.end),
                            "lineText": line_text,
                            "isWriteAccess": is_query_alias_definition,
                            "isDefinition": is_definition || is_query_alias_definition,
                        }))
                    })
                    .collect();
                return Some(serde_json::json!({
                    "refs": refs,
                    "symbolName": quoted_symbol_name,
                    "symbolStartOffset": offset,
                    "symbolDisplayString": quoted_symbol_name,
                }));
            }
            if let Some(mut project) = self.build_project_for_file(&file)
                && let Some(canonical_loc) = self.canonical_definition_for_alias_position(
                    &file,
                    &arena,
                    &source_text,
                    query_offset,
                )
                && let Some(locs) =
                    project.find_references(&canonical_loc.file_path, canonical_loc.range.start)
            {
                let restrict_to_quoted =
                    Self::quoted_specifier_literal_at_offset(&arena, &source_text, query_offset)
                        .is_some();
                let definition_locs = [canonical_loc];
                let refs: Vec<serde_json::Value> = locs
                    .iter()
                    .filter(|loc| {
                        !restrict_to_quoted
                            || self.is_quoted_import_or_export_specifier_location(loc)
                    })
                    .filter_map(|loc| {
                        let source = self
                            .open_files
                            .get(&loc.file_path)
                            .cloned()
                            .or_else(|| std::fs::read_to_string(&loc.file_path).ok())?;
                        let line_text = source
                            .lines()
                            .nth(loc.range.start.line as usize)
                            .unwrap_or("")
                            .to_string();
                        let is_definition = definition_locs
                            .iter()
                            .any(|def| def.file_path == loc.file_path && def.range == loc.range);
                        Some(serde_json::json!({
                            "file": loc.file_path,
                            "start": Self::lsp_to_tsserver_position(loc.range.start),
                            "end": Self::lsp_to_tsserver_position(loc.range.end),
                            "lineText": line_text,
                            "isWriteAccess": false,
                            "isDefinition": is_definition,
                        }))
                    })
                    .collect();
                return Some(serde_json::json!({
                    "refs": refs,
                    "symbolName": "",
                    "symbolStartOffset": offset,
                    "symbolDisplayString": "",
                }));
            }
            let provider =
                FindReferences::new(&arena, &binder, &line_map, file.clone(), &source_text);
            let (_symbol_id, ref_infos) = provider.find_references_with_symbol(root, position)?;
            let project_locations = self
                .build_project_for_file(&file)
                .and_then(|mut project| project.find_references(&file, position));

            let (symbol_name, symbol_start_offset) = {
                let ref_offset = line_map.position_to_offset(position, &source_text)?;
                let node_idx = tsz::lsp::utils::find_node_at_offset(&arena, ref_offset);
                let symbol_name = if node_idx.is_some() {
                    arena
                        .get_identifier_text(node_idx)
                        .map(std::string::ToString::to_string)
                } else {
                    None
                };
                let symbol_start_offset = node_idx
                    .into_option()
                    .and_then(|idx| arena.get(idx))
                    .map(|node| {
                        line_map
                            .offset_to_position(node.pos, &source_text)
                            .character
                            + 1
                    })
                    .unwrap_or(offset);
                Some((symbol_name.unwrap_or_default(), symbol_start_offset))
            }
            .unwrap_or_default();

            let interner = TypeInterner::new();
            let hover_provider =
                HoverProvider::new(&arena, &binder, &line_map, &interner, &source_text, file);
            let mut type_cache = None;
            let symbol_display_string = hover_provider
                .get_hover(root, position, &mut type_cache)
                .map(|info| info.display_string)
                .unwrap_or_default();

            let locations: Vec<_> = project_locations.unwrap_or_else(|| {
                ref_infos
                    .iter()
                    .map(|ref_info| ref_info.location.clone())
                    .collect()
            });

            let refs: Vec<serde_json::Value> = locations
                .iter()
                .filter_map(|location| {
                    let source = self
                        .open_files
                        .get(&location.file_path)
                        .cloned()
                        .or_else(|| std::fs::read_to_string(&location.file_path).ok())?;
                    let line_text = source
                        .lines()
                        .nth(location.range.start.line as usize)
                        .unwrap_or("")
                        .to_string();
                    let local_ref = ref_infos
                        .iter()
                        .find(|ref_info| ref_info.location == *location);
                    Some(serde_json::json!({
                        "file": location.file_path,
                        "start": Self::lsp_to_tsserver_position(location.range.start),
                        "end": Self::lsp_to_tsserver_position(location.range.end),
                        "lineText": line_text,
                        "isWriteAccess": local_ref.is_some_and(|ref_info| ref_info.is_write_access),
                        "isDefinition": local_ref.is_some_and(|ref_info| ref_info.is_definition),
                    }))
                })
                .collect();
            Some(serde_json::json!({
                "refs": refs,
                "symbolName": symbol_name,
                "symbolStartOffset": symbol_start_offset,
                "symbolDisplayString": symbol_display_string,
            }))
        })();
        self.success_response(
            seq,
            request,
            Some(result.unwrap_or(serde_json::json!({
                "refs": [],
                "symbolName": "",
                "symbolStartOffset": 0,
                "symbolDisplayString": "",
            }))),
        )
    }

    pub(crate) fn handle_document_highlights(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let (file, line, offset) = Self::extract_file_position(&request.arguments)?;
            // Issue #3710: tsserver passes a `filesToSearch` array so the
            // language service can group highlights across files. When the
            // client asks for >1 file (or any non-primary file), use the
            // project-level reference search and group the resulting
            // locations by file. The single-file `DocumentHighlightProvider`
            // path is kept as a fallback when no file list is supplied.
            let files_to_search: Vec<String> = request
                .arguments
                .get("filesToSearch")
                .and_then(serde_json::Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();

            if !files_to_search.is_empty()
                && let Some(grouped) =
                    self.document_highlights_via_project(&file, line, offset, &files_to_search)
            {
                return Some(grouped);
            }
            // Otherwise fall through to the single-file path.

            let (arena, binder, root, source_text) = self.parse_and_bind_file(&file)?;
            let line_map = LineMap::build(&source_text);
            let position = Self::tsserver_to_lsp_position(line, offset);
            let provider = DocumentHighlightProvider::new(&arena, &binder, &line_map, &source_text);
            let highlights = provider.get_document_highlights(root, position)?;

            // Group highlights by file (tsserver groups by file, each with highlightSpans)
            // Issue #3710: dedupe by (start, end, kind) — the provider can
            // emit the same span twice when a node is reachable through
            // multiple resolution paths.
            let mut seen = std::collections::HashSet::new();
            let highlight_spans: Vec<serde_json::Value> = highlights
                .iter()
                .filter_map(|hl| {
                    let kind_str = match hl.kind {
                        Some(tsz::lsp::highlighting::DocumentHighlightKind::Read) => "reference",
                        Some(tsz::lsp::highlighting::DocumentHighlightKind::Write) => {
                            "writtenReference"
                        }
                        Some(tsz::lsp::highlighting::DocumentHighlightKind::Text) | None => "none",
                    };
                    let key = (
                        hl.range.start.line,
                        hl.range.start.character,
                        hl.range.end.line,
                        hl.range.end.character,
                        kind_str,
                    );
                    if !seen.insert(key) {
                        return None;
                    }
                    let mut span = serde_json::json!({
                        "start": Self::lsp_to_tsserver_position(hl.range.start),
                        "end": Self::lsp_to_tsserver_position(hl.range.end),
                        "kind": kind_str,
                    });
                    if let Some((context_start, context_end)) =
                        import_context_for_range(&source_text, hl.range)
                    {
                        span["contextStart"] = Self::lsp_to_tsserver_position(context_start);
                        span["contextEnd"] = Self::lsp_to_tsserver_position(context_end);
                    }
                    Some(span)
                })
                .collect();
            Some(serde_json::json!([{
                "file": file,
                "highlightSpans": highlight_spans,
            }]))
        })();
        self.success_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    /// Find highlight groups across `files_to_search` using the project-level
    /// reference index. Issue #3710: tsserver consumers (e.g. VS Code) supply
    /// the editor's open-file set in `filesToSearch`; the language service
    /// returns highlight groups per file.
    fn document_highlights_via_project(
        &self,
        file: &str,
        line: u32,
        offset: u32,
        files_to_search: &[String],
    ) -> Option<serde_json::Value> {
        let mut project = self.build_project_for_file(file)?;
        let position = Self::tsserver_to_lsp_position(line, offset);
        let locs = project.find_references(file, position)?;
        type RangeKey = (u32, u32, u32, u32);
        let allowed: rustc_hash::FxHashSet<&str> =
            files_to_search.iter().map(String::as_str).collect();
        let mut grouped: rustc_hash::FxHashMap<String, std::collections::BTreeSet<RangeKey>> =
            rustc_hash::FxHashMap::default();
        for loc in locs {
            if !allowed.contains(loc.file_path.as_str()) {
                continue;
            }
            grouped.entry(loc.file_path.clone()).or_default().insert((
                loc.range.start.line,
                loc.range.start.character,
                loc.range.end.line,
                loc.range.end.character,
            ));
        }
        if grouped.is_empty() {
            return None;
        }
        let groups: Vec<serde_json::Value> = grouped
            .into_iter()
            .map(|(file_name, ranges)| {
                // Resolve the file's source so we can attach
                // contextStart/contextEnd for highlights that land on an
                // import statement, matching the single-file path.
                let file_source = self
                    .open_files
                    .get(&file_name)
                    .cloned()
                    .or_else(|| std::fs::read_to_string(&file_name).ok())
                    .unwrap_or_default();
                let highlight_spans: Vec<serde_json::Value> = ranges
                    .into_iter()
                    .map(|(sl, sc, el, ec)| {
                        let start = tsz_common::position::Position::new(sl, sc);
                        let end = tsz_common::position::Position::new(el, ec);
                        let is_decl = file_name == file
                            && start == Self::tsserver_to_lsp_position(line, offset);
                        let mut span = serde_json::json!({
                            "start": Self::lsp_to_tsserver_position(start),
                            "end": Self::lsp_to_tsserver_position(end),
                            "kind": if is_decl { "writtenReference" } else { "reference" },
                        });
                        if let Some((context_start, context_end)) = import_context_for_range(
                            &file_source,
                            tsz_common::position::Range::new(start, end),
                        ) {
                            span["contextStart"] = Self::lsp_to_tsserver_position(context_start);
                            span["contextEnd"] = Self::lsp_to_tsserver_position(context_end);
                        }
                        span
                    })
                    .collect();
                serde_json::json!({
                    "file": file_name,
                    "highlightSpans": highlight_spans,
                })
            })
            .collect();
        Some(serde_json::json!(groups))
    }

    pub(crate) fn handle_rename(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let (file, line, offset) = Self::extract_file_position(&request.arguments)?;
            if let Some(native) = self.try_native_typescript_operation(serde_json::json!({
                "op": "rename",
                "file": file,
                "line": line,
                "offset": offset,
                "findInStrings": request.arguments.get("findInStrings").and_then(serde_json::Value::as_bool).unwrap_or(false),
                "findInComments": request.arguments.get("findInComments").and_then(serde_json::Value::as_bool).unwrap_or(false),
                "preferences": request.arguments.get("preferences").cloned().unwrap_or(serde_json::json!({})),
                "providePrefixAndSuffixTextForRename": request.arguments.get("providePrefixAndSuffixTextForRename").cloned().unwrap_or(serde_json::Value::Null),
                "allowRenameOfImportPath": request.arguments.get("allowRenameOfImportPath").cloned().unwrap_or(serde_json::Value::Null),
            })) {
                return Some(native);
            }
            let (arena, binder, root, source_text) = self.parse_and_bind_file(&file)?;
            let line_map = LineMap::build(&source_text);
            let position = Self::tsserver_to_lsp_position(line, offset);
            let raw_query_offset = line_map.position_to_offset(position, &source_text)?;
            let query_offset =
                Self::adjusted_quoted_specifier_offset(&arena, &source_text, raw_query_offset);
            let position = line_map.offset_to_position(query_offset, &source_text);
            let provider =
                RenameProvider::new(&arena, &binder, &line_map, file.clone(), &source_text);

            // Use the rich prepare_rename_info to get display name, kind, etc.
            let info = provider.prepare_rename_info(root, position);
            if !info.can_rename {
                return Some(serde_json::json!({
                    "info": {
                        "canRename": false,
                        "localizedErrorMessage": info.localized_error_message.unwrap_or_else(|| "You cannot rename this element.".to_string())
                    },
                    "locs": []
                }));
            }

            let rename_seed =
                Self::quoted_specifier_literal_at_offset(&arena, &source_text, query_offset);
            let start_offset = line_map
                .position_to_offset(info.trigger_span.start, &source_text)
                .unwrap_or(0) as usize;
            let end_offset = line_map
                .position_to_offset(info.trigger_span.end, &source_text)
                .unwrap_or(0) as usize;
            let trigger_length = end_offset.saturating_sub(start_offset);

            if let Some(mut project) = self.build_project_for_file(&file)
                && let Some(locs) = self.quoted_alias_chain_references(
                    &mut project,
                    &file,
                    &arena,
                    &source_text,
                    query_offset,
                    position,
                    true,
                )
            {
                let mut grouped: rustc_hash::FxHashMap<String, Vec<serde_json::Value>> =
                    rustc_hash::FxHashMap::default();
                for loc in locs {
                    let source = self
                        .open_files
                        .get(&loc.file_path)
                        .cloned()
                        .or_else(|| std::fs::read_to_string(&loc.file_path).ok())
                        .unwrap_or_default();
                    let loc_line_map = LineMap::build(&source);
                    let start_off = loc_line_map
                        .position_to_offset(loc.range.start, &source)
                        .unwrap_or(0);
                    let end_off = loc_line_map
                        .position_to_offset(loc.range.end, &source)
                        .unwrap_or(start_off);
                    if let Some(seed) = rename_seed.as_ref() {
                        let text = source
                            .get(start_off as usize..end_off as usize)
                            .unwrap_or("");
                        if text != seed {
                            continue;
                        }
                    }
                    let mut loc_json = serde_json::json!({
                        "start": Self::lsp_to_tsserver_position(loc.range.start),
                        "end": Self::lsp_to_tsserver_position(loc.range.end),
                    });
                    if let Some((ctx_start, ctx_end)) =
                        Self::import_statement_context_span(&source, start_off)
                    {
                        loc_json["contextStart"] = Self::lsp_to_tsserver_position(
                            loc_line_map.offset_to_position(ctx_start, &source),
                        );
                        loc_json["contextEnd"] = Self::lsp_to_tsserver_position(
                            loc_line_map.offset_to_position(ctx_end, &source),
                        );
                    }
                    grouped
                        .entry(loc.file_path.clone())
                        .or_default()
                        .push(loc_json);
                }
                let locs_json: Vec<serde_json::Value> = grouped
                    .into_iter()
                    .map(|(file_name, file_locs)| {
                        serde_json::json!({
                            "file": file_name,
                            "locs": file_locs,
                        })
                    })
                    .collect();
                return Some(serde_json::json!({
                    "info": {
                        "canRename": true,
                        "displayName": info.display_name,
                        "fullDisplayName": info.full_display_name,
                        "kind": info.kind,
                        "kindModifiers": info.kind_modifiers,
                        "triggerSpan": {
                            "start": Self::lsp_to_tsserver_position(info.trigger_span.start),
                            "length": trigger_length
                        }
                    },
                    "locs": locs_json
                }));
            }

            if let Some(mut project) = self.build_project_for_file(&file)
                && let Some(canonical_loc) = self.canonical_definition_for_alias_position(
                    &file,
                    &arena,
                    &source_text,
                    query_offset,
                )
                && let Some(locs) =
                    project.find_references(&canonical_loc.file_path, canonical_loc.range.start)
            {
                let restrict_to_quoted =
                    Self::quoted_specifier_literal_at_offset(&arena, &source_text, query_offset)
                        .is_some();
                let mut grouped: rustc_hash::FxHashMap<String, Vec<serde_json::Value>> =
                    rustc_hash::FxHashMap::default();
                for loc in locs {
                    if restrict_to_quoted
                        && !self.is_quoted_import_or_export_specifier_location(&loc)
                    {
                        continue;
                    }
                    grouped
                        .entry(loc.file_path.clone())
                        .or_default()
                        .push(serde_json::json!({
                            "start": Self::lsp_to_tsserver_position(loc.range.start),
                            "end": Self::lsp_to_tsserver_position(loc.range.end),
                        }));
                }
                let locs_json: Vec<serde_json::Value> = grouped
                    .into_iter()
                    .map(|(file_name, file_locs)| {
                        serde_json::json!({
                            "file": file_name,
                            "locs": file_locs,
                        })
                    })
                    .collect();
                return Some(serde_json::json!({
                    "info": {
                        "canRename": true,
                        "displayName": info.display_name,
                        "fullDisplayName": info.full_display_name,
                        "kind": info.kind,
                        "kindModifiers": info.kind_modifiers,
                        "triggerSpan": {
                            "start": Self::lsp_to_tsserver_position(info.trigger_span.start),
                            "length": trigger_length
                        }
                    },
                    "locs": locs_json
                }));
            }

            // Get rename locations from references with symbol info
            let find_refs =
                FindReferences::new(&arena, &binder, &line_map, file.clone(), &source_text);
            let (symbol_id, ref_infos) = find_refs
                .find_references_with_symbol(root, position)
                .unwrap_or((SymbolId::NONE, Vec::new()));

            // Get definition info for context spans
            let def_provider =
                GoToDefinition::new(&arena, &binder, &line_map, file.clone(), &source_text);
            let def_infos = if symbol_id.is_some() {
                def_provider.definition_infos_from_symbol(symbol_id)
            } else {
                None
            };

            let file_locs: Vec<serde_json::Value> = ref_infos
                .iter()
                .map(|ref_info| {
                    let mut loc = serde_json::json!({
                        "start": Self::lsp_to_tsserver_position(ref_info.location.range.start),
                        "end": Self::lsp_to_tsserver_position(ref_info.location.range.end),
                    });
                    // Add contextSpan for definition locations
                    if ref_info.is_definition
                        && let Some(ref defs) = def_infos
                    {
                        for def in defs {
                            if def.location.range == ref_info.location.range
                                && let Some(ref ctx) = def.context_span
                            {
                                loc["contextStart"] = Self::lsp_to_tsserver_position(ctx.start);
                                loc["contextEnd"] = Self::lsp_to_tsserver_position(ctx.end);
                                break;
                            }
                        }
                    }
                    loc
                })
                .collect();
            Some(serde_json::json!({
                "info": {
                    "canRename": true,
                    "displayName": info.display_name,
                    "fullDisplayName": info.full_display_name,
                    "kind": info.kind,
                    "kindModifiers": info.kind_modifiers,
                    "triggerSpan": {
                        "start": Self::lsp_to_tsserver_position(info.trigger_span.start),
                        "length": trigger_length
                    }
                },
                "locs": [{
                    "file": file,
                    "locs": file_locs,
                }]
            }))
        })();
        self.success_response(
            seq,
            request,
            Some(result.unwrap_or(serde_json::json!({
                "info": {"canRename": false, "localizedErrorMessage": "Not yet implemented"},
                "locs": []
            }))),
        )
    }

    pub(crate) fn handle_references_full(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let (file, line, offset) = Self::extract_file_position(&request.arguments)?;
            let (arena, binder, root, source_text) = self.parse_and_bind_file(&file)?;
            let line_map = LineMap::build(&source_text);
            let raw_query_position = Self::tsserver_to_lsp_position(line, offset);
            let raw_query_offset = line_map.position_to_offset(raw_query_position, &source_text)?;
            let query_offset =
                Self::adjusted_quoted_specifier_offset(&arena, &source_text, raw_query_offset);
            let position = line_map.offset_to_position(query_offset, &source_text);
            let ref_provider =
                FindReferences::new(&arena, &binder, &line_map, file.clone(), &source_text);
            let symbol_refs = ref_provider.find_references_with_symbol(root, position);

            let use_quoted_alias_fallback = Self::is_quoted_import_or_export_specifier_offset(
                &arena,
                &source_text,
                query_offset,
            ) || match &symbol_refs {
                Some((_symbol_id, refs)) => refs.is_empty(),
                None => true,
            };
            if use_quoted_alias_fallback
                && let Some(mut project) = self.build_project_for_file(&file)
                && let Some(entries) = self.build_quoted_alias_referenced_symbols(
                    &mut project,
                    &file,
                    &arena,
                    &source_text,
                    query_offset,
                    position,
                )
            {
                return Some(serde_json::json!(entries));
            }

            // Get references with the resolved symbol
            let (symbol_id, ref_infos) = symbol_refs?;

            // Get definition metadata using GoToDefinition helpers
            let def_provider =
                GoToDefinition::new(&arena, &binder, &line_map, file.clone(), &source_text);
            let def_infos = def_provider.definition_infos_from_symbol(symbol_id);

            // Get symbol info for display
            let symbol = binder.symbols.get(symbol_id)?;
            let kind_str = def_provider.symbol_flags_to_kind_string(symbol.flags);
            let symbol_name = symbol.escaped_name.clone();

            // Use HoverProvider to get the display string with type info
            let interner = TypeInterner::new();
            let hover_provider = HoverProvider::new(
                &arena,
                &binder,
                &line_map,
                &interner,
                &source_text,
                file.clone(),
            );
            let mut type_cache = None;
            let hover_info = hover_provider.get_hover(root, position, &mut type_cache);
            let display_string = hover_info
                .as_ref()
                .map(|h| h.display_string.clone())
                .unwrap_or_default();

            // Build definition object using first definition info
            let definition = if let Some(ref defs) = def_infos {
                if let Some(first_def) = defs.first() {
                    let def_start =
                        line_map.position_to_offset(first_def.location.range.start, &source_text);
                    let def_end =
                        line_map.position_to_offset(first_def.location.range.end, &source_text);

                    // Use display_string from HoverProvider if available for proper type info
                    let name = if !display_string.is_empty() {
                        display_string.clone()
                    } else {
                        format!("{} {}", first_def.kind, first_def.name)
                    };
                    let display_parts = if !display_string.is_empty() {
                        Self::parse_display_string_to_parts(
                            &display_string,
                            &first_def.kind,
                            &first_def.name,
                        )
                    } else {
                        Self::build_simple_display_parts(&first_def.kind, &first_def.name)
                    };

                    let mut def_json = serde_json::json!({
                        "containerKind": "",
                        "containerName": "",
                        "kind": first_def.kind,
                        "name": name,
                        "displayParts": display_parts,
                        "fileName": file,
                        "textSpan": {
                            "start": def_start.unwrap_or(0),
                            "length": def_end.unwrap_or(0).saturating_sub(def_start.unwrap_or(0)),
                        },
                    });
                    if let Some(ref ctx) = first_def.context_span {
                        let ctx_start = line_map.position_to_offset(ctx.start, &source_text);
                        let ctx_end = line_map.position_to_offset(ctx.end, &source_text);
                        let ctx_start_off = ctx_start.unwrap_or(0);
                        let ctx_end_off = ctx_end.unwrap_or(0);
                        let def_start_off = def_start.unwrap_or(0);
                        let def_end_off = def_end.unwrap_or(0);
                        // Skip contextSpan when it matches textSpan (e.g., catch clause vars)
                        if ctx_start_off != def_start_off || ctx_end_off != def_end_off {
                            def_json["contextSpan"] = serde_json::json!({
                                "start": ctx_start_off,
                                "length": ctx_end_off.saturating_sub(ctx_start_off),
                            });
                        }
                    }
                    if first_def.kind == "alias"
                        && let Some((ctx_start_off, ctx_end_off)) =
                            Self::import_statement_context_span(
                                &source_text,
                                def_start.unwrap_or(0),
                            )
                    {
                        def_json["contextSpan"] = serde_json::json!({
                            "start": ctx_start_off,
                            "length": ctx_end_off.saturating_sub(ctx_start_off),
                        });
                    }
                    def_json
                } else {
                    Self::build_fallback_definition(&file, &kind_str, &symbol_name)
                }
            } else {
                Self::build_fallback_definition(&file, &kind_str, &symbol_name)
            };

            // Build references array with byte-offset textSpans
            // Compute cursor offset for isDefinition check - TypeScript only sets
            // isDefinition=true when the cursor is ON the definition reference
            let cursor_offset = line_map
                .position_to_offset(position, &source_text)
                .unwrap_or(0);

            let mut references: Vec<serde_json::Value> = ref_infos
                .iter()
                .map(|ref_info| {
                    let start =
                        line_map.position_to_offset(ref_info.location.range.start, &source_text);
                    let end =
                        line_map.position_to_offset(ref_info.location.range.end, &source_text);
                    let start_off = start.unwrap_or(0);
                    let end_off = end.unwrap_or(0);

                    // isDefinition is only true when: (1) the reference IS a definition,
                    // AND (2) the cursor is at that reference's position
                    let is_definition = ref_info.is_definition
                        && cursor_offset >= start_off
                        && cursor_offset < end_off;

                    let mut ref_json = serde_json::json!({
                        "fileName": ref_info.location.file_path,
                        "textSpan": {
                            "start": start_off,
                            "length": end_off.saturating_sub(start_off),
                        },
                        "isWriteAccess": ref_info.is_write_access,
                        "isDefinition": is_definition,
                    });

                    // Add contextSpan for definition references
                    // Skip when contextSpan matches textSpan (e.g., catch clause variables)
                    if ref_info.is_definition
                        && let Some(ref defs) = def_infos
                    {
                        for def in defs {
                            if def.location.range == ref_info.location.range
                                && let Some(ref ctx) = def.context_span
                            {
                                let ctx_start =
                                    line_map.position_to_offset(ctx.start, &source_text);
                                let ctx_end = line_map.position_to_offset(ctx.end, &source_text);
                                let ctx_start_off = ctx_start.unwrap_or(0);
                                let ctx_end_off = ctx_end.unwrap_or(0);
                                // Only add contextSpan if it differs from the textSpan
                                if ctx_start_off != start_off || ctx_end_off != end_off {
                                    ref_json["contextSpan"] = serde_json::json!({
                                        "start": ctx_start_off,
                                        "length": ctx_end_off.saturating_sub(ctx_start_off),
                                    });
                                }
                                break;
                            }
                        }
                        if symbol.flags & tsz::binder::symbol_flags::ALIAS != 0
                            && let Some((ctx_start_off, ctx_end_off)) =
                                Self::import_statement_context_span(&source_text, start_off)
                        {
                            ref_json["contextSpan"] = serde_json::json!({
                                "start": ctx_start_off,
                                "length": ctx_end_off.saturating_sub(ctx_start_off),
                            });
                        }
                    }
                    ref_json
                })
                .collect();
            if Self::is_quoted_import_or_export_specifier_offset(&arena, &source_text, query_offset)
                && let Some(mut project) = self.build_project_for_file(&file)
                && let Some(locs) = self.quoted_alias_chain_references(
                    &mut project,
                    &file,
                    &arena,
                    &source_text,
                    query_offset,
                    position,
                    false,
                )
            {
                let canonical_loc = self.canonical_definition_for_alias_position(
                    &file,
                    &arena,
                    &source_text,
                    query_offset,
                );
                let mut seen: rustc_hash::FxHashSet<(String, u32, u32)> = references
                    .iter()
                    .filter_map(|entry| {
                        let file_name = entry.get("fileName")?.as_str()?.to_string();
                        let span = entry.get("textSpan")?;
                        let start = span.get("start")?.as_u64()? as u32;
                        let len = span.get("length")?.as_u64()? as u32;
                        Some((file_name, start, len))
                    })
                    .collect();
                for loc in locs {
                    let loc_source = self
                        .open_files
                        .get(&loc.file_path)
                        .cloned()
                        .or_else(|| std::fs::read_to_string(&loc.file_path).ok())
                        .unwrap_or_default();
                    let loc_line_map = LineMap::build(&loc_source);
                    let start_off = loc_line_map
                        .position_to_offset(loc.range.start, &loc_source)
                        .unwrap_or(0);
                    let end_off = loc_line_map
                        .position_to_offset(loc.range.end, &loc_source)
                        .unwrap_or(start_off);
                    let len = end_off.saturating_sub(start_off);
                    let key = (loc.file_path.clone(), start_off, len);
                    if !seen.insert(key) {
                        continue;
                    }
                    let is_definition = canonical_loc.as_ref().is_some_and(|def| {
                        loc.file_path == def.file_path
                            && loc.range == def.range
                            && cursor_offset >= start_off
                            && cursor_offset < end_off
                    });
                    references.push(serde_json::json!({
                        "fileName": loc.file_path,
                        "textSpan": {
                            "start": start_off,
                            "length": len,
                        },
                        "isWriteAccess": false,
                        "isDefinition": is_definition,
                    }));
                }
            }

            // Return as ReferencedSymbol array (single entry for single-file)
            Some(serde_json::json!([{
                "definition": definition,
                "references": references,
            }]))
        })();
        self.success_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }
}
