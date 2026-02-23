//! Navigation, definition, and reference handlers for tsz-server.

use super::{Server, TsServerRequest, TsServerResponse};
use tsz::binder::SymbolId;
use tsz::lsp::definition::GoToDefinition;
use tsz::lsp::highlighting::DocumentHighlightProvider;
use tsz::lsp::hover::HoverProvider;
use tsz::lsp::implementation::GoToImplementationProvider;
use tsz::lsp::position::LineMap;
use tsz::lsp::project::Project;
use tsz::lsp::references::FindReferences;
use tsz::lsp::rename::RenameProvider;
use tsz::lsp::symbols::document_symbols::DocumentSymbolProvider;
use tsz::parser::node::NodeAccess;
use tsz_solver::TypeInterner;

/// Bundled context for a parsed file, reducing parameter count in helpers.
pub(super) struct ParsedFileContext<'a> {
    pub(super) arena: &'a tsz::parser::node::NodeArena,
    pub(super) binder: &'a tsz::binder::BinderState,
    pub(super) line_map: &'a LineMap,
    pub(super) root: tsz::parser::NodeIndex,
    pub(super) source_text: &'a str,
    pub(super) file: &'a str,
}

/// Map a `DocumentSymbol`'s kind + `kind_modifiers` to the tsserver `ScriptElementKind` string.
fn symbol_kind_to_tsserver(
    kind: tsz::lsp::symbols::document_symbols::SymbolKind,
    kind_modifiers: &str,
) -> &'static str {
    use tsz::lsp::symbols::document_symbols::SymbolKind;
    match kind {
        SymbolKind::Module => "module",
        SymbolKind::Class => "class",
        SymbolKind::Method => "method",
        SymbolKind::Property | SymbolKind::Field => "property",
        SymbolKind::Constructor => "constructor",
        SymbolKind::Enum => "enum",
        SymbolKind::Interface => "interface",
        SymbolKind::Function => "function",
        SymbolKind::Variable => {
            if kind_modifiers.contains("let") {
                "let"
            } else {
                "var"
            }
        }
        SymbolKind::Constant => "const",
        SymbolKind::EnumMember => "enum member",
        SymbolKind::TypeParameter => "type parameter",
        SymbolKind::Struct => "type",
        _ => "unknown",
    }
}

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
                && let Some(def) = self.definition_info_from_location(&canonical_loc)
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
                .map(|info| Self::definition_info_to_json(info, &file))
                .collect();
            Some(serde_json::json!(body))
        })();
        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    pub(crate) fn handle_definition_and_bound_span(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
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
                && let Some(definition) = self.definition_info_from_location(&canonical_loc)
            {
                let text_span = if Self::is_quoted_import_or_export_specifier_offset(
                    &arena,
                    &source_text,
                    offset,
                ) {
                    let node_idx = tsz::lsp::utils::find_node_at_or_before_offset(
                        &arena,
                        offset,
                        &source_text,
                    );
                    if node_idx.is_some() {
                        if let Some(node) = arena.get(node_idx) {
                            serde_json::json!({
                                "start": Self::lsp_to_tsserver_position(line_map.offset_to_position(node.pos, &source_text)),
                                "end": Self::lsp_to_tsserver_position(line_map.offset_to_position(node.end, &source_text)),
                            })
                        } else {
                            serde_json::json!({
                                "start": Self::lsp_to_tsserver_position(position),
                                "end": Self::lsp_to_tsserver_position(position),
                            })
                        }
                    } else {
                        serde_json::json!({
                            "start": Self::lsp_to_tsserver_position(position),
                            "end": Self::lsp_to_tsserver_position(position),
                        })
                    }
                } else {
                    serde_json::json!({
                        "start": Self::lsp_to_tsserver_position(position),
                        "end": Self::lsp_to_tsserver_position(position),
                    })
                };
                let text_span = serde_json::json!({
                    "start": text_span["start"].clone(),
                    "end": text_span["end"].clone(),
                });
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

            // Build definitions array with rich metadata
            let definitions: Vec<serde_json::Value> = infos
                .iter()
                .map(|info| Self::definition_info_to_json(info, &file))
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
            let text_span = symbol_range
                .map(|range| {
                    serde_json::json!({
                        "start": Self::lsp_to_tsserver_position(range.start),
                        "end": Self::lsp_to_tsserver_position(range.end),
                    })
                })
                .unwrap_or_else(|| {
                    serde_json::json!({
                        "start": Self::lsp_to_tsserver_position(position),
                        "end": Self::lsp_to_tsserver_position(position),
                    })
                });

            Some(serde_json::json!({
                "definitions": definitions,
                "textSpan": text_span,
            }))
        })();
        self.stub_response(
            seq,
            request,
            Some(result.unwrap_or(serde_json::json!({
                "definitions": [],
                "textSpan": {"start": {"line": 1, "offset": 1}, "end": {"line": 1, "offset": 1}}
            }))),
        )
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
                let definition_locs = project.get_definition(&file, position).unwrap_or_default();
                let refs: Vec<serde_json::Value> = locs
                    .iter()
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
                }));
            }
            let provider = FindReferences::new(&arena, &binder, &line_map, file, &source_text);
            let (_symbol_id, ref_infos) = provider.find_references_with_symbol(root, position)?;

            // Try to get symbol name from the position
            let symbol_name = {
                let ref_offset = line_map.position_to_offset(position, &source_text)?;
                let node_idx = tsz::lsp::utils::find_node_at_offset(&arena, ref_offset);
                if node_idx.is_some() {
                    arena
                        .get_identifier_text(node_idx)
                        .map(std::string::ToString::to_string)
                } else {
                    None
                }
            }
            .unwrap_or_default();

            let refs: Vec<serde_json::Value> = ref_infos
                .iter()
                .map(|ref_info| {
                    serde_json::json!({
                        "file": ref_info.location.file_path,
                        "start": Self::lsp_to_tsserver_position(ref_info.location.range.start),
                        "end": Self::lsp_to_tsserver_position(ref_info.location.range.end),
                        "lineText": ref_info.line_text,
                        "isWriteAccess": ref_info.is_write_access,
                        "isDefinition": ref_info.is_definition,
                    })
                })
                .collect();
            Some(serde_json::json!({
                "refs": refs,
                "symbolName": symbol_name,
            }))
        })();
        self.stub_response(
            seq,
            request,
            Some(result.unwrap_or(serde_json::json!({"refs": [], "symbolName": ""}))),
        )
    }

    pub(crate) fn handle_document_highlights(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let (file, line, offset) = Self::extract_file_position(&request.arguments)?;
            let (arena, binder, root, source_text) = self.parse_and_bind_file(&file)?;
            let line_map = LineMap::build(&source_text);
            let position = Self::tsserver_to_lsp_position(line, offset);
            let provider = DocumentHighlightProvider::new(&arena, &binder, &line_map, &source_text);
            let highlights = provider.get_document_highlights(root, position)?;

            // Group highlights by file (tsserver groups by file, each with highlightSpans)
            let highlight_spans: Vec<serde_json::Value> = highlights
                .iter()
                .map(|hl| {
                    let kind_str = match hl.kind {
                        Some(tsz::lsp::highlighting::DocumentHighlightKind::Read) => "reference",
                        Some(tsz::lsp::highlighting::DocumentHighlightKind::Write) => {
                            "writtenReference"
                        }
                        Some(tsz::lsp::highlighting::DocumentHighlightKind::Text) | None => "none",
                    };
                    serde_json::json!({
                        "start": Self::lsp_to_tsserver_position(hl.range.start),
                        "end": Self::lsp_to_tsserver_position(hl.range.end),
                        "kind": kind_str,
                    })
                })
                .collect();
            // All highlights are in the same file for now
            Some(serde_json::json!([{
                "file": file,
                "highlightSpans": highlight_spans,
            }]))
        })();
        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    pub(crate) fn handle_rename(
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

            // Compute trigger span length from the range
            let start_offset = line_map
                .position_to_offset(info.trigger_span.start, &source_text)
                .unwrap_or(0) as usize;
            let end_offset = line_map
                .position_to_offset(info.trigger_span.end, &source_text)
                .unwrap_or(0) as usize;
            let trigger_length = end_offset.saturating_sub(start_offset);
            let rename_seed =
                Self::quoted_specifier_literal_at_offset(&arena, &source_text, query_offset);

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
        self.stub_response(
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
        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    fn build_quoted_alias_referenced_symbols(
        &mut self,
        project: &mut Project,
        file: &str,
        arena: &tsz::parser::node::NodeArena,
        source_text: &str,
        query_offset: u32,
        query_position: tsz_common::position::Position,
    ) -> Option<Vec<serde_json::Value>> {
        let locs = self.quoted_alias_chain_references(
            project,
            file,
            arena,
            source_text,
            query_offset,
            query_position,
            false,
        )?;
        let line_map = LineMap::build(source_text);
        let cursor_offset = line_map
            .position_to_offset(query_position, source_text)
            .unwrap_or(0);

        #[derive(Default)]
        struct RefGroup {
            definition: serde_json::Value,
            references: Vec<serde_json::Value>,
            seen_refs: rustc_hash::FxHashSet<(String, u32, u32)>,
        }

        let mut groups: Vec<RefGroup> = Vec::new();
        let mut group_index_by_key: rustc_hash::FxHashMap<String, usize> =
            rustc_hash::FxHashMap::default();
        let mut seen_refs_global: rustc_hash::FxHashSet<(String, u32, u32)> =
            rustc_hash::FxHashSet::default();

        for seed_loc in locs {
            let mut use_seed = false;
            let mut seed_source_text = String::new();
            if let Some(seed_source) = self
                .open_files
                .get(&seed_loc.file_path)
                .cloned()
                .or_else(|| std::fs::read_to_string(&seed_loc.file_path).ok())
            {
                seed_source_text = seed_source.clone();
                let line_text = seed_source
                    .lines()
                    .nth(seed_loc.range.start.line as usize)
                    .unwrap_or("")
                    .trim_start();
                let is_export_line = line_text.starts_with("export ");
                let is_import_line = line_text.starts_with("import ");
                let is_quoted_seed = self.is_quoted_import_or_export_specifier_location(&seed_loc);

                // Group seeds around symbol-producing locations:
                // - quoted names from export specifiers
                // - local alias identifiers from import specifiers
                use_seed =
                    (is_quoted_seed && is_export_line) || (!is_quoted_seed && is_import_line);
            }
            if !use_seed {
                continue;
            }
            let seed_line_map = LineMap::build(&seed_source_text);
            let seed_start = seed_line_map
                .position_to_offset(seed_loc.range.start, &seed_source_text)
                .unwrap_or(0) as usize;
            let seed_end = seed_line_map
                .position_to_offset(seed_loc.range.end, &seed_source_text)
                .unwrap_or(seed_start as u32) as usize;
            let seed_text = seed_source_text
                .get(seed_start..seed_end)
                .unwrap_or_default()
                .trim()
                .to_string();
            if seed_text.is_empty() {
                continue;
            }

            let (definition, def_file, def_start, def_len) =
                self.build_alias_definition_from_location(&seed_loc);
            let group_key = format!("{def_file}:{def_start}:{def_len}");
            let group_idx = if let Some(idx) = group_index_by_key.get(&group_key).copied() {
                idx
            } else {
                let idx = groups.len();
                groups.push(RefGroup {
                    definition,
                    ..RefGroup::default()
                });
                group_index_by_key.insert(group_key, idx);
                idx
            };

            let mut symbol_refs = project
                .find_references(&seed_loc.file_path, seed_loc.range.start)
                .unwrap_or_default();
            symbol_refs.push(seed_loc);

            for mut loc in symbol_refs {
                if let Some((loc_arena, _binder, _root, loc_source)) =
                    self.parse_and_bind_file(&loc.file_path)
                {
                    let loc_line_map = LineMap::build(&loc_source);
                    if let Some(start_off) =
                        loc_line_map.position_to_offset(loc.range.start, &loc_source)
                        && Self::is_quoted_import_or_export_specifier_offset(
                            &loc_arena,
                            &loc_source,
                            start_off,
                        )
                        && let Some(inner_range) = Self::quoted_specifier_inner_range_at_offset(
                            &loc_arena,
                            &loc_source,
                            start_off,
                        )
                    {
                        loc.range = inner_range;
                    }
                }

                let loc_source = self
                    .open_files
                    .get(&loc.file_path)
                    .cloned()
                    .or_else(|| std::fs::read_to_string(&loc.file_path).ok())
                    .unwrap_or_default();
                let loc_line_map = LineMap::build(&loc_source);
                let start = loc_line_map
                    .position_to_offset(loc.range.start, &loc_source)
                    .unwrap_or(0);
                let end = loc_line_map
                    .position_to_offset(loc.range.end, &loc_source)
                    .unwrap_or(start);
                let len = end.saturating_sub(start);
                let key = (loc.file_path.clone(), start, len);
                if !groups[group_idx].seen_refs.insert(key) {
                    continue;
                }
                let global_key = (loc.file_path.clone(), start, len);
                if !seen_refs_global.insert(global_key) {
                    continue;
                }
                let loc_text = loc_source
                    .get(start as usize..end as usize)
                    .unwrap_or_default()
                    .trim();
                if loc_text != seed_text {
                    continue;
                }

                let is_definition = loc.file_path == file
                    && start <= cursor_offset
                    && cursor_offset < end
                    && loc.file_path == def_file
                    && start == def_start
                    && len == def_len;

                groups[group_idx].references.push(serde_json::json!({
                    "fileName": loc.file_path,
                    "textSpan": {
                        "start": start,
                        "length": len,
                    },
                    "isWriteAccess": false,
                    "isDefinition": is_definition,
                }));
            }
        }

        if groups.is_empty() {
            return None;
        }

        for group in &mut groups {
            group.references.sort_by(|a, b| {
                let a_file = a
                    .get("fileName")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                let b_file = b
                    .get("fileName")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                let file_cmp = a_file.cmp(b_file);
                if file_cmp != std::cmp::Ordering::Equal {
                    return file_cmp;
                }
                let a_start = a
                    .get("textSpan")
                    .and_then(|span| span.get("start"))
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);
                let b_start = b
                    .get("textSpan")
                    .and_then(|span| span.get("start"))
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);
                a_start.cmp(&b_start)
            });
        }

        groups.sort_by(|a, b| {
            let a_file = a
                .definition
                .get("fileName")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            let b_file = b
                .definition
                .get("fileName")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            let a_is_query_file = a_file == file;
            let b_is_query_file = b_file == file;
            if a_is_query_file != b_is_query_file {
                return b_is_query_file.cmp(&a_is_query_file);
            }
            let file_cmp = a_file.cmp(b_file);
            if file_cmp != std::cmp::Ordering::Equal {
                return file_cmp;
            }
            let a_start = a
                .definition
                .get("textSpan")
                .and_then(|span| span.get("start"))
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0);
            let b_start = b
                .definition
                .get("textSpan")
                .and_then(|span| span.get("start"))
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0);
            a_start.cmp(&b_start)
        });

        Some(
            groups
                .into_iter()
                .filter(|group| !group.references.is_empty())
                .map(|group| {
                    serde_json::json!({
                        "definition": group.definition,
                        "references": group.references,
                    })
                })
                .collect(),
        )
    }

    fn build_alias_definition_from_location(
        &mut self,
        loc: &tsz_common::position::Location,
    ) -> (serde_json::Value, String, u32, u32) {
        fn extract_alias_rhs(display: &str) -> Option<String> {
            if let Some((_, rhs)) = display.rsplit_once(" = ") {
                return Some(rhs.trim().to_string());
            }
            if let Some((_, rhs)) = display.rsplit_once(": ") {
                return Some(rhs.trim().to_string());
            }
            None
        }

        let mut source_text = self
            .open_files
            .get(&loc.file_path)
            .cloned()
            .or_else(|| std::fs::read_to_string(&loc.file_path).ok())
            .unwrap_or_default();
        let mut target_range = loc.range;
        let mut parsed = self.parse_and_bind_file(&loc.file_path);
        if let Some((arena, _binder, _root, source)) = parsed.as_ref() {
            source_text = source.clone();
            let lm = LineMap::build(&source_text);
            if let Some(offset) = lm.position_to_offset(loc.range.start, &source_text) {
                let node_idx =
                    tsz::lsp::utils::find_node_at_or_before_offset(arena, offset, &source_text);
                let spec_idx = if Self::find_ancestor_of_kind(
                    arena,
                    node_idx,
                    tsz::parser::syntax_kind_ext::IMPORT_SPECIFIER,
                )
                .is_some()
                {
                    Self::find_ancestor_of_kind(
                        arena,
                        node_idx,
                        tsz::parser::syntax_kind_ext::IMPORT_SPECIFIER,
                    )
                } else {
                    Self::find_ancestor_of_kind(
                        arena,
                        node_idx,
                        tsz::parser::syntax_kind_ext::EXPORT_SPECIFIER,
                    )
                };
                if spec_idx.is_some()
                    && let Some(spec_node) = arena.get(spec_idx)
                    && let Some(spec) = arena.get_specifier(spec_node)
                    && spec.name.is_some()
                    && let Some(alias_node) = arena.get(spec.name)
                {
                    target_range = tsz_common::position::Range::new(
                        lm.offset_to_position(alias_node.pos, &source_text),
                        lm.offset_to_position(alias_node.end, &source_text),
                    );
                }
            }
        }

        let line_map = LineMap::build(&source_text);
        let start = line_map
            .position_to_offset(target_range.start, &source_text)
            .unwrap_or(0);
        let end = line_map
            .position_to_offset(target_range.end, &source_text)
            .unwrap_or(start);
        let len = end.saturating_sub(start);

        let display = parsed
            .take()
            .and_then(|(arena, binder, root, _source)| {
                let lm = LineMap::build(&source_text);
                let interner = TypeInterner::new();
                let hover = HoverProvider::new(
                    &arena,
                    &binder,
                    &lm,
                    &interner,
                    &source_text,
                    loc.file_path.clone(),
                );
                let mut type_cache = None;
                hover
                    .get_hover(root, target_range.start, &mut type_cache)
                    .map(|h| h.display_string)
            })
            .unwrap_or_else(|| "alias".to_string());
        let mut display = display;
        if display == "alias" || display.starts_with("(alias) module ") {
            let line_text = source_text
                .lines()
                .nth(target_range.start.line as usize)
                .unwrap_or("")
                .trim_start();
            let import_or_export = if line_text.starts_with("export ") {
                "export"
            } else if line_text.starts_with("import ") {
                "import"
            } else {
                ""
            };
            let keyword = if line_text.contains("{ type ") || line_text.starts_with("type ") {
                "type"
            } else {
                "const"
            };
            let alias_start = line_map
                .position_to_offset(target_range.start, &source_text)
                .unwrap_or(0) as usize;
            let alias_end = line_map
                .position_to_offset(target_range.end, &source_text)
                .unwrap_or(alias_start as u32) as usize;
            let alias_name = source_text
                .get(alias_start..alias_end)
                .unwrap_or_default()
                .trim()
                .to_string();
            let canonical_rhs = self.parse_and_bind_file(&loc.file_path).and_then(
                |(arena, _binder, _root, parsed_source)| {
                    let lm = LineMap::build(&parsed_source);
                    let query_off = lm.position_to_offset(loc.range.start, &parsed_source)?;
                    let canonical_loc = self.canonical_definition_for_alias_position(
                        &loc.file_path,
                        &arena,
                        &parsed_source,
                        query_off,
                    )?;
                    let (canon_arena, canon_binder, canon_root, canon_source) =
                        self.parse_and_bind_file(&canonical_loc.file_path)?;
                    let canon_lm = LineMap::build(&canon_source);
                    let interner = TypeInterner::new();
                    let hover = HoverProvider::new(
                        &canon_arena,
                        &canon_binder,
                        &canon_lm,
                        &interner,
                        &canon_source,
                        canonical_loc.file_path.clone(),
                    );
                    let mut type_cache = None;
                    hover
                        .get_hover(canon_root, canonical_loc.range.start, &mut type_cache)
                        .and_then(|h| extract_alias_rhs(&h.display_string))
                },
            );
            if !alias_name.is_empty()
                && !import_or_export.is_empty()
                && let Some(rhs) = canonical_rhs
            {
                display = if keyword == "type" {
                    format!("(alias) type {alias_name} = {rhs}\n{import_or_export} {alias_name}")
                } else {
                    format!("(alias) const {alias_name}: {rhs}\n{import_or_export} {alias_name}")
                };
            }
        }

        let def = serde_json::json!({
            "containerKind": "",
            "containerName": "",
            "kind": "alias",
            "name": display,
            "displayParts": Self::parse_display_string_to_parts(&display, "alias", "alias"),
            "fileName": loc.file_path.clone(),
            "textSpan": { "start": start, "length": len },
        });
        (def, loc.file_path.clone(), start, len)
    }

    pub(crate) fn build_fallback_definition(
        file: &str,
        kind: &str,
        name: &str,
    ) -> serde_json::Value {
        serde_json::json!({
            "containerKind": "",
            "containerName": "",
            "kind": kind,
            "name": format!("{} {}", kind, name),
            "displayParts": Self::build_simple_display_parts(kind, name),
            "fileName": file,
            "textSpan": { "start": 0, "length": 0 },
        })
    }

    pub(crate) fn build_simple_display_parts(kind: &str, name: &str) -> Vec<serde_json::Value> {
        let mut parts = vec![];
        if !kind.is_empty() {
            parts.push(serde_json::json!({ "text": kind, "kind": "keyword" }));
            parts.push(serde_json::json!({ "text": " ", "kind": "space" }));
        }
        let name_kind = Self::symbol_kind_to_display_part_kind(kind);
        parts.push(serde_json::json!({ "text": name, "kind": name_kind }));
        parts
    }

    pub(crate) fn symbol_kind_to_display_part_kind(kind: &str) -> &'static str {
        match kind {
            "class" => "className",
            "function" => "functionName",
            "interface" => "interfaceName",
            "enum" => "enumName",
            "enum member" => "enumMemberName",
            "module" | "namespace" => "moduleName",
            "type" => "aliasName",
            "method" => "methodName",
            "property" => "propertyName",
            _ => "localName",
        }
    }

    /// Parse a display string (e.g. "const x: number") into structured displayParts.
    /// This handles common patterns from the `HoverProvider`.
    pub(crate) fn parse_display_string_to_parts(
        display_string: &str,
        kind: &str,
        name: &str,
    ) -> Vec<serde_json::Value> {
        let name_kind = Self::symbol_kind_to_display_part_kind(kind);

        // Handle prefixed forms like "(local var) x: type" or "(parameter) x: type"
        let s = display_string;

        // Special-case alias module displays:
        // "(alias) module \"jquery\"\nimport x"
        if let Some(rest) = s.strip_prefix("(alias) module ") {
            let mut parts = vec![
                serde_json::json!({ "text": "(", "kind": "punctuation" }),
                serde_json::json!({ "text": "alias", "kind": "text" }),
                serde_json::json!({ "text": ")", "kind": "punctuation" }),
                serde_json::json!({ "text": " ", "kind": "space" }),
                serde_json::json!({ "text": "module", "kind": "keyword" }),
                serde_json::json!({ "text": " ", "kind": "space" }),
            ];

            if let Some(after_quote) = rest.strip_prefix('"')
                && let Some(end_quote_idx) = after_quote.find('"')
            {
                let quoted = &after_quote[..end_quote_idx];
                parts.push(
                    serde_json::json!({ "text": format!("\"{quoted}\""), "kind": "stringLiteral" }),
                );
                let after_module = &after_quote[end_quote_idx + 1..];
                if let Some(import_rest) = after_module.strip_prefix("\nimport ") {
                    parts.push(serde_json::json!({ "text": "\n", "kind": "lineBreak" }));
                    parts.push(serde_json::json!({ "text": "import", "kind": "keyword" }));
                    parts.push(serde_json::json!({ "text": " ", "kind": "space" }));
                    if let Some(eq_idx) = import_rest.find(" = ") {
                        let alias_name = import_rest[..eq_idx].trim();
                        parts.push(serde_json::json!({ "text": alias_name, "kind": "aliasName" }));
                        parts.push(serde_json::json!({ "text": import_rest[eq_idx..].to_string(), "kind": "text" }));
                    } else {
                        parts.push(
                            serde_json::json!({ "text": import_rest.trim(), "kind": "aliasName" }),
                        );
                    }
                    return parts;
                }
                return parts;
            }
        }

        // Check for parenthesized prefix like "(local var)" or "(parameter)"
        if let Some(rest) = s.strip_prefix('(')
            && let Some(paren_end) = rest.find(')')
        {
            let prefix = &rest[..paren_end];
            let after_paren = rest[paren_end + 1..].trim_start();

            let mut parts = vec![];
            parts.push(serde_json::json!({ "text": "(", "kind": "punctuation" }));

            // Split prefix words
            let prefix_words: Vec<&str> = prefix.split_whitespace().collect();
            for (i, word) in prefix_words.iter().enumerate() {
                if i > 0 {
                    parts.push(serde_json::json!({ "text": " ", "kind": "space" }));
                }
                parts.push(serde_json::json!({ "text": *word, "kind": "keyword" }));
            }
            parts.push(serde_json::json!({ "text": ")", "kind": "punctuation" }));
            parts.push(serde_json::json!({ "text": " ", "kind": "space" }));

            // Parse the rest: "name: type" or "name(sig): type"
            Self::parse_name_and_type(after_paren, name_kind, &mut parts);
            return parts;
        }

        // Handle "keyword name: type" or "keyword name" patterns
        let keywords = [
            "const",
            "let",
            "var",
            "function",
            "class",
            "interface",
            "enum",
            "type",
            "namespace",
        ];
        for kw in &keywords {
            if let Some(rest) = s.strip_prefix(kw)
                && rest.starts_with(' ')
            {
                let mut parts = vec![];
                parts.push(serde_json::json!({ "text": *kw, "kind": "keyword" }));
                parts.push(serde_json::json!({ "text": " ", "kind": "space" }));
                let rest = rest.trim_start();
                Self::parse_name_and_type(rest, name_kind, &mut parts);
                return parts;
            }
        }

        // Fallback: just use the display_string as-is
        Self::build_simple_display_parts(kind, name)
    }

    /// Parse "name: type" or "name(params): type" or just "name" from a string.
    pub(crate) fn parse_name_and_type(
        s: &str,
        name_kind: &str,
        parts: &mut Vec<serde_json::Value>,
    ) {
        // Find where the name ends - it could be followed by ':', '(', '<', '=', or end of string
        let name_end = s.find([':', '(', '<', '=']).unwrap_or(s.len());
        let name_part = s[..name_end].trim_end();

        if !name_part.is_empty() {
            // Check if name contains '.' for qualified names like "Foo.bar"
            if let Some(dot_pos) = name_part.rfind('.') {
                let container = &name_part[..dot_pos];
                let member = &name_part[dot_pos + 1..];
                parts.push(serde_json::json!({ "text": container, "kind": "className" }));
                parts.push(serde_json::json!({ "text": ".", "kind": "punctuation" }));
                parts.push(serde_json::json!({ "text": member, "kind": name_kind }));
            } else {
                parts.push(serde_json::json!({ "text": name_part, "kind": name_kind }));
            }
        }

        let remaining = &s[name_end..];
        if remaining.is_empty() {
            return;
        }

        // Handle signature parts like "(params): type" or "= type" or ": type"
        if remaining.starts_with('(') {
            // Function signature - add everything as-is for now with punctuation
            Self::parse_signature(remaining, parts);
        } else if let Some(rest) = remaining.strip_prefix(": ") {
            parts.push(serde_json::json!({ "text": ":", "kind": "punctuation" }));
            parts.push(serde_json::json!({ "text": " ", "kind": "space" }));
            Self::parse_type_string(rest, parts);
        } else if let Some(rest) = remaining.strip_prefix(":") {
            parts.push(serde_json::json!({ "text": ":", "kind": "punctuation" }));
            parts.push(serde_json::json!({ "text": " ", "kind": "space" }));
            Self::parse_type_string(rest.trim_start(), parts);
        } else if let Some(rest) = remaining.strip_prefix(" = ") {
            parts.push(serde_json::json!({ "text": " ", "kind": "space" }));
            parts.push(serde_json::json!({ "text": "=", "kind": "operator" }));
            parts.push(serde_json::json!({ "text": " ", "kind": "space" }));
            Self::parse_type_string(rest, parts);
        }
    }

    /// Parse a type string into display parts.
    pub(crate) fn parse_type_string(type_str: &str, parts: &mut Vec<serde_json::Value>) {
        let type_str = type_str.trim();
        if type_str.is_empty() {
            return;
        }

        // Check for TypeScript keyword types
        let keyword_types = [
            "any",
            "boolean",
            "bigint",
            "never",
            "null",
            "number",
            "object",
            "string",
            "symbol",
            "undefined",
            "unknown",
            "void",
            "true",
            "false",
        ];
        if keyword_types.contains(&type_str) {
            parts.push(serde_json::json!({ "text": type_str, "kind": "keyword" }));
            return;
        }

        // Check for numeric literal
        if type_str.parse::<f64>().is_ok() {
            parts.push(serde_json::json!({ "text": type_str, "kind": "stringLiteral" }));
            return;
        }

        // Check for string literal (starts and ends with quotes)
        if (type_str.starts_with('"') && type_str.ends_with('"'))
            || (type_str.starts_with('\'') && type_str.ends_with('\''))
        {
            parts.push(serde_json::json!({ "text": type_str, "kind": "stringLiteral" }));
            return;
        }

        // Default: treat as text (could be a complex type, interface name, etc.)
        parts.push(serde_json::json!({ "text": type_str, "kind": "text" }));
    }

    /// Parse a function signature like "(x: number): string" into parts.
    pub(crate) fn parse_signature(sig: &str, parts: &mut Vec<serde_json::Value>) {
        // For now, add the whole signature as text parts
        // This handles the common case of function signatures
        parts.push(serde_json::json!({ "text": sig, "kind": "text" }));
    }

    pub(crate) fn handle_navtree(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let (arena, _binder, root, source_text) = self.parse_and_bind_file(file)?;
            let line_map = LineMap::build(&source_text);
            let provider = DocumentSymbolProvider::new(&arena, &line_map, &source_text);
            let symbols = provider.get_document_symbols(root);

            fn symbol_to_navtree(
                sym: &tsz::lsp::symbols::document_symbols::DocumentSymbol,
            ) -> serde_json::Value {
                let kind = if matches!(
                    sym.kind,
                    tsz::lsp::symbols::document_symbols::SymbolKind::File
                        | tsz::lsp::symbols::document_symbols::SymbolKind::Namespace
                ) {
                    "module"
                } else {
                    symbol_kind_to_tsserver(sym.kind, &sym.kind_modifiers)
                };
                let children: Vec<serde_json::Value> =
                    sym.children.iter().map(symbol_to_navtree).collect();
                let mut obj = serde_json::json!({
                    "text": sym.name,
                    "kind": kind,
                    "spans": [{
                        "start": {
                            "line": sym.range.start.line + 1,
                            "offset": sym.range.start.character + 1,
                        },
                        "end": {
                            "line": sym.range.end.line + 1,
                            "offset": sym.range.end.character + 1,
                        },
                    }],
                });
                if !children.is_empty() {
                    obj["childItems"] = serde_json::json!(children);
                }
                // Filter out internal "let" modifier
                let kind_mods = sym
                    .kind_modifiers
                    .split(',')
                    .filter(|m| !m.is_empty() && *m != "let")
                    .collect::<Vec<_>>()
                    .join(",");
                if !kind_mods.is_empty() {
                    obj["kindModifiers"] = serde_json::json!(kind_mods);
                }
                obj
            }

            let child_items: Vec<serde_json::Value> =
                symbols.iter().map(symbol_to_navtree).collect();

            // Compute the end span based on source text length
            let total_lines = source_text.lines().count();
            let last_line_len = source_text.lines().last().map_or(0, str::len);
            Some(serde_json::json!({
                "text": "<global>",
                "kind": "script",
                "childItems": child_items,
                "spans": [{"start": {"line": 1, "offset": 1}, "end": {"line": total_lines, "offset": last_line_len + 1}}],
            }))
        })();
        self.stub_response(
            seq,
            request,
            Some(result.unwrap_or(serde_json::json!({
                "text": "<global>",
                "kind": "script",
                "childItems": [],
                "spans": [{"start": {"line": 1, "offset": 1}, "end": {"line": 1, "offset": 1}}],
            }))),
        )
    }

    pub(crate) fn handle_navbar(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let (arena, _binder, root, source_text) = self.parse_and_bind_file(file)?;
            let line_map = LineMap::build(&source_text);
            let provider = DocumentSymbolProvider::new(&arena, &line_map, &source_text);
            let symbols = provider.get_document_symbols(root);

            /// Check if a symbol should appear as its own entry in the primary
            /// navigation bar menu (matching TypeScript's shouldAppearInPrimaryNavBarMenu).
            const fn should_appear_in_primary_navbar(
                sym: &tsz::lsp::symbols::document_symbols::DocumentSymbol,
            ) -> bool {
                use tsz::lsp::symbols::document_symbols::SymbolKind;
                // Items with children always appear
                if !sym.children.is_empty() {
                    return true;
                }
                // Container-like declarations always appear
                matches!(
                    sym.kind,
                    SymbolKind::Class
                        | SymbolKind::Enum
                        | SymbolKind::Interface
                        | SymbolKind::Module
                        | SymbolKind::Namespace
                        | SymbolKind::File
                        | SymbolKind::Struct // type alias
                        | SymbolKind::Function
                )
            }

            fn navbar_child_item(
                c: &tsz::lsp::symbols::document_symbols::DocumentSymbol,
            ) -> serde_json::Value {
                let mut item = serde_json::json!({
                    "text": c.name,
                    "kind": symbol_kind_to_tsserver(c.kind, &c.kind_modifiers),
                    "spans": [{
                        "start": {
                            "line": c.range.start.line + 1,
                            "offset": c.range.start.character + 1,
                        },
                        "end": {
                            "line": c.range.end.line + 1,
                            "offset": c.range.end.character + 1,
                        },
                    }],
                });
                let kind_mods = c
                    .kind_modifiers
                    .split(',')
                    .filter(|m| !m.is_empty() && *m != "let")
                    .collect::<Vec<_>>()
                    .join(",");
                if !kind_mods.is_empty() {
                    item["kindModifiers"] = serde_json::json!(kind_mods);
                }
                item
            }

            fn symbol_to_navbar_item(
                sym: &tsz::lsp::symbols::document_symbols::DocumentSymbol,
                indent: usize,
                items: &mut Vec<serde_json::Value>,
            ) {
                let kind = if matches!(
                    sym.kind,
                    tsz::lsp::symbols::document_symbols::SymbolKind::File
                        | tsz::lsp::symbols::document_symbols::SymbolKind::Namespace
                ) {
                    "module"
                } else {
                    symbol_kind_to_tsserver(sym.kind, &sym.kind_modifiers)
                };
                let child_items: Vec<serde_json::Value> =
                    sym.children.iter().map(navbar_child_item).collect();
                let mut parent_item = serde_json::json!({
                    "text": sym.name,
                    "kind": kind,
                    "indent": indent,
                    "spans": [{
                        "start": {
                            "line": sym.range.start.line + 1,
                            "offset": sym.range.start.character + 1,
                        },
                        "end": {
                            "line": sym.range.end.line + 1,
                            "offset": sym.range.end.character + 1,
                        },
                    }],
                });
                if !child_items.is_empty() {
                    parent_item["childItems"] = serde_json::json!(child_items);
                }
                let kind_mods = sym
                    .kind_modifiers
                    .split(',')
                    .filter(|m| !m.is_empty() && *m != "let")
                    .collect::<Vec<_>>()
                    .join(",");
                if !kind_mods.is_empty() {
                    parent_item["kindModifiers"] = serde_json::json!(kind_mods);
                }
                items.push(parent_item);
                // Only recurse into children that should appear in the primary navbar
                for child in &sym.children {
                    if should_appear_in_primary_navbar(child) {
                        symbol_to_navbar_item(child, indent + 1, items);
                    }
                }
            }

            let mut items = Vec::new();
            // Root item
            let total_lines = source_text.lines().count();
            let last_line_len = source_text.lines().last().map_or(0, str::len);
            let child_items: Vec<serde_json::Value> =
                symbols.iter().map(navbar_child_item).collect();
            let mut root = serde_json::json!({
                "text": "<global>",
                "kind": "script",
                "indent": 0,
                "spans": [{"start": {"line": 1, "offset": 1}, "end": {"line": total_lines, "offset": last_line_len + 1}}],
            });
            if !child_items.is_empty() {
                root["childItems"] = serde_json::json!(child_items);
            }
            items.push(root);
            // Only add top-level symbols that qualify as primary navbar items
            for sym in &symbols {
                if should_appear_in_primary_navbar(sym) {
                    symbol_to_navbar_item(sym, 1, &mut items);
                }
            }
            Some(serde_json::json!(items))
        })();
        self.stub_response(
            seq,
            request,
            Some(result.unwrap_or(serde_json::json!([{
                "text": "<global>",
                "kind": "script",
                "childItems": [],
                "indent": 0,
                "spans": [{"start": {"line": 1, "offset": 1}, "end": {"line": 1, "offset": 1}}],
            }]))),
        )
    }

    pub(crate) fn handle_navto(&mut self, seq: u64, request: &TsServerRequest) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let search_value = request
                .arguments
                .get("searchValue")
                .and_then(|v| v.as_str())?;
            if search_value.is_empty() {
                return Some(serde_json::json!([]));
            }
            let search_lower = search_value.to_lowercase();
            let mut nav_items: Vec<serde_json::Value> = Vec::new();
            let file_paths: Vec<String> = self.open_files.keys().cloned().collect();
            for file_path in &file_paths {
                if let Some((arena, _binder, root, source_text)) =
                    self.parse_and_bind_file(file_path)
                {
                    let line_map = LineMap::build(&source_text);
                    let provider = DocumentSymbolProvider::new(&arena, &line_map, &source_text);
                    let symbols = provider.get_document_symbols(root);
                    Self::collect_navto_items(
                        &symbols,
                        search_value,
                        &search_lower,
                        file_path,
                        &mut nav_items,
                    );
                }
            }
            Some(serde_json::json!(nav_items))
        })();
        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    pub(crate) fn collect_navto_items(
        symbols: &[tsz::lsp::symbols::document_symbols::DocumentSymbol],
        search_value: &str,
        search_lower: &str,
        file_path: &str,
        result: &mut Vec<serde_json::Value>,
    ) {
        for sym in symbols {
            let name_lower = sym.name.to_lowercase();
            if name_lower.contains(search_lower) {
                let is_case_sensitive = sym.name.contains(search_value);
                let kind = symbol_kind_to_tsserver(sym.kind, &sym.kind_modifiers);
                let match_kind = if name_lower == *search_lower {
                    "exact"
                } else if name_lower.starts_with(search_lower) {
                    "prefix"
                } else {
                    "substring"
                };
                // Filter out internal "let" modifier from kind_modifiers
                let kind_mods = sym
                    .kind_modifiers
                    .split(',')
                    .filter(|m| !m.is_empty() && *m != "let")
                    .collect::<Vec<_>>()
                    .join(",");
                result.push(serde_json::json!({
                    "name": sym.name,
                    "kind": kind,
                    "kindModifiers": kind_mods,
                    "matchKind": match_kind,
                    "isCaseSensitive": is_case_sensitive,
                    "file": file_path,
                    "start": {
                        "line": sym.range.start.line + 1,
                        "offset": sym.range.start.character + 1,
                    },
                    "end": {
                        "line": sym.range.end.line + 1,
                        "offset": sym.range.end.character + 1,
                    },
                }));
            }
            Self::collect_navto_items(&sym.children, search_value, search_lower, file_path, result);
        }
    }

    pub(crate) fn handle_implementation(
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
                GoToImplementationProvider::new(&arena, &binder, &line_map, file, &source_text);
            let locations = provider.get_implementations(root, position)?;
            let body: Vec<serde_json::Value> = locations
                .iter()
                .map(|loc| {
                    serde_json::json!({
                        "file": loc.file_path,
                        "start": Self::lsp_to_tsserver_position(loc.range.start),
                        "end": Self::lsp_to_tsserver_position(loc.range.end),
                    })
                })
                .collect();
            Some(serde_json::json!(body))
        })();
        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    pub(crate) fn handle_file_references(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let project = self.build_project_for_file(file)?;

            // Dependency graph is populated by set_file() during project construction
            let dependents = project.get_file_dependents(file);
            let refs: Vec<serde_json::Value> = dependents
                .iter()
                .map(|dep_file| {
                    serde_json::json!({
                        "file": dep_file,
                        "start": { "line": 1, "offset": 1 },
                        "end": { "line": 1, "offset": 1 },
                        "lineText": "",
                        "isWriteAccess": false,
                        "isDefinition": false,
                    })
                })
                .collect();

            let file_name = std::path::Path::new(file)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(file);

            Some(serde_json::json!({
                "refs": refs,
                "symbolName": format!("\"{}\"", file_name),
            }))
        })();
        self.stub_response(
            seq,
            request,
            Some(result.unwrap_or(serde_json::json!({"refs": [], "symbolName": ""}))),
        )
    }
}
