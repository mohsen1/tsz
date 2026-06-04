impl Server {
    fn is_class_member_snippet_context(
        source_text: &str,
        line_map: &LineMap,
        position: Position,
    ) -> bool {
        let Some(offset) = line_map.position_to_offset(position, source_text) else {
            return false;
        };
        let end = (offset as usize).min(source_text.len());
        let text = &source_text[..end];
        let Some(class_pos) = text.rfind("class ") else {
            return false;
        };
        let Some(rel_open) = text[class_pos..].find('{') else {
            return false;
        };
        let open = class_pos + rel_open;
        if open + 1 >= end {
            return true;
        }
        let mut depth = 1i32;
        for &b in &text.as_bytes()[open + 1..] {
            match b {
                b'{' => depth += 1,
                b'}' => depth -= 1,
                _ => {}
            }
            if depth <= 0 {
                return false;
            }
        }
        depth == 1
    }

    fn completion_result_at_position(
        provider: &Completions<'_>,
        root: tsz::parser::base::NodeIndex,
        position: Position,
    ) -> (Position, Option<tsz::lsp::completions::CompletionResult>) {
        (position, provider.get_completion_result(root, position))
    }

    fn is_bare_identifier_expression_prefix(
        source_text: &str,
        line_map: &LineMap,
        position: Position,
    ) -> bool {
        let Some(offset) = line_map.position_to_offset(position, source_text) else {
            return false;
        };
        let prefix = &source_text[..offset as usize];
        let line_start = prefix.rfind('\n').map_or(0, |idx| idx + 1);
        let line = prefix[line_start..].trim();
        !line.is_empty()
            && !line.chars().next().is_some_and(|ch| ch.is_ascii_digit())
            && line
                .chars()
                .all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
    }

    fn is_class_member_declaration_prefix_context(
        source_text: &str,
        line_map: &LineMap,
        position: Position,
    ) -> bool {
        let Some(offset) = line_map.position_to_offset(position, source_text) else {
            return false;
        };
        let prefix = &source_text[..offset as usize];
        let line_start = prefix.rfind('\n').map_or(0, |idx| idx + 1);
        let line = prefix[line_start..].trim();
        if !(line.is_empty() || Self::is_identifier(line)) {
            return false;
        }

        let mut brace_stack: Vec<usize> = Vec::new();
        for (idx, ch) in prefix.char_indices() {
            match ch {
                '{' => brace_stack.push(idx),
                '}' => {
                    let _ = brace_stack.pop();
                }
                _ => {}
            }
        }
        let Some(class_body_start) = brace_stack.last().copied() else {
            return false;
        };
        let before_brace = prefix[..class_body_start].trim_end();
        let header_start = before_brace.rfind(['{', '}', ';']).map_or(0, |idx| idx + 1);
        let header = before_brace[header_start..].trim();
        header
            .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '$'))
            .any(|part| part == "class")
    }

    fn is_type_annotation_identifier_prefix_context(
        source_text: &str,
        line_map: &LineMap,
        position: Position,
    ) -> bool {
        let Some(offset) = line_map.position_to_offset(position, source_text) else {
            return false;
        };
        let end = (offset as usize).min(source_text.len());
        let prefix = source_text[..end].trim_end();
        let line_start = prefix.rfind('\n').map_or(0, |idx| idx + 1);
        let line = &prefix[line_start..];
        if line.is_empty() {
            return false;
        }

        let bytes = line.as_bytes();
        let mut idx = bytes.len();
        while idx > 0 {
            let ch = bytes[idx - 1] as char;
            if ch == '_' || ch == '$' || ch.is_ascii_alphanumeric() {
                idx -= 1;
            } else {
                break;
            }
        }
        if idx == bytes.len() {
            return false;
        }

        line[..idx].trim_end().ends_with(':')
    }

    fn prune_deeper_auto_import_duplicates(items: Vec<CompletionItem>) -> Vec<CompletionItem> {
        let mut best_rank_by_label: std::collections::HashMap<String, (usize, usize)> =
            std::collections::HashMap::new();

        for item in &items {
            let Some(source) = item.source.as_deref() else {
                continue;
            };
            let is_path_like_source = source.starts_with('.') || source.starts_with('/');
            if !item.has_action || !is_path_like_source {
                continue;
            }
            let depth = source.matches('/').count();
            let index_penalty = usize::from(
                source == "."
                    || source == ".."
                    || source.ends_with("/index")
                    || source.ends_with("/index.ts")
                    || source.ends_with("/index.js"),
            );
            best_rank_by_label
                .entry(item.label.clone())
                .and_modify(|current| {
                    if (depth, index_penalty) < *current {
                        *current = (depth, index_penalty);
                    }
                })
                .or_insert((depth, index_penalty));
        }

        items
            .into_iter()
            .filter(|item| {
                let Some(source) = item.source.as_deref() else {
                    return true;
                };
                let is_path_like_source = source.starts_with('.') || source.starts_with('/');
                if !item.has_action || !is_path_like_source {
                    return true;
                }
                let depth = source.matches('/').count();
                let index_penalty = usize::from(
                    source == "."
                        || source == ".."
                        || source.ends_with("/index")
                        || source.ends_with("/index.ts")
                        || source.ends_with("/index.js"),
                );
                let Some((best_depth, best_index_penalty)) = best_rank_by_label.get(&item.label)
                else {
                    return true;
                };
                (depth, index_penalty) <= (*best_depth, *best_index_penalty)
            })
            .collect()
    }

    fn merge_non_member_completion_items(
        provider_items: Vec<CompletionItem>,
        project_items: Vec<CompletionItem>,
    ) -> Vec<CompletionItem> {
        if project_items.is_empty() {
            return provider_items;
        }
        if provider_items.is_empty() {
            return project_items;
        }

        let mut merged = provider_items;
        let mut seen = FxHashSet::default();
        for item in &merged {
            seen.insert(Self::completion_merge_key(item));
        }

        for item in project_items {
            let key = Self::completion_merge_key(&item);
            if seen.insert(key) {
                merged.push(item);
            }
        }

        merged
    }

    fn completion_merge_key(
        item: &CompletionItem,
    ) -> (String, Option<String>, String, Option<String>) {
        (
            item.label.clone(),
            item.source.clone(),
            Self::completion_kind_to_str(item.kind).to_string(),
            item.additional_text_edits
                .as_ref()
                .and_then(|edits| edits.first().map(|edit| edit.new_text.clone())),
        )
    }

    fn maybe_add_verbatim_commonjs_auto_import_items(
        &self,
        file_name: &str,
        _source_text: &str,
        items: Vec<CompletionItem>,
    ) -> Vec<CompletionItem> {
        if !Self::is_ts_like_file(file_name) {
            return items;
        }
        let fallback = self.verbatim_commonjs_auto_import_items(file_name);
        if fallback.is_empty() {
            items
        } else {
            Self::merge_non_member_completion_items(items, fallback)
        }
    }

    fn maybe_add_merged_class_function_members(
        mut items: Vec<CompletionItem>,
        source_text: &str,
        completion_offset: u32,
        is_member_completion: bool,
    ) -> Vec<CompletionItem> {
        if !is_member_completion {
            return items;
        }
        if !Self::looks_like_merged_class_member_completion_context(source_text, completion_offset)
        {
            return items;
        }
        if !items.iter().any(|item| item.label == "prototype") {
            return items;
        }
        if items
            .iter()
            .any(|item| matches!(item.label.as_str(), "apply" | "call" | "bind"))
        {
            return items;
        }

        let mut existing_labels = FxHashSet::default();
        for item in &items {
            existing_labels.insert(item.label.clone());
        }

        let function_members = [
            (
                "apply",
                CompletionItemKind::Method,
                Some("declare"),
                None,
                true,
            ),
            (
                "call",
                CompletionItemKind::Method,
                Some("declare"),
                None,
                true,
            ),
            (
                "bind",
                CompletionItemKind::Method,
                Some("declare"),
                None,
                true,
            ),
            (
                "toString",
                CompletionItemKind::Method,
                Some("declare"),
                None,
                true,
            ),
            (
                "length",
                CompletionItemKind::Property,
                Some("declare"),
                Some("number"),
                false,
            ),
            (
                "arguments",
                CompletionItemKind::Property,
                Some("declare"),
                Some("any"),
                false,
            ),
            (
                "caller",
                CompletionItemKind::Property,
                Some("declare"),
                None,
                false,
            ),
        ];

        for (name, kind, kind_modifiers, detail, is_snippet) in function_members {
            if !existing_labels.insert(name.to_string()) {
                continue;
            }
            let mut item = CompletionItem::new(name.to_string(), kind);
            item.sort_text = Some(sort_priority::LOCATION_PRIORITY.to_string());
            if let Some(kind_modifiers) = kind_modifiers {
                item.kind_modifiers = Some(kind_modifiers.to_string());
            }
            if let Some(detail) = detail {
                item.detail = Some(detail.to_string());
            }
            if is_snippet {
                item.insert_text = Some(format!("{name}($1)"));
                item.is_snippet = true;
            }
            items.push(item);
        }

        items
    }

    fn looks_like_merged_class_member_completion_context(
        source_text: &str,
        completion_offset: u32,
    ) -> bool {
        let prefix_end = (completion_offset as usize).min(source_text.len());
        let prefix = &source_text[..prefix_end];
        let trimmed = prefix.trim_end();
        let Some(before_dot) = trimmed.strip_suffix('.') else {
            return false;
        };
        let before_dot = before_dot.trim_end();
        let ident = before_dot
            .rsplit(|c: char| !(c == '_' || c == '$' || c.is_ascii_alphanumeric()))
            .next()
            .unwrap_or_default();
        if ident.is_empty() {
            return false;
        }
        ident
            .chars()
            .next()
            .is_some_and(|ch| ch == '_' || ch.is_ascii_uppercase())
    }

    fn is_ts_like_file(path: &str) -> bool {
        matches!(
            Path::new(path)
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.to_ascii_lowercase())
                .as_deref(),
            Some("ts" | "tsx" | "mts" | "cts")
        )
    }

    fn verbatim_commonjs_auto_import_items(&self, file_name: &str) -> Vec<CompletionItem> {
        let mut out = Vec::new();
        let mut seen = FxHashSet::default();
        let scan_paths =
            Self::fallback_class_member_scan_paths(&self.open_files, &self.external_project_files);

        for path in &scan_paths {
            let Some(content) = self
                .open_files
                .get(path)
                .cloned()
                .or_else(|| std::fs::read_to_string(path).ok())
            else {
                continue;
            };
            for (module_specifier, alias, members) in
                Self::extract_ambient_export_equals_modules(&content)
            {
                Self::push_verbatim_commonjs_auto_import_item(
                    &mut out,
                    &mut seen,
                    &module_specifier,
                    &alias,
                    &alias,
                    CompletionItemKind::Variable,
                );
                for member in members {
                    Self::push_verbatim_commonjs_auto_import_item(
                        &mut out,
                        &mut seen,
                        &module_specifier,
                        &alias,
                        &member,
                        CompletionItemKind::Function,
                    );
                }
            }
        }

        for path in scan_paths {
            if path == file_name || !Self::is_js_like_completion_file(&path) {
                continue;
            }
            let Some(content) = self
                .open_files
                .get(&path)
                .cloned()
                .or_else(|| std::fs::read_to_string(&path).ok())
            else {
                continue;
            };
            if !content.contains("module.exports") {
                continue;
            }
            let Some(module_specifier) = Self::relative_module_specifier(file_name, &path) else {
                continue;
            };
            let alias = Self::commonjs_binding_name_from_specifier(&module_specifier);
            if alias.is_empty() {
                continue;
            }

            let members = Self::extract_module_exports_object_members(&content);
            if members.is_empty() {
                continue;
            }

            Self::push_verbatim_commonjs_auto_import_item(
                &mut out,
                &mut seen,
                &module_specifier,
                &alias,
                &alias,
                CompletionItemKind::Variable,
            );
            for member in members {
                Self::push_verbatim_commonjs_auto_import_item(
                    &mut out,
                    &mut seen,
                    &module_specifier,
                    &alias,
                    &member,
                    CompletionItemKind::Function,
                );
            }
        }

        out
    }

    fn push_verbatim_commonjs_auto_import_item(
        out: &mut Vec<CompletionItem>,
        seen: &mut FxHashSet<(String, String)>,
        module_specifier: &str,
        alias: &str,
        label: &str,
        kind: CompletionItemKind,
    ) {
        let key = (label.to_string(), module_specifier.to_string());
        if !seen.insert(key) {
            return;
        }

        let insert_text = if label == alias {
            alias.to_string()
        } else {
            format!("{alias}.{label}")
        };
        let import_stmt = format!("import {alias} = require(\"{module_specifier}\");\n\n");
        let edits = vec![tsz::lsp::rename::TextEdit::new(
            tsz::lsp::position::Range::new(
                tsz::lsp::position::Position::new(0, 0),
                tsz::lsp::position::Position::new(0, 0),
            ),
            import_stmt,
        )];
        let module_specifier_str = module_specifier.to_string();
        let item = CompletionItem::new(label.to_string(), kind)
            .with_has_action()
            .with_sort_text(sort_priority::AUTO_IMPORT)
            .with_source(module_specifier_str.clone())
            .with_source_display(module_specifier_str)
            .with_kind_modifiers("export".to_string())
            .with_insert_text(insert_text)
            .with_additional_edits(edits);
        out.push(item);
    }

    fn is_js_like_completion_file(path: &str) -> bool {
        matches!(
            Path::new(path)
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.to_ascii_lowercase())
                .as_deref(),
            Some("js" | "jsx" | "mjs" | "cjs")
        )
    }

    fn relative_module_specifier(from_file: &str, target_file: &str) -> Option<String> {
        let from = Path::new(from_file);
        let target = Path::new(target_file);
        let (Some(from_parent), Some(target_parent)) = (from.parent(), target.parent()) else {
            return None;
        };
        if from_parent != target_parent {
            return None;
        }
        let stem = target.file_stem()?.to_str()?;
        Some(format!("./{stem}"))
    }

    fn commonjs_binding_name_from_specifier(specifier: &str) -> String {
        let trimmed = specifier.trim();
        let last_segment = if trimmed.starts_with('@') {
            trimmed.rsplit('/').next().unwrap_or(trimmed)
        } else {
            trimmed
                .trim_start_matches("./")
                .trim_start_matches("../")
                .rsplit('/')
                .next()
                .unwrap_or(trimmed)
        };
        let base = last_segment
            .trim_end_matches(".d.ts")
            .trim_end_matches(".d.mts")
            .trim_end_matches(".d.cts")
            .trim_end_matches(".ts")
            .trim_end_matches(".tsx")
            .trim_end_matches(".mts")
            .trim_end_matches(".cts")
            .trim_end_matches(".js")
            .trim_end_matches(".jsx")
            .trim_end_matches(".mjs")
            .trim_end_matches(".cjs");

        let mut out = String::new();
        let mut upper_next = false;
        for ch in base.chars() {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '$' {
                if out.is_empty() {
                    out.push(ch.to_ascii_lowercase());
                } else if upper_next {
                    out.push(ch.to_ascii_uppercase());
                    upper_next = false;
                } else {
                    out.push(ch);
                }
            } else {
                upper_next = true;
            }
        }
        out
    }

    fn commonjs_require_member_completion_items(
        &self,
        file_name: &str,
        source_text: &str,
        completion_offset: u32,
    ) -> Vec<CompletionItem> {
        let Some((receiver, member_prefix)) =
            Self::member_receiver_and_prefix(source_text, completion_offset)
        else {
            return Vec::new();
        };
        let Some(module_specifier) =
            Self::require_module_specifier_for_alias(source_text, &receiver)
        else {
            return Vec::new();
        };

        let candidate_paths =
            Self::resolve_auto_import_source_candidate_paths(file_name, &module_specifier);
        for target_path in candidate_paths {
            let Some((arena, binder, root, target_source_text)) =
                self.parse_and_bind_file(&target_path)
            else {
                continue;
            };
            let exports = Self::extract_commonjs_assignment_exports(&target_source_text);
            if exports.is_empty() {
                continue;
            }
            let mut seen = FxHashSet::default();
            let mut items = Vec::new();
            for (export_name, local_name) in exports {
                if !export_name.starts_with(&member_prefix) || !seen.insert(export_name.clone()) {
                    continue;
                }
                let mut item = CompletionItem::new(export_name.clone(), CompletionItemKind::Alias)
                    .with_sort_text(sort_priority::MEMBER);
                let mut alias_detail = format!("var {export_name}");
                if let Some(function_type) = Self::function_initializer_type_annotation(
                    &local_name,
                    &binder,
                    &arena,
                    &target_source_text,
                ) {
                    alias_detail.push_str(": ");
                    alias_detail.push_str(&function_type);
                }
                alias_detail.push('\n');
                alias_detail.push_str(&format!("import {receiver}.{export_name}"));
                item = item.with_detail(alias_detail);
                if let Some(symbol_id) = binder.file_locals.get(&local_name)
                    && let Some(symbol) = binder.symbols.get(symbol_id)
                    && let Some(decl) = symbol.primary_declaration()
                {
                    let doc = jsdoc_for_node(&arena, root, decl, &target_source_text);
                    if !doc.is_empty() {
                        item = item.with_documentation(doc);
                    }
                }
                items.push(item);
            }
            if !items.is_empty() {
                return items;
            }
        }

        Vec::new()
    }

    fn member_receiver_and_prefix(
        source_text: &str,
        completion_offset: u32,
    ) -> Option<(String, String)> {
        let prefix_end = (completion_offset as usize).min(source_text.len());
        let prefix = &source_text[..prefix_end];
        let trimmed = prefix.trim_end();
        let dot_idx = trimmed.rfind('.')?;
        let before_dot = trimmed[..dot_idx].trim_end();
        let receiver = before_dot
            .rsplit(|c: char| !(c == '_' || c == '$' || c.is_ascii_alphanumeric()))
            .next()
            .unwrap_or_default();
        if receiver.is_empty() || !Self::is_identifier(receiver) {
            return None;
        }
        let member_prefix = trimmed[dot_idx + 1..].trim();
        if !member_prefix.is_empty() && !Self::is_identifier(member_prefix) {
            return None;
        }
        Some((receiver.to_string(), member_prefix.to_string()))
    }

    fn require_module_specifier_for_alias(source_text: &str, alias: &str) -> Option<String> {
        for line in source_text.lines() {
            let trimmed = line.trim();
            if !trimmed.starts_with("import ") || !trimmed.contains("require(") {
                continue;
            }
            let Some(eq_idx) = trimmed.find('=') else {
                continue;
            };
            let alias_part = trimmed["import ".len()..eq_idx].trim();
            if alias_part != alias {
                continue;
            }
            let Some(require_idx) = trimmed.find("require(") else {
                continue;
            };
            let after_require = &trimmed[require_idx + "require(".len()..];
            let quote = after_require.chars().next()?;
            if quote != '"' && quote != '\'' {
                continue;
            }
            let end_rel = after_require[1..].find(quote)?;
            let specifier = after_require[1..1 + end_rel].trim();
            if !specifier.is_empty() {
                return Some(specifier.to_string());
            }
        }
        None
    }

    fn extract_commonjs_assignment_exports(content: &str) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for line in content.lines() {
            let trimmed = line.trim();
            for prefix in ["exports.", "module.exports."] {
                let Some(rest) = trimmed.strip_prefix(prefix) else {
                    continue;
                };
                let name_end = rest
                    .find(|ch: char| !(ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()))
                    .unwrap_or(rest.len());
                if name_end == 0 {
                    continue;
                }
                let export_name = rest[..name_end].trim();
                if !Self::is_identifier(export_name) {
                    continue;
                }
                let after_name = rest[name_end..].trim_start();
                let Some(after_eq) = after_name.strip_prefix('=') else {
                    continue;
                };
                let rhs = after_eq.trim_start();
                let rhs_end = rhs
                    .find(|ch: char| !(ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()))
                    .unwrap_or(rhs.len());
                let local_name = rhs[..rhs_end].trim();
                if local_name.is_empty() || !Self::is_identifier(local_name) {
                    continue;
                }
                out.push((export_name.to_string(), local_name.to_string()));
            }
        }
        out
    }

    fn extract_module_exports_object_members(content: &str) -> Vec<String> {
        let Some(exports_idx) = content.find("module.exports") else {
            return Vec::new();
        };
        let Some(open_rel) = content[exports_idx..].find('{') else {
            return Vec::new();
        };
        let body_start = exports_idx + open_rel + 1;
        let bytes = content.as_bytes();
        let mut depth = 1usize;
        let mut idx = body_start;
        while idx < bytes.len() {
            match bytes[idx] as char {
                '{' => depth += 1,
                '}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            idx += 1;
        }
        if idx <= body_start || idx > content.len() {
            return Vec::new();
        }
        let body = &content[body_start..idx];
        let mut seen = FxHashSet::default();
        let mut out = Vec::new();
        for line in body.lines() {
            let trimmed = line.trim();
            let Some((raw_name, _)) = trimmed.split_once(':') else {
                continue;
            };
            let name = raw_name.trim().trim_matches('"').trim_matches('\'');
            let name_str = name.to_string();
            if Self::is_identifier(name) && seen.insert(name_str.clone()) {
                out.push(name_str);
            }
        }
        out
    }

    fn extract_ambient_export_equals_modules(content: &str) -> Vec<(String, String, Vec<String>)> {
        let mut modules = Vec::new();
        let mut cursor = 0usize;
        while let Some(decl_rel) = content[cursor..].find("declare module ") {
            let decl_start = cursor + decl_rel;
            let after_decl = decl_start + "declare module ".len();
            let quote = content[after_decl..]
                .chars()
                .find(|ch| *ch == '"' || *ch == '\'');
            let Some(quote) = quote else {
                cursor = after_decl;
                continue;
            };
            let quote_start = content[after_decl..].find(quote).map(|i| after_decl + i);
            let Some(quote_start) = quote_start else {
                cursor = after_decl;
                continue;
            };
            let module_name_start = quote_start + 1;
            let Some(quote_end_rel) = content[module_name_start..].find(quote) else {
                cursor = module_name_start;
                continue;
            };
            let module_name_end = module_name_start + quote_end_rel;
            let module_name = content[module_name_start..module_name_end].trim();
            let Some(open_brace_rel) = content[module_name_end..].find('{') else {
                cursor = module_name_end;
                continue;
            };
            let body_start = module_name_end + open_brace_rel + 1;
            let mut depth = 1usize;
            let bytes = content.as_bytes();
            let mut idx = body_start;
            while idx < bytes.len() {
                match bytes[idx] as char {
                    '{' => depth += 1,
                    '}' => {
                        depth = depth.saturating_sub(1);
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
                idx += 1;
            }
            if idx <= body_start || idx > content.len() {
                cursor = body_start;
                continue;
            }
            let body = &content[body_start..idx];
            let alias = body
                .lines()
                .find_map(|line| {
                    let trimmed = line.trim();
                    let rest = trimmed.strip_prefix("export = ")?;
                    let alias = rest.trim_end_matches(';').trim();
                    Self::is_identifier(alias).then(|| alias.to_string())
                })
                .unwrap_or_default();
            if !alias.is_empty() {
                let mut members = Vec::new();
                let mut seen = FxHashSet::default();
                for line in body.lines() {
                    let trimmed = line.trim();
                    let Some(paren_idx) = trimmed.find('(') else {
                        continue;
                    };
                    let mut head = trimmed[..paren_idx].trim();
                    if head.ends_with('?') {
                        head = head.trim_end_matches('?').trim();
                    }
                    let head_str = head.to_string();
                    if Self::is_identifier(head) && seen.insert(head_str.clone()) {
                        members.push(head_str);
                    }
                }
                modules.push((module_name.to_string(), alias, members));
            }
            cursor = idx + 1;
        }
        modules
    }

    fn is_identifier(text: &str) -> bool {
        let mut chars = text.chars();
        let Some(first) = chars.next() else {
            return false;
        };
        if !(first.is_ascii_alphabetic() || first == '_' || first == '$') {
            return false;
        }
        chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$')
    }

    fn string_pref(preferences: Option<&serde_json::Value>, key: &str) -> Option<String> {
        preferences
            .and_then(|p| p.get(key))
            .and_then(serde_json::Value::as_str)
            .map(std::string::ToString::to_string)
    }

    fn string_array_pref(
        preferences: Option<&serde_json::Value>,
        key: &str,
    ) -> Option<Vec<String>> {
        preferences
            .and_then(|p| p.get(key))
            .and_then(serde_json::Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(std::string::ToString::to_string))
                    .collect()
            })
    }

    fn bool_pref_or_default(
        preferences: Option<&serde_json::Value>,
        key: &str,
        default: bool,
    ) -> bool {
        preferences
            .and_then(|p| p.get(key))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(default)
    }

    fn is_default_auto_import_item(item: &CompletionItem) -> bool {
        if !item.has_action {
            return false;
        }
        let Some(edits) = item.additional_text_edits.as_ref() else {
            return false;
        };
        edits
            .iter()
            .any(|edit| Self::import_text_is_default_binding_for_label(&edit.new_text, &item.label))
    }

    fn import_text_is_default_binding_for_label(new_text: &str, label: &str) -> bool {
        let mut text = new_text.trim_start();
        if let Some(rest) = text.strip_prefix("import type ") {
            text = rest.trim_start();
        } else if let Some(rest) = text.strip_prefix("import ") {
            text = rest.trim_start();
        } else {
            return false;
        }

        if text.starts_with('{') || text.starts_with('*') {
            return false;
        }

        let Some(binding) = Self::parse_default_import_binding(text) else {
            return false;
        };
        if !label.is_empty() && binding != label {
            return false;
        }

        let rest = text[binding.len()..].trim_start();
        rest.starts_with("from ")
    }

    fn parse_default_import_binding(text: &str) -> Option<&str> {
        let bytes = text.as_bytes();
        let first = bytes.first().copied()?;
        if !(first.is_ascii_alphabetic() || first == b'_' || first == b'$') {
            return None;
        }
        let mut end = 1usize;
        while end < bytes.len() {
            let b = bytes[end];
            if b.is_ascii_alphanumeric() || b == b'_' || b == b'$' {
                end += 1;
            } else {
                break;
            }
        }
        Some(&text[..end])
    }

    fn normalize_completion_source_for_match(source: &str) -> String {
        const SOURCE_SUFFIXES: [&str; 11] = [
            ".d.ts", ".d.mts", ".d.cts", ".ts", ".tsx", ".mts", ".cts", ".js", ".jsx", ".mjs",
            ".cjs",
        ];
        let mut normalized = source
            .trim()
            .trim_matches('\"')
            .trim_matches('\'')
            .replace('\\', "/");
        if let Some(stripped) = normalized.strip_prefix("node:") {
            normalized = stripped.to_string();
        }
        for suffix in SOURCE_SUFFIXES {
            if let Some(base) = normalized.strip_suffix(suffix)
                && !base.is_empty()
            {
                normalized = base.to_string();
                break;
            }
        }
        if let Some(base) = normalized.strip_suffix("/index")
            && !base.is_empty()
        {
            normalized = base.to_string();
        }
        normalized
    }

    fn completion_sources_match(item_source: Option<&str>, requested_source: &str) -> bool {
        let Some(item_source) = item_source else {
            return false;
        };
        let item_source = Self::normalize_completion_source_for_match(item_source);
        let requested_source = Self::normalize_completion_source_for_match(requested_source);
        item_source == requested_source
    }

    fn auto_import_export_name(item: &CompletionItem) -> Option<String> {
        if Self::is_default_auto_import_item(item) {
            return Some("default".to_string());
        }
        if let Some(edits) = item.additional_text_edits.as_ref() {
            for edit in edits {
                if let Some(name) = Self::named_import_export_name_from_text(&edit.new_text) {
                    return Some(name);
                }
            }
        }
        // Fallback for auto-import items whose additional_text_edits haven't
        // been attached yet (e.g. batched-detail flows where the edits arrive
        // on the details request, not the initial list): the completion label
        // IS the export name for named imports. Only accept when the item
        // carries auto-import metadata and the label is a valid identifier,
        // so ClassMemberSnippet / member-access labels are not misread.
        if item.has_action && item.source.is_some() && Self::is_identifier(&item.label) {
            return Some(item.label.clone());
        }
        None
    }

    fn auto_import_entry_kind_override(
        &self,
        current_file: &str,
        item: &CompletionItem,
    ) -> Option<CompletionItemKind> {
        let has_auto_import_source = item.has_action && item.source.is_some();
        if !has_auto_import_source {
            return None;
        }
        let export_name = Self::auto_import_export_name(item)?;
        if export_name == "default" {
            return Some(CompletionItemKind::Property);
        }
        self.auto_import_export_literal_info(current_file, item, &export_name)
            .map(|(kind, _)| kind)
    }

    fn named_import_export_name_from_text(new_text: &str) -> Option<String> {
        for import_prefix in ["import {", "import type {"] {
            let Some((_, imports)) =
                Self::parse_named_import_clause(new_text, import_prefix, "} from ")
            else {
                continue;
            };
            let first = imports
                .split(',')
                .map(str::trim)
                .find(|part| !part.is_empty())?;
            let first = first.trim_start_matches("type ").trim();
            let export_name = first.split(" as ").next().unwrap_or(first).trim();
            if !export_name.is_empty() {
                return Some(export_name.to_string());
            }
        }
        None
    }

    fn auto_import_export_literal_info(
        &self,
        current_file: &str,
        item: &CompletionItem,
        export_name: &str,
    ) -> Option<(CompletionItemKind, String)> {
        let module_specifier = item
            .additional_text_edits
            .as_ref()
            .and_then(|edits| {
                edits.iter().find_map(|edit| {
                    Self::extract_module_specifier_from_import_text(&edit.new_text)
                })
            })
            .or(item.source.as_deref())?;

        let mut candidates =
            Self::resolve_auto_import_source_candidate_paths(current_file, module_specifier);
        if candidates.is_empty()
            && let Some(source) = item.source.as_deref()
        {
            candidates = Self::resolve_auto_import_source_candidate_paths(current_file, source);
        }

        for candidate in candidates {
            let normalized_candidate = Self::normalize_virtual_path(&candidate);
            let source = self
                .open_files
                .get(&candidate)
                .cloned()
                .or_else(|| self.open_files.get(&normalized_candidate).cloned())
                .or_else(|| std::fs::read_to_string(&candidate).ok())
                .or_else(|| std::fs::read_to_string(&normalized_candidate).ok());
            let Some(source) = source else {
                continue;
            };
            if export_name == "default" {
                if let Some(default_type) = Self::default_export_literal_type_text(&source) {
                    return Some((CompletionItemKind::Property, default_type));
                }
            } else if let Some(named_info) = Self::named_export_literal_info(&source, export_name) {
                return Some(named_info);
            }
        }
        None
    }

    fn resolve_auto_import_source_candidate_paths(
        current_file: &str,
        module_specifier: &str,
    ) -> Vec<String> {
        let normalized = module_specifier.trim();
        if normalized.is_empty() {
            return Vec::new();
        }

        let mut candidates = Vec::new();
        let exts = tsz_common::file_extensions::TSC_TS_JS_RESOLUTION_EXTENSIONS_BARE;

        let push_path = |out: &mut Vec<String>, path: std::path::PathBuf| {
            let normalized = Self::normalize_virtual_path(&path.to_string_lossy());
            if !out.contains(&normalized) {
                out.push(normalized);
            }
        };

        if normalized.starts_with('.') {
            let Some(base_dir) = std::path::Path::new(current_file).parent() else {
                return Vec::new();
            };
            let joined = base_dir.join(normalized);
            if joined.extension().is_some() {
                push_path(&mut candidates, joined);
                return candidates;
            }

            for ext in exts {
                push_path(
                    &mut candidates,
                    base_dir.join(format!("{normalized}.{ext}")),
                );
            }
            for ext in exts {
                push_path(
                    &mut candidates,
                    base_dir.join(normalized).join(format!("index.{ext}")),
                );
            }
            return candidates;
        }

        if normalized.starts_with('/') {
            let absolute = std::path::PathBuf::from(normalized);
            if absolute.extension().is_some() {
                push_path(&mut candidates, absolute);
                return candidates;
            }
            for ext in exts {
                push_path(
                    &mut candidates,
                    std::path::PathBuf::from(format!("{normalized}.{ext}")),
                );
            }
            for ext in exts {
                push_path(
                    &mut candidates,
                    std::path::Path::new(normalized).join(format!("index.{ext}")),
                );
            }
            return candidates;
        }

        Vec::new()
    }

    fn normalize_virtual_path(path: &str) -> String {
        use std::path::Component;

        let mut normalized = std::path::PathBuf::new();
        for component in std::path::Path::new(path).components() {
            match component {
                Component::CurDir => {}
                Component::ParentDir => {
                    let _ = normalized.pop();
                }
                other => normalized.push(other.as_os_str()),
            }
        }
        normalized.to_string_lossy().replace('\\', "/")
    }

    fn default_export_literal_type_text(source_text: &str) -> Option<String> {
        let mut search_start = 0usize;
        let marker = "export default";
        while let Some(rel_idx) = source_text[search_start..].find(marker) {
            let start = search_start + rel_idx + marker.len();
            let rest = source_text[start..].trim_start();
            if rest.is_empty() {
                return None;
            }

            let expr = rest
                .split(';')
                .next()
                .unwrap_or("")
                .split('\n')
                .next()
                .unwrap_or("")
                .trim();
            if let Some(literal) = Self::literal_type_text(expr) {
                return Some(literal.to_string());
            }

            search_start = start;
            if search_start >= source_text.len() {
                break;
            }
        }
        None
    }

    fn named_export_literal_info(
        source_text: &str,
        export_name: &str,
    ) -> Option<(CompletionItemKind, String)> {
        for line in source_text.lines() {
            let trimmed = line.trim();
            for (keyword, kind) in [
                ("export const ", CompletionItemKind::Const),
                ("export let ", CompletionItemKind::Let),
                ("export var ", CompletionItemKind::Variable),
            ] {
                let Some(after_keyword) = trimmed.strip_prefix(keyword) else {
                    continue;
                };
                let name_end = after_keyword
                    .find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '$'))
                    .unwrap_or(after_keyword.len());
                let declared_name = after_keyword[..name_end].trim();
                if declared_name != export_name {
                    continue;
                }
                let Some(eq_idx) = after_keyword.find('=') else {
                    continue;
                };
                let initializer = after_keyword[eq_idx + 1..]
                    .split(';')
                    .next()
                    .unwrap_or("")
                    .trim();
                if let Some(literal) = Self::literal_type_text(initializer) {
                    return Some((kind, literal.to_string()));
                }
            }
        }
        None
    }

    fn literal_type_text(text: &str) -> Option<&str> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return None;
        }
        let bytes = trimmed.as_bytes();
        if bytes.len() >= 2
            && ((bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
                || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\''))
        {
            return Some(trimmed);
        }
        if trimmed == "true" || trimmed == "false" {
            return Some(trimmed);
        }
        if trimmed
            .chars()
            .all(|ch| ch.is_ascii_digit() || ch == '_' || ch == '.' || ch == '-' || ch == '+')
            && trimmed.chars().any(|ch| ch.is_ascii_digit())
        {
            return Some(trimmed);
        }
        None
    }

    fn completion_doc_and_tags(
        item: Option<&CompletionItem>,
        name: &str,
        arena: &tsz::parser::node::NodeArena,
        binder: &tsz::binder::BinderState,
        root: tsz::parser::base::NodeIndex,
        source_text: &str,
    ) -> (serde_json::Value, Vec<serde_json::Value>) {
        let mut raw_doc = item
            .and_then(|i| i.documentation.as_ref())
            .map(std::string::ToString::to_string)
            .unwrap_or_default();
        let supplemental_jsdoc =
            Self::leading_jsdoc_block_for_symbol(name, binder, arena, source_text);

        if raw_doc.is_empty()
            && let Some(symbol_id) = binder.file_locals.get(name)
            && let Some(symbol) = binder.symbols.get(symbol_id)
            && let Some(decl) = symbol.primary_declaration()
        {
            raw_doc = jsdoc_for_node(arena, root, decl, source_text);
        }
        if raw_doc.trim().is_empty()
            && let Some(supplemental_jsdoc) = supplemental_jsdoc.as_deref()
        {
            raw_doc = Self::normalize_jsdoc_text_for_parse(supplemental_jsdoc);
        }

        if raw_doc.trim().is_empty() {
            return (serde_json::json!([]), Vec::new());
        }

        let mut parsed = parse_jsdoc(&raw_doc);
        if let Some(supplemental_jsdoc) = supplemental_jsdoc.as_deref() {
            let supplemental_doc = Self::normalize_jsdoc_text_for_parse(supplemental_jsdoc);
            let supplemental_parsed = parse_jsdoc(&supplemental_doc);
            if parsed
                .summary
                .as_deref()
                .is_none_or(|text| text.trim().is_empty())
            {
                parsed.summary = supplemental_parsed.summary;
            }
            if parsed.params.is_empty() {
                parsed.params = supplemental_parsed.params;
            }
            if parsed.tags.is_empty() {
                parsed.tags = supplemental_parsed.tags;
            }
        }
        let summary = parsed
            .summary
            .filter(|text| !text.trim().is_empty())
            .unwrap_or_else(|| raw_doc.clone());
        let documentation = inline_links::build_doc_display_parts(&summary);

        let mut tags = Vec::new();
        let mut param_names: Vec<String> = parsed.params.keys().cloned().collect();
        if param_names.is_empty() {
            let fallback_doc = supplemental_jsdoc
                .as_deref()
                .map(Self::normalize_jsdoc_text_for_parse)
                .unwrap_or_else(|| Self::normalize_jsdoc_text_for_parse(&raw_doc));
            param_names = Self::jsdoc_param_names_from_text(&fallback_doc);
        }
        param_names.sort();
        param_names.dedup();
        for param_name in param_names {
            tags.push(serde_json::json!({
                "name": "param",
                "text": param_name,
            }));
        }
        for tag in parsed.tags {
            tags.push(serde_json::json!({
                "name": tag.name,
                "text": tag.text,
            }));
        }

        (documentation, tags)
    }

    fn leading_jsdoc_block_for_symbol(
        name: &str,
        binder: &tsz::binder::BinderState,
        arena: &tsz::parser::node::NodeArena,
        source_text: &str,
    ) -> Option<String> {
        use tsz::parser::syntax_kind_ext;

        let symbol_id = binder.file_locals.get(name)?;
        let symbol = binder.symbols.get(symbol_id)?;
        let decl = symbol.primary_declaration()?;
        let node = arena.get(decl)?;
        let anchor = if node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            if let Some(ext) = arena.get_extended(decl) {
                let list_idx = ext.parent;
                if let Some(list_node) = arena.get(list_idx) {
                    if list_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
                        if let Some(list_ext) = arena.get_extended(list_idx) {
                            let stmt_idx = list_ext.parent;
                            if let Some(stmt_node) = arena.get(stmt_idx) {
                                if stmt_node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
                                    stmt_node.pos as usize
                                } else {
                                    node.pos as usize
                                }
                            } else {
                                node.pos as usize
                            }
                        } else {
                            node.pos as usize
                        }
                    } else {
                        node.pos as usize
                    }
                } else {
                    node.pos as usize
                }
            } else {
                node.pos as usize
            }
        } else {
            node.pos as usize
        };
        Self::leading_jsdoc_block_before_offset_for_details(source_text, anchor)
            .map(|text| text.to_string())
    }

    fn leading_jsdoc_block_before_offset_for_details(
        source_text: &str,
        offset: usize,
    ) -> Option<&str> {
        let clamped = offset.min(source_text.len());
        let prefix = &source_text[..clamped];
        let comment_end = prefix.rfind("*/")?;
        let after_comment = &prefix[comment_end + 2..];
        if !after_comment.chars().all(char::is_whitespace) {
            return None;
        }
        let comment_start = prefix[..comment_end].rfind("/**")?;
        Some(&prefix[comment_start + 3..comment_end])
    }

    fn normalize_jsdoc_text_for_parse(doc: &str) -> String {
        doc.lines()
            .map(|line| {
                let trimmed = line.trim_start();
                if let Some(stripped) = trimmed.strip_prefix('*') {
                    stripped.trim_start()
                } else {
                    trimmed
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string()
    }

    fn jsdoc_param_names_from_text(doc: &str) -> Vec<String> {
        let mut names = Vec::new();
        for line in doc.lines() {
            let trimmed = line.trim();
            let Some(rest) = trimmed.strip_prefix("@param") else {
                continue;
            };
            let mut rest = rest.trim();
            if let Some(type_payload) = rest.strip_prefix('{')
                && let Some(close_idx) = type_payload.find('}')
            {
                rest = type_payload[close_idx + 1..].trim_start();
            }
            let Some(raw_name) = rest.split_whitespace().next() else {
                continue;
            };
            let normalized = Self::normalize_jsdoc_param_name_for_tags(raw_name);
            if !normalized.is_empty() {
                names.push(normalized);
            }
        }
        names
    }

    fn normalize_jsdoc_param_name_for_tags(name: &str) -> String {
        let mut normalized = name.trim();
        if normalized.starts_with('[') && normalized.ends_with(']') && normalized.len() > 2 {
            normalized = &normalized[1..normalized.len() - 1];
        }
        if let Some(eq_idx) = normalized.find('=') {
            normalized = &normalized[..eq_idx];
        }
        if let Some(stripped) = normalized.strip_prefix("...") {
            normalized = stripped;
        }
        normalized.trim().to_string()
    }

    pub(super) fn extract_module_specifier_from_import_text(new_text: &str) -> Option<&str> {
        let candidates = [" from \"", " from '", "import \"", "import '"];
        for marker in candidates {
            let Some(start_idx) = new_text.find(marker) else {
                continue;
            };
            let quote = marker.chars().last()?;
            let rest = &new_text[start_idx + marker.len()..];
            let end_idx = rest.find(quote)?;
            return Some(&rest[..end_idx]);
        }
        None
    }

    // Display parts rendering, signature help handler, and tokenization utilities
    // are in handlers_completions_display.rs
}
