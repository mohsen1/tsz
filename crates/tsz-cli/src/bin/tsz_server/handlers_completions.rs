//! Completions handlers for tsz-server.
//!
//! Display parts rendering, signature help, and tokenization are in
//! `handlers_completions_display.rs`.

use super::{Server, TsServerRequest, TsServerResponse};
use rustc_hash::FxHashSet;
use std::cmp::Ordering;
use std::path::Path;
use tsz::lsp::Project;
use tsz::lsp::completions::{CompletionItem, CompletionItemKind, Completions, sort_priority};
use tsz::lsp::position::{LineMap, Position};
use tsz_solver::TypeInterner;

impl Server {
    fn completion_probe_positions(
        position: Position,
        line_map: &LineMap,
        source_text: &str,
    ) -> Vec<Position> {
        let mut probes = vec![position];
        let Some(base_offset) = line_map.position_to_offset(position, source_text) else {
            return probes;
        };
        let len = source_text.len() as u32;
        let bytes = source_text.as_bytes();

        if base_offset < len
            && bytes[base_offset as usize] == b'.'
            && base_offset + 2 < len
            && bytes[(base_offset + 1) as usize] == b'/'
            && bytes[(base_offset + 2) as usize] == b'*'
        {
            for candidate in [base_offset + 1, base_offset + 2, base_offset + 3] {
                if candidate < len {
                    let probe = line_map.offset_to_position(candidate, source_text);
                    if !probes.contains(&probe) {
                        probes.push(probe);
                    }
                }
            }
        }

        let Some(marker_start) = Self::fourslash_marker_comment_start(source_text, base_offset)
        else {
            return probes;
        };
        let marker_end = source_text[marker_start as usize..]
            .find("*/")
            .map(|rel_end| marker_start + rel_end as u32 + 2);
        let line_start = source_text[..marker_start as usize]
            .rfind('\n')
            .map_or(0, |idx| idx + 1);
        if source_text[line_start..marker_start as usize].contains("//") {
            return vec![position];
        }
        probes.clear();
        probes.push(position);
        for candidate in [
            marker_start.saturating_sub(1),
            marker_start.saturating_sub(2),
            marker_start.saturating_sub(3),
        ] {
            if candidate < len {
                let probe = line_map.offset_to_position(candidate, source_text);
                if !probes.contains(&probe) {
                    probes.push(probe);
                }
            }
        }
        if let Some(marker_end) = marker_end {
            for candidate in [marker_end, marker_end.saturating_add(1)] {
                if candidate < len {
                    let probe = line_map.offset_to_position(candidate, source_text);
                    if !probes.contains(&probe) {
                        probes.push(probe);
                    }
                }
            }
        }
        if probes.is_empty() {
            probes.push(position);
        }
        probes
    }

    fn fourslash_marker_comment_start(source_text: &str, base_offset: u32) -> Option<u32> {
        let bytes = source_text.as_bytes();
        let len = bytes.len() as u32;
        if len < 4 {
            return None;
        }
        let offset = base_offset.min(len.saturating_sub(1));
        let search_start = offset.saturating_sub(8);
        let search_end = (offset + 1).min(len.saturating_sub(2));
        for start in search_start..=search_end {
            if bytes[start as usize] != b'/' || bytes[(start + 1) as usize] != b'*' {
                continue;
            }
            let mut end = start + 2;
            while end + 1 < len && end - start <= 8 {
                if bytes[end as usize] == b'*' && bytes[(end + 1) as usize] == b'/' {
                    let digits = &bytes[(start + 2) as usize..end as usize];
                    let is_fourslash_marker =
                        digits.is_empty() || digits.iter().all(u8::is_ascii_digit);
                    if is_fourslash_marker {
                        let comment_end = end + 1;
                        if offset >= start && offset <= comment_end {
                            return Some(start);
                        }
                    }
                    break;
                }
                end += 1;
            }
        }
        None
    }

    fn is_class_member_snippet_context(
        source_text: &str,
        line_map: &LineMap,
        position: Position,
    ) -> bool {
        let Some(offset) = line_map.position_to_offset(position, source_text) else {
            return false;
        };
        let end =
            if let Some(marker_start) = Self::fourslash_marker_comment_start(source_text, offset) {
                marker_start.saturating_sub(1) as usize
            } else {
                offset as usize
            }
            .min(source_text.len());
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

    fn completion_result_with_probes(
        provider: &Completions<'_>,
        root: tsz::parser::base::NodeIndex,
        position: Position,
        line_map: &LineMap,
        source_text: &str,
    ) -> (Position, Option<tsz::lsp::completions::CompletionResult>) {
        let probe_positions = Self::completion_probe_positions(position, line_map, source_text);
        let has_multiple_probes = probe_positions.len() > 1;
        let mut selected_position = position;
        let mut selected_result = None;
        let mut selected_score = (false, 0usize, false);
        for probe_position in probe_positions {
            let candidate = provider.get_completion_result(root, probe_position);
            let score = candidate
                .as_ref()
                .map(|result| {
                    (
                        result.is_member_completion && !result.entries.is_empty(),
                        result.entries.len(),
                        result.is_new_identifier_location,
                    )
                })
                .unwrap_or((false, 0usize, false));
            if selected_result.is_none() {
                selected_position = probe_position;
                selected_result = candidate;
                selected_score = score;
                continue;
            }

            let selected_is_member = selected_score.0;
            let candidate_is_member = score.0;
            let should_replace = if candidate_is_member && !selected_is_member {
                true
            } else if candidate_is_member && selected_is_member {
                score.1 > selected_score.1
            } else if has_multiple_probes && !candidate_is_member && !selected_is_member {
                score.1 > 0
                    && (selected_score.1 == 0
                        || (selected_score.2 && !score.2)
                        || (score.2 == selected_score.2 && score.1 < selected_score.1))
            } else {
                false
            };
            if should_replace {
                selected_position = probe_position;
                selected_result = candidate;
                selected_score = score;
            }
        }
        (selected_position, selected_result)
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
            seen.insert((item.label.clone(), item.source.clone()));
        }

        for item in project_items {
            let key = (item.label.clone(), item.source.clone());
            if seen.insert(key) {
                merged.push(item);
            }
        }

        merged
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
        let mut files = self.open_files.clone();
        if !files.contains_key(file_name)
            && let Ok(content) = std::fs::read_to_string(file_name)
        {
            files.insert(file_name.to_string(), content);
        }
        Self::add_project_config_files(&mut files, file_name);
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

        items.sort_by(|a, b| {
            compare_case_sensitive_ui(a.effective_sort_text(), b.effective_sort_text())
                .then_with(|| compare_case_sensitive_ui(&a.label, &b.label))
                .then_with(|| compare_completion_sources(a.source.as_deref(), b.source.as_deref()))
        });
    }

    fn completion_entry_from_item(
        item: &tsz::lsp::completions::CompletionItem,
        line_map: &LineMap,
        source_text: &str,
        include_insert_text: bool,
    ) -> serde_json::Value {
        let kind = Self::completion_kind_to_str(item.kind);
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

    fn strip_trailing_fourslash_marker_text(text: &str) -> &str {
        let trimmed = text.trim_end();
        if let Some(start) = trimmed.rfind("/*") {
            let after = &trimmed[start + 2..];
            if !after.contains("*/") {
                return trimmed[..start].trim_end();
            }
        }
        if !trimmed.ends_with("*/") {
            return trimmed;
        }
        let Some(start) = trimmed.rfind("/*") else {
            return trimmed;
        };
        let marker = &trimmed[start + 2..trimmed.len() - 2];
        if marker.is_empty() || marker.bytes().all(|b| b.is_ascii_digit()) {
            trimmed[..start].trim_end()
        } else {
            trimmed
        }
    }

    fn is_import_meta_member_context(source_text: &str, offset: u32) -> bool {
        let end = (offset as usize).min(source_text.len());
        let trimmed = Self::strip_trailing_fourslash_marker_text(&source_text[..end]).trim_end();
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
        let trimmed = Self::strip_trailing_fourslash_marker_text(&source_text[..end]).trim_end();
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
            let (completion_position, completion_result) = Self::completion_result_with_probes(
                &provider,
                root,
                position,
                &line_map,
                &source_text,
            );
            let provider_items = completion_result
                .as_ref()
                .map(|result| result.entries.clone())
                .unwrap_or_default();
            let project_items =
                self.project_completion_items(&file, completion_position, Some(preferences));
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
                    Self::completion_entry_from_item(
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
            let is_new_identifier_location = if (include_class_member_snippets
                && has_class_member_snippet)
                || Self::is_class_member_declaration_prefix_context(
                    &source_text,
                    &line_map,
                    completion_position,
                ) {
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
            let (completion_position, completion_result) = Self::completion_result_with_probes(
                &provider,
                root,
                position,
                &line_map,
                &source_text,
            );
            let provider_items = completion_result
                .as_ref()
                .map(|result| result.entries.clone())
                .unwrap_or_default();
            let mut project_items =
                self.project_completion_items(file, completion_position, Some(preferences));
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
                    completion_position,
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
            Self::sort_tsserver_completion_items(&mut items);
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
                    let (name, requested_source) = if let Some(s) = entry_name.as_str() {
                        (s.to_string(), None)
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
                        )
                    } else {
                        (String::new(), None)
                    };
                    // Try to find the matching completion item.
                    // When source metadata is missing for duplicate labels, prefer
                    // ClassMemberSnippet entries to keep tsserver details/code-action
                    // pairing stable for snippet-backed completions.
                    let normalize_source =
                        |s: &str| s.trim().trim_matches('\"').trim_matches('\'').to_string();
                    let source_matches = |item_source: Option<&str>, requested_source: &str| {
                        let Some(item_source) = item_source else {
                            return false;
                        };
                        let normalize_no_ext = |s: &str| {
                            let s = normalize_source(s);

                            s.strip_suffix(".js")
                                .or_else(|| s.strip_suffix(".mjs"))
                                .or_else(|| s.strip_suffix(".cjs"))
                                .unwrap_or(&s)
                                .to_string()
                        };
                        let item_source = normalize_no_ext(item_source);
                        let requested_source = normalize_no_ext(requested_source);
                        if item_source == requested_source {
                            return true;
                        }
                        false
                    };
                    let mut item = if let Some(source) = requested_source.as_deref() {
                        items.iter().find(|i| {
                            i.label == name && source_matches(i.source.as_deref(), source)
                        })
                    } else {
                        items
                            .iter()
                            .find(|i| {
                                i.label == name
                                    && i.source.as_deref() == Some("ClassMemberSnippet/")
                            })
                            .or_else(|| items.iter().find(|i| i.label == name))
                    };
                    if item.is_none() && requested_source.as_deref() == Some("ClassMemberSnippet/")
                    {
                        item = snippet_items.iter().find(|i| {
                            i.label == name && i.source.as_deref() == Some("ClassMemberSnippet/")
                        });
                    }
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
                    let is_auto_import_item =
                        item.is_some_and(|i| i.has_action && i.source.as_ref().is_some());
                    if let Some(documentation) = documentation
                        && !is_auto_import_item
                    {
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
                                    "newText": new_text,
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
}
