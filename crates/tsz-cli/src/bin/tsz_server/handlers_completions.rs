//! Completions handlers for tsz-server.
//!
//! Display parts rendering, signature help, and tokenization are in
//! `handlers_completions_display.rs`.

use super::{Server, TsServerRequest, TsServerResponse};
use crate::handlers_completions_parameters::trailing_function_parameter_names_at_declaration_end;
use rustc_hash::{FxHashMap, FxHashSet};
use std::cmp::Ordering;
use tsz::lsp::completions::{CompletionItem, CompletionItemKind, Completions, sort_priority};
use tsz::lsp::position::{LineMap, Position};
use tsz_solver::construction::TypeInterner;

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

    pub(crate) fn handle_completions(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let is_legacy_completions = request.command == "completions";
        let result = (|| -> Option<serde_json::Value> {
            let (file, line, offset) = Self::extract_file_position(&request.arguments)?;
            let (arena, binder, root, source_text) = self.parse_and_bind_file(&file)?;
            let line_map = LineMap::build(&source_text);
            let position = Self::tsserver_to_lsp_position(line, offset);
            if let Some(base_offset) = line_map.position_to_offset(position, &source_text)
                && Self::is_line_comment_position(&source_text, base_offset)
            {
                if is_legacy_completions {
                    return Some(serde_json::json!([]));
                }
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
                let blocked = trailing_function_parameter_names_at_declaration_end(
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
                    .is_some_and(|r| r.is_new_identifier_location)
            };
            let default_commit_characters =
                (!is_new_identifier_location).then_some(serde_json::json!([".", ",", ";"]));

            if is_legacy_completions {
                return Some(serde_json::Value::Array(entries));
            }

            let mut response = serde_json::json!({
                "isGlobalCompletion": completion_result.as_ref().is_some_and(|r| r.is_global_completion),
                "isMemberCompletion": completion_result.as_ref().is_some_and(|r| r.is_member_completion),
                "isNewIdentifierLocation": is_new_identifier_location,
                "entries": entries,
            });
            if let Some(default_commit_characters) = default_commit_characters {
                response["defaultCommitCharacters"] = default_commit_characters;
            }

            Some(response)
        })();
        let fallback = if is_legacy_completions {
            serde_json::json!([])
        } else {
            serde_json::json!({
                "isGlobalCompletion": false,
                "isMemberCompletion": false,
                "isNewIdentifierLocation": false,
                "entries": []
            })
        };
        self.success_response(seq, request, Some(result.unwrap_or(fallback)))
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
                    if item.is_some_and(|i| i.has_action && i.source.is_some())
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
                        item.is_some_and(|i| i.has_action && i.source.is_some());
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
        self.success_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
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
