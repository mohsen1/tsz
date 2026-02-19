//! Hover, definition, navigation, and reference handlers for tsz-server.

use super::{Server, TsServerRequest, TsServerResponse};
use tsz::binder::SymbolId;
use tsz::lsp::definition::GoToDefinition;
use tsz::lsp::document_symbols::DocumentSymbolProvider;
use tsz::lsp::highlighting::DocumentHighlightProvider;
use tsz::lsp::hover::HoverProvider;
use tsz::lsp::implementation::GoToImplementationProvider;
use tsz::lsp::position::LineMap;
use tsz::lsp::references::FindReferences;
use tsz::lsp::rename::RenameProvider;
use tsz::parser::node::NodeAccess;
use tsz_solver::TypeInterner;

impl Server {
    pub(crate) fn handle_quickinfo(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let (file, line, offset) = Self::extract_file_position(&request.arguments)?;
            let (arena, binder, root, source_text) = self.parse_and_bind_file(&file)?;
            let line_map = LineMap::build(&source_text);
            let position = Self::tsserver_to_lsp_position(line, offset);
            let interner = TypeInterner::new();
            let provider =
                HoverProvider::new(&arena, &binder, &line_map, &interner, &source_text, file);
            let mut type_cache = None;
            let info = provider.get_hover(root, position, &mut type_cache)?;

            // Use structured fields from HoverInfo when available,
            // falling back to parsing from markdown contents
            let display_string = if !info.display_string.is_empty() {
                info.display_string.clone()
            } else {
                info.contents
                    .iter()
                    .find(|c| c.contains("```"))
                    .map(|c| {
                        c.replace("```typescript\n", "")
                            .replace("\n```", "")
                            .trim()
                            .to_string()
                    })
                    .unwrap_or_default()
            };

            let documentation = if !info.documentation.is_empty() {
                info.documentation.clone()
            } else {
                info.contents
                    .iter()
                    .find(|c| !c.contains("```"))
                    .cloned()
                    .unwrap_or_default()
            };

            let kind = if !info.kind.is_empty() {
                info.kind.clone()
            } else {
                "unknown".to_string()
            };

            let kind_modifiers = info.kind_modifiers.clone();

            let range = info
                .range
                .unwrap_or_else(|| tsz::lsp::position::Range::new(position, position));
            // Build tags array from JSDoc tags when available
            let tags: Vec<serde_json::Value> = info
                .tags
                .iter()
                .map(|tag| {
                    serde_json::json!({
                        "name": tag.name,
                        "text": tag.text,
                    })
                })
                .collect();

            // Return documentation as a structured display parts array when non-empty,
            // or empty string when there's no documentation. The SessionClient handles
            // string documentation by wrapping in [{kind:"text", text:doc}].
            // When doc is "", that creates [{kind:"text",text:""}] (length 1) which
            // causes an unwanted blank line in baseline output.
            // Return as empty array [] to avoid the blank line.
            let doc_value: serde_json::Value = if documentation.is_empty() {
                serde_json::json!([])
            } else {
                serde_json::json!([{"kind": "text", "text": documentation}])
            };

            Some(serde_json::json!({
                "displayString": display_string,
                "documentation": doc_value,
                "kind": kind,
                "kindModifiers": kind_modifiers,
                "tags": tags,
                "start": Self::lsp_to_tsserver_position(range.start),
                "end": Self::lsp_to_tsserver_position(range.end),
            }))
        })();

        // When quickinfo fails to resolve, return a response with valid start/end
        // spans. The harness accesses body.start.line and body.end.line, so an
        // empty object {} would cause "Cannot read properties of undefined".
        let fallback = (|| -> Option<serde_json::Value> {
            let (_, line, offset) = Self::extract_file_position(&request.arguments)?;
            let position = Self::tsserver_to_lsp_position(line, offset);
            Some(serde_json::json!({
                "displayString": "",
                "documentation": "",
                "kind": "",
                "kindModifiers": "",
                "tags": [],
                "start": Self::lsp_to_tsserver_position(position),
                "end": Self::lsp_to_tsserver_position(position),
            }))
        })();
        self.stub_response(
            seq,
            request,
            result.or(fallback).or(Some(serde_json::json!({
                "displayString": "",
                "documentation": "",
                "kind": "",
                "kindModifiers": "",
                "tags": [],
                "start": {"line": 1, "offset": 1},
                "end": {"line": 1, "offset": 1},
            }))),
        )
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
            let provider =
                GoToDefinition::new(&arena, &binder, &line_map, file.clone(), &source_text);
            let infos = provider.get_definition_info(root, position)?;
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
            let provider =
                GoToDefinition::new(&arena, &binder, &line_map, file.clone(), &source_text);
            let infos = provider.get_definition_info(root, position)?;

            // Build definitions array with rich metadata
            let definitions: Vec<serde_json::Value> = infos
                .iter()
                .map(|info| Self::definition_info_to_json(info, &file))
                .collect();

            // Compute the textSpan for the word at the cursor position
            let ref_offset = line_map.position_to_offset(position, &source_text)?;
            let node_idx = tsz::lsp::utils::find_node_at_offset(&arena, ref_offset);
            let text_span = if node_idx.is_some() {
                if let Some(node) = arena.get(node_idx) {
                    let start_pos = line_map.offset_to_position(node.pos, &source_text);
                    let end_pos = line_map.offset_to_position(node.end, &source_text);
                    serde_json::json!({
                        "start": Self::lsp_to_tsserver_position(start_pos),
                        "end": Self::lsp_to_tsserver_position(end_pos),
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
            };

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
            let position = Self::tsserver_to_lsp_position(line, offset);

            // Get references with the resolved symbol
            let ref_provider =
                FindReferences::new(&arena, &binder, &line_map, file.clone(), &source_text);
            let (symbol_id, ref_infos) =
                ref_provider.find_references_with_symbol(root, position)?;

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

            let references: Vec<serde_json::Value> = ref_infos
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
                    }
                    ref_json
                })
                .collect();

            // Return as ReferencedSymbol array (single entry for single-file)
            Some(serde_json::json!([{
                "definition": definition,
                "references": references,
            }]))
        })();
        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
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
                sym: &tsz::lsp::document_symbols::DocumentSymbol,
            ) -> serde_json::Value {
                let kind = match sym.kind {
                    tsz::lsp::document_symbols::SymbolKind::File
                    | tsz::lsp::document_symbols::SymbolKind::Module
                    | tsz::lsp::document_symbols::SymbolKind::Namespace => "module",
                    tsz::lsp::document_symbols::SymbolKind::Class => "class",
                    tsz::lsp::document_symbols::SymbolKind::Method => "method",
                    tsz::lsp::document_symbols::SymbolKind::Property
                    | tsz::lsp::document_symbols::SymbolKind::Field => "property",
                    tsz::lsp::document_symbols::SymbolKind::Constructor => "constructor",
                    tsz::lsp::document_symbols::SymbolKind::Enum => "enum",
                    tsz::lsp::document_symbols::SymbolKind::Interface => "interface",
                    tsz::lsp::document_symbols::SymbolKind::Function => "function",
                    tsz::lsp::document_symbols::SymbolKind::Variable => "var",
                    tsz::lsp::document_symbols::SymbolKind::Constant => "const",
                    tsz::lsp::document_symbols::SymbolKind::EnumMember => "enum member",
                    tsz::lsp::document_symbols::SymbolKind::TypeParameter => "type parameter",
                    tsz::lsp::document_symbols::SymbolKind::Struct => "type",
                    _ => "unknown",
                };
                let children: Vec<serde_json::Value> =
                    sym.children.iter().map(symbol_to_navtree).collect();
                serde_json::json!({
                    "text": sym.name,
                    "kind": kind,
                    "childItems": children,
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
                })
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

            fn symbol_to_navbar_item(
                sym: &tsz::lsp::document_symbols::DocumentSymbol,
                indent: usize,
                items: &mut Vec<serde_json::Value>,
            ) {
                let kind = match sym.kind {
                    tsz::lsp::document_symbols::SymbolKind::File
                    | tsz::lsp::document_symbols::SymbolKind::Module
                    | tsz::lsp::document_symbols::SymbolKind::Namespace => "module",
                    tsz::lsp::document_symbols::SymbolKind::Class => "class",
                    tsz::lsp::document_symbols::SymbolKind::Method => "method",
                    tsz::lsp::document_symbols::SymbolKind::Property
                    | tsz::lsp::document_symbols::SymbolKind::Field => "property",
                    tsz::lsp::document_symbols::SymbolKind::Constructor => "constructor",
                    tsz::lsp::document_symbols::SymbolKind::Enum => "enum",
                    tsz::lsp::document_symbols::SymbolKind::Interface => "interface",
                    tsz::lsp::document_symbols::SymbolKind::Function => "function",
                    tsz::lsp::document_symbols::SymbolKind::Variable => "var",
                    tsz::lsp::document_symbols::SymbolKind::Constant => "const",
                    tsz::lsp::document_symbols::SymbolKind::EnumMember => "enum member",
                    tsz::lsp::document_symbols::SymbolKind::TypeParameter => "type parameter",
                    tsz::lsp::document_symbols::SymbolKind::Struct => "type",
                    _ => "unknown",
                };
                let child_items: Vec<serde_json::Value> = sym
                    .children
                    .iter()
                    .map(|c| {
                        serde_json::json!({
                            "text": c.name,
                            "kind": match c.kind {
                                tsz::lsp::document_symbols::SymbolKind::Function => "function",
                                tsz::lsp::document_symbols::SymbolKind::Class => "class",
                                tsz::lsp::document_symbols::SymbolKind::Method => "method",
                                tsz::lsp::document_symbols::SymbolKind::Property => "property",
                                tsz::lsp::document_symbols::SymbolKind::Variable => "var",
                                tsz::lsp::document_symbols::SymbolKind::Constant => "const",
                                tsz::lsp::document_symbols::SymbolKind::Enum => "enum",
                                tsz::lsp::document_symbols::SymbolKind::Interface => "interface",
                                tsz::lsp::document_symbols::SymbolKind::EnumMember => "enum member",
                                tsz::lsp::document_symbols::SymbolKind::Struct => "type",
                                _ => "unknown",
                            },
                        })
                    })
                    .collect();
                items.push(serde_json::json!({
                    "text": sym.name,
                    "kind": kind,
                    "childItems": child_items,
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
                }));
                for child in &sym.children {
                    symbol_to_navbar_item(child, indent + 1, items);
                }
            }

            let mut items = Vec::new();
            // Root item
            let total_lines = source_text.lines().count();
            let last_line_len = source_text.lines().last().map_or(0, str::len);
            let child_items: Vec<serde_json::Value> = symbols
                .iter()
                .map(|sym| {
                    serde_json::json!({
                        "text": sym.name,
                        "kind": match sym.kind {
                            tsz::lsp::document_symbols::SymbolKind::Function => "function",
                            tsz::lsp::document_symbols::SymbolKind::Class => "class",
                            tsz::lsp::document_symbols::SymbolKind::Method => "method",
                            tsz::lsp::document_symbols::SymbolKind::Property => "property",
                            tsz::lsp::document_symbols::SymbolKind::Variable => "var",
                            tsz::lsp::document_symbols::SymbolKind::Constant => "const",
                            tsz::lsp::document_symbols::SymbolKind::Enum => "enum",
                            tsz::lsp::document_symbols::SymbolKind::Interface => "interface",
                            tsz::lsp::document_symbols::SymbolKind::EnumMember => "enum member",
                            _ => "unknown",
                        },
                    })
                })
                .collect();
            items.push(serde_json::json!({
                "text": "<global>",
                "kind": "script",
                "childItems": child_items,
                "indent": 0,
                "spans": [{"start": {"line": 1, "offset": 1}, "end": {"line": total_lines, "offset": last_line_len + 1}}],
            }));
            // Flatten children
            for sym in &symbols {
                symbol_to_navbar_item(sym, 1, &mut items);
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
        symbols: &[tsz::lsp::document_symbols::DocumentSymbol],
        search_value: &str,
        search_lower: &str,
        file_path: &str,
        result: &mut Vec<serde_json::Value>,
    ) {
        for sym in symbols {
            let name_lower = sym.name.to_lowercase();
            if name_lower.contains(search_lower) {
                let is_case_sensitive = sym.name.contains(search_value);
                let kind = match sym.kind {
                    tsz::lsp::document_symbols::SymbolKind::Module => "module",
                    tsz::lsp::document_symbols::SymbolKind::Class => "class",
                    tsz::lsp::document_symbols::SymbolKind::Method => "method",
                    tsz::lsp::document_symbols::SymbolKind::Property
                    | tsz::lsp::document_symbols::SymbolKind::Field => "property",
                    tsz::lsp::document_symbols::SymbolKind::Constructor => "constructor",
                    tsz::lsp::document_symbols::SymbolKind::Enum => "enum",
                    tsz::lsp::document_symbols::SymbolKind::Interface => "interface",
                    tsz::lsp::document_symbols::SymbolKind::Function => "function",
                    tsz::lsp::document_symbols::SymbolKind::Variable => "var",
                    tsz::lsp::document_symbols::SymbolKind::Constant => "const",
                    tsz::lsp::document_symbols::SymbolKind::EnumMember => "enum member",
                    tsz::lsp::document_symbols::SymbolKind::TypeParameter => "type parameter",
                    _ => "unknown",
                };
                let match_kind = if name_lower == *search_lower {
                    "exact"
                } else if name_lower.starts_with(search_lower) {
                    "prefix"
                } else {
                    "substring"
                };
                result.push(serde_json::json!({
                    "name": sym.name,
                    "kind": kind,
                    "kindModifiers": "",
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
        self.stub_response(
            seq,
            request,
            Some(serde_json::json!({"refs": [], "symbolName": ""})),
        )
    }
}
