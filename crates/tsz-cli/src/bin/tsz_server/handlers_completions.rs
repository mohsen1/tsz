//! Completions and signature help handlers for tsz-server.

use super::{Server, TsServerRequest, TsServerResponse};
use tsz::lsp::Project;
use tsz::lsp::completions::Completions;
use tsz::lsp::position::LineMap;
use tsz::lsp::signature_help::SignatureHelpProvider;
use tsz_solver::TypeInterner;

impl Server {
    pub(crate) const fn completion_kind_to_str(
        kind: tsz::lsp::completions::CompletionItemKind,
    ) -> &'static str {
        match kind {
            tsz::lsp::completions::CompletionItemKind::Variable => "var",
            tsz::lsp::completions::CompletionItemKind::Function => "function",
            tsz::lsp::completions::CompletionItemKind::Class => "class",
            tsz::lsp::completions::CompletionItemKind::Method => "method",
            tsz::lsp::completions::CompletionItemKind::Parameter => "parameter",
            tsz::lsp::completions::CompletionItemKind::Property => "property",
            tsz::lsp::completions::CompletionItemKind::Keyword => "keyword",
            tsz::lsp::completions::CompletionItemKind::Interface => "interface",
            tsz::lsp::completions::CompletionItemKind::Enum => "enum",
            tsz::lsp::completions::CompletionItemKind::TypeAlias => "type",
            tsz::lsp::completions::CompletionItemKind::Module => "module",
            tsz::lsp::completions::CompletionItemKind::TypeParameter => "type parameter",
            tsz::lsp::completions::CompletionItemKind::Constructor => "constructor",
        }
    }

    fn project_completion_items(
        &self,
        file_name: &str,
        position: tsz::lsp::position::Position,
    ) -> Vec<tsz::lsp::completions::CompletionItem> {
        let mut files = self.open_files.clone();
        if !files.contains_key(file_name)
            && let Ok(content) = std::fs::read_to_string(file_name)
        {
            files.insert(file_name.to_string(), content);
        }
        if files.is_empty() {
            return Vec::new();
        }

        let mut project = Project::new();
        project.set_import_module_specifier_ending(
            self.completion_import_module_specifier_ending.clone(),
        );
        for (path, text) in files {
            project.set_file(path, text);
        }
        project
            .get_completions(file_name, position)
            .unwrap_or_default()
    }

    fn completion_entry_from_item(
        item: &tsz::lsp::completions::CompletionItem,
        line_map: &LineMap,
        source_text: &str,
    ) -> serde_json::Value {
        let kind = Self::completion_kind_to_str(item.kind);
        let sort_text = item.effective_sort_text();
        let mut entry = serde_json::json!({
            "name": item.label,
            "kind": kind,
            "sortText": sort_text,
            "kindModifiers": item.kind_modifiers.clone().unwrap_or_default(),
        });

        if item.has_action {
            entry["hasAction"] = serde_json::json!(true);
            if let Some(insert_text) = item.insert_text.as_ref() {
                entry["insertText"] = serde_json::json!(insert_text);
            }
            if item.is_snippet {
                entry["isSnippet"] = serde_json::json!(true);
            }
        }
        if let Some(source) = item.source.as_ref() {
            entry["source"] = serde_json::json!(source);
            entry["sourceDisplay"] = serde_json::json!([{ "text": source, "kind": "text" }]);
            entry["data"] = serde_json::json!({
                "name": item.label,
                "source": source,
            });
        }
        if let Some((start, end)) = item.replacement_span {
            let start_pos = line_map.offset_to_position(start, source_text);
            let end_pos = line_map.offset_to_position(end, source_text);
            entry["replacementSpan"] = serde_json::json!({
                "start": Self::lsp_to_tsserver_position(start_pos),
                "end": Self::lsp_to_tsserver_position(end_pos),
            });
        }

        entry
    }

    pub(crate) fn handle_completions(
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
            let provider = Completions::new_with_types(
                &arena,
                &binder,
                &line_map,
                &interner,
                &source_text,
                file.clone(),
            );
            let completion_result = provider.get_completion_result(root, position);
            let provider_items = completion_result
                .as_ref()
                .map(|result| result.entries.clone())
                .unwrap_or_default();
            let project_items = self.project_completion_items(&file, position);
            let items = if project_items.is_empty() {
                provider_items
            } else {
                project_items
            };

            let entries: Vec<serde_json::Value> = items
                .iter()
                .map(|item| Self::completion_entry_from_item(item, &line_map, &source_text))
                .collect();

            Some(serde_json::json!({
                "isGlobalCompletion": completion_result.as_ref().map(|r| r.is_global_completion).unwrap_or(false),
                "isMemberCompletion": completion_result.as_ref().map(|r| r.is_member_completion).unwrap_or(false),
                "isNewIdentifierLocation": completion_result.as_ref().map(|r| r.is_new_identifier_location).unwrap_or(false),
                "entries": entries,
            }))
        })();
        self.stub_response(
            seq,
            request,
            Some(result.unwrap_or(serde_json::json!({
                "isGlobalCompletion": false,
                "isMemberCompletion": false,
                "isNewIdentifierLocation": false,
                "entries": []
            }))),
        )
    }

    pub(crate) fn handle_completion_details(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let entry_names = request.arguments.get("entryNames")?.as_array()?;
            let (arena, binder, root, source_text) = self.parse_and_bind_file(file)?;
            let line_map = LineMap::build(&source_text);
            let interner = TypeInterner::new();
            let provider = Completions::new_with_types(
                &arena,
                &binder,
                &line_map,
                &interner,
                &source_text,
                file.to_string(),
            );
            let line = request.arguments.get("line")?.as_u64()? as u32;
            let offset = request.arguments.get("offset")?.as_u64()? as u32;
            let position = Self::tsserver_to_lsp_position(line, offset);
            let completion_result = provider.get_completion_result(root, position);
            let provider_items = completion_result
                .as_ref()
                .map(|result| result.entries.clone())
                .unwrap_or_default();
            let project_items = self.project_completion_items(file, position);
            let items = if project_items.is_empty() {
                provider_items
            } else {
                project_items
            };
            let member_parent = completion_result
                .as_ref()
                .and_then(|result| {
                    result
                        .is_member_completion
                        .then(|| provider.get_member_completion_parent_type_name(root, position))
                })
                .flatten();
            let details: Vec<serde_json::Value> = entry_names
                .iter()
                .map(|entry_name| {
                    let (name, requested_source) = if let Some(s) = entry_name.as_str() {
                        (s.to_string(), None)
                    } else if let Some(obj) = entry_name.as_object() {
                        (
                            obj.get("name")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            obj.get("source")
                                .and_then(|v| v.as_str())
                                .map(std::string::ToString::to_string),
                        )
                    } else {
                        (String::new(), None)
                    };
                    // Try to find the matching completion item
                    let item = items.iter().find(|i| {
                        if i.label != name {
                            return false;
                        }
                        if let Some(source) = requested_source.as_deref() {
                            i.source.as_deref() == Some(source)
                        } else {
                            true
                        }
                    });
                    let kind = item.map_or("property", |i| Self::completion_kind_to_str(i.kind));
                    let kind_modifiers =
                        item.and_then(|i| i.kind_modifiers.as_deref()).unwrap_or("");
                    let display_parts = Self::build_completion_display_parts(
                        item,
                        &name,
                        member_parent.as_deref(),
                        &arena,
                        &binder,
                        &source_text,
                    );
                    let documentation = item
                        .and_then(|i| i.documentation.as_ref())
                        .filter(|doc| !doc.is_empty())
                        .map(|doc| serde_json::json!([{"text": doc, "kind": "text"}]));
                    let mut detail = serde_json::Map::new();
                    detail.insert("name".to_string(), serde_json::json!(name));
                    detail.insert("kind".to_string(), serde_json::json!(kind));
                    detail.insert(
                        "kindModifiers".to_string(),
                        serde_json::json!(kind_modifiers),
                    );
                    detail.insert("displayParts".to_string(), display_parts);
                    if let Some(documentation) = documentation {
                        detail.insert("documentation".to_string(), documentation);
                    }
                    if let Some(source) = item.and_then(|i| i.source.as_ref()) {
                        let source_display =
                            serde_json::json!([{ "text": source, "kind": "text" }]);
                        detail.insert("source".to_string(), source_display.clone());
                        detail.insert("sourceDisplay".to_string(), source_display);
                    }
                    if let Some(item) = item
                        && item.has_action
                        && let Some(edits) = item.additional_text_edits.as_ref()
                        && !edits.is_empty()
                    {
                        let text_changes: Vec<serde_json::Value> = edits
                            .iter()
                            .map(|edit| {
                                let start = line_map
                                    .position_to_offset(edit.range.start, &source_text)
                                    .unwrap_or(0);
                                let end = line_map
                                    .position_to_offset(edit.range.end, &source_text)
                                    .unwrap_or(start);
                                serde_json::json!({
                                    "span": {
                                        "start": start,
                                        "length": end.saturating_sub(start),
                                    },
                                    "newText": edit.new_text,
                                })
                            })
                            .collect();

                        let description = item
                            .source
                            .as_deref()
                            .map(|source| format!("Add import from \"{source}\""))
                            .unwrap_or_else(|| format!("Apply completion for '{}'", item.label));

                        detail.insert(
                            "codeActions".to_string(),
                            serde_json::json!([{
                                "description": description,
                                "changes": [{
                                    "fileName": file,
                                    "textChanges": text_changes,
                                }],
                            }]),
                        );
                    }
                    serde_json::Value::Object(detail)
                })
                .collect();
            Some(serde_json::json!(details))
        })();
        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    /// Build rich displayParts for a completion entry, matching TypeScript's format.
    /// Generates structured parts like: class `ClassName`, var name: Type, function name(...), etc.
    pub(crate) fn build_completion_display_parts(
        item: Option<&tsz::lsp::completions::CompletionItem>,
        name: &str,
        member_parent: Option<&str>,
        arena: &tsz::parser::node::NodeArena,
        binder: &tsz::binder::BinderState,
        source_text: &str,
    ) -> serde_json::Value {
        use tsz::lsp::completions::CompletionItemKind;

        let Some(item) = item else {
            return serde_json::json!([{"text": name, "kind": "text"}]);
        };

        let mut parts: Vec<serde_json::Value> = Vec::new();

        match item.kind {
            CompletionItemKind::Class => {
                parts.push(serde_json::json!({"text": "class", "kind": "keyword"}));
                parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                parts.push(serde_json::json!({"text": name, "kind": "className"}));
                if Self::is_merged_namespace_symbol(name, binder) {
                    parts.push(serde_json::json!({"text": "\n", "kind": "lineBreak"}));
                    parts.push(serde_json::json!({"text": "namespace", "kind": "keyword"}));
                    parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                    parts.push(serde_json::json!({"text": name, "kind": "moduleName"}));
                }
            }
            CompletionItemKind::Interface => {
                parts.push(serde_json::json!({"text": "interface", "kind": "keyword"}));
                parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                parts.push(serde_json::json!({"text": name, "kind": "interfaceName"}));
            }
            CompletionItemKind::Enum => {
                parts.push(serde_json::json!({"text": "enum", "kind": "keyword"}));
                parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                parts.push(serde_json::json!({"text": name, "kind": "enumName"}));
            }
            CompletionItemKind::Module => {
                parts.push(serde_json::json!({"text": "namespace", "kind": "keyword"}));
                parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                parts.push(serde_json::json!({"text": name, "kind": "moduleName"}));
            }
            CompletionItemKind::TypeAlias => {
                parts.push(serde_json::json!({"text": "type", "kind": "keyword"}));
                parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                parts.push(serde_json::json!({"text": name, "kind": "aliasName"}));
            }
            CompletionItemKind::TypeParameter => {
                parts.push(serde_json::json!({"text": "(", "kind": "punctuation"}));
                parts.push(serde_json::json!({"text": "type parameter", "kind": "text"}));
                parts.push(serde_json::json!({"text": ")", "kind": "punctuation"}));
                parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                parts.push(serde_json::json!({"text": name, "kind": "typeParameterName"}));
            }
            CompletionItemKind::Function => {
                parts.push(serde_json::json!({"text": "function", "kind": "keyword"}));
                parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                parts.push(serde_json::json!({"text": name, "kind": "functionName"}));
                Self::append_function_signature_from_source(
                    &mut parts,
                    name,
                    binder,
                    arena,
                    source_text,
                );
                if Self::is_merged_namespace_symbol(name, binder) {
                    parts.push(serde_json::json!({"text": "\n", "kind": "lineBreak"}));
                    parts.push(serde_json::json!({"text": "namespace", "kind": "keyword"}));
                    parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                    parts.push(serde_json::json!({"text": name, "kind": "moduleName"}));
                }
            }
            CompletionItemKind::Method => {
                parts.push(serde_json::json!({"text": "(", "kind": "punctuation"}));
                parts.push(serde_json::json!({"text": "method", "kind": "text"}));
                parts.push(serde_json::json!({"text": ")", "kind": "punctuation"}));
                parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                let qualified_name = member_parent
                    .map(|parent| format!("{parent}.{name}"))
                    .unwrap_or_else(|| name.to_string());
                parts.push(serde_json::json!({"text": qualified_name, "kind": "methodName"}));
                if let Some(sig) = item
                    .detail
                    .as_deref()
                    .and_then(Self::method_signature_from_detail)
                {
                    parts.push(serde_json::json!({"text": sig, "kind": "text"}));
                }
            }
            CompletionItemKind::Property => {
                parts.push(serde_json::json!({"text": "(", "kind": "punctuation"}));
                parts.push(serde_json::json!({"text": "property", "kind": "text"}));
                parts.push(serde_json::json!({"text": ")", "kind": "punctuation"}));
                parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                let qualified_name = member_parent
                    .map(|parent| format!("{parent}.{name}"))
                    .unwrap_or_else(|| name.to_string());
                parts.push(serde_json::json!({"text": qualified_name, "kind": "propertyName"}));
                let has_annotation = Self::append_type_annotation_from_source(
                    &mut parts,
                    name,
                    binder,
                    arena,
                    source_text,
                );
                if !has_annotation
                    && let Some(detail) = item.detail.as_deref()
                    && !detail.is_empty()
                {
                    parts.push(serde_json::json!({"text": ":", "kind": "punctuation"}));
                    parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                    parts.push(serde_json::json!({"text": detail, "kind": "keyword"}));
                }
            }
            CompletionItemKind::Variable | CompletionItemKind::Parameter => {
                if item.kind == CompletionItemKind::Parameter {
                    parts.push(serde_json::json!({"text": "(", "kind": "punctuation"}));
                    parts.push(serde_json::json!({"text": "parameter", "kind": "text"}));
                    parts.push(serde_json::json!({"text": ")", "kind": "punctuation"}));
                    parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                    parts.push(serde_json::json!({"text": name, "kind": "parameterName"}));
                } else {
                    let keyword =
                        Self::get_var_keyword_from_source(name, binder, arena, source_text)
                            .unwrap_or({
                                if let Some(ref detail) = item.detail {
                                    match detail.as_str() {
                                        "var" => "var",
                                        _ => "let",
                                    }
                                } else {
                                    "var"
                                }
                            });
                    parts.push(serde_json::json!({"text": keyword, "kind": "keyword"}));
                    parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                    parts.push(serde_json::json!({"text": name, "kind": "localName"}));
                }
                let has_annotation = Self::append_type_annotation_from_source(
                    &mut parts,
                    name,
                    binder,
                    arena,
                    source_text,
                );
                if !has_annotation
                    && item.kind == CompletionItemKind::Parameter
                    && let Some(detail) = item.detail.as_deref()
                    && !detail.is_empty()
                {
                    parts.push(serde_json::json!({"text": ":", "kind": "punctuation"}));
                    parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                    parts.push(serde_json::json!({"text": detail, "kind": "keyword"}));
                }
            }
            CompletionItemKind::Keyword => {
                parts.push(serde_json::json!({"text": name, "kind": "keyword"}));
            }
            CompletionItemKind::Constructor => {
                parts.push(serde_json::json!({"text": "constructor", "kind": "keyword"}));
                parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                parts.push(serde_json::json!({"text": name, "kind": "className"}));
            }
        }

        serde_json::json!(parts)
    }

    fn is_merged_namespace_symbol(name: &str, binder: &tsz::binder::BinderState) -> bool {
        use tsz::binder::symbol_flags;

        binder
            .file_locals
            .get(name)
            .and_then(|sym_id| binder.symbols.get(sym_id))
            .is_some_and(|symbol| {
                (symbol.flags & (symbol_flags::FUNCTION | symbol_flags::CLASS)) != 0
                    && (symbol.flags
                        & (symbol_flags::VALUE_MODULE | symbol_flags::NAMESPACE_MODULE))
                        != 0
            })
    }

    fn method_signature_from_detail(detail: &str) -> Option<String> {
        if !detail.starts_with('(') {
            return None;
        }
        Some(Self::arrow_to_colon(detail))
    }

    fn arrow_to_colon(type_string: &str) -> String {
        let bytes = type_string.as_bytes();
        let mut depth = 0i32;
        let mut last_close = None;
        for (i, &b) in bytes.iter().enumerate() {
            match b {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        last_close = Some(i);
                    }
                }
                _ => {}
            }
        }
        if let Some(close_idx) = last_close {
            let after = &type_string[close_idx + 1..];
            if let Some(arrow_pos) = after.find(" => ") {
                let before = &type_string[..close_idx + 1];
                let ret = &after[arrow_pos + 4..];
                return format!("{before}: {ret}");
            }
        }
        type_string.to_string()
    }

    /// Determine var/let/const from the declaration source text.
    pub(crate) fn get_var_keyword_from_source(
        name: &str,
        binder: &tsz::binder::BinderState,
        arena: &tsz::parser::node::NodeArena,
        source_text: &str,
    ) -> Option<&'static str> {
        use tsz::parser::syntax_kind_ext;

        let symbol_id = binder.file_locals.get(name)?;
        let sym = binder.symbols.get(symbol_id)?;
        let decl = if sym.value_declaration.is_some() {
            sym.value_declaration
        } else {
            *sym.declarations.first()?
        };
        let node = arena.get(decl)?;
        if node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            return None;
        }
        // Walk up to VariableStatement to find the keyword
        let ext = arena.get_extended(decl)?;
        let parent = ext.parent;
        let parent_node = arena.get(parent)?;
        if parent_node.kind != syntax_kind_ext::VARIABLE_DECLARATION_LIST {
            return None;
        }
        let gp_ext = arena.get_extended(parent)?;
        let gp = gp_ext.parent;
        let gp_node = arena.get(gp)?;
        if gp_node.kind != syntax_kind_ext::VARIABLE_STATEMENT {
            return None;
        }
        // Read the first keyword from the statement text
        let start = gp_node.pos as usize;
        let end = gp_node.end.min(source_text.len() as u32) as usize;
        if start >= end {
            return None;
        }
        let stmt_text = source_text[start..end].trim_start();
        if stmt_text.starts_with("const ") || stmt_text.starts_with("const\t") {
            Some("const")
        } else if stmt_text.starts_with("let ") || stmt_text.starts_with("let\t") {
            Some("let")
        } else if stmt_text.starts_with("var ") || stmt_text.starts_with("var\t") {
            Some("var")
        } else {
            None
        }
    }

    /// Extract function signature from source text and append as displayParts.
    pub(crate) fn append_function_signature_from_source(
        parts: &mut Vec<serde_json::Value>,
        name: &str,
        binder: &tsz::binder::BinderState,
        arena: &tsz::parser::node::NodeArena,
        source_text: &str,
    ) {
        let decl_text = binder.file_locals.get(name).and_then(|sid| {
            let sym = binder.symbols.get(sid)?;
            let decl = if sym.value_declaration.is_some() {
                sym.value_declaration
            } else {
                *sym.declarations.first()?
            };
            let node = arena.get(decl)?;
            let start = node.pos as usize;
            let end = node.end.min(source_text.len() as u32) as usize;
            (start < end).then(|| &source_text[start..end])
        });

        if let Some(text) = decl_text
            && let Some(open) = text.find('(')
        {
            let mut depth = 0;
            let mut close = None;
            for (i, ch) in text[open..].char_indices() {
                match ch {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            close = Some(open + i);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            if let Some(close_pos) = close {
                let params_text = &text[open + 1..close_pos];
                parts.push(serde_json::json!({"text": "(", "kind": "punctuation"}));
                let params: Vec<&str> = if params_text.trim().is_empty() {
                    vec![]
                } else {
                    params_text.split(',').collect()
                };
                for (i, param) in params.iter().enumerate() {
                    if i > 0 {
                        parts.push(serde_json::json!({"text": ",", "kind": "punctuation"}));
                        parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                    }
                    let param = param.trim();
                    if let Some(colon_pos) = param.find(':') {
                        let pname = param[..colon_pos].trim();
                        let ptype = param[colon_pos + 1..].trim();
                        parts.push(serde_json::json!({"text": pname, "kind": "parameterName"}));
                        parts.push(serde_json::json!({"text": ":", "kind": "punctuation"}));
                        parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                        parts.push(serde_json::json!({"text": ptype, "kind": "keyword"}));
                    } else {
                        parts.push(serde_json::json!({"text": param, "kind": "parameterName"}));
                    }
                }
                parts.push(serde_json::json!({"text": ")", "kind": "punctuation"}));

                let after_close = text[close_pos + 1..].trim_start();
                if let Some(rest) = after_close.strip_prefix(':') {
                    let ret_type = rest.trim_start();
                    let ret_type = ret_type.split(['{', '\n']).next().unwrap_or("").trim();
                    if !ret_type.is_empty() {
                        parts.push(serde_json::json!({"text": ":", "kind": "punctuation"}));
                        parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                        parts.push(serde_json::json!({"text": ret_type, "kind": "keyword"}));
                    }
                } else {
                    parts.push(serde_json::json!({"text": ":", "kind": "punctuation"}));
                    parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                    parts.push(serde_json::json!({"text": "void", "kind": "keyword"}));
                }
                return;
            }
        }

        // Fallback: empty parens
        parts.push(serde_json::json!({"text": "(", "kind": "punctuation"}));
        parts.push(serde_json::json!({"text": ")", "kind": "punctuation"}));
        parts.push(serde_json::json!({"text": ":", "kind": "punctuation"}));
        parts.push(serde_json::json!({"text": " ", "kind": "space"}));
        parts.push(serde_json::json!({"text": "void", "kind": "keyword"}));
    }

    /// Extract type annotation from source text and append as displayParts.
    pub(crate) fn append_type_annotation_from_source(
        parts: &mut Vec<serde_json::Value>,
        name: &str,
        binder: &tsz::binder::BinderState,
        arena: &tsz::parser::node::NodeArena,
        source_text: &str,
    ) -> bool {
        let decl_text = binder.file_locals.get(name).and_then(|sid| {
            let sym = binder.symbols.get(sid)?;
            let decl = if sym.value_declaration.is_some() {
                sym.value_declaration
            } else {
                *sym.declarations.first()?
            };
            let node = arena.get(decl)?;
            let start = node.pos as usize;
            let end = node.end.min(source_text.len() as u32) as usize;
            (start < end).then(|| &source_text[start..end])
        });

        if let Some(text) = decl_text {
            // Find the name, then look for : after it
            if let Some(name_pos) = text.find(name) {
                let after_name = &text[name_pos + name.len()..];
                let after_name = after_name.trim_start();
                if let Some(rest) = after_name.strip_prefix(':') {
                    let type_text = rest.trim_start();
                    let type_text = type_text
                        .split(['=', ';', '\n'])
                        .next()
                        .unwrap_or("")
                        .trim();
                    if !type_text.is_empty() {
                        parts.push(serde_json::json!({"text": ":", "kind": "punctuation"}));
                        parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                        parts.push(serde_json::json!({"text": type_text, "kind": "keyword"}));
                        return true;
                    }
                }
            }
        }
        false
    }

    pub(crate) fn handle_signature_help(
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
            let provider = SignatureHelpProvider::new(
                &arena,
                &binder,
                &line_map,
                &interner,
                &source_text,
                file,
            );
            let mut type_cache = None;
            let sig_help = provider.get_signature_help(root, position, &mut type_cache)?;
            let items: Vec<serde_json::Value> = sig_help
                .signatures
                .iter()
                .map(|sig| {
                    let params: Vec<serde_json::Value> = sig
                        .parameters
                        .iter()
                        .map(|p| {
                            let display_parts = Self::tokenize_param_label(&p.label);
                            // Build param JSON with correct field order:
                            // name, documentation, displayParts, isOptional, isRest
                            let mut map = serde_json::Map::new();
                            map.insert("name".to_string(), serde_json::json!(p.name));
                            if let Some(ref doc) = p.documentation {
                                map.insert(
                                    "documentation".to_string(),
                                    serde_json::json!([{"text": doc, "kind": "text"}]),
                                );
                            } else {
                                map.insert("documentation".to_string(), serde_json::json!([]));
                            }
                            map.insert(
                                "displayParts".to_string(),
                                serde_json::json!(display_parts),
                            );
                            map.insert("isOptional".to_string(), serde_json::json!(p.is_optional));
                            map.insert("isRest".to_string(), serde_json::json!(p.is_rest));
                            serde_json::Value::Object(map)
                        })
                        .collect();
                    let name_kind = if sig.is_constructor {
                        "className"
                    } else {
                        "functionName"
                    };
                    let prefix_parts = Self::tokenize_sig_prefix(&sig.prefix, name_kind);
                    let suffix_parts = Self::tokenize_sig_suffix(&sig.suffix, name_kind);
                    let mut item = serde_json::json!({
                        "isVariadic": sig.is_variadic,
                        "prefixDisplayParts": prefix_parts,
                        "suffixDisplayParts": suffix_parts,
                        "separatorDisplayParts": [
                            {"text": ",", "kind": "punctuation"},
                            {"text": " ", "kind": "space"}
                        ],
                        "parameters": params,
                    });
                    if let Some(ref doc) = sig.documentation {
                        item["documentation"] = serde_json::json!([{"text": doc, "kind": "text"}]);
                    }
                    // Omit "documentation" when empty (TypeScript omits it)
                    // Build tags: param tags from parameter documentation + non-param tags
                    let mut tags: Vec<serde_json::Value> = Vec::new();
                    // Add @param tags from parameter documentation
                    for p in &sig.parameters {
                        if let Some(ref doc) = p.documentation
                            && !doc.is_empty()
                        {
                            tags.push(serde_json::json!({
                                "name": "param",
                                "text": [
                                    {"text": &p.name, "kind": "parameterName"},
                                    {"text": " ", "kind": "space"},
                                    {"text": doc, "kind": "text"}
                                ]
                            }));
                        }
                    }
                    // Add non-param tags (e.g. @returns, @mytag)
                    for tag in &sig.tags {
                        if tag.text.is_empty() {
                            tags.push(serde_json::json!({
                                "name": &tag.name,
                                "text": []
                            }));
                        } else {
                            tags.push(serde_json::json!({
                                "name": &tag.name,
                                "text": [{"text": &tag.text, "kind": "text"}]
                            }));
                        }
                    }
                    item["tags"] = serde_json::json!(tags);
                    item
                })
                .collect();
            Some(serde_json::json!({
                "items": items,
                "applicableSpan": {
                    "start": sig_help.applicable_span_start,
                    "length": sig_help.applicable_span_length,
                },
                "selectedItemIndex": sig_help.active_signature,
                "argumentIndex": sig_help.active_parameter,
                "argumentCount": sig_help.argument_count,
            }))
        })();
        // Always return a body - processResponse asserts !!response.body.
        // When no signature help is found, return empty items array.
        // The test-worker converts empty items to undefined.
        let body = result.unwrap_or_else(|| {
            serde_json::json!({
                "items": [],
                "applicableSpan": { "start": 0, "length": 0 },
                "selectedItemIndex": 0,
                "argumentIndex": 0,
                "argumentCount": 0,
            })
        });
        self.stub_response(seq, request, Some(body))
    }

    /// Determine the display part kind for a type string.
    pub(crate) fn type_display_kind(type_str: &str) -> &'static str {
        match type_str {
            "void" | "number" | "string" | "boolean" | "any" | "never" | "undefined" | "null"
            | "unknown" | "object" | "symbol" | "bigint" | "true" | "false" => "keyword",
            _ => "text",
        }
    }

    /// Tokenize a signature prefix like "foo(" or "foo<T>(" into display parts.
    pub(crate) fn tokenize_sig_prefix(prefix: &str, name_kind: &str) -> Vec<serde_json::Value> {
        let mut parts = Vec::new();
        // The prefix ends with '('
        if let Some(stripped) = prefix.strip_suffix('(') {
            // Check for type params like "foo<T>"
            if let Some(angle_pos) = stripped.find('<') {
                let name = &stripped[..angle_pos];
                if !name.is_empty() {
                    parts.push(serde_json::json!({"text": name, "kind": name_kind}));
                }
                parts.push(serde_json::json!({"text": "<", "kind": "punctuation"}));
                let type_params_inner = &stripped[angle_pos + 1..];
                let type_params_inner = type_params_inner
                    .strip_suffix('>')
                    .unwrap_or(type_params_inner);
                // Tokenize type parameters
                Self::tokenize_type_params(type_params_inner, &mut parts);
                parts.push(serde_json::json!({"text": ">", "kind": "punctuation"}));
            } else if !stripped.is_empty() {
                parts.push(serde_json::json!({"text": stripped, "kind": name_kind}));
            }
            parts.push(serde_json::json!({"text": "(", "kind": "punctuation"}));
        } else {
            // Fallback
            parts.push(serde_json::json!({"text": prefix, "kind": "text"}));
        }
        parts
    }

    /// Tokenize type parameters like "T, U extends string" into display parts.
    pub(crate) fn tokenize_type_params(input: &str, parts: &mut Vec<serde_json::Value>) {
        let params: Vec<&str> = input.split(',').collect();
        for (i, param) in params.iter().enumerate() {
            if i > 0 {
                parts.push(serde_json::json!({"text": ",", "kind": "punctuation"}));
                parts.push(serde_json::json!({"text": " ", "kind": "space"}));
            }
            let trimmed = param.trim();
            if let Some(ext_pos) = trimmed.find(" extends ") {
                let name = &trimmed[..ext_pos];
                parts.push(serde_json::json!({"text": name, "kind": "typeParameterName"}));
                parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                parts.push(serde_json::json!({"text": "extends", "kind": "keyword"}));
                parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                let constraint = &trimmed[ext_pos + 9..];
                let kind = Self::type_display_kind(constraint);
                parts.push(serde_json::json!({"text": constraint, "kind": kind}));
            } else {
                parts.push(serde_json::json!({"text": trimmed, "kind": "typeParameterName"}));
            }
        }
    }

    /// Tokenize a signature suffix like "): void" into display parts.
    pub(crate) fn tokenize_sig_suffix(suffix: &str, name_kind: &str) -> Vec<serde_json::Value> {
        let mut parts = Vec::new();
        // Suffix is typically "): returnType"
        if let Some(rest) = suffix.strip_prefix(')') {
            parts.push(serde_json::json!({"text": ")", "kind": "punctuation"}));
            if let Some(rest) = rest.strip_prefix(':') {
                parts.push(serde_json::json!({"text": ":", "kind": "punctuation"}));
                if let Some(rest) = rest.strip_prefix(' ') {
                    parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                    // For constructors, use className kind for return type
                    if name_kind == "className" {
                        parts.push(serde_json::json!({"text": rest, "kind": "className"}));
                    } else {
                        Self::tokenize_type_expr(rest, &mut parts);
                    }
                } else if !rest.is_empty() {
                    if name_kind == "className" {
                        parts.push(serde_json::json!({"text": rest, "kind": "className"}));
                    } else {
                        Self::tokenize_type_expr(rest, &mut parts);
                    }
                }
            } else if !rest.is_empty() {
                parts.push(serde_json::json!({"text": rest, "kind": "text"}));
            }
        } else {
            parts.push(serde_json::json!({"text": suffix, "kind": "text"}));
        }
        parts
    }

    /// Tokenize a type expression into display parts.
    pub(crate) fn tokenize_type_expr(type_str: &str, parts: &mut Vec<serde_json::Value>) {
        // Handle type predicates: "x is Type"
        if let Some(is_pos) = type_str.find(" is ") {
            let before = &type_str[..is_pos];
            // Check for "asserts x is Type"
            if let Some(param_name) = before.strip_prefix("asserts ") {
                parts.push(serde_json::json!({"text": "asserts", "kind": "keyword"}));
                parts.push(serde_json::json!({"text": " ", "kind": "space"}));
                parts.push(serde_json::json!({"text": param_name, "kind": "parameterName"}));
            } else {
                parts.push(serde_json::json!({"text": before, "kind": "parameterName"}));
            }
            parts.push(serde_json::json!({"text": " ", "kind": "space"}));
            parts.push(serde_json::json!({"text": "is", "kind": "keyword"}));
            parts.push(serde_json::json!({"text": " ", "kind": "space"}));
            let after = &type_str[is_pos + 4..];
            let kind = Self::type_display_kind(after);
            parts.push(serde_json::json!({"text": after, "kind": kind}));
            return;
        }
        let kind = Self::type_display_kind(type_str);
        parts.push(serde_json::json!({"text": type_str, "kind": kind}));
    }

    /// Tokenize a parameter label like "x: number" or "...args: string[]" into display parts.
    pub(crate) fn tokenize_param_label(label: &str) -> Vec<serde_json::Value> {
        let mut parts = Vec::new();
        let remaining = label;

        // Handle rest parameter prefix
        let remaining = if let Some(rest) = remaining.strip_prefix("...") {
            parts.push(serde_json::json!({"text": "...", "kind": "punctuation"}));
            rest
        } else {
            remaining
        };

        // Split at ": " for name and type
        if let Some(colon_pos) = remaining.find(": ") {
            let name_part = &remaining[..colon_pos];
            // Handle optional marker
            let (name, has_question) = if let Some(n) = name_part.strip_suffix('?') {
                (n, true)
            } else {
                (name_part, false)
            };
            parts.push(serde_json::json!({"text": name, "kind": "parameterName"}));
            if has_question {
                parts.push(serde_json::json!({"text": "?", "kind": "punctuation"}));
            }
            parts.push(serde_json::json!({"text": ":", "kind": "punctuation"}));
            parts.push(serde_json::json!({"text": " ", "kind": "space"}));
            let type_str = &remaining[colon_pos + 2..];
            Self::tokenize_type_expr(type_str, &mut parts);
        } else {
            // No colon - just a parameter name
            parts.push(serde_json::json!({"text": remaining, "kind": "parameterName"}));
        }

        parts
    }
}
