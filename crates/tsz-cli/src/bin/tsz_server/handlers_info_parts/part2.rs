impl Server {
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
            let full = request.command == "navtree-full";
            let native_open_files = if let Some(text) = self.open_files.get(file) {
                let mut map = serde_json::Map::new();
                map.insert(file.to_string(), serde_json::Value::String(text.clone()));
                serde_json::Value::Object(map)
            } else {
                serde_json::json!({})
            };
            if !full
                && let Some(native) = self.try_native_typescript_operation(serde_json::json!({
                "op": "navtree",
                "file": file,
                "openFiles": native_open_files,
                }))
            {
                return Some(native);
            }
            let (arena, _binder, root, source_text) = self.parse_and_bind_file(file)?;
            // tsc's navigationBar treats a file as "external module"
            // only when it has ES module indicators (import/export
            // statements). Binder's `is_external_module` additionally
            // fires on CommonJS `exports.X` / `module.exports = …`,
            // which makes the root render as `"<file>" module` even
            // when tsc would emit `<global> script`. Compute a
            // narrower check here.
            let is_external_module = is_es_module_for_navbar(&arena, root, file);
            let line_map = LineMap::build(&source_text);
            let provider = DocumentSymbolProvider::new(&arena, &line_map, &source_text);
            let mut symbols = provider.get_document_symbols(root);
            sort_symbols_deep(&mut symbols);

            fn range_to_line_span(range: tsz_common::position::Range) -> serde_json::Value {
                serde_json::json!({
                    "start": {
                        "line": range.start.line + 1,
                        "offset": range.start.character + 1,
                    },
                    "end": {
                        "line": range.end.line + 1,
                        "offset": range.end.character + 1,
                    },
                })
            }

            // The tsserver `TextSpan` protocol expresses `start` and `length`
            // in UTF-16 code units, matching how TypeScript's compiler walks
            // source text. `LineMap::position_to_offset` returns Rust byte
            // offsets, so we must remap before reporting the span (issue
            // #3912). `byte_offset_to_utf16_units` counts the UTF-16 units
            // produced by the prefix `&source_text[..byte_offset]` without
            // panicking on non-char-boundary offsets.
            fn byte_offset_to_utf16_units(source: &str, byte_offset: u32) -> u32 {
                let target = byte_offset as usize;
                if target == 0 {
                    return 0;
                }
                let mut utf16_count = 0u32;
                for (i, ch) in source.char_indices() {
                    if i >= target {
                        break;
                    }
                    utf16_count += ch.len_utf16() as u32;
                }
                utf16_count
            }

            fn range_to_text_span(
                line_map: &LineMap,
                source_text: &str,
                range: tsz_common::position::Range,
            ) -> serde_json::Value {
                let start_byte = line_map
                    .position_to_offset(range.start, source_text)
                    .unwrap_or(0);
                let end_byte = line_map
                    .position_to_offset(range.end, source_text)
                    .unwrap_or(start_byte);
                let start = byte_offset_to_utf16_units(source_text, start_byte);
                let end = byte_offset_to_utf16_units(source_text, end_byte);
                serde_json::json!({
                    "start": start,
                    "length": end.saturating_sub(start),
                })
            }

            fn symbol_to_navtree(
                sym: &tsz::lsp::symbols::document_symbols::DocumentSymbol,
                line_map: &LineMap,
                source_text: &str,
                full: bool,
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
                let children: Vec<serde_json::Value> = sym
                    .children
                    .iter()
                    .map(|child| symbol_to_navtree(child, line_map, source_text, full))
                    .collect();
                let span = if full {
                    range_to_text_span(line_map, source_text, sym.range)
                } else {
                    range_to_line_span(sym.range)
                };
                let mut obj = serde_json::json!({
                    "text": sym.name,
                    "kind": kind,
                    "spans": [span],
                });
                if full {
                    obj["nameSpan"] =
                        range_to_text_span(line_map, source_text, sym.selection_range);
                }
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

            let child_items: Vec<serde_json::Value> = symbols
                .iter()
                .map(|sym| symbol_to_navtree(sym, &line_map, &source_text, full))
                .collect();

            // Compute the end span based on source text length
            let total_lines = source_text.lines().count();
            let last_line_len = source_text.lines().last().map_or(0, str::len);
            // External modules get a filename-as-module wrapper instead of
            // the `<global>` / `script` header that scripts use. Matches
            // tsserver's `getNavigationTree` output.
            let (text, kind) = if is_external_module {
                let basename = std::path::Path::new(file)
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or("")
                    .to_string();
                // tsc's `getItemName` wraps the filename-derived module
                // name with `escapeString` (double-quote style) before
                // quoting. Mirrors that so control characters render as
                // their escape sequences (\t, \n, \\…).
                (
                    format!("\"{}\"", escape_string_double_quote(&basename)),
                    "module",
                )
            } else {
                ("<global>".to_string(), "script")
            };
            let root_span = if full {
                serde_json::json!({
                    "start": 0,
                    "length": source_text.encode_utf16().count(),
                })
            } else {
                serde_json::json!({"start": {"line": 1, "offset": 1}, "end": {"line": total_lines, "offset": last_line_len + 1}})
            };
            Some(serde_json::json!({
                "text": text,
                "kind": kind,
                "childItems": child_items,
                "spans": [root_span],
            }))
        })();
        let fallback = if request.command == "navtree-full" {
            serde_json::json!({
                "text": "<global>",
                "kind": "script",
                "childItems": [],
                "spans": [{"start": 0, "length": 0}],
            })
        } else {
            serde_json::json!({
                "text": "<global>",
                "kind": "script",
                "childItems": [],
                "spans": [{"start": {"line": 1, "offset": 1}, "end": {"line": 1, "offset": 1}}],
            })
        };
        self.success_response(seq, request, Some(result.unwrap_or(fallback)))
    }

    pub(crate) fn handle_navbar(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let native_open_files = if let Some(text) = self.open_files.get(file) {
                let mut map = serde_json::Map::new();
                map.insert(file.to_string(), serde_json::Value::String(text.clone()));
                serde_json::Value::Object(map)
            } else {
                serde_json::json!({})
            };
            if let Some(native) = self.try_native_typescript_operation(serde_json::json!({
                "op": "navbar",
                "file": file,
                "openFiles": native_open_files,
            })) {
                return Some(native);
            }
            let (arena, _binder, root, source_text) = self.parse_and_bind_file(file)?;
            let is_external_module = is_es_module_for_navbar(&arena, root, file);
            let line_map = LineMap::build(&source_text);
            let provider = DocumentSymbolProvider::new(&arena, &line_map, &source_text);
            let mut symbols = provider.get_document_symbols(root);
            sort_symbols_deep(&mut symbols);

            /// Check if a symbol should appear as its own entry in the
            /// primary navigation bar menu. Mirrors tsc's
            /// `shouldAppearInPrimaryNavBarMenu` /
            /// `isTopLevelFunctionDeclaration`: leaf functions only promote
            /// when their parent is `SourceFile` / `ModuleBlock` / `Method` /
            /// `Constructor` — not `SetAccessor` / `GetAccessor`, which is why
            /// `function f() {}` inside a setter stays collapsed.
            const fn should_appear_in_primary_navbar(
                sym: &tsz::lsp::symbols::document_symbols::DocumentSymbol,
                parent_kind: Option<tsz::lsp::symbols::document_symbols::SymbolKind>,
            ) -> bool {
                use tsz::lsp::symbols::document_symbols::SymbolKind;
                if !sym.children.is_empty() {
                    return true;
                }
                match sym.kind {
                    SymbolKind::Class
                    | SymbolKind::Enum
                    | SymbolKind::Interface
                    | SymbolKind::Module
                    | SymbolKind::Namespace
                    | SymbolKind::File
                    | SymbolKind::Struct => true,
                    // tsc's `isTopLevelFunctionDeclaration` allows only
                    // SourceFile, ModuleBlock, MethodDeclaration, and
                    // Constructor as parents. A function at the source
                    // file level (None parent) or inside a method /
                    // constructor body surfaces — but a function inside
                    // a namespace body is NOT top-level here, because
                    // tsc's parent-check sees the ModuleDeclaration
                    // nav node (which isn't in the allowed list), not
                    // the underlying ModuleBlock.
                    SymbolKind::Function => matches!(
                        parent_kind,
                        None // root (source file)
                            | Some(
                                SymbolKind::File | SymbolKind::Method | SymbolKind::Constructor
                            )
                    ),
                    _ => false,
                }
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
                    if should_appear_in_primary_navbar(child, Some(sym.kind)) {
                        symbol_to_navbar_item(child, indent + 1, items);
                    }
                }
            }

            let mut items = Vec::new();
            // Root item — external modules get a filename-as-module header
            // (matches tsserver's `getNavigationBarItems` — mirrors the
            // navtree wrapping in `handle_navtree`).
            let total_lines = source_text.lines().count();
            let last_line_len = source_text.lines().last().map_or(0, str::len);
            let child_items: Vec<serde_json::Value> =
                symbols.iter().map(navbar_child_item).collect();
            let (root_text, root_kind) = if is_external_module {
                let basename = std::path::Path::new(file)
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or("")
                    .to_string();
                (
                    format!("\"{}\"", escape_string_double_quote(&basename)),
                    "module",
                )
            } else {
                ("<global>".to_string(), "script")
            };
            let mut root = serde_json::json!({
                "text": root_text,
                "kind": root_kind,
                "indent": 0,
                "spans": [{"start": {"line": 1, "offset": 1}, "end": {"line": total_lines, "offset": last_line_len + 1}}],
            });
            if !child_items.is_empty() {
                root["childItems"] = serde_json::json!(child_items);
            }
            items.push(root);
            // Only add top-level symbols that qualify as primary navbar
            // items. `parent_kind = None` corresponds to the source file
            // root — tsc treats SourceFile as a valid "top-level" parent
            // for promoting leaf functions.
            for sym in &symbols {
                if should_appear_in_primary_navbar(sym, None) {
                    symbol_to_navbar_item(sym, 1, &mut items);
                }
            }
            Some(serde_json::json!(items))
        })();
        self.success_response(
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
            if let Some(native) = self.try_native_typescript_operation(serde_json::json!({
                "op": "navto",
                "searchValue": search_value,
                "file": request.arguments.get("file").and_then(serde_json::Value::as_str).unwrap_or(""),
            })) {
                return Some(native);
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
        self.success_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
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
            let position = Self::tsserver_to_lsp_position(line, offset);
            let mut project = self.build_project_for_file(&file)?;
            let locations = project.get_implementations(&file, position)?;
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
        self.success_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    pub(crate) fn handle_file_references(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let project = self.build_project_for_file(file)?;

            let refs: Vec<serde_json::Value> = project
                .find_file_references(file)
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
                    Some(serde_json::json!({
                        "file": loc.file_path,
                        "start": Self::lsp_to_tsserver_position(loc.range.start),
                        "end": Self::lsp_to_tsserver_position(loc.range.end),
                        "lineText": line_text,
                        "isWriteAccess": false,
                        "isDefinition": false,
                    }))
                })
                .collect();

            Some(serde_json::json!({
                "refs": refs,
                "symbolName": format!("\"{}\"", file),
            }))
        })();
        self.success_response(
            seq,
            request,
            Some(result.unwrap_or(serde_json::json!({"refs": [], "symbolName": ""}))),
        )
    }
}
