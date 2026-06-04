impl Server {
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
        self.success_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
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
        self.success_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
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
        self.success_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    /// Issue #3753: resolve an outgoing-call import-binding to the actual
    /// exported declaration in the target module.
    ///
    /// Returns `None` when the module specifier can't be resolved (bare
    /// package imports, missing files, parse failures, no matching export).
    /// In that case the caller falls back to the local-import-binding item.
    pub(crate) fn resolve_import_call_hierarchy_target(
        &mut self,
        importer_file: &str,
        request: &ImportResolutionRequest,
    ) -> Option<CallHierarchyItem> {
        // Only handle relative-path specifiers for now. Bare specifiers
        // would need module-resolution machinery the LSP server doesn't
        // wire up yet.
        let spec = &request.module_specifier;
        if !(spec.starts_with("./") || spec.starts_with("../")) {
            return None;
        }

        let importer_path = std::path::Path::new(importer_file);
        let importer_dir = importer_path.parent()?;
        let resolved_path = self.resolve_relative_module_specifier(importer_dir, spec)?;
        let resolved_str = resolved_path.to_string_lossy().into_owned();

        let (arena, binder, _root, source_text) = self.parse_and_bind_file(&resolved_str)?;
        let line_map = LineMap::build(&source_text);
        let provider = CallHierarchyProvider::new(
            &arena,
            &binder,
            &line_map,
            resolved_str.clone(),
            &source_text,
        );

        // Find the exported binding by name. Default imports map to a
        // declaration tagged as default; named imports map to the named
        // export. Namespace imports are not resolved here (no specific
        // export to point at).
        let target_name = request.exported_name.as_deref()?;
        let decl_idx = Self::find_exported_callable(&arena, &binder, target_name)?;

        // Use the provider's prepare-by-position path: locate any identifier
        // at the resolved declaration's position, then build a hierarchy
        // item for it. Falls back to a synthesized item if prepare doesn't
        // recognize the position.
        let decl_node = arena.get(decl_idx)?;
        let pos = line_map.offset_to_position(decl_node.pos, &source_text);
        if let Some(item) = provider.prepare(_root, pos) {
            return Some(CallHierarchyItem {
                uri: resolved_str,
                ..item
            });
        }
        let span_pos = line_map.offset_to_position(decl_node.pos, &source_text);
        let span_end = line_map.offset_to_position(decl_node.end, &source_text);
        Some(CallHierarchyItem {
            name: target_name.to_string(),
            kind: tsz_lsp::SymbolKind::Function,
            uri: resolved_str,
            range: tsz_common::position::Range::new(span_pos, span_end),
            selection_range: tsz_common::position::Range::new(span_pos, span_end),
            container_name: None,
        })
    }

    /// Locate an exported callable (function declaration / class declaration
    /// / variable initializer) by exported name within the bound source
    /// file. Searches `binder.symbols` for symbols tagged as exported with a
    /// matching name and returns the first declaration `NodeIndex`.
    fn find_exported_callable(
        arena: &tsz_parser::parser::node::NodeArena,
        binder: &tsz_binder::BinderState,
        target_name: &str,
    ) -> Option<tsz_parser::NodeIndex> {
        if let Some(sym_id) = binder.file_locals.get(target_name)
            && let Some(symbol) = binder.symbols.get(sym_id)
            && let Some(&decl) = symbol.declarations.first()
        {
            // Skip the symbol if its first declaration is itself an import-binding.
            let kind = arena.get(decl).map(|n| n.kind);
            let is_import_binding = matches!(
                kind,
                Some(k) if k == tsz_parser::syntax_kind_ext::IMPORT_SPECIFIER
                    || k == tsz_parser::syntax_kind_ext::IMPORT_CLAUSE
                    || k == tsz_parser::syntax_kind_ext::NAMESPACE_IMPORT
            );
            if !is_import_binding {
                return Some(decl);
            }
        }
        None
    }

    /// Issue #3753 follow-up: report whether `target_name` (a top-level local
    /// in `target_file`) is also the file's default export — i.e. the same
    /// declaration backs both `module_exports[target_file][target_name]` and
    /// `module_exports[target_file]["default"]`. tsc treats `import x from
    /// "./a"` and `import { x } from "./a"` as both reaching such a function,
    /// so the cross-file caller scan needs to accept default-import bindings
    /// in addition to named-import bindings.
    ///
    /// Returns false when the file can't be parsed/bound, when no `default`
    /// export exists, or when `target_name` and `default` resolve to disjoint
    /// declaration nodes (the typical case for plain `export function`).
    fn target_is_default_export(&self, target_file: &str, target_name: &str) -> bool {
        if target_name == "default" {
            return false;
        }
        let Some((_arena, binder, _root, _src)) = self.parse_and_bind_file(target_file) else {
            return false;
        };
        let Some(file_exports) = binder.module_exports.get(target_file) else {
            return false;
        };
        let Some(default_sid) = file_exports.get("default") else {
            return false;
        };
        let Some(target_sid) = file_exports.get(target_name) else {
            return false;
        };
        if default_sid == target_sid {
            return true;
        }
        let Some(default_sym) = binder.symbols.get(default_sid) else {
            return false;
        };
        let Some(target_sym) = binder.symbols.get(target_sid) else {
            return false;
        };
        // Same declaration node backs both keys: `export default function NAME`,
        // `function NAME() {}; export { NAME as default }`, etc.
        for &decl in &default_sym.declarations {
            if target_sym.declarations.contains(&decl) {
                return true;
            }
        }
        false
    }

    /// Issue #3753: scan the other open files for cross-file callers that
    /// reach `target_item` via an `import` binding. tsc reports those as
    /// incoming calls; without this scan tsz only saw within-file callers
    /// because each `parse_and_bind_file` call only sees one file's
    /// arena/binder.
    ///
    /// For every other open file:
    /// 1. Parse + bind it.
    /// 2. Walk its `IMPORT_DECLARATION` nodes.
    /// 3. For each import whose module specifier resolves to the target's
    ///    file, find the local binding for `target_item.name` (matching by
    ///    exported-name when the spec uses `import { foo as bar }`).
    /// 4. Run that file's `incoming_calls` provider with the local-binding
    ///    position so callers within that file get aggregated correctly.
    pub(crate) fn collect_cross_file_incoming_calls(
        &mut self,
        target_file: &str,
        target_item: &CallHierarchyItem,
    ) -> Vec<CallHierarchyIncomingCall> {
        let mut results: Vec<CallHierarchyIncomingCall> = Vec::new();
        let target_file_canon = Self::canonicalize_path_str(target_file);
        let target_name = target_item.name.clone();
        // Issue #3753 follow-up: a function exported as `export default
        // function NAME` (or `export { NAME as default }`) is reachable in
        // other files via either `import { NAME } from "./a"` or `import
        // <local> from "./a"`. tsc reports both as incoming calls of NAME.
        // Detect whether the target is the file's default export so the
        // default-import / `default`-aliased-named-import branches below also
        // bind to it, not just exported-name matches.
        let target_is_default_export = self.target_is_default_export(target_file, &target_name);
        // Snapshot the keys so we don't iterate while parse_and_bind_file
        // potentially mutates `open_files`.
        let other_files: Vec<String> = self
            .open_files
            .keys()
            .filter(|k| Self::canonicalize_path_str(k) != target_file_canon)
            .cloned()
            .collect();

        for other_file in other_files {
            let Some((arena, binder, root, source_text)) = self.parse_and_bind_file(&other_file)
            else {
                continue;
            };
            let line_map = LineMap::build(&source_text);
            let provider = CallHierarchyProvider::new(
                &arena,
                &binder,
                &line_map,
                other_file.clone(),
                &source_text,
            );

            // Find IMPORT_DECLARATION nodes whose module specifier resolves
            // to the target file, and collect the matching local-binding
            // identifier positions.
            // Collect (binding identifier NodeIndex, local name) for each
            // matching import binding so we can ask the provider which
            // callers reference that local within this file.
            let mut local_bindings: Vec<(tsz_parser::NodeIndex, String)> = Vec::new();
            // Issue #3753 follow-up: collect namespace-import bindings
            // (`import * as ns from "./a"`). For these the import
            // doesn't bind `target_name` directly; we scan for
            // `<ns>.<target_name>(…)` member calls instead.
            let mut namespace_bindings: Vec<tsz_parser::NodeIndex> = Vec::new();
            for node in arena.nodes.iter() {
                if node.kind != tsz_parser::syntax_kind_ext::IMPORT_DECLARATION {
                    continue;
                }
                let Some(import_decl) = arena.get_import_decl(node) else {
                    continue;
                };
                let Some(spec_node) = arena.get(import_decl.module_specifier) else {
                    continue;
                };
                let Some(spec_lit) = arena.get_literal(spec_node) else {
                    continue;
                };
                let spec_text = &spec_lit.text;
                if !(spec_text.starts_with("./") || spec_text.starts_with("../")) {
                    continue;
                }
                let importer_dir = std::path::Path::new(&other_file)
                    .parent()
                    .unwrap_or_else(|| std::path::Path::new(""));
                let Some(resolved_path) =
                    self.resolve_relative_module_specifier(importer_dir, spec_text)
                else {
                    continue;
                };
                let resolved_canon = Self::canonicalize_path_str(&resolved_path.to_string_lossy());
                if resolved_canon != target_file_canon {
                    continue;
                }

                // Found an import from target_file. Walk its named-imports
                // / default-import / namespace-import bindings to find the
                // local-name identifier whose exported name matches
                // `target_name`. Capture the identifier's position so we
                // can re-run the provider's incoming_calls there.
                if let Some(clause_node) = arena.get(import_decl.import_clause)
                    && clause_node.kind == tsz_parser::syntax_kind_ext::IMPORT_CLAUSE
                {
                    let clause = arena.get_import_clause(clause_node);
                    if let Some(clause) = clause {
                        // Default binding (`import target from "./a"`):
                        // fires when the user asked for incoming calls on a
                        // symbol literally named "default", or when the
                        // resolved target is the file's default export — both
                        // forms are reachable from any default-import binding.
                        if clause.name.is_some()
                            && (target_name == "default" || target_is_default_export)
                            && let Some(name_node) = arena.get(clause.name)
                            && let Some(ident) = arena.get_identifier(name_node)
                        {
                            local_bindings.push((clause.name, ident.escaped_text.clone()));
                        }
                        // Namespace import (`import * as ns from "./a"`):
                        // record the local namespace identifier so the
                        // caller scan can match `<ns>.<target_name>()`
                        // member calls. NamespaceImport reuses
                        // `NamedImportsData` storage with the local name
                        // in the `name` field and an empty elements list.
                        if clause.named_bindings.is_some()
                            && let Some(nb_node) = arena.get(clause.named_bindings)
                            && nb_node.kind == tsz_parser::syntax_kind_ext::NAMESPACE_IMPORT
                            && let Some(ns_import) = arena.get_named_imports(nb_node)
                            && ns_import.name.is_some()
                            && let Some(name_node) = arena.get(ns_import.name)
                            && arena.get_identifier(name_node).is_some()
                        {
                            namespace_bindings.push(ns_import.name);
                        }
                        // Named bindings — walk the named_bindings child for
                        // `NamedImports` (`import { foo } from "./a"`).
                        if clause.named_bindings.is_some()
                            && let Some(nb_node) = arena.get(clause.named_bindings)
                            && nb_node.kind == tsz_parser::syntax_kind_ext::NAMED_IMPORTS
                            && let Some(named) = arena.get_named_imports(nb_node)
                        {
                            {
                                for &spec_idx in &named.elements.nodes {
                                    let Some(specifier_node) = arena.get(spec_idx) else {
                                        continue;
                                    };
                                    let Some(specifier) = arena.get_specifier(specifier_node)
                                    else {
                                        continue;
                                    };
                                    // Matched exported name (property_name when aliased,
                                    // otherwise the binding name).
                                    let exported = if specifier.property_name.is_some()
                                        && let Some(prop_node) = arena.get(specifier.property_name)
                                        && let Some(ident) = arena.get_identifier(prop_node)
                                    {
                                        ident.escaped_text.clone()
                                    } else if let Some(name_node) = arena.get(specifier.name)
                                        && let Some(ident) = arena.get_identifier(name_node)
                                    {
                                        ident.escaped_text.clone()
                                    } else {
                                        continue;
                                    };
                                    let exported_matches = exported == target_name
                                        || (target_is_default_export && exported == "default");
                                    if !exported_matches {
                                        continue;
                                    }
                                    let Some(name_node) = arena.get(specifier.name) else {
                                        continue;
                                    };
                                    let local = arena
                                        .get_identifier(name_node)
                                        .map(|i| i.escaped_text.clone())
                                        .unwrap_or_else(|| target_name.clone());
                                    local_bindings.push((specifier.name, local));
                                }
                            }
                        }
                    }
                }
            }

            let _ = root;
            for (decl_idx, local_name) in local_bindings {
                let calls = provider.incoming_calls_for_decl_in_file(decl_idx, &local_name);
                for call in calls {
                    results.push(call);
                }
            }
            for ns_decl_idx in namespace_bindings {
                let calls = provider.incoming_calls_for_namespace_member(ns_decl_idx, &target_name);
                for call in calls {
                    results.push(call);
                }
            }
        }

        results
    }

    /// Best-effort canonical form for path comparison: prefer
    /// `std::fs::canonicalize`, fall back to the raw normalized string.
    fn canonicalize_path_str(path: &str) -> String {
        let p = std::path::Path::new(path);
        if let Ok(canon) = std::fs::canonicalize(p) {
            return canon.to_string_lossy().into_owned();
        }
        Self::normalize_path(p).to_string_lossy().into_owned()
    }

    /// Strip `.` segments and resolve `..` segments from a path while
    /// preserving the root. Used to normalize the result of
    /// `Path::join("/foo", "./bar")` (which yields `/foo/./bar`) into the
    /// canonical `/foo/bar` so it matches `open_files` keys and `exists()`.
    fn normalize_path(path: &std::path::Path) -> std::path::PathBuf {
        use std::path::Component;
        let mut out = std::path::PathBuf::new();
        for component in path.components() {
            match component {
                Component::CurDir => {}
                Component::ParentDir => {
                    if !out.pop() {
                        out.push("..");
                    }
                }
                other => out.push(other.as_os_str()),
            }
        }
        if out.as_os_str().is_empty() {
            out.push(".");
        }
        out
    }

    /// Resolve a relative module specifier (e.g. `"./a"`, `"../foo/bar"`)
    /// against the importing file's directory. Tries `.ts`, `.tsx`,
    /// `.d.ts`, `.js`, `.jsx`, `.mts`, `.cts`, then bare path. Returns the
    /// first candidate that exists on disk or matches a key in the
    /// `open_files` map (so unsaved buffers count for resolution).
    fn resolve_relative_module_specifier(
        &self,
        importer_dir: &std::path::Path,
        specifier: &str,
    ) -> Option<std::path::PathBuf> {
        let base = Self::normalize_path(&importer_dir.join(specifier));
        const EXTS: &[&str] = tsz_common::file_extensions::TSC_TS_JS_RESOLUTION_EXTENSIONS_BARE;
        let exists_anywhere = |p: &std::path::Path| -> bool {
            if p.exists() {
                return true;
            }
            let key = p.to_string_lossy().into_owned();
            self.open_files.contains_key(&key)
        };
        if base.extension().is_some() && exists_anywhere(&base) {
            return Some(base);
        }
        for ext in EXTS {
            let candidate = base.with_extension(ext);
            if exists_anywhere(&candidate) {
                return Some(candidate);
            }
        }
        if base.is_dir() {
            for ext in EXTS {
                let candidate = base.join(format!("index.{ext}"));
                if exists_anywhere(&candidate) {
                    return Some(candidate);
                }
            }
        }
        Some(base)
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
                CallHierarchyProvider::new(&arena, &binder, &line_map, file.clone(), &source_text);

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

                // Issue #3753: scan the other open files for cross-file
                // callers that reach this target via an `import` binding.
                // tsc reports those as incoming calls from the importing
                // file; tsz only saw within-file callers before.
                let prepared_target = positions
                    .iter()
                    .find_map(|probe| provider.prepare(root, *probe));
                if let Some(target_item) = prepared_target {
                    let cross_calls = self.collect_cross_file_incoming_calls(&file, &target_item);
                    for call in cross_calls {
                        // Avoid duplicates if the caller's local resolution already
                        // produced an entry pointing at the same span.
                        let already_present = calls.iter().any(|existing| {
                            existing.from.uri == call.from.uri
                                && existing.from.selection_range == call.from.selection_range
                        });
                        if !already_present {
                            calls.push(call);
                        }
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
                // Issue #3753: when an outgoing callee resolves to an `import`
                // binding, follow it across to the imported module's source
                // file and replace `to` with the actual export's location.
                // tsc points at the exported declaration, not at the
                // local import binding.
                let mut resolved_calls: Vec<tsz_lsp::CallHierarchyOutgoingCall> =
                    Vec::with_capacity(calls.len());
                for call in calls {
                    if let Some(import_req) = call.import_resolution.clone()
                        && let Some(resolved_to) =
                            self.resolve_import_call_hierarchy_target(&file, &import_req)
                    {
                        resolved_calls.push(CallHierarchyOutgoingCall {
                            to: resolved_to,
                            from_ranges: call.from_ranges,
                            import_resolution: None,
                        });
                    } else {
                        resolved_calls.push(call);
                    }
                }
                let calls = resolved_calls;
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
        self.success_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    /// `configurePlugin` — stores plugin configuration for future use.
    pub(crate) fn handle_configure_plugin(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        if let Some(plugin_name) = request.arguments.get("pluginName").and_then(|v| v.as_str()) {
            let config = request
                .arguments
                .get("configuration")
                .cloned()
                .unwrap_or(serde_json::json!({}));
            self.plugin_configs.insert(plugin_name.to_string(), config);
        }
        self.success_response(seq, request, None)
    }

    /// `getMoveToRefactoringFileSuggestions` — suggests files a symbol can be moved to.
    pub(crate) fn handle_get_move_to_refactoring_file_suggestions(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let file_path = std::path::Path::new(file);
            let file_ext = file_path.extension()?.to_str()?;

            // Determine which extensions are compatible
            let compatible_exts: &[&str] = match file_ext {
                "ts" => &["ts"],
                "tsx" => &["tsx", "ts"],
                "js" => &["js"],
                "jsx" => &["jsx", "js"],
                "mts" => &["mts", "ts"],
                "cts" => &["cts", "ts"],
                "mjs" => &["mjs", "js"],
                "cjs" => &["cjs", "js"],
                _ => &[file_ext],
            };

            let push_candidate = |files: &mut Vec<String>,
                                  candidate: &str,
                                  compatible_exts: &[&str]| {
                if candidate == file || files.iter().any(|p| p == candidate) {
                    return;
                }
                if candidate.ends_with(".d.ts")
                    || candidate.ends_with(".d.mts")
                    || candidate.ends_with(".d.cts")
                {
                    return;
                }
                if candidate.contains("/node_modules/") || candidate.contains("\\node_modules\\") {
                    return;
                }
                if let Some(ext) = std::path::Path::new(candidate)
                    .extension()
                    .and_then(|e| e.to_str())
                    && compatible_exts.contains(&ext)
                {
                    files.push(candidate.to_string());
                }
            };

            // Collect candidate files from open files, the configured tsconfig
            // project (issue #3798), and external project lists.
            let mut files: Vec<String> = Vec::new();
            for open_path in self.open_files.keys() {
                push_candidate(&mut files, open_path, compatible_exts);
            }
            // Issue #3798: include files from the owning tsconfig project, not
            // just open files. tsc's language service ranges over the whole
            // project file set when ranking move-to-file targets.
            if let Some(project) = self.compile_on_save_project(file) {
                for pf in &project.file_names {
                    push_candidate(&mut files, pf, compatible_exts);
                }
            }
            for project_files in self.external_project_files.values() {
                for pf in project_files {
                    push_candidate(&mut files, pf, compatible_exts);
                }
            }

            files.sort();

            // Issue #3798: derive the suggested new-file name from the
            // declaration's identifier in the requested range, falling back
            // to "newFile" when no identifier is found. tsc names the
            // suggestion after the moved symbol (e.g. "moveMe.ts").
            let parent = file_path.parent().unwrap_or(std::path::Path::new(""));
            let symbol_stub = Self::move_to_file_symbol_name(self, request)
                .unwrap_or_else(|| "newFile".to_string());
            let new_file_name = parent.join(format!("{symbol_stub}.{file_ext}"));

            Some(serde_json::json!({
                "newFileName": new_file_name.to_string_lossy(),
                "files": files
            }))
        })();

        self.success_response(
            seq,
            request,
            Some(result.unwrap_or(serde_json::json!({"newFileName": "", "files": []}))),
        )
    }

    /// Extract the leading declaration identifier inside the request's
    /// range. Used by `move-to-file` suggestions to name the new file
    /// after the moved symbol (issue #3798). Falls back to None when no
    /// matching declaration is found in the source slice.
    fn move_to_file_symbol_name(&self, request: &TsServerRequest) -> Option<String> {
        let file = request.arguments.get("file")?.as_str()?;
        let source_text = self.open_files.get(file)?;
        let line_map = LineMap::build(source_text);
        let start_line = request.arguments.get("startLine")?.as_u64()? as u32;
        let start_offset = request.arguments.get("startOffset")?.as_u64()? as u32;
        let end_line = request.arguments.get("endLine")?.as_u64()? as u32;
        let end_offset = request.arguments.get("endOffset")?.as_u64()? as u32;
        let start_pos = Self::tsserver_to_lsp_position(start_line, start_offset);
        let end_pos = Self::tsserver_to_lsp_position(end_line, end_offset);
        let start_byte = line_map.position_to_offset(start_pos, source_text)? as usize;
        let end_byte = line_map.position_to_offset(end_pos, source_text)? as usize;
        let slice = source_text.get(start_byte..end_byte.min(source_text.len()))?;
        // Look for the leading exported declaration's name. The text-search
        // approximation matches tsc's typical move-to-file behavior for the
        // common cases (function/class/const/let/var/interface/type/enum).
        let after_export = slice
            .trim_start()
            .strip_prefix("export ")
            .map_or(slice.trim_start(), str::trim_start);
        for keyword in [
            "function ",
            "class ",
            "interface ",
            "enum ",
            "type ",
            "const ",
            "let ",
            "var ",
            "namespace ",
            "module ",
            "abstract class ",
            "default function ",
            "default class ",
        ] {
            if let Some(rest) = after_export.strip_prefix(keyword) {
                let name: String = rest
                    .trim_start()
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
                    .collect();
                if !name.is_empty() {
                    return Some(name);
                }
            }
        }
        None
    }

    /// `preparePasteEdits` — checks whether paste-with-imports is available.
    ///
    /// Returns `true` if the pasted content comes from a known source file
    /// (indicating we can potentially add imports).
    pub(crate) fn handle_prepare_paste_edits(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<bool> {
            if let Some(copied_text_span) = request
                .arguments
                .get("copiedTextSpan")
                .and_then(|value| value.as_array())
            {
                let file = request
                    .arguments
                    .get("file")
                    .and_then(|value| value.as_str())?;
                let has_source =
                    self.open_files.contains_key(file) || std::path::Path::new(file).exists();
                let has_non_empty_span = copied_text_span.iter().any(|span| {
                    span.get("length")
                        .and_then(|value| value.as_u64())
                        .is_some_and(|length| length > 0)
                });
                return Some(has_source && has_non_empty_span);
            }

            let copied_from = request
                .arguments
                .get("copiedFromFile")
                .and_then(|v| v.as_str())?;
            if self.open_files.contains_key(copied_from)
                || std::path::Path::new(copied_from).exists()
            {
                return Some(true);
            }
            Some(false)
        })();

        self.success_response(
            seq,
            request,
            Some(serde_json::json!(result.unwrap_or(false))),
        )
    }

    /// `getPasteEdits` — generates import additions for pasted code.
    ///
    /// Parses the pasted text, identifies unresolved identifiers, and generates
    /// import statements from the source file's exports.
    pub(crate) fn handle_get_paste_edits(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let target_file = request.arguments.get("file")?.as_str()?;
            let pasted_text = request
                .arguments
                .get("pastedText")
                .and_then(|v| v.as_array())
                .and_then(|arr| {
                    let texts = arr
                        .iter()
                        .filter_map(|value| value.as_str())
                        .collect::<Vec<_>>();
                    (!texts.is_empty()).then_some(texts)
                })?;
            let pasted_text_joined = pasted_text.join("\n");
            let paste_locations = request
                .arguments
                .get("pasteLocations")
                .and_then(|value| value.as_array());

            let copied_from = request
                .arguments
                .get("copiedFrom")
                .and_then(|v| v.get("file"))
                .and_then(|v| v.as_str())?;

            // Extract import lines from source file that the pasted code may reference
            let source_content = self
                .open_files
                .get(copied_from)
                .cloned()
                .or_else(|| std::fs::read_to_string(copied_from).ok())?;

            // Find export names from the source file
            let source_exports: Vec<String> = source_content
                .lines()
                .filter_map(|line| {
                    let trimmed = line.trim();
                    if trimmed.starts_with("export ") {
                        // Extract exported identifier names
                        let rest = trimmed.strip_prefix("export ")?;
                        // "export function foo" / "export class Foo" / "export const bar"
                        for keyword in &[
                            "function ",
                            "class ",
                            "const ",
                            "let ",
                            "var ",
                            "enum ",
                            "interface ",
                            "type ",
                            "abstract class ",
                            "async function ",
                        ] {
                            if let Some(after) = rest.strip_prefix(keyword) {
                                let name: String = after
                                    .chars()
                                    .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
                                    .collect();
                                if !name.is_empty() {
                                    return Some(name);
                                }
                            }
                        }
                    }
                    None
                })
                .collect();

            if source_exports.is_empty() {
                return None;
            }

            // Check which source exports appear in pasted text but not in target file
            let target_content = self
                .open_files
                .get(target_file)
                .cloned()
                .or_else(|| std::fs::read_to_string(target_file).ok())?;

            let mut names_to_import: Vec<String> = Vec::new();
            for export_name in &source_exports {
                // Check if the export name appears in pasted code
                if !pasted_text_joined.contains(export_name.as_str()) {
                    continue;
                }
                // Check if the target already imports/declares it
                let already_exists = target_content.lines().any(|line| {
                    let t = line.trim();
                    t.contains(export_name.as_str())
                        && (t.starts_with("import ")
                            || t.starts_with("const ")
                            || t.starts_with("let ")
                            || t.starts_with("var ")
                            || t.starts_with("function ")
                            || t.starts_with("class ")
                            || t.starts_with("interface ")
                            || t.starts_with("type ")
                            || t.starts_with("enum "))
                });
                if !already_exists {
                    names_to_import.push(export_name.clone());
                }
            }

            if names_to_import.is_empty() {
                return None;
            }

            names_to_import.sort();
            names_to_import.dedup();

            // Compute relative import path from target to source
            let target_dir = std::path::Path::new(target_file)
                .parent()
                .unwrap_or_else(|| std::path::Path::new(""));
            let import_path =
                Self::compute_relative_import(target_dir, std::path::Path::new(copied_from));

            // Find insertion point: after last import line, or at top of file
            let mut insert_line = 0u32;
            for (i, line) in target_content.lines().enumerate() {
                let t = line.trim();
                if t.starts_with("import ") || t.starts_with("import{") {
                    insert_line = i as u32 + 1;
                }
            }

            // Build import statement
            let import_suffix = if insert_line == 0 { "\n\n" } else { "\n" };
            let import_text = format!(
                "import {{ {} }} from \"{}\";{}",
                names_to_import.join(", "),
                import_path,
                import_suffix
            );
            let line_map = LineMap::build(&target_content);
            let import_offset = line_map.position_to_offset(
                Position {
                    line: insert_line,
                    character: 0,
                },
                &target_content,
            )?;
            let mut text_changes = vec![serde_json::json!({
                "span": { "start": import_offset, "length": 0 },
                "newText": import_text
            })];

            if let Some(paste_locations) = paste_locations {
                for (index, location) in paste_locations.iter().enumerate() {
                    let start = location.get("start")?;
                    let start_line = start.get("line")?.as_u64()? as u32;
                    let start_offset = start.get("offset")?.as_u64()? as u32;
                    let start_pos = Position {
                        line: start_line.saturating_sub(1),
                        character: start_offset.saturating_sub(1),
                    };
                    let start_offset = line_map.position_to_offset(start_pos, &target_content)?;
                    let new_text = pasted_text
                        .get(index)
                        .or_else(|| pasted_text.first())
                        .copied()
                        .unwrap_or("");
                    text_changes.push(serde_json::json!({
                        "span": { "start": start_offset, "length": 0 },
                        "newText": new_text
                    }));
                }
            }

            Some(serde_json::json!({
                "edits": [{
                    "fileName": target_file,
                    "textChanges": text_changes
                }],
                "fixId": "providePostPasteEdits"
            }))
        })();

        self.success_response(
            seq,
            request,
            Some(result.unwrap_or(serde_json::json!({"edits": [], "fixId": ""}))),
        )
    }

    /// `mapCode` — maps code snippets to insertion locations in a file.
    ///
    /// Parses code snippets and finds appropriate insertion points based on
    /// the AST structure and optional focus locations.
    pub(crate) fn handle_map_code(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let mapping = request.arguments.get("mapping")?;
            let contents = mapping.get("contents")?.as_array()?;

            if contents.is_empty() {
                return None;
            }

            let file_content = self
                .open_files
                .get(file)
                .cloned()
                .or_else(|| std::fs::read_to_string(file).ok())?;

            // Determine insertion point from focus locations if provided
            let insert_line = if let Some(focus) = mapping
                .get("focusLocations")
                .and_then(|v| v.as_array())
                .and_then(|arr| arr.last())
                .and_then(|v| v.as_array())
                .and_then(|arr| arr.last())
            {
                // Focus location gives us a span — insert after it
                focus
                    .get("end")
                    .and_then(|e| e.get("line"))
                    .and_then(|l| l.as_u64())
                    .unwrap_or(0) as u32
            } else {
                // Default: insert at end of file
                file_content.lines().count() as u32
            };

            let mut text_changes = Vec::new();
            for content_val in contents {
                if let Some(snippet) = content_val.as_str() {
                    if snippet.trim().is_empty() {
                        continue;
                    }
                    text_changes.push(serde_json::json!({
                        "start": { "line": insert_line + 1, "offset": 1 },
                        "end": { "line": insert_line + 1, "offset": 1 },
                        "newText": format!("{snippet}\n")
                    }));
                }
            }

            if text_changes.is_empty() {
                return None;
            }

            Some(serde_json::json!([{
                "fileName": file,
                "textChanges": text_changes
            }]))
        })();

        self.success_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
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
        self.success_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }
}
