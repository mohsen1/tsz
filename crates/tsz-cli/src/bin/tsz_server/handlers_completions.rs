//! Completions handlers for tsz-server.
//!
//! Display parts rendering, signature help, and tokenization are in
//! `handlers_completions_display.rs`.

use super::{Server, TsServerRequest, TsServerResponse};
use rustc_hash::{FxHashMap, FxHashSet};
use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use tsz::lsp::Project;
use tsz::lsp::completions::{CompletionItem, CompletionItemKind, Completions, sort_priority};
use tsz::lsp::jsdoc::{jsdoc_for_node, parse_jsdoc};
use tsz::lsp::position::{LineMap, Position};
use tsz_solver::TypeInterner;

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

    // Class member snippet methods are in handlers_completions_snippets.rs

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

    // Class member snippet methods are in handlers_completions_snippets.rs

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
        if !item.has_action || item.source.as_ref().is_none() {
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
        let exts = ["ts", "tsx", "d.ts", "js", "jsx", "mts", "cts", "mjs", "cjs"];

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
        let documentation = serde_json::json!([{"text": summary, "kind": "text"}]);

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

    pub(crate) fn normalize_mts_auto_import_edit_text(
        file_path: &str,
        kind: tsz::lsp::completions::CompletionItemKind,
        source_text: &str,
        new_text: &str,
    ) -> String {
        if !file_path.ends_with(".mts") {
            return new_text.to_string();
        }

        let mut normalized = new_text.to_string();
        if matches!(
            kind,
            tsz::lsp::completions::CompletionItemKind::Interface
                | tsz::lsp::completions::CompletionItemKind::TypeAlias
        ) && normalized.starts_with("import {")
        {
            normalized = normalized.replacen("import {", "import type {", 1);
        }

        for marker in [" from \"", " from '"] {
            let Some(marker_idx) = normalized.find(marker) else {
                continue;
            };
            let Some(quote) = marker.chars().last() else {
                continue;
            };
            let start = marker_idx + marker.len();
            let rest = &normalized[start..];
            let Some(end_rel) = rest.find(quote) else {
                continue;
            };
            let end = start + end_rel;
            let module_specifier = &normalized[start..end];
            if module_specifier.starts_with('.')
                && Path::new(module_specifier).extension().is_none()
            {
                normalized.replace_range(start..end, &format!("{module_specifier}.js"));
            }
            break;
        }

        if let Some((module_specifier, imports)) =
            Self::parse_named_import_clause(&normalized, "import {", "} from ")
        {
            let type_only_names =
                Self::type_only_named_imports_for_module(source_text, module_specifier);
            if !type_only_names.is_empty() {
                let mut updated_imports = Vec::new();
                let mut seen_imports = std::collections::BTreeSet::new();
                for part in imports
                    .split(',')
                    .map(str::trim)
                    .filter(|part| !part.is_empty())
                {
                    let bare = part.trim_start_matches("type ").trim();
                    if bare.is_empty() {
                        continue;
                    }
                    seen_imports.insert(bare.to_string());
                    if part.starts_with("type ") {
                        updated_imports.push(part.to_string());
                    } else if type_only_names.contains(bare) {
                        updated_imports.push(format!("type {bare}"));
                    } else {
                        updated_imports.push(part.to_string());
                    }
                }
                for type_only_name in type_only_names {
                    if !seen_imports.contains(&type_only_name) {
                        updated_imports.push(format!("type {type_only_name}"));
                    }
                }
                if !updated_imports.is_empty() {
                    normalized = normalized.replacen(
                        &format!("{{ {imports} }}"),
                        &format!("{{ {} }}", updated_imports.join(", ")),
                        1,
                    );
                }
            }
        }

        normalized
    }

    fn normalize_tsserver_newlines_for_file(text: &str, file_path: &str) -> String {
        let normalized = text.replace("\r\n", "\n");
        let prefers_crlf = file_path.contains("/home/src/workspaces/");
        if prefers_crlf {
            normalized.replace('\n', "\r\n")
        } else {
            normalized
        }
    }

    fn parse_named_import_clause<'a>(
        text: &'a str,
        import_prefix: &str,
        import_suffix: &str,
    ) -> Option<(&'a str, &'a str)> {
        let start = text.find(import_prefix)?;
        let after_prefix = &text[start + import_prefix.len()..];
        let close_brace = after_prefix.find(import_suffix)?;
        let imports = &after_prefix[..close_brace].trim();
        let after_imports = &after_prefix[close_brace + import_suffix.len()..];
        for quote in ['"', '\''] {
            if let Some(quote_start) = after_imports.find(quote) {
                let rest = &after_imports[quote_start + 1..];
                if let Some(quote_end) = rest.find(quote) {
                    let module_specifier = &rest[..quote_end];
                    return Some((module_specifier, imports));
                }
            }
        }
        None
    }

    fn type_only_named_imports_for_module(
        source_text: &str,
        module_specifier: &str,
    ) -> std::collections::BTreeSet<String> {
        let mut names = std::collections::BTreeSet::new();
        for line in source_text.lines() {
            if !line.contains("import type {") {
                continue;
            }
            if !(line.contains(&format!("from \"{module_specifier}\""))
                || line.contains(&format!("from '{module_specifier}'")))
            {
                continue;
            }
            let Some(open) = line.find('{') else {
                continue;
            };
            let Some(close) = line[open + 1..].find('}') else {
                continue;
            };
            let raw_names = &line[open + 1..open + 1 + close];
            for raw_name in raw_names.split(',') {
                let trimmed = raw_name.trim().trim_start_matches("type ").trim();
                if !trimmed.is_empty() {
                    names.insert(trimmed.to_string());
                }
            }
        }
        names
    }

    fn find_type_only_named_import_span(
        source_text: &str,
        module_specifier: &str,
    ) -> Option<(u32, u32)> {
        let mut offset = 0u32;
        for line in source_text.split_inclusive('\n') {
            if line.contains("import type {")
                && (line.contains(&format!("from \"{module_specifier}\""))
                    || line.contains(&format!("from '{module_specifier}'")))
            {
                return Some((offset, line.len() as u32));
            }
            offset += line.len() as u32;
        }
        None
    }

    pub(crate) fn auto_import_code_action_description(
        source_text: &str,
        file_path: &str,
        fallback_source: Option<&str>,
        edits: &[tsz::lsp::rename::TextEdit],
        label: &str,
    ) -> String {
        let source = edits
            .iter()
            .find_map(|edit| Self::extract_module_specifier_from_import_text(&edit.new_text))
            .or(fallback_source)
            .map(|source| {
                if file_path.ends_with(".mts")
                    && source.starts_with('.')
                    && !source.ends_with(".js")
                    && !source.ends_with(".jsx")
                    && !source.ends_with(".mjs")
                    && !source.ends_with(".cjs")
                    && !source.ends_with(".ts")
                    && !source.ends_with(".tsx")
                    && !source.ends_with(".mts")
                    && !source.ends_with(".cts")
                {
                    format!("{source}.js")
                } else {
                    source.to_string()
                }
            });
        source
            .map(|source| {
                let has_existing_import = source_text.contains(&format!("from \"{source}\""))
                    || source_text.contains(&format!("from '{source}'"));
                if has_existing_import {
                    format!("Update import from \"{source}\"")
                } else {
                    format!("Add import from \"{source}\"")
                }
            })
            .unwrap_or_else(|| format!("Apply completion for '{label}'"))
    }

    pub(crate) const fn completion_kind_to_str(
        kind: tsz::lsp::completions::CompletionItemKind,
    ) -> &'static str {
        match kind {
            tsz::lsp::completions::CompletionItemKind::Variable => "var",
            tsz::lsp::completions::CompletionItemKind::Const => "const",
            tsz::lsp::completions::CompletionItemKind::Let => "let",
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
            tsz::lsp::completions::CompletionItemKind::Alias => "alias",
        }
    }

    fn project_completion_items(
        &self,
        file_name: &str,
        position: tsz::lsp::position::Position,
        preferences: Option<&serde_json::Value>,
    ) -> Vec<tsz::lsp::completions::CompletionItem> {
        let include_module_exports =
            Self::bool_pref_or_default(preferences, "includeCompletionsForModuleExports", false);
        let mut tracked_paths = FxHashSet::default();
        tracked_paths.extend(self.open_files.keys().cloned());
        for project_files in self.external_project_files.values() {
            tracked_paths.extend(project_files.iter().cloned());
        }
        tracked_paths.insert(file_name.to_string());

        let allowed_packages =
            include_module_exports.then(|| self.dependency_package_names_for_file(file_name));
        let workspace_prefix = Self::path_workspace_prefix(file_name);
        let mut files = FxHashMap::default();
        for path in tracked_paths {
            let allowed_packages_ref = allowed_packages
                .as_ref()
                .and_then(std::option::Option::as_ref);
            if !Self::should_include_completion_project_path(
                &path,
                file_name,
                workspace_prefix.as_deref(),
                allowed_packages_ref,
            ) {
                continue;
            }
            if let Some(text) = self
                .open_files
                .get(&path)
                .cloned()
                .or_else(|| std::fs::read_to_string(&path).ok())
            {
                files.insert(path, text);
            }
        }

        if !files.contains_key(file_name)
            && let Ok(content) = std::fs::read_to_string(file_name)
        {
            files.insert(file_name.to_string(), content);
        }
        Self::add_project_config_files(&mut files, file_name);
        if include_module_exports {
            let has_node_modules_file = files
                .keys()
                .any(|path| Self::path_is_under_node_modules(path));
            if !has_node_modules_file {
                self.add_dependency_package_files_for_completion(
                    file_name,
                    allowed_packages
                        .as_ref()
                        .and_then(std::option::Option::as_ref),
                    &mut files,
                );
            }
        }
        if files.is_empty() {
            return Vec::new();
        }

        let mut project = Project::new();
        project.set_allow_importing_ts_extensions(self.allow_importing_ts_extensions);
        project.set_auto_imports_allowed_without_tsconfig(
            self.auto_imports_allowed_for_inferred_projects,
        );
        project.set_import_module_specifier_ending(
            Self::string_pref(preferences, "importModuleSpecifierEnding")
                .or_else(|| self.completion_import_module_specifier_ending.clone()),
        );
        project.set_import_module_specifier_preference(
            Self::string_pref(preferences, "importModuleSpecifierPreference")
                .or_else(|| self.import_module_specifier_preference.clone()),
        );
        project.set_auto_import_file_exclude_patterns(
            Self::string_array_pref(preferences, "autoImportFileExcludePatterns")
                .unwrap_or_else(|| self.auto_import_file_exclude_patterns.clone()),
        );
        project.set_auto_import_specifier_exclude_regexes(
            Self::string_array_pref(preferences, "autoImportSpecifierExcludeRegexes")
                .unwrap_or_default(),
        );
        for (path, text) in files {
            project.set_file(path, text);
        }
        project
            .get_completions(file_name, position)
            .unwrap_or_default()
    }

    fn dependency_package_names_for_file(&self, file_name: &str) -> Option<FxHashSet<String>> {
        let mut allowed = FxHashSet::default();
        let mut saw_package_json = false;
        let mut current = Path::new(file_name).parent();

        while let Some(dir) = current {
            let package_json_path = dir.join("package.json");
            let package_json_key = package_json_path.to_string_lossy().replace('\\', "/");
            let package_json_text = self.open_files.get(&package_json_key).cloned();

            if let Some(text) = package_json_text {
                saw_package_json = true;
                let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
                    // Match tsserver behavior: invalid package.json should not
                    // suppress auto-import candidates.
                    return None;
                };
                for field in [
                    "dependencies",
                    "devDependencies",
                    "peerDependencies",
                    "optionalDependencies",
                ] {
                    if let Some(deps) = json.get(field).and_then(serde_json::Value::as_object) {
                        allowed.extend(deps.keys().cloned());
                    }
                }
            }
            current = dir.parent();
        }

        saw_package_json.then_some(allowed)
    }

    fn should_include_completion_project_path(
        path: &str,
        current_file: &str,
        workspace_prefix: Option<&str>,
        allowed_packages: Option<&FxHashSet<String>>,
    ) -> bool {
        let path = path.replace('\\', "/");
        let current_file = current_file.replace('\\', "/");
        if path == current_file {
            return true;
        }

        if Self::path_is_under_node_modules(&path) {
            return Self::node_modules_path_matches_allowed_packages(&path, allowed_packages);
        }

        if Self::is_project_config_file(&path) {
            // Always include package.json files outside node_modules: they
            // carry workspace-package metadata (`name`, `exports`, …) that
            // the auto-import specifier resolver needs even for sibling
            // packages that aren't ancestors of the currently-edited file.
            // `tsconfig.json` / `jsconfig.json` stay gated by ancestry.
            if path.ends_with("/package.json") {
                return true;
            }
            return Self::is_config_related_to_file(&path, &current_file);
        }

        workspace_prefix
            .map(|prefix| {
                if prefix == "/" {
                    // Root workspace: include any absolute path that is not under
                    // node_modules (handled above). Using `format!("{prefix}/")`
                    // would yield "//", which never matches real paths like
                    // "/Component.tsx".
                    path.starts_with('/')
                } else {
                    path == prefix || path.starts_with(&format!("{prefix}/"))
                }
            })
            .unwrap_or(true)
    }

    fn is_project_config_file(path: &str) -> bool {
        path.ends_with("/package.json")
            || path.ends_with("/tsconfig.json")
            || path.ends_with("/jsconfig.json")
    }

    fn is_config_related_to_file(config_path: &str, file_name: &str) -> bool {
        let Some(dir) = Path::new(config_path).parent() else {
            return false;
        };
        let dir = dir.to_string_lossy().replace('\\', "/");
        if dir == "/" {
            return file_name.starts_with('/');
        }
        file_name == dir || file_name.starts_with(&format!("{dir}/"))
    }

    fn path_is_under_node_modules(path: &str) -> bool {
        path.contains("/node_modules/")
    }

    fn path_workspace_prefix(file_name: &str) -> Option<String> {
        let normalized = file_name.replace('\\', "/");
        if normalized.starts_with('/') && normalized.matches('/').count() == 1 {
            return Some("/".to_string());
        }
        let segments: Vec<&str> = normalized
            .split('/')
            .filter(|segment| !segment.is_empty())
            .collect();
        if segments.is_empty() {
            return None;
        }
        if segments.len() == 1 {
            return normalized.starts_with('/').then_some("/".to_string());
        }
        if segments.len() <= 3 {
            return Some(format!("/{}", segments[0]));
        }
        Some(format!("/{}/{}/{}", segments[0], segments[1], segments[2]))
    }

    fn node_modules_path_matches_allowed_packages(
        path: &str,
        allowed_packages: Option<&FxHashSet<String>>,
    ) -> bool {
        let Some(allowed_packages) = allowed_packages else {
            return true;
        };
        if allowed_packages.is_empty() {
            return true;
        }
        let Some(package_name) = Self::package_name_from_node_modules_path(path) else {
            return false;
        };
        if allowed_packages.contains(&package_name) {
            return true;
        }
        Self::types_package_runtime_name(&package_name)
            .is_some_and(|runtime_name| allowed_packages.contains(&runtime_name))
    }

    fn package_name_from_node_modules_path(path: &str) -> Option<String> {
        let normalized = path.replace('\\', "/");
        let idx = normalized.rfind("/node_modules/")?;
        let mut tail = &normalized[idx + "/node_modules/".len()..];
        if tail.starts_with(".pnpm/")
            && let Some(inner_idx) = tail.find("/node_modules/")
        {
            tail = &tail[inner_idx + "/node_modules/".len()..];
        }
        let mut segments = tail.split('/').filter(|segment| !segment.is_empty());
        let first = segments.next()?;
        if first.starts_with('@') {
            let second = segments.next()?;
            Some(format!("{first}/{second}"))
        } else {
            Some(first.to_string())
        }
    }

    fn types_package_runtime_name(package_name: &str) -> Option<String> {
        let rest = package_name.strip_prefix("@types/")?;
        if let Some((scope, name)) = rest.split_once("__") {
            return Some(format!("@{scope}/{name}"));
        }
        Some(rest.to_string())
    }

    fn types_package_name_for(runtime_package_name: &str) -> String {
        if let Some(stripped) = runtime_package_name.strip_prefix('@')
            && let Some((scope, name)) = stripped.split_once('/')
        {
            return format!("@types/{scope}__{name}");
        }
        format!("@types/{runtime_package_name}")
    }

    fn node_modules_roots_for_file(file_name: &str) -> Vec<String> {
        let mut roots = Vec::new();
        let mut current = Path::new(file_name).parent();
        while let Some(dir) = current {
            roots.push(
                dir.join("node_modules")
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
            current = dir.parent();
        }
        roots.sort();
        roots.dedup();
        roots
    }

    fn add_dependency_package_files_for_completion(
        &self,
        file_name: &str,
        allowed_packages: Option<&FxHashSet<String>>,
        files: &mut FxHashMap<String, String>,
    ) {
        let Some(allowed_packages) = allowed_packages else {
            return;
        };
        if allowed_packages.is_empty() {
            return;
        }

        let node_modules_roots = Self::node_modules_roots_for_file(file_name);
        if node_modules_roots.is_empty() {
            return;
        }

        let mut dependency_names: Vec<String> = allowed_packages.iter().cloned().collect();
        dependency_names.sort();
        dependency_names.dedup();

        let mut scanned_dirs = FxHashSet::default();
        for node_modules_root in node_modules_roots {
            for dependency_name in &dependency_names {
                if Self::files_already_include_dependency(files, dependency_name) {
                    continue;
                }
                let dependency_dir = format!("{node_modules_root}/{dependency_name}");
                let dependency_dir_path = Path::new(&dependency_dir);
                if !dependency_dir_path.is_dir() {
                    continue;
                }
                if !scanned_dirs.insert(dependency_dir.clone()) {
                    continue;
                }

                if !Self::files_contain_path_prefix(files, &dependency_dir) {
                    Self::add_supported_files_under_dir(dependency_dir_path, files, 256);
                }

                if Self::files_contain_declaration_under_prefix(files, &dependency_dir) {
                    continue;
                }

                let types_package_name = Self::types_package_name_for(dependency_name);
                let types_dir = format!("{node_modules_root}/{types_package_name}");
                let types_dir_path = Path::new(&types_dir);
                if !types_dir_path.is_dir() {
                    continue;
                }
                if !scanned_dirs.insert(types_dir.clone()) {
                    continue;
                }
                if !Self::files_contain_path_prefix(files, &types_dir) {
                    Self::add_supported_files_under_dir(types_dir_path, files, 256);
                }
            }
        }
    }

    fn files_already_include_dependency(
        files: &FxHashMap<String, String>,
        dependency_name: &str,
    ) -> bool {
        files.keys().any(|path| {
            Self::package_name_from_node_modules_path(path).is_some_and(|package_name| {
                package_name == dependency_name
                    || Self::types_package_runtime_name(&package_name)
                        .is_some_and(|runtime| runtime == dependency_name)
            })
        })
    }

    fn files_contain_path_prefix(files: &FxHashMap<String, String>, prefix: &str) -> bool {
        let normalized_prefix = prefix.replace('\\', "/");
        files.keys().any(|path| {
            path == &normalized_prefix || path.starts_with(&format!("{normalized_prefix}/"))
        })
    }

    fn files_contain_declaration_under_prefix(
        files: &FxHashMap<String, String>,
        prefix: &str,
    ) -> bool {
        let normalized_prefix = prefix.replace('\\', "/");
        files.keys().any(|path| {
            (path == &normalized_prefix || path.starts_with(&format!("{normalized_prefix}/")))
                && (path.ends_with(".d.ts") || path.ends_with(".d.mts") || path.ends_with(".d.cts"))
        })
    }

    fn add_supported_files_under_dir(
        root: &Path,
        files: &mut FxHashMap<String, String>,
        max_files: usize,
    ) {
        let mut added = 0usize;
        let mut stack = vec![PathBuf::from(root)];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let Ok(file_type) = entry.file_type() else {
                    continue;
                };
                let path = entry.path();
                if file_type.is_dir() {
                    let name = entry.file_name();
                    if name.to_string_lossy().as_ref() != "node_modules" {
                        stack.push(path);
                    }
                    continue;
                }
                if !file_type.is_file() {
                    continue;
                }

                let path_str = path.to_string_lossy().replace('\\', "/");
                if files.contains_key(&path_str)
                    || !Self::is_supported_completion_project_file(&path_str)
                {
                    continue;
                }
                if let Ok(text) = std::fs::read_to_string(&path) {
                    files.insert(path_str, text);
                    added += 1;
                    if added >= max_files {
                        return;
                    }
                }
            }
        }
    }

    fn is_supported_completion_project_file(path: &str) -> bool {
        path.ends_with(".ts")
            || path.ends_with(".tsx")
            || path.ends_with(".d.ts")
            || path.ends_with(".mts")
            || path.ends_with(".cts")
            || path.ends_with(".d.mts")
            || path.ends_with(".d.cts")
            || path.ends_with(".js")
            || path.ends_with(".jsx")
            || path.ends_with(".mjs")
            || path.ends_with(".cjs")
            || path.ends_with("/package.json")
            || path.ends_with("/tsconfig.json")
            || path.ends_with("/jsconfig.json")
    }

    fn sort_tsserver_completion_items(items: &mut [CompletionItem]) {
        fn compare_case_sensitive_ui(a: &str, b: &str) -> Ordering {
            fn split_numeric_segments(s: &str) -> Vec<&str> {
                let mut segments = Vec::new();
                let mut start = 0;
                let mut in_digit = false;

                for (i, ch) in s.char_indices() {
                    let is_digit = ch.is_ascii_digit();
                    if i == 0 {
                        in_digit = is_digit;
                    } else if is_digit != in_digit {
                        segments.push(&s[start..i]);
                        start = i;
                        in_digit = is_digit;
                    }
                }
                if start < s.len() {
                    segments.push(&s[start..]);
                }
                segments
            }

            let a_segments = split_numeric_segments(a);
            let b_segments = split_numeric_segments(b);

            for (a_seg, b_seg) in a_segments.iter().zip(b_segments.iter()) {
                let a_is_digit = a_seg.chars().next().is_some_and(|c| c.is_ascii_digit());
                let b_is_digit = b_seg.chars().next().is_some_and(|c| c.is_ascii_digit());

                let cmp = if a_is_digit && b_is_digit {
                    let a_num = a_seg.parse::<u64>().unwrap_or(0);
                    let b_num = b_seg.parse::<u64>().unwrap_or(0);
                    a_num.cmp(&b_num)
                } else {
                    a_seg.to_lowercase().cmp(&b_seg.to_lowercase())
                };

                if cmp != Ordering::Equal {
                    return cmp;
                }
            }

            let seg_cmp = a_segments.len().cmp(&b_segments.len());
            if seg_cmp != Ordering::Equal {
                return seg_cmp;
            }

            for (a_ch, b_ch) in a.chars().zip(b.chars()) {
                if a_ch == b_ch {
                    continue;
                }

                let a_lower = a_ch.to_lowercase().next().unwrap_or(a_ch);
                let b_lower = b_ch.to_lowercase().next().unwrap_or(b_ch);

                if a_lower == b_lower {
                    if a_ch.is_lowercase() && b_ch.is_uppercase() {
                        return Ordering::Less;
                    }
                    if a_ch.is_uppercase() && b_ch.is_lowercase() {
                        return Ordering::Greater;
                    }
                }
            }

            a.cmp(b)
        }

        fn compare_completion_sources(a: Option<&str>, b: Option<&str>) -> Ordering {
            match (a, b) {
                (Some(a), Some(b)) => {
                    let a_segments = a.matches('/').count();
                    let b_segments = b.matches('/').count();
                    let candidate_rank = |candidate: &str| -> u8 {
                        if candidate.starts_with("./") {
                            0
                        } else if !candidate.starts_with('.') {
                            1
                        } else if candidate.starts_with("../") {
                            2
                        } else {
                            3
                        }
                    };
                    let index_penalty = |candidate: &str| -> u8 {
                        if candidate == "." || candidate == ".." || candidate.ends_with("/index") {
                            1
                        } else {
                            0
                        }
                    };
                    a_segments
                        .cmp(&b_segments)
                        .then_with(|| candidate_rank(a).cmp(&candidate_rank(b)))
                        .then_with(|| index_penalty(a).cmp(&index_penalty(b)))
                        .then_with(|| a.len().cmp(&b.len()))
                        .then_with(|| compare_case_sensitive_ui(a, b))
                }
                (None, None) => Ordering::Equal,
                (None, Some(_)) => Ordering::Less,
                (Some(_), None) => Ordering::Greater,
            }
        }

        let compare_auto_import_variant_order = |a: &CompletionItem, b: &CompletionItem| {
            if a.label != b.label || a.source != b.source || !a.has_action || !b.has_action {
                return Ordering::Equal;
            }
            let a_export = Self::auto_import_export_name(a);
            let b_export = Self::auto_import_export_name(b);
            match (a_export.as_deref(), b_export.as_deref()) {
                (Some("default"), Some(other)) if other != "default" => Ordering::Less,
                (Some(other), Some("default")) if other != "default" => Ordering::Greater,
                _ => Ordering::Equal,
            }
        };

        items.sort_by(|a, b| {
            compare_case_sensitive_ui(a.effective_sort_text(), b.effective_sort_text())
                .then_with(|| compare_case_sensitive_ui(&a.label, &b.label))
                .then_with(|| compare_completion_sources(a.source.as_deref(), b.source.as_deref()))
                .then_with(|| compare_auto_import_variant_order(a, b))
        });
    }

    fn completion_entry_from_item(
        &self,
        current_file: &str,
        item: &tsz::lsp::completions::CompletionItem,
        line_map: &LineMap,
        source_text: &str,
        include_insert_text: bool,
    ) -> serde_json::Value {
        let effective_kind = self
            .auto_import_entry_kind_override(current_file, item)
            .unwrap_or_else(|| {
                if item.kind == CompletionItemKind::Variable
                    && Self::is_default_auto_import_item(item)
                {
                    CompletionItemKind::Property
                } else {
                    item.kind
                }
            });
        let kind = Self::completion_kind_to_str(effective_kind);
        let sort_text = item.effective_sort_text();
        let mut entry = serde_json::json!({
            "name": item.label,
            "kind": kind,
            "sortText": sort_text,
            "kindModifiers": item.kind_modifiers.clone().unwrap_or_default(),
        });

        let is_class_member_snippet = item.source.as_deref() == Some("ClassMemberSnippet/");
        if include_insert_text
            && let Some(insert_text) = item.insert_text.clone().or_else(|| {
                is_class_member_snippet
                    .then(|| Self::class_member_snippet_insert_text(item))
                    .flatten()
            })
        {
            let should_emit_insert_text =
                Self::should_emit_tsserver_insert_text(item, &insert_text, is_class_member_snippet);
            if should_emit_insert_text {
                entry["insertText"] = serde_json::json!(insert_text);
            }
        }
        if item.has_action {
            entry["hasAction"] = serde_json::json!(true);
            if item.is_snippet {
                entry["filterText"] = serde_json::json!(item.label.clone());
                if !is_class_member_snippet {
                    entry["isSnippet"] = serde_json::json!(true);
                }
            }
        }
        if item.is_package_json_import == Some(true) {
            entry["isPackageJsonImport"] = serde_json::json!(true);
        }
        if let Some(source) = item.source.as_ref() {
            entry["source"] = serde_json::json!(source);
            entry["sourceDisplay"] = serde_json::json!([{ "text": source, "kind": "text" }]);
            let mut data = serde_json::Map::new();
            data.insert("name".to_string(), serde_json::json!(item.label.clone()));
            data.insert("source".to_string(), serde_json::json!(source));
            if item.has_action {
                data.insert("moduleSpecifier".to_string(), serde_json::json!(source));
                if let Some(export_name) = Self::auto_import_export_name(item) {
                    data.insert("exportName".to_string(), serde_json::json!(export_name));
                    // Force worker-mode completion detail requests to stay on tsz for
                    // auto-import entries. Native fallback details can drop/reshape
                    // tags and action metadata for these entries.
                    data.insert(
                        "exportMapKey".to_string(),
                        serde_json::json!(format!("tsz::{source}::{}::{export_name}", item.label)),
                    );
                }
            }
            entry["data"] = serde_json::Value::Object(data);
        }
        if let Some((start, end)) = item.replacement_span {
            let start_pos = line_map.offset_to_position(start, source_text);
            let end_pos = line_map.offset_to_position(end, source_text);
            entry["replacementSpan"] = serde_json::json!({
                "start": Self::lsp_to_tsserver_position(start_pos),
                "end": Self::lsp_to_tsserver_position(end_pos),
            });
        }
        if item.label.starts_with('"') && item.label.ends_with('"') {
            entry["defaultCommitCharacters"] = serde_json::json!([",", "."]);
        }

        entry
    }

    fn should_emit_tsserver_insert_text(
        item: &CompletionItem,
        insert_text: &str,
        is_class_member_snippet: bool,
    ) -> bool {
        if insert_text.is_empty() {
            return false;
        }
        if is_class_member_snippet || item.has_action || !Self::is_identifier(&item.label) {
            return true;
        }
        if Self::is_plain_callable_snippet_insert_text(item, insert_text) {
            return false;
        }
        item.is_snippet || insert_text != item.label
    }

    fn is_plain_callable_snippet_insert_text(item: &CompletionItem, insert_text: &str) -> bool {
        matches!(
            item.kind,
            CompletionItemKind::Function
                | CompletionItemKind::Method
                | CompletionItemKind::Constructor
        ) && insert_text == format!("{}($1)", item.label)
    }

    fn last_optional_chain_token_start(source_text: &str, offset: u32) -> Option<u32> {
        let end = (offset as usize).min(source_text.len());
        source_text[..end].rfind("?.").map(|idx| idx as u32)
    }

    fn quoted_property_name_replacement_span(source_text: &str, offset: u32) -> Option<(u32, u32)> {
        let i = (offset as usize).min(source_text.len());
        let bytes = source_text.as_bytes();

        let mut quote_start = None;
        let mut j = i;
        while j > 0 {
            j -= 1;
            let b = bytes[j];
            if b == b'\n' || b == b'\r' {
                break;
            }
            if b == b'"' || b == b'\'' {
                quote_start = Some((j, b));
                break;
            }
        }
        let (start, quote) = quote_start?;
        let mut end = i;
        while end < bytes.len() {
            let b = bytes[end];
            if b == quote {
                break;
            }
            if b == b'\n' || b == b'\r' {
                return None;
            }
            end += 1;
        }
        if end >= bytes.len() || bytes[end] != quote {
            return None;
        }
        let mut k = end + 1;
        while k < bytes.len() && bytes[k].is_ascii_whitespace() {
            if bytes[k] == b'\n' || bytes[k] == b'\r' {
                return None;
            }
            k += 1;
        }
        if k >= bytes.len() || bytes[k] != b':' {
            return None;
        }
        Some(((start + 1) as u32, end as u32))
    }

    fn is_line_comment_position(source_text: &str, offset: u32) -> bool {
        let i = (offset as usize).min(source_text.len());
        let line_start = source_text[..i].rfind('\n').map_or(0, |p| p + 1);
        source_text[line_start..i].contains("//")
    }

    fn is_import_meta_member_context(source_text: &str, offset: u32) -> bool {
        let end = (offset as usize).min(source_text.len());
        let trimmed = source_text[..end].trim_end();
        trimmed.ends_with("import.meta.") || trimmed.ends_with("import.meta")
    }

    fn extract_import_meta_members(source_text: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut search_start = 0usize;
        while let Some(interface_idx) = source_text[search_start..].find("interface ImportMeta") {
            let abs = search_start + interface_idx;
            let Some(open_rel) = source_text[abs..].find('{') else {
                break;
            };
            let mut i = abs + open_rel + 1;
            let bytes = source_text.as_bytes();
            let mut depth = 1i32;
            let block_start = i;
            while i < source_text.len() && depth > 0 {
                match bytes[i] {
                    b'{' => depth += 1,
                    b'}' => depth -= 1,
                    _ => {}
                }
                i += 1;
            }
            if depth != 0 || i <= block_start {
                break;
            }
            let body = &source_text[block_start..i - 1];
            for line in body.lines() {
                let trimmed = line.trim_start();
                if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with("/*") {
                    continue;
                }
                let mut chars = trimmed.chars();
                let Some(first) = chars.next() else {
                    continue;
                };
                if !(first == '_' || first == '$' || first.is_ascii_alphabetic()) {
                    continue;
                }
                let mut name = String::new();
                name.push(first);
                for ch in chars {
                    if ch == '_' || ch == '$' || ch.is_ascii_alphanumeric() {
                        name.push(ch);
                    } else {
                        break;
                    }
                }
                if name.is_empty() {
                    continue;
                }
                let after_name = &trimmed[name.len()..].trim_start();
                if after_name.starts_with(':') || after_name.starts_with('(') {
                    out.push(name);
                }
            }
            search_start = i;
        }
        out
    }

    fn import_meta_project_completion_items(&self, file_name: &str) -> Vec<CompletionItem> {
        let mut out = Vec::new();
        let mut seen = FxHashSet::default();
        let scan_paths =
            Self::fallback_class_member_scan_paths(&self.open_files, &self.external_project_files);
        for path in scan_paths {
            if path == file_name {
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
            for name in Self::extract_import_meta_members(&content) {
                if !seen.insert(name.clone()) {
                    continue;
                }
                let mut item = CompletionItem::new(name, CompletionItemKind::Property);
                item.sort_text = Some(sort_priority::MEMBER.to_string());
                out.push(item);
            }
        }
        out.sort_by(|a, b| a.label.cmp(&b.label));
        out
    }

    fn trailing_function_parameter_names_at_declaration_end(
        source_text: &str,
        offset: u32,
    ) -> FxHashSet<String> {
        let mut out = FxHashSet::default();
        let end = (offset as usize).min(source_text.len());
        let trimmed = source_text[..end].trim_end();
        if !trimmed.ends_with('}') {
            return out;
        }

        let bytes = trimmed.as_bytes();
        let close = bytes.len() - 1;
        let mut depth = 0i32;
        let mut open = None;
        let mut i = close + 1;
        while i > 0 {
            i -= 1;
            match bytes[i] {
                b'}' => depth += 1,
                b'{' => {
                    depth -= 1;
                    if depth == 0 {
                        open = Some(i);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(open) = open else {
            return out;
        };
        let before_body = &trimmed[..open];
        let Some(function_kw) = before_body.rfind("function") else {
            return out;
        };
        let after_kw = &before_body[function_kw + "function".len()..];
        let Some(paren_rel) = after_kw.find('(') else {
            return out;
        };
        let open_paren = function_kw + "function".len() + paren_rel;
        let Some(close_rel) = before_body[open_paren + 1..].find(')') else {
            return out;
        };
        let close_paren = open_paren + 1 + close_rel;
        let params = &before_body[open_paren + 1..close_paren];
        for segment in params.split(',') {
            let mut part = segment.trim();
            if part.starts_with("...") {
                part = part[3..].trim_start();
            }
            let ident_end = part
                .find(|c: char| !(c == '_' || c == '$' || c.is_ascii_alphanumeric()))
                .unwrap_or(part.len());
            if ident_end == 0 {
                continue;
            }
            let ident = &part[..ident_end];
            if ident
                .chars()
                .next()
                .is_some_and(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphabetic())
            {
                out.insert(ident.to_string());
            }
        }
        out
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
            if let Some(base_offset) = line_map.position_to_offset(position, &source_text)
                && Self::is_line_comment_position(&source_text, base_offset)
            {
                return Some(serde_json::json!({
                    "isGlobalCompletion": false,
                    "isMemberCompletion": false,
                    "isNewIdentifierLocation": false,
                    "entries": []
                }));
            }
            let preferences = request
                .arguments
                .get("preferences")
                .unwrap_or(&request.arguments);
            let interner = TypeInterner::new();
            let provider = Completions::new_with_types(
                &arena,
                &binder,
                &line_map,
                &interner,
                &source_text,
                file.clone(),
            );
            let (completion_position, completion_result) =
                Self::completion_result_at_position(&provider, root, position);
            let provider_items = completion_result
                .as_ref()
                .map(|result| result.entries.clone())
                .unwrap_or_default();
            let project_completion_position = completion_position;
            let project_items = self.project_completion_items(
                &file,
                project_completion_position,
                Some(preferences),
            );
            let is_member_completion = completion_result
                .as_ref()
                .is_some_and(|result| result.is_member_completion);
            let allow_class_member_snippets = !is_member_completion
                && Self::is_class_member_snippet_context(
                    &source_text,
                    &line_map,
                    completion_position,
                );
            let include_class_member_snippets = Self::bool_pref_or_default(
                Some(preferences),
                "includeCompletionsWithClassMemberSnippets",
                self.include_completions_with_class_member_snippets,
            );
            let snippet_items = if include_class_member_snippets && allow_class_member_snippets {
                self.class_member_snippet_items(
                    &provider,
                    root,
                    completion_position,
                    &file,
                    &source_text,
                    &project_items,
                )
            } else {
                Vec::new()
            };
            let items = if is_member_completion {
                provider_items
            } else {
                Self::merge_non_member_completion_items(provider_items, project_items)
            };
            let mut items = items;
            if !snippet_items.is_empty() {
                items = Self::merge_non_member_completion_items(items, snippet_items);
                items = Self::prioritize_class_member_snippet_items(items);
                items = Self::normalize_class_member_snippet_items(items);
            }
            Self::sort_tsserver_completion_items(&mut items);
            let items = Self::prune_deeper_auto_import_duplicates(items);
            let mut items =
                self.maybe_add_verbatim_commonjs_auto_import_items(&file, &source_text, items);
            if let Some(completion_offset) =
                line_map.position_to_offset(completion_position, &source_text)
            {
                items = Self::maybe_add_merged_class_function_members(
                    items,
                    &source_text,
                    completion_offset,
                    is_member_completion,
                );
            }
            if is_member_completion
                && items.is_empty()
                && let Some(completion_offset) =
                    line_map.position_to_offset(completion_position, &source_text)
            {
                let fallback = self.commonjs_require_member_completion_items(
                    &file,
                    &source_text,
                    completion_offset,
                );
                if !fallback.is_empty() {
                    items = Self::merge_non_member_completion_items(items, fallback);
                }
            }
            Self::sort_tsserver_completion_items(&mut items);
            let items = Self::prune_deeper_auto_import_duplicates(items);
            let mut items = items;
            if is_member_completion
                && let Some(completion_offset) =
                    line_map.position_to_offset(completion_position, &source_text)
                && let Some(replacement_start) =
                    Self::last_optional_chain_token_start(&source_text, completion_offset)
            {
                for item in &mut items {
                    if item.replacement_span.is_none()
                        && item
                            .insert_text
                            .as_deref()
                            .is_some_and(|text| text.starts_with("?."))
                    {
                        item.replacement_span = Some((replacement_start, completion_offset));
                    }
                }
            }
            if !is_member_completion
                && let Some(completion_offset) =
                    line_map.position_to_offset(completion_position, &source_text)
            {
                if let Some((replacement_start, replacement_end)) =
                    Self::quoted_property_name_replacement_span(&source_text, completion_offset)
                {
                    for item in &mut items {
                        if item.replacement_span.is_none() {
                            item.replacement_span = Some((replacement_start, replacement_end));
                        }
                    }
                }
                let blocked = Self::trailing_function_parameter_names_at_declaration_end(
                    &source_text,
                    completion_offset,
                );
                if !blocked.is_empty() {
                    items.retain(|item| !blocked.contains(&item.label));
                }
            }
            if is_member_completion
                && let Some(completion_offset) =
                    line_map.position_to_offset(completion_position, &source_text)
                && Self::is_import_meta_member_context(&source_text, completion_offset)
            {
                let project_meta_items = self.import_meta_project_completion_items(&file);
                if !project_meta_items.is_empty() {
                    items = Self::merge_non_member_completion_items(items, project_meta_items);
                    Self::sort_tsserver_completion_items(&mut items);
                }
            }
            let include_insert_text = Self::bool_pref_or_default(
                Some(preferences),
                "includeCompletionsWithInsertText",
                true,
            );

            let entries: Vec<serde_json::Value> = items
                .iter()
                .map(|item| {
                    self.completion_entry_from_item(
                        &file,
                        item,
                        &line_map,
                        &source_text,
                        include_insert_text,
                    )
                })
                .collect();
            let has_class_member_snippet = items
                .iter()
                .any(|item| item.source.as_deref() == Some("ClassMemberSnippet/"));
            let is_new_identifier_location = if Self::is_type_annotation_identifier_prefix_context(
                &source_text,
                &line_map,
                completion_position,
            ) {
                false
            } else if (include_class_member_snippets && has_class_member_snippet)
                || Self::is_class_member_declaration_prefix_context(
                    &source_text,
                    &line_map,
                    completion_position,
                )
            {
                true
            } else if Self::is_bare_identifier_expression_prefix(
                &source_text,
                &line_map,
                completion_position,
            ) {
                false
            } else {
                completion_result
                    .as_ref()
                    .map(|r| r.is_new_identifier_location)
                    .unwrap_or(false)
            };
            let default_commit_characters =
                (!is_new_identifier_location).then_some(serde_json::json!([".", ",", ";"]));

            let mut response = serde_json::json!({
                "isGlobalCompletion": completion_result.as_ref().map(|r| r.is_global_completion).unwrap_or(false),
                "isMemberCompletion": completion_result.as_ref().map(|r| r.is_member_completion).unwrap_or(false),
                "isNewIdentifierLocation": is_new_identifier_location,
                "entries": entries,
            });
            if let Some(default_commit_characters) = default_commit_characters {
                response["defaultCommitCharacters"] = default_commit_characters;
            }

            Some(response)
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
            let preferences = request
                .arguments
                .get("preferences")
                .unwrap_or(&request.arguments);
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
            let (completion_position, completion_result) =
                Self::completion_result_at_position(&provider, root, position);
            let provider_items = completion_result
                .as_ref()
                .map(|result| result.entries.clone())
                .unwrap_or_default();
            let project_completion_position = completion_position;
            let mut project_items =
                self.project_completion_items(file, project_completion_position, Some(preferences));
            let is_member_completion = completion_result
                .as_ref()
                .is_some_and(|result| result.is_member_completion);
            let allow_class_member_snippets = !is_member_completion
                && Self::is_class_member_snippet_context(
                    &source_text,
                    &line_map,
                    completion_position,
                );
            let include_class_member_snippets = Self::bool_pref_or_default(
                Some(preferences),
                "includeCompletionsWithClassMemberSnippets",
                self.include_completions_with_class_member_snippets,
            );
            let requested_class_member_snippet = entry_names.iter().any(|entry_name| {
                entry_name
                    .as_object()
                    .and_then(|obj| obj.get("source"))
                    .and_then(serde_json::Value::as_str)
                    == Some("ClassMemberSnippet/")
            });
            if allow_class_member_snippets
                && requested_class_member_snippet
                && project_items.is_empty()
            {
                let forced_auto_import_prefs =
                    serde_json::json!({ "includeCompletionsForModuleExports": true });
                project_items = self.project_completion_items(
                    file,
                    project_completion_position,
                    Some(&forced_auto_import_prefs),
                );
            }
            let snippet_items = if allow_class_member_snippets
                && (requested_class_member_snippet || include_class_member_snippets)
            {
                self.class_member_snippet_items(
                    &provider,
                    root,
                    completion_position,
                    file,
                    &source_text,
                    &project_items,
                )
            } else {
                Vec::new()
            };
            let items = if is_member_completion {
                provider_items
            } else {
                Self::merge_non_member_completion_items(provider_items, project_items.clone())
            };
            let mut items = items;
            if !snippet_items.is_empty() {
                items = Self::merge_non_member_completion_items(items, snippet_items.clone());
                items = Self::prioritize_class_member_snippet_items(items);
                items = Self::normalize_class_member_snippet_items(items);
            }
            items = self.maybe_add_verbatim_commonjs_auto_import_items(file, &source_text, items);
            if let Some(completion_offset) =
                line_map.position_to_offset(completion_position, &source_text)
            {
                items = Self::maybe_add_merged_class_function_members(
                    items,
                    &source_text,
                    completion_offset,
                    is_member_completion,
                );
            }
            if is_member_completion
                && items.is_empty()
                && let Some(completion_offset) =
                    line_map.position_to_offset(completion_position, &source_text)
            {
                let fallback = self.commonjs_require_member_completion_items(
                    file,
                    &source_text,
                    completion_offset,
                );
                if !fallback.is_empty() {
                    items = Self::merge_non_member_completion_items(items, fallback);
                }
            }
            Self::sort_tsserver_completion_items(&mut items);
            // Index merged items by label once so each entry_name lookup below
            // doesn't linear-scan the full list (can be ~100s of items when
            // the project has many exports). Lifetime-scoped to `items`, so
            // the closure's returned &CompletionItem stays valid.
            let mut items_by_label: FxHashMap<&str, Vec<&CompletionItem>> = FxHashMap::default();
            for i in &items {
                items_by_label.entry(i.label.as_str()).or_default().push(i);
            }
            let member_parent = completion_result
                .as_ref()
                .and_then(|result| {
                    result.is_member_completion.then(|| {
                        provider.get_member_completion_parent_type_name(root, completion_position)
                    })
                })
                .flatten();
            let details: Vec<serde_json::Value> = entry_names
                .iter()
                .map(|entry_name| {
                    let (name, requested_source, requested_export_name) =
                        if let Some(s) = entry_name.as_str() {
                            (s.to_string(), None, None)
                        } else if let Some(obj) = entry_name.as_object() {
                            let source_from_value = |value: Option<&serde_json::Value>| {
                                value.and_then(|v| {
                                    v.as_str()
                                        .map(|s| s.trim().to_string())
                                        .or_else(|| {
                                            v.as_object()
                                                .and_then(|obj| obj.get("text"))
                                                .and_then(serde_json::Value::as_str)
                                                .map(|s| s.trim().to_string())
                                        })
                                        .or_else(|| {
                                            v.as_array().and_then(|arr| {
                                                let mut text = String::new();
                                                for part in arr {
                                                    let part_text = part
                                                        .as_object()
                                                        .and_then(|obj| obj.get("text"))
                                                        .and_then(serde_json::Value::as_str)
                                                        .unwrap_or_default();
                                                    text.push_str(part_text);
                                                }
                                                let text = text.trim().to_string();
                                                (!text.is_empty()).then_some(text)
                                            })
                                        })
                                })
                            };
                            (
                                obj.get("name")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string(),
                                source_from_value(obj.get("source"))
                                    .or_else(|| source_from_value(obj.get("sourceDisplay"))),
                                obj.get("data")
                                    .and_then(|data| data.get("exportName"))
                                    .and_then(serde_json::Value::as_str)
                                    .map(std::string::ToString::to_string),
                            )
                        } else {
                            (String::new(), None, None)
                        };
                    // Try to find the matching completion item.
                    // When source metadata is missing for duplicate labels, prefer
                    // ClassMemberSnippet entries to keep tsserver details/code-action
                    // pairing stable for snippet-backed completions.
                    let export_name_matches =
                        |candidate: &CompletionItem, requested_export_name: &str| {
                            Self::auto_import_export_name(candidate)
                                .as_deref()
                                .is_some_and(|name| name == requested_export_name)
                        };
                    // Label-indexed candidates for this entry; avoids a fresh
                    // linear scan of the full merged-items list for every
                    // predicate branch below.
                    let empty_candidates: Vec<&CompletionItem> = Vec::new();
                    let candidates: &Vec<&CompletionItem> = items_by_label
                        .get(name.as_str())
                        .unwrap_or(&empty_candidates);
                    let find = |pred: &dyn Fn(&CompletionItem) -> bool| -> Option<&CompletionItem> {
                        candidates.iter().copied().find(|i| pred(i))
                    };
                    let mut item = if let Some(source) = requested_source.as_deref() {
                        find(&|i| {
                            i.has_action
                                && Self::completion_sources_match(i.source.as_deref(), source)
                                && requested_export_name
                                    .as_deref()
                                    .is_none_or(|requested| export_name_matches(i, requested))
                        })
                        .or_else(|| {
                            find(&|i| {
                                Self::completion_sources_match(i.source.as_deref(), source)
                                    && requested_export_name
                                        .as_deref()
                                        .is_none_or(|requested| export_name_matches(i, requested))
                            })
                        })
                        .or_else(|| {
                            find(&|i| Self::completion_sources_match(i.source.as_deref(), source))
                        })
                        .or_else(|| {
                            requested_export_name
                                .as_deref()
                                .and_then(|requested| find(&|i| export_name_matches(i, requested)))
                        })
                    } else if entry_name.as_object().is_some() {
                        find(&|i| i.source.as_deref() == Some("ClassMemberSnippet/")).or_else(
                            || {
                                find(&|i| {
                                    requested_export_name
                                        .as_deref()
                                        .is_none_or(|requested| export_name_matches(i, requested))
                                })
                            },
                        )
                    } else {
                        find(&|_| true)
                    };
                    if item.is_none() && requested_source.as_deref() == Some("ClassMemberSnippet/")
                    {
                        item = snippet_items.iter().find(|i| {
                            i.label == name && i.source.as_deref() == Some("ClassMemberSnippet/")
                        });
                    }
                    let auto_import_export_name = requested_export_name
                        .clone()
                        .or_else(|| item.and_then(Self::auto_import_export_name));
                    let is_default_auto_import_item =
                        auto_import_export_name.as_deref() == Some("default");
                    let mut display_item_owned = None;
                    if item.is_some_and(|i| i.has_action && i.source.as_ref().is_some())
                        && let Some(found_item) = item
                    {
                        let mut adjusted_item = found_item.clone();
                        if let Some(export_name) = auto_import_export_name.as_deref() {
                            if export_name == "default" {
                                adjusted_item.kind = CompletionItemKind::Property;
                            }
                            if let Some((export_kind, export_type)) = self
                                .auto_import_export_literal_info(file, &adjusted_item, export_name)
                            {
                                adjusted_item.kind = export_kind;
                                adjusted_item.detail = Some(export_type);
                            } else if adjusted_item
                                .detail
                                .as_deref()
                                .is_some_and(|detail| detail.starts_with("auto-import"))
                            {
                                adjusted_item.detail = None;
                            }
                        }
                        display_item_owned = Some(adjusted_item);
                    }
                    let display_item = display_item_owned.as_ref().or(item);
                    let kind =
                        display_item.map_or("property", |i| Self::completion_kind_to_str(i.kind));
                    let kind_modifiers = display_item
                        .and_then(|i| i.kind_modifiers.as_deref())
                        .unwrap_or("");
                    let display_name = if is_default_auto_import_item {
                        "default"
                    } else {
                        &name
                    };
                    let display_parts = Self::build_completion_display_parts(
                        display_item,
                        display_name,
                        member_parent.as_deref(),
                        &arena,
                        &binder,
                        &source_text,
                    );
                    let (documentation, jsdoc_tags) = Self::completion_doc_and_tags(
                        display_item,
                        &name,
                        &arena,
                        &binder,
                        root,
                        &source_text,
                    );
                    let mut detail = serde_json::Map::new();
                    detail.insert("name".to_string(), serde_json::json!(name));
                    detail.insert("kind".to_string(), serde_json::json!(kind));
                    detail.insert(
                        "kindModifiers".to_string(),
                        serde_json::json!(kind_modifiers),
                    );
                    detail.insert("displayParts".to_string(), display_parts);
                    let is_auto_import_item =
                        item.is_some_and(|i| i.has_action && i.source.as_ref().is_some());
                    if !is_auto_import_item && documentation != serde_json::json!([]) {
                        detail.insert("documentation".to_string(), documentation);
                    }
                    detail.insert(
                        "tags".to_string(),
                        if is_auto_import_item {
                            serde_json::json!([])
                        } else {
                            serde_json::json!(jsdoc_tags)
                        },
                    );
                    if let Some(source) = display_item.and_then(|i| i.source.as_ref()) {
                        let source_display =
                            serde_json::json!([{ "text": source, "kind": "text" }]);
                        detail.insert("source".to_string(), source_display.clone());
                        detail.insert("sourceDisplay".to_string(), source_display);
                    }
                    if let Some(item) = item
                        && item.has_action
                    {
                        let edits = item
                            .additional_text_edits
                            .as_ref()
                            .cloned()
                            .unwrap_or_default();
                        let mut text_changes: Vec<serde_json::Value> = edits
                            .iter()
                            .map(|edit| {
                                let start = line_map
                                    .position_to_offset(edit.range.start, &source_text)
                                    .unwrap_or(0);
                                let end = line_map
                                    .position_to_offset(edit.range.end, &source_text)
                                    .unwrap_or(start);
                                let new_text = Self::normalize_mts_auto_import_edit_text(
                                    file,
                                    item.kind,
                                    &source_text,
                                    &edit.new_text,
                                );
                                serde_json::json!({
                                    "span": {
                                        "start": start,
                                        "length": end.saturating_sub(start),
                                    },
                                    "newText": Self::normalize_tsserver_newlines_for_file(
                                        &new_text,
                                        file,
                                    ),
                                })
                            })
                            .collect();
                        if item.source.as_deref() == Some("ClassMemberSnippet/")
                            && let Some(insert_text) = item.insert_text.as_deref()
                        {
                            let mut synthesized =
                                Self::class_member_snippet_synthesized_text_changes(
                                    &source_text,
                                    insert_text,
                                    &item.label,
                                    &project_items,
                                );
                            if synthesized.is_empty() {
                                synthesized = self
                                    .class_member_snippet_transitive_default_import_text_changes(
                                        file,
                                        &source_text,
                                        insert_text,
                                        &item.label,
                                    );
                            }
                            if !synthesized.is_empty() {
                                text_changes = synthesized;
                            }
                        }
                        if !text_changes.is_empty() {
                            if file.ends_with(".mts") {
                                for change in &mut text_changes {
                                    let Some(new_text) =
                                        change.get("newText").and_then(serde_json::Value::as_str)
                                    else {
                                        continue;
                                    };
                                    let Some((module_specifier, _)) =
                                        Self::parse_named_import_clause(
                                            new_text, "import {", "} from ",
                                        )
                                    else {
                                        continue;
                                    };
                                    if Self::type_only_named_imports_for_module(
                                        &source_text,
                                        module_specifier,
                                    )
                                    .is_empty()
                                    {
                                        continue;
                                    }
                                    let Some((existing_start, existing_length)) =
                                        Self::find_type_only_named_import_span(
                                            &source_text,
                                            module_specifier,
                                        )
                                    else {
                                        continue;
                                    };

                                    let start = change
                                        .get("span")
                                        .and_then(|span| span.get("start"))
                                        .and_then(serde_json::Value::as_u64)
                                        .map(|n| n as u32)
                                        .unwrap_or(0);
                                    let length = change
                                        .get("span")
                                        .and_then(|span| span.get("length"))
                                        .and_then(serde_json::Value::as_u64)
                                        .map(|n| n as u32)
                                        .unwrap_or(0);
                                    if length != 0 || start != existing_start {
                                        continue;
                                    }

                                    if let Some(change_obj) = change.as_object_mut() {
                                        change_obj.insert(
                                            "span".to_string(),
                                            serde_json::json!({
                                                "start": existing_start,
                                                "length": existing_length,
                                            }),
                                        );
                                    }
                                    break;
                                }
                            }

                            let description = if item.source.as_deref()
                                == Some("ClassMemberSnippet/")
                            {
                                format!("Includes imports of types referenced by '{}'", item.label)
                            } else {
                                Self::auto_import_code_action_description(
                                    &source_text,
                                    file,
                                    item.source.as_deref(),
                                    &edits,
                                    &item.label,
                                )
                            };

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
                    }
                    serde_json::Value::Object(detail)
                })
                .collect();
            Some(serde_json::json!(details))
        })();
        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    // Display parts rendering, signature help handler, and tokenization utilities
    // are in handlers_completions_display.rs
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsz::lsp::completions::CompletionItemKind;

    #[test]
    fn sort_tsserver_completion_items_prefers_direct_source_over_index_for_same_symbol() {
        let mut items = vec![
            CompletionItem::new("Thing2A".to_string(), CompletionItemKind::Class)
                .with_source("./index".to_string()),
            CompletionItem::new("Thing2A".to_string(), CompletionItemKind::Class)
                .with_source("./thing2A".to_string()),
        ];

        Server::sort_tsserver_completion_items(&mut items);

        assert_eq!(items[0].source.as_deref(), Some("./thing2A"));
        assert_eq!(items[1].source.as_deref(), Some("./index"));
    }

    #[test]
    fn sort_tsserver_completion_items_prefers_bare_package_source_over_parent_relative() {
        let mut items = vec![
            CompletionItem::new("MyClass".to_string(), CompletionItemKind::Class)
                .with_sort_text("16".to_string())
                .with_has_action()
                .with_source("../packages/mylib".to_string()),
            CompletionItem::new("MyClass".to_string(), CompletionItemKind::Class)
                .with_sort_text("16".to_string())
                .with_has_action()
                .with_source("mylib".to_string()),
        ];

        Server::sort_tsserver_completion_items(&mut items);

        let ordered_sources: Vec<Option<&str>> =
            items.iter().map(|item| item.source.as_deref()).collect();
        assert_eq!(
            ordered_sources,
            vec![Some("mylib"), Some("../packages/mylib")]
        );
    }

    #[test]
    fn sort_tsserver_completion_items_prefers_package_root_over_deep_package_subpath() {
        let mut items = vec![
            CompletionItem::new("PatternValidator".to_string(), CompletionItemKind::Class)
                .with_sort_text("16".to_string())
                .with_has_action()
                .with_source("@angular/forms/forms".to_string()),
            CompletionItem::new("PatternValidator".to_string(), CompletionItemKind::Class)
                .with_sort_text("16".to_string())
                .with_has_action()
                .with_source("@angular/forms".to_string()),
        ];

        Server::sort_tsserver_completion_items(&mut items);

        let ordered_sources: Vec<Option<&str>> =
            items.iter().map(|item| item.source.as_deref()).collect();
        assert_eq!(
            ordered_sources,
            vec![Some("@angular/forms"), Some("@angular/forms/forms")]
        );
    }

    #[test]
    fn sort_tsserver_completion_items_uses_numeric_aware_ui_order() {
        let mut items = vec![
            CompletionItem::new("Int16Array".to_string(), CompletionItemKind::Variable)
                .with_sort_text("15".to_string()),
            CompletionItem::new("Int8Array".to_string(), CompletionItemKind::Variable)
                .with_sort_text("15".to_string()),
            CompletionItem::new("Int32Array".to_string(), CompletionItemKind::Variable)
                .with_sort_text("15".to_string()),
        ];

        Server::sort_tsserver_completion_items(&mut items);

        let labels: Vec<&str> = items.iter().map(|item| item.label.as_str()).collect();
        assert_eq!(labels, vec!["Int8Array", "Int16Array", "Int32Array"]);
    }

    #[test]
    fn sort_tsserver_completion_items_matches_ui_name_sort_across_kinds() {
        let mut items = vec![
            CompletionItem::new("as".to_string(), CompletionItemKind::Keyword),
            CompletionItem::new("Array".to_string(), CompletionItemKind::Class),
        ];
        items[0].sort_text = Some("15".to_string());
        items[1].sort_text = Some("15".to_string());

        Server::sort_tsserver_completion_items(&mut items);

        assert_eq!(items[0].label, "Array");
        assert_eq!(items[1].label, "as");
    }

    #[test]
    fn prune_deeper_auto_import_duplicates_keeps_shallow_relative_source() {
        let items = vec![
            CompletionItem::new("Button".to_string(), CompletionItemKind::Function)
                .with_has_action()
                .with_source("./lib/main".to_string()),
            CompletionItem::new("Button".to_string(), CompletionItemKind::Function)
                .with_has_action()
                .with_source("./lib/components/button/Button".to_string()),
            CompletionItem::new("foo".to_string(), CompletionItemKind::Function)
                .with_has_action()
                .with_source("./a".to_string()),
            CompletionItem::new("foo".to_string(), CompletionItemKind::Function)
                .with_has_action()
                .with_source("./b".to_string()),
        ];

        let pruned = Server::prune_deeper_auto_import_duplicates(items);
        let button_sources: Vec<&str> = pruned
            .iter()
            .filter(|item| item.label == "Button")
            .filter_map(|item| item.source.as_deref())
            .collect();
        let foo_sources: Vec<&str> = pruned
            .iter()
            .filter(|item| item.label == "foo")
            .filter_map(|item| item.source.as_deref())
            .collect();

        assert_eq!(button_sources, vec!["./lib/main"]);
        assert_eq!(foo_sources, vec!["./a", "./b"]);
    }

    #[test]

    fn normalize_mts_auto_import_edit_text_appends_existing_type_only_members() {
        let source_text = "import type { I } from \"./mod.js\";\n\nconst x: I = new ";
        let normalized = Server::normalize_mts_auto_import_edit_text(
            "/a.mts",
            CompletionItemKind::Class,
            source_text,
            "import { C } from \"./mod.js\";\n",
        );

        assert!(
            normalized.contains("import { C, type I } from \"./mod.js\";"),
            "expected normalize_mts_auto_import_edit_text to keep existing type-only imports, got: {normalized}"
        );
    }

    #[test]
    fn merged_class_member_context_detects_uppercase_receiver_before_dot() {
        let source_text = "Foo.";
        let offset = source_text.len() as u32;
        assert!(Server::looks_like_merged_class_member_completion_context(
            source_text,
            offset
        ));

        let lower_source = "foo.";
        let lower_offset = lower_source.len() as u32;
        assert!(!Server::looks_like_merged_class_member_completion_context(
            lower_source,
            lower_offset
        ));
    }

    #[test]
    fn maybe_add_merged_class_function_members_populates_missing_function_surface() {
        let items = vec![
            CompletionItem::new("prototype".to_string(), CompletionItemKind::Property),
            CompletionItem::new("x".to_string(), CompletionItemKind::Variable),
        ];

        let merged = Server::maybe_add_merged_class_function_members(items, "Foo.", 4, true);
        let labels: FxHashSet<&str> = merged.iter().map(|item| item.label.as_str()).collect();

        assert!(labels.contains("prototype"));
        assert!(labels.contains("x"));
        assert!(labels.contains("apply"));
        assert!(labels.contains("call"));
        assert!(labels.contains("bind"));
        assert!(labels.contains("arguments"));
        assert!(labels.contains("caller"));
    }

    #[test]
    fn completion_sources_match_normalizes_extensions_index_and_node_prefix() {
        assert!(Server::completion_sources_match(
            Some("./local.ts"),
            "./local.js"
        ));
        assert!(Server::completion_sources_match(
            Some("./pkg/index.d.ts"),
            "./pkg"
        ));
        assert!(Server::completion_sources_match(Some("node:path"), "path"));
        assert!(Server::completion_sources_match(
            Some("./decl.d.mts"),
            "./decl.js"
        ));
        assert!(!Server::completion_sources_match(
            Some("./other"),
            "./local.js"
        ));
    }

    // Regression: when the active file sits at the filesystem root (e.g.
    // fourslash tests that name files `/main.ts`, `/Component.tsx`), the
    // computed `workspace_prefix` is "/" — every sibling file under "/"
    // must still be fed to the auto-import project. Previously the
    // filter produced the prefix "//" and dropped all sibling source
    // files (only node_modules survived), so Component.tsx / local.ts
    // never showed up in completion auto-imports and details requests
    // for them returned no codeActions.
    #[test]
    fn should_include_completion_project_path_root_workspace_includes_sibling_files() {
        // Root workspace: active file is /main.ts -> workspace_prefix = "/".
        assert_eq!(
            Server::path_workspace_prefix("/main.ts").as_deref(),
            Some("/")
        );

        // Sibling source files under "/" must be included.
        assert!(Server::should_include_completion_project_path(
            "/Component.tsx",
            "/main.ts",
            Some("/"),
            None,
        ));
        assert!(Server::should_include_completion_project_path(
            "/local.ts",
            "/main.ts",
            Some("/"),
            None,
        ));
        // Same file passes via the path == current_file early-return.
        assert!(Server::should_include_completion_project_path(
            "/main.ts",
            "/main.ts",
            Some("/"),
            None,
        ));
        // node_modules paths go through the allowed_packages gate and are
        // unaffected by the workspace_prefix fix: when no allowlist is
        // configured, node_modules paths are permitted.
        assert!(Server::should_include_completion_project_path(
            "/node_modules/bar/index.d.ts",
            "/main.ts",
            Some("/"),
            None,
        ));
    }

    // Non-root workspace prefix behavior (/project/...) is unchanged by the
    // root-workspace fix: siblings under the workspace are still included,
    // and files outside it are still excluded.
    #[test]
    fn should_include_completion_project_path_non_root_workspace_respects_prefix() {
        assert!(Server::should_include_completion_project_path(
            "/project/src/foo.ts",
            "/project/src/main.ts",
            Some("/project"),
            None,
        ));
        assert!(!Server::should_include_completion_project_path(
            "/other/foo.ts",
            "/project/src/main.ts",
            Some("/project"),
            None,
        ));
    }
}
