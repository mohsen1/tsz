//! Class member snippet logic for completion handlers.
//!
//! Extracted from `handlers_completions.rs` to keep individual files under 2000 LOC.
//! Contains all `class_member_snippet_*` methods, fallback scanning, import synthesis,
//! and related helpers.

use super::Server;
use rustc_hash::FxHashSet;
use std::collections::BTreeSet;
use std::path::Path;
use tsz::lsp::completions::{CompletionItem, Completions};
use tsz::lsp::position::LineMap;
use tsz::parser::node::NodeAccess;

impl Server {
    pub(super) fn prioritize_class_member_snippet_items(
        items: Vec<CompletionItem>,
    ) -> Vec<CompletionItem> {
        let snippet_labels: FxHashSet<String> = items
            .iter()
            .filter(|item| item.source.as_deref() == Some("ClassMemberSnippet/"))
            .map(|item| item.label.clone())
            .collect();
        if snippet_labels.is_empty() {
            return items;
        }

        items
            .into_iter()
            .filter(|item| {
                !snippet_labels.contains(&item.label)
                    || item.source.as_deref() == Some("ClassMemberSnippet/")
            })
            .collect()
    }

    pub(super) fn normalize_class_member_snippet_items(
        mut items: Vec<CompletionItem>,
    ) -> Vec<CompletionItem> {
        for item in &mut items {
            if item.source.as_deref() != Some("ClassMemberSnippet/") {
                continue;
            }
            item.has_action = true;
            item.is_snippet = true;
            if item.insert_text.is_none() {
                item.insert_text = Self::class_member_snippet_insert_text(item);
            }
        }
        items
    }

    fn class_member_snippet_insert_text(item: &CompletionItem) -> Option<String> {
        let detail = item.detail.as_deref().unwrap_or("").trim();
        let is_getter = item
            .kind_modifiers
            .as_deref()
            .is_some_and(|mods| mods.split(',').any(|m| m.trim() == "getter"));
        match item.kind {
            tsz::lsp::completions::CompletionItemKind::Property => {
                if is_getter {
                    let ty = if detail.is_empty() { "any" } else { detail };
                    return Some(format!(
                        "get {}(): {} {{\n}}",
                        item.label,
                        ty.trim_end_matches(';').trim()
                    ));
                }
                if detail.is_empty() {
                    Some(format!("{};", item.label))
                } else {
                    Some(format!(
                        "{}: {};",
                        item.label,
                        detail.trim_end_matches(';').trim()
                    ))
                }
            }
            tsz::lsp::completions::CompletionItemKind::Method => {
                let (params, return_type) = detail.split_once("=>")?;
                let params = Self::normalize_method_snippet_params(params);
                let return_type = Self::normalize_method_snippet_return_type(return_type);
                if !params.starts_with('(') || !params.ends_with(')') || return_type.is_empty() {
                    return None;
                }
                Some(format!(
                    "public {}{}: {} {{\n}}",
                    item.label, params, return_type
                ))
            }
            _ => None,
        }
    }

    fn normalize_method_snippet_params(params: &str) -> String {
        let params = params.trim();
        if !params.starts_with('(') || !params.ends_with(')') {
            return params.to_string();
        }
        let inner = &params[1..params.len().saturating_sub(1)];
        let inner = inner.trim_end();
        let inner = inner.strip_suffix(',').unwrap_or(inner).trim();
        format!("({inner})")
    }

    fn normalize_method_snippet_return_type(return_type: &str) -> String {
        let return_type = return_type.trim().trim_end_matches(';').trim_end();
        let return_type = return_type
            .strip_suffix('{')
            .unwrap_or(return_type)
            .trim_end();
        return_type.trim().to_string()
    }

    fn class_member_snippet_type_identifiers(
        insert_text: &str,
        member_name: &str,
    ) -> BTreeSet<String> {
        const IGNORED: &[&str] = &[
            "public",
            "private",
            "protected",
            "readonly",
            "async",
            "abstract",
            "override",
            "static",
            "declare",
            "string",
            "number",
            "boolean",
            "bigint",
            "symbol",
            "object",
            "unknown",
            "never",
            "any",
            "void",
            "undefined",
            "null",
            "true",
            "false",
            "this",
            "Promise",
            "Array",
        ];

        let mut out = BTreeSet::new();
        let mut token = String::new();
        let flush = |token: &mut String, out: &mut BTreeSet<String>| {
            if token.is_empty() {
                return;
            }
            let candidate = token.clone();
            token.clear();
            let first = candidate.chars().next();
            let looks_like_type = first.is_some_and(|ch| ch == '_' || ch.is_ascii_uppercase());
            if !looks_like_type {
                return;
            }
            if candidate == member_name || IGNORED.iter().any(|ignored| *ignored == candidate) {
                return;
            }
            out.insert(candidate);
        };

        for ch in insert_text.chars() {
            if ch == '_' || ch == '$' || ch.is_ascii_alphanumeric() {
                token.push(ch);
            } else {
                flush(&mut token, &mut out);
            }
        }
        flush(&mut token, &mut out);
        out
    }

    fn class_member_snippet_additional_edits(
        insert_text: &str,
        member_name: &str,
        project_items: &[CompletionItem],
    ) -> Vec<tsz::lsp::rename::TextEdit> {
        let required_names = Self::class_member_snippet_type_identifiers(insert_text, member_name);
        if required_names.is_empty() {
            return Vec::new();
        }

        let mut edits = Vec::new();
        let mut seen = BTreeSet::new();
        for required_name in required_names {
            let Some((matched_label, item)) =
                Self::best_project_item_for_required_name(&required_name, project_items)
            else {
                continue;
            };
            for edit in item.additional_text_edits.as_ref().into_iter().flatten() {
                let mut edit = edit.clone();
                if matched_label != required_name {
                    edit.new_text = Self::rewrite_import_name_in_edit(
                        &edit.new_text,
                        &matched_label,
                        &required_name,
                    );
                }
                let key = (
                    edit.range.start.line,
                    edit.range.start.character,
                    edit.range.end.line,
                    edit.range.end.character,
                    edit.new_text.clone(),
                );
                if seen.insert(key) {
                    edits.push(edit);
                }
            }
        }
        edits
    }

    fn best_project_item_for_required_name<'a>(
        required_name: &str,
        project_items: &'a [CompletionItem],
    ) -> Option<(String, &'a CompletionItem)> {
        let base_name = required_name.strip_suffix('_');
        let mut best: Option<(String, &CompletionItem)> = None;
        for item in project_items {
            if !item.has_action || item.additional_text_edits.is_none() {
                continue;
            }
            let matched_label = if item.label == required_name {
                Some(item.label.clone())
            } else if let Some(base_name) = base_name {
                (item.label == base_name).then_some(item.label.clone())
            } else {
                None
            };
            let Some(matched_label) = matched_label else {
                continue;
            };
            match best {
                Some((_, current)) => {
                    let current_source = current.source.as_deref().unwrap_or_default();
                    let item_source = item.source.as_deref().unwrap_or_default();
                    let current_depth = current_source.matches('/').count();
                    let item_depth = item_source.matches('/').count();
                    if (item_depth, item_source.len()) < (current_depth, current_source.len()) {
                        best = Some((matched_label, item));
                    }
                }
                None => best = Some((matched_label, item)),
            }
        }
        best
    }

    fn rewrite_import_name_in_edit(new_text: &str, from: &str, to: &str) -> String {
        if from == to {
            return new_text.to_string();
        }
        if let Some(rest) = new_text.strip_prefix(&format!("import {from} from ")) {
            return format!("import {to} from {rest}");
        }
        if let Some(rest) = new_text.strip_prefix(&format!("import type {from} from ")) {
            return format!("import type {to} from {rest}");
        }
        if new_text.contains("import {") || new_text.contains("import type {") {
            return new_text
                .replace(&format!("{{ {from} }}"), &format!("{{ {to} }}"))
                .replace(&format!(", {from}"), &format!(", {to}"))
                .replace(&format!("{from}, "), &format!("{to}, "));
        }
        new_text.to_string()
    }

    pub(super) fn class_member_snippet_synthesized_text_changes(
        source_text: &str,
        insert_text: &str,
        member_name: &str,
        project_items: &[CompletionItem],
    ) -> Vec<serde_json::Value> {
        let required_names = Self::class_member_snippet_type_identifiers(insert_text, member_name);
        if required_names.is_empty() {
            return Vec::new();
        }

        let fallback_source = Self::infer_extends_import_source(source_text);
        let side_effect_sources = Self::side_effect_import_modules(source_text);
        let mut changes = Vec::new();
        for required_name in required_names {
            let candidate_sources: Vec<&str> = project_items
                .iter()
                .find(|item| {
                    item.label == required_name
                        && item.has_action
                        && item.source.as_deref().is_some()
                })
                .and_then(|item| item.source.as_deref())
                .into_iter()
                .chain(
                    project_items
                        .iter()
                        .filter(|item| {
                            item.label == required_name
                                && item.has_action
                                && item.source.as_deref().is_some()
                        })
                        .filter_map(|item| item.source.as_deref()),
                )
                .collect();
            let source = candidate_sources
                .iter()
                .copied()
                .find(|candidate| Self::has_side_effect_import_for_module(source_text, candidate))
                .or_else(|| {
                    (side_effect_sources.len() == 1).then(|| side_effect_sources[0].as_str())
                })
                .or_else(|| {
                    candidate_sources.iter().copied().find(|candidate| {
                        Self::find_named_import_line_for_module(source_text, candidate).is_some()
                    })
                })
                .or_else(|| {
                    candidate_sources
                        .iter()
                        .copied()
                        .min_by_key(|candidate| (candidate.matches('/').count(), candidate.len()))
                })
                .or(fallback_source.as_deref());
            let Some(source) = source else {
                continue;
            };

            if let Some((line_start, line_end, imported_names)) =
                Self::find_named_import_line_for_module(source_text, source)
            {
                if imported_names.iter().any(|name| name == &required_name) {
                    continue;
                }
                let mut merged = imported_names;
                merged.push(required_name.clone());
                merged.sort();
                merged.dedup();
                changes.push(serde_json::json!({
                    "span": {
                        "start": line_start,
                        "length": line_end.saturating_sub(line_start),
                    },
                    "newText": format!("import {{ {} }} from \"{}\";\n", merged.join(", "), source),
                }));
            } else {
                let insert_offset = if Self::has_side_effect_import_for_module(source_text, source)
                {
                    Self::import_insertion_offset_after_import_block(source_text)
                } else {
                    0
                };
                changes.push(serde_json::json!({
                    "span": {
                        "start": insert_offset,
                        "length": 0,
                    },
                    "newText": format!("import {{ {} }} from \"{}\";\n", required_name, source),
                }));
            }
        }
        changes
    }

    pub(super) fn class_member_snippet_transitive_default_import_text_changes(
        &self,
        file_name: &str,
        source_text: &str,
        insert_text: &str,
        member_name: &str,
    ) -> Vec<serde_json::Value> {
        let required_names = Self::class_member_snippet_type_identifiers(insert_text, member_name);
        if required_names.is_empty() {
            return Vec::new();
        }
        let local_underscored =
            self.class_member_snippet_underscored_type_names(file_name, source_text, &[]);
        let alias_sources = self.recursive_default_import_alias_sources(file_name, source_text);
        let mut changes = Vec::new();
        for required_name in required_names {
            if local_underscored.contains(&required_name) && source_text.contains(&required_name) {
                continue;
            }
            let source = alias_sources.get(&required_name).cloned().or_else(|| {
                required_name
                    .strip_suffix('_')
                    .and_then(|base| alias_sources.get(base).cloned())
            });
            let Some(source) = source else {
                continue;
            };
            let existing_import = format!("import {required_name} from \"{source}\";");
            if source_text.contains(&existing_import) {
                continue;
            }
            changes.push(serde_json::json!({
                "span": { "start": 0, "length": 0 },
                "newText": format!("import {required_name} from \"{source}\";\n"),
            }));
        }
        changes
    }

    fn recursive_default_import_alias_sources(
        &self,
        file_name: &str,
        source_text: &str,
    ) -> std::collections::BTreeMap<String, String> {
        let mut out = std::collections::BTreeMap::new();
        let mut queue =
            std::collections::VecDeque::from([(file_name.to_string(), source_text.to_string())]);
        let mut visited = BTreeSet::new();
        while let Some((current_path, current_source)) = queue.pop_front() {
            if !visited.insert(current_path.clone()) {
                continue;
            }
            for (alias, source) in Self::default_import_aliases_in_source(&current_source) {
                out.entry(alias).or_insert(source);
            }
            for path in Self::resolve_imported_module_files(
                &current_path,
                &current_source,
                &self.open_files,
            ) {
                if visited.contains(&path) {
                    continue;
                }
                if let Some(text) = self.open_files.get(&path) {
                    queue.push_back((path, text.clone()));
                    continue;
                }
                if let Ok(text) = std::fs::read_to_string(&path) {
                    queue.push_back((path, text));
                }
            }
        }
        out
    }

    fn default_import_aliases_in_source(source_text: &str) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for line in source_text.lines() {
            let trimmed = line.trim();
            if !trimmed.starts_with("import ") || !trimmed.contains(" from ") {
                continue;
            }
            if trimmed.starts_with("import {") || trimmed.starts_with("import type {") {
                continue;
            }
            let Some(import_start) = trimmed.strip_prefix("import ") else {
                continue;
            };
            let Some((alias, rest)) = import_start.split_once(" from ") else {
                continue;
            };
            let alias = alias.trim();
            if alias.is_empty() || alias.contains(',') || alias.contains('{') {
                continue;
            }
            let source = rest.trim().trim_end_matches(';').trim();
            let source = source
                .trim_start_matches('"')
                .trim_end_matches('"')
                .trim_start_matches('\'')
                .trim_end_matches('\'')
                .to_string();
            if source.is_empty() {
                continue;
            }
            out.push((alias.to_string(), source));
        }
        out
    }

    fn infer_extends_import_source(source_text: &str) -> Option<String> {
        let extends_idx = source_text.find("extends ")?;
        let rest = &source_text[extends_idx + "extends ".len()..];
        let base_name: String = rest
            .chars()
            .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '$')
            .collect();
        if base_name.is_empty() {
            return None;
        }

        for line in source_text.lines() {
            if !line.contains("import {") || !line.contains(" from ") {
                continue;
            }
            let Some(open) = line.find('{') else {
                continue;
            };
            let Some(close_rel) = line[open + 1..].find('}') else {
                continue;
            };
            let close = open + 1 + close_rel;
            let imports = line[open + 1..close]
                .split(',')
                .map(str::trim)
                .collect::<Vec<_>>();
            if !imports.iter().any(|name| *name == base_name) {
                continue;
            }
            if let Some(source) = Self::extract_module_specifier_from_import_text(line) {
                return Some(source.to_string());
            }
        }
        None
    }

    fn find_named_import_line_for_module(
        source_text: &str,
        module_specifier: &str,
    ) -> Option<(usize, usize, Vec<String>)> {
        let mut offset = 0usize;
        for line in source_text.split_inclusive('\n') {
            let matches_module = line.contains(&format!("from \"{module_specifier}\""))
                || line.contains(&format!("from '{module_specifier}'"));
            if !matches_module || !line.contains("import {") {
                offset += line.len();
                continue;
            }
            let open = line.find('{')?;
            let close_rel = line[open + 1..].find('}')?;
            let close = open + 1 + close_rel;
            let names = line[open + 1..close]
                .split(',')
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(std::string::ToString::to_string)
                .collect::<Vec<_>>();
            return Some((offset, offset + line.len(), names));
        }
        None
    }

    fn has_side_effect_import_for_module(source_text: &str, module_specifier: &str) -> bool {
        let double_quoted = format!("import \"{module_specifier}\";");
        let single_quoted = format!("import '{module_specifier}';");
        source_text.lines().any(|line| {
            let trimmed = line.trim();
            trimmed == double_quoted || trimmed == single_quoted
        })
    }

    fn side_effect_import_modules(source_text: &str) -> Vec<String> {
        let mut out = Vec::new();
        for line in source_text.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("import \"")
                && let Some(module) = rest.strip_suffix("\";")
            {
                out.push(module.to_string());
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("import '")
                && let Some(module) = rest.strip_suffix("';")
            {
                out.push(module.to_string());
            }
        }
        out
    }

    fn import_insertion_offset_after_import_block(source_text: &str) -> usize {
        let mut offset = 0usize;
        let mut last_import_end = 0usize;
        for line in source_text.split_inclusive('\n') {
            let trimmed = line.trim_start();
            if trimmed.starts_with("import ") {
                offset += line.len();
                last_import_end = offset;
                continue;
            }
            break;
        }
        last_import_end
    }

    pub(super) fn class_member_snippet_items(
        &self,
        provider: &Completions<'_>,
        root: tsz::parser::NodeIndex,
        position: tsz::lsp::position::Position,
        file_name: &str,
        source_text: &str,
        project_items: &[CompletionItem],
    ) -> Vec<CompletionItem> {
        let provider_candidates = provider.get_class_member_snippet_candidates(root, position);
        let fallback_candidates =
            self.class_member_snippet_fallback_candidates(file_name, position);
        let mut candidates =
            Self::merge_class_member_snippet_candidates(provider_candidates, fallback_candidates);
        let synthesized_candidates =
            Self::synthesized_class_member_snippet_candidates_from_project_items(project_items);
        candidates =
            Self::merge_class_member_snippet_candidates(candidates, synthesized_candidates);
        let underscored_type_names =
            self.class_member_snippet_underscored_type_names(file_name, source_text, project_items);

        let mut items = Vec::new();
        for candidate in candidates {
            let Some(insert_text) = Self::class_member_snippet_insert_text(&candidate) else {
                continue;
            };
            let insert_text = Self::normalize_property_snippet_type_alias_names(
                &insert_text,
                &underscored_type_names,
            );
            let edits = Self::class_member_snippet_additional_edits(
                &insert_text,
                &candidate.label,
                project_items,
            );
            let mut item = CompletionItem::new(candidate.label.clone(), candidate.kind)
                .with_sort_text(tsz::lsp::completions::sort_priority::LOCATION_PRIORITY)
                .with_insert_text(insert_text)
                .as_snippet()
                .with_has_action()
                .with_source("ClassMemberSnippet/".to_string());
            if let Some(detail) = candidate.detail.as_ref() {
                item = item.with_detail(detail.clone());
            }
            if let Some(mods) = candidate.kind_modifiers.as_ref() {
                item = item.with_kind_modifiers(mods.clone());
            }
            if !edits.is_empty() {
                item = item.with_additional_edits(edits);
            }
            items.push(item);
        }
        items
    }

    fn class_member_snippet_underscored_type_names(
        &self,
        file_name: &str,
        source_text: &str,
        project_items: &[CompletionItem],
    ) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        for item in project_items {
            if item.has_action && item.label.ends_with('_') {
                out.insert(item.label.clone());
            }
        }
        let mut queue =
            std::collections::VecDeque::from([(file_name.to_string(), source_text.to_string())]);
        let mut visited = BTreeSet::new();
        while let Some((current_path, current_source)) = queue.pop_front() {
            if !visited.insert(current_path.clone()) {
                continue;
            }
            Self::collect_underscored_type_names_from_source(&current_source, &mut out);
            for path in Self::resolve_imported_module_files(
                &current_path,
                &current_source,
                &self.open_files,
            ) {
                if visited.contains(&path) {
                    continue;
                }
                if let Some(text) = self.open_files.get(&path) {
                    queue.push_back((path, text.clone()));
                    continue;
                }
                if let Ok(text) = std::fs::read_to_string(&path) {
                    queue.push_back((path, text));
                }
            }
        }
        out
    }

    fn collect_underscored_type_names_from_source(source_text: &str, out: &mut BTreeSet<String>) {
        let mut token = String::new();
        for ch in source_text.chars() {
            if ch == '_' || ch == '$' || ch.is_ascii_alphanumeric() {
                token.push(ch);
                continue;
            }
            if token.ends_with('_') {
                out.insert(token.clone());
            }
            token.clear();
        }
        if token.ends_with('_') {
            out.insert(token);
        }
    }

    fn normalize_property_snippet_type_alias_names(
        insert_text: &str,
        underscored: &BTreeSet<String>,
    ) -> String {
        if !insert_text.ends_with(';') || insert_text.contains('{') {
            return insert_text.to_string();
        }
        let Some(colon_idx) = insert_text.find(':') else {
            return insert_text.to_string();
        };
        if underscored.is_empty() {
            return insert_text.to_string();
        }

        let mut out = String::with_capacity(insert_text.len() + 16);
        out.push_str(&insert_text[..=colon_idx]);
        let rhs = &insert_text[colon_idx + 1..];
        let mut current = String::new();
        let flush = |current: &mut String, out: &mut String, underscored: &BTreeSet<String>| {
            if current.is_empty() {
                return;
            }
            let token = std::mem::take(current);
            if !token.ends_with('_') {
                let candidate = format!("{token}_");
                if underscored.contains(&candidate) {
                    out.push_str(&candidate);
                    return;
                }
            }
            out.push_str(&token);
        };
        for ch in rhs.chars() {
            if ch == '_' || ch == '$' || ch.is_ascii_alphanumeric() {
                current.push(ch);
            } else {
                flush(&mut current, &mut out, underscored);
                out.push(ch);
            }
        }
        flush(&mut current, &mut out, underscored);
        out
    }

    fn merge_class_member_snippet_candidates(
        mut provider_candidates: Vec<CompletionItem>,
        fallback_candidates: Vec<CompletionItem>,
    ) -> Vec<CompletionItem> {
        for fallback in fallback_candidates {
            if let Some(idx) = provider_candidates
                .iter()
                .position(|candidate| candidate.label == fallback.label)
            {
                let existing_ok =
                    Self::class_member_snippet_insert_text(&provider_candidates[idx]).is_some();
                let fallback_ok = Self::class_member_snippet_insert_text(&fallback).is_some();
                if !existing_ok && fallback_ok {
                    provider_candidates[idx] = fallback;
                }
                continue;
            }
            provider_candidates.push(fallback);
        }
        provider_candidates
    }

    fn synthesized_class_member_snippet_candidates_from_project_items(
        project_items: &[CompletionItem],
    ) -> Vec<CompletionItem> {
        let mut out = Vec::new();
        let mut seen = BTreeSet::new();
        for item in project_items {
            if !item.has_action || item.source.is_none() {
                continue;
            }
            let Some(first) = item.label.chars().next() else {
                continue;
            };
            if !first.is_ascii_lowercase() {
                continue;
            }
            let kind = match item.kind {
                tsz::lsp::completions::CompletionItemKind::Method
                | tsz::lsp::completions::CompletionItemKind::Function => {
                    tsz::lsp::completions::CompletionItemKind::Method
                }
                _ => tsz::lsp::completions::CompletionItemKind::Property,
            };
            let mut candidate = CompletionItem::new(item.label.clone(), kind);
            if let Some(detail) = item.detail.as_ref() {
                candidate = candidate.with_detail(detail.clone());
            }
            if let Some(mods) = item.kind_modifiers.as_ref() {
                candidate = candidate.with_kind_modifiers(mods.clone());
            }
            if Self::class_member_snippet_insert_text(&candidate).is_none() {
                continue;
            }
            if seen.insert(candidate.label.clone()) {
                out.push(candidate);
            }
        }
        out
    }

    fn class_member_snippet_fallback_candidates(
        &self,
        file_name: &str,
        position: tsz::lsp::position::Position,
    ) -> Vec<CompletionItem> {
        let Some((arena, _binder, _root, source_text)) = self.parse_and_bind_file(file_name) else {
            return Vec::new();
        };
        let line_map = LineMap::build(&source_text);
        let Some(offset) = line_map.position_to_offset(position, &source_text) else {
            return Vec::new();
        };
        let prefix = Self::identifier_prefix_before_offset(&source_text, offset as usize);
        let mut scan_paths =
            Self::fallback_class_member_scan_paths(&self.open_files, &self.external_project_files);
        for resolved_path in
            Self::resolve_imported_module_files(file_name, &source_text, &self.open_files)
        {
            if !scan_paths.iter().any(|path| path == &resolved_path) {
                scan_paths.push(resolved_path);
            }
        }

        let mut out = Vec::new();
        let mut base_names = Self::enclosing_class_extends_names(&arena, offset);
        if base_names.is_empty() {
            self.collect_fallback_base_members_recursive(
                "",
                &scan_paths,
                &mut BTreeSet::new(),
                &mut out,
            );
        } else {
            let mut visited = BTreeSet::new();
            for base_name in base_names.drain(..) {
                self.collect_fallback_base_members_recursive(
                    &base_name,
                    &scan_paths,
                    &mut visited,
                    &mut out,
                );
            }
        }
        if !prefix.is_empty() {
            out.retain(|item| item.label.starts_with(&prefix));
        }
        out.sort_by(|a, b| a.label.cmp(&b.label));
        out
    }

    fn enclosing_class_extends_names(
        arena: &tsz::parser::node::NodeArena,
        offset: u32,
    ) -> Vec<String> {
        let mut best_class: Option<(u32, u32, &tsz::parser::node::ClassData)> = None;
        for node in &arena.nodes {
            if node.kind != tsz::parser::syntax_kind_ext::CLASS_DECLARATION
                && node.kind != tsz::parser::syntax_kind_ext::CLASS_EXPRESSION
            {
                continue;
            }
            if node.pos > offset || node.end < offset {
                continue;
            }
            let Some(class_data) = arena.get_class(node) else {
                continue;
            };
            match best_class {
                Some((best_start, best_end, _))
                    if (best_end - best_start) <= (node.end - node.pos) =>
                {
                    continue;
                }
                _ => best_class = Some((node.pos, node.end, class_data)),
            }
        }

        best_class
            .map(|(_, _, class_data)| Self::class_extends_names(arena, class_data))
            .unwrap_or_default()
    }

    fn identifier_prefix_before_offset(source_text: &str, offset: usize) -> String {
        let bytes = source_text.as_bytes();
        let mut i = offset.min(bytes.len());
        while i > 0 && bytes[i - 1].is_ascii_whitespace() {
            i -= 1;
        }
        let end = i;
        while i > 0 {
            let ch = bytes[i - 1] as char;
            if ch == '_' || ch == '$' || ch.is_ascii_alphanumeric() {
                i -= 1;
            } else {
                break;
            }
        }
        if i < end {
            source_text[i..end].to_string()
        } else {
            String::new()
        }
    }

    fn class_extends_names(
        arena: &tsz::parser::node::NodeArena,
        class_data: &tsz::parser::node::ClassData,
    ) -> Vec<String> {
        let mut names = Vec::new();
        let Some(clauses) = class_data.heritage_clauses.as_ref() else {
            return names;
        };
        for &clause_idx in &clauses.nodes {
            let Some(clause_node) = arena.get(clause_idx) else {
                continue;
            };
            let Some(heritage) = arena.get_heritage_clause(clause_node) else {
                continue;
            };
            if heritage.token != tsz_scanner::SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }
            for &type_idx in &heritage.types.nodes {
                let Some(type_node) = arena.get(type_idx) else {
                    continue;
                };
                let Some(expr) = arena.get_expr_type_args(type_node).map(|t| t.expression) else {
                    continue;
                };
                if let Some(name) = arena.get_identifier_text(expr) {
                    names.push(name.to_string());
                }
            }
        }
        names
    }

    fn node_text(source_text: &str, node: &tsz::parser::node::Node) -> String {
        let start = node.pos.min(source_text.len() as u32) as usize;
        let end = node.end.min(source_text.len() as u32) as usize;
        if start >= end {
            String::new()
        } else {
            source_text[start..end].trim().to_string()
        }
    }

    fn collect_fallback_base_members_recursive(
        &self,
        class_name: &str,
        scan_paths: &[String],
        visited: &mut BTreeSet<String>,
        out: &mut Vec<CompletionItem>,
    ) {
        if !class_name.is_empty() && !visited.insert(class_name.to_string()) {
            return;
        }

        for path in scan_paths {
            let Some((arena, _binder, _root, source_text)) = self.parse_and_bind_file(path) else {
                continue;
            };
            for node in &arena.nodes {
                if node.kind != tsz::parser::syntax_kind_ext::CLASS_DECLARATION
                    && node.kind != tsz::parser::syntax_kind_ext::CLASS_EXPRESSION
                {
                    continue;
                }
                let Some(class_data) = arena.get_class(node) else {
                    continue;
                };
                let Some(name) = arena.get_identifier_text(class_data.name) else {
                    continue;
                };
                if !class_name.is_empty() && name != class_name {
                    continue;
                }

                for &member_idx in &class_data.members.nodes {
                    let Some(member_node) = arena.get(member_idx) else {
                        continue;
                    };
                    match member_node.kind {
                        k if k == tsz::parser::syntax_kind_ext::PROPERTY_DECLARATION => {
                            let Some(prop) = arena.get_property_decl(member_node) else {
                                continue;
                            };
                            let Some(member_name) = arena.get_identifier_text(prop.name) else {
                                continue;
                            };
                            let detail = if prop.type_annotation.is_some() {
                                arena
                                    .get(prop.type_annotation)
                                    .map(|n| Self::node_text(&source_text, n))
                                    .unwrap_or_else(|| "any".to_string())
                            } else {
                                "any".to_string()
                            };
                            let item = CompletionItem::new(
                                member_name.to_string(),
                                tsz::lsp::completions::CompletionItemKind::Property,
                            )
                            .with_detail(detail);
                            if !out.iter().any(|i| i.label == item.label) {
                                out.push(item);
                            }
                        }
                        k if k == tsz::parser::syntax_kind_ext::GET_ACCESSOR => {
                            let Some(accessor) = arena.get_accessor(member_node) else {
                                continue;
                            };
                            let Some(member_name) = arena.get_identifier_text(accessor.name) else {
                                continue;
                            };
                            let detail = if accessor.type_annotation.is_some() {
                                arena
                                    .get(accessor.type_annotation)
                                    .map(|n| Self::node_text(&source_text, n))
                                    .unwrap_or_else(|| "any".to_string())
                            } else {
                                "any".to_string()
                            };
                            let item = CompletionItem::new(
                                member_name.to_string(),
                                tsz::lsp::completions::CompletionItemKind::Property,
                            )
                            .with_detail(detail)
                            .with_kind_modifiers("getter".to_string());
                            if !out.iter().any(|i| i.label == item.label) {
                                out.push(item);
                            }
                        }
                        k if k == tsz::parser::syntax_kind_ext::METHOD_DECLARATION => {
                            let Some(method) = arena.get_method_decl(member_node) else {
                                continue;
                            };
                            let Some(member_name) = arena.get_identifier_text(method.name) else {
                                continue;
                            };
                            let mut params = Vec::new();
                            for &param_idx in &method.parameters.nodes {
                                let Some(param_node) = arena.get(param_idx) else {
                                    continue;
                                };
                                let text = Self::node_text(&source_text, param_node);
                                if !text.is_empty() {
                                    params.push(text);
                                }
                            }
                            let return_type = if method.type_annotation.is_some() {
                                arena
                                    .get(method.type_annotation)
                                    .map(|n| Self::node_text(&source_text, n))
                                    .unwrap_or_else(|| "void".to_string())
                            } else {
                                "void".to_string()
                            };
                            let detail = format!("({}) => {}", params.join(", "), return_type);
                            let item = CompletionItem::new(
                                member_name.to_string(),
                                tsz::lsp::completions::CompletionItemKind::Method,
                            )
                            .with_detail(detail);
                            if !out.iter().any(|i| i.label == item.label) {
                                out.push(item);
                            }
                        }
                        _ => {}
                    }
                }

                if !class_name.is_empty() {
                    for base in Self::class_extends_names(&arena, class_data) {
                        self.collect_fallback_base_members_recursive(
                            &base, scan_paths, visited, out,
                        );
                    }
                }
            }
        }
    }

    pub(super) fn fallback_class_member_scan_paths(
        open_files: &rustc_hash::FxHashMap<String, String>,
        external_project_files: &rustc_hash::FxHashMap<String, Vec<String>>,
    ) -> Vec<String> {
        let mut paths: Vec<String> = open_files.keys().cloned().collect();
        for project_files in external_project_files.values() {
            for file_path in project_files {
                if !paths.iter().any(|path| path == file_path) {
                    paths.push(file_path.clone());
                }
            }
        }
        paths
    }

    pub(super) fn resolve_imported_module_files(
        file_name: &str,
        source_text: &str,
        open_files: &rustc_hash::FxHashMap<String, String>,
    ) -> Vec<String> {
        let mut out = Vec::new();
        let mut seen = BTreeSet::new();
        let Some(file_parent) = std::path::Path::new(file_name).parent() else {
            return out;
        };
        for specifier in Self::module_specifiers_in_source(source_text) {
            if specifier.is_empty() {
                continue;
            }
            let resolved =
                Self::resolve_module_specifier_for_fallback(file_parent, &specifier, open_files);
            for path in resolved {
                if seen.insert(path.clone()) {
                    out.push(path);
                }
            }
        }
        out
    }

    fn module_specifiers_in_source(source_text: &str) -> Vec<String> {
        const MARKERS: [&str; 4] = [" from \"", " from '", "import \"", "import '"];
        let mut specifiers = Vec::new();
        for marker in MARKERS {
            let Some(quote) = marker.chars().last() else {
                continue;
            };
            let mut start = 0;
            while let Some(idx) = source_text[start..].find(marker) {
                let spec_start = start + idx + marker.len();
                let rest = &source_text[spec_start..];
                let Some(end) = rest.find(quote) else {
                    break;
                };
                specifiers.push(rest[..end].to_string());
                start = spec_start + end + 1;
            }
        }
        specifiers
    }

    fn resolve_module_specifier_for_fallback(
        file_parent: &Path,
        specifier: &str,
        open_files: &rustc_hash::FxHashMap<String, String>,
    ) -> Vec<String> {
        let mut candidates = Vec::new();
        let mut push_candidate = |path: std::path::PathBuf| {
            candidates.push(path.to_string_lossy().to_string());
        };
        if specifier.starts_with('.') || specifier.starts_with('/') {
            let base = if specifier.starts_with('/') {
                Path::new(specifier).to_path_buf()
            } else {
                file_parent.join(specifier)
            };
            let normalized = Self::normalize_fallback_path(base);
            Self::push_module_file_candidates(&mut push_candidate, &normalized);
        } else {
            let mut current = Some(file_parent.to_path_buf());
            while let Some(dir) = current {
                let base = dir.join("node_modules").join(specifier);
                let normalized = Self::normalize_fallback_path(base);
                Self::push_module_file_candidates(&mut push_candidate, &normalized);
                current = dir.parent().map(std::path::Path::to_path_buf);
            }
        }

        candidates
            .into_iter()
            .filter(|candidate| {
                open_files.contains_key(candidate) || std::path::Path::new(candidate).exists()
            })
            .collect()
    }

    fn normalize_fallback_path(path: std::path::PathBuf) -> std::path::PathBuf {
        use std::path::Component;

        let mut normalized = std::path::PathBuf::new();
        for component in path.components() {
            match component {
                Component::CurDir => {}
                Component::ParentDir => {
                    normalized.pop();
                }
                Component::Normal(part) => normalized.push(part),
                Component::RootDir => normalized.push(Component::RootDir.as_os_str()),
                Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            }
        }
        normalized
    }

    fn push_module_file_candidates<F>(push: &mut F, base: &Path)
    where
        F: FnMut(std::path::PathBuf),
    {
        push(base.to_path_buf());
        let ext = base.extension().and_then(std::ffi::OsStr::to_str);
        if ext.is_some() {
            // Handle TS sources imported via runtime extensions (e.g. "./node.js" -> "./node.ts").
            if matches!(ext, Some("js" | "jsx" | "mjs" | "cjs")) {
                let stem = base.with_extension("");
                for ts_ext in [".d.ts", ".ts", ".tsx", ".mts", ".cts"] {
                    let mut with_ext = stem.as_os_str().to_os_string();
                    with_ext.push(ts_ext);
                    push(std::path::PathBuf::from(with_ext));
                }
                push(stem);
            }
            return;
        }

        for ext in [".d.ts", ".ts", ".tsx", ".js", ".jsx", ".mts", ".cts"] {
            let mut with_ext = base.as_os_str().to_os_string();
            with_ext.push(ext);
            push(std::path::PathBuf::from(with_ext));
        }
        for leaf in [
            "index.d.ts",
            "index.ts",
            "index.tsx",
            "index.js",
            "index.jsx",
            "index.mts",
            "index.cts",
        ] {
            push(base.join(leaf));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustc_hash::FxHashMap;
    use tsz::lsp::completions::CompletionItemKind;

    #[test]
    fn prioritize_class_member_snippet_items_keeps_snippet_variant_for_same_label() {
        let items = vec![
            CompletionItem::new("container".to_string(), CompletionItemKind::Property)
                .with_source("@sapphire/pieces".to_string()),
            CompletionItem::new("container".to_string(), CompletionItemKind::Property)
                .with_source("ClassMemberSnippet/".to_string())
                .with_has_action()
                .as_snippet()
                .with_insert_text("container: Container;".to_string()),
            CompletionItem::new("other".to_string(), CompletionItemKind::Property),
        ];

        let prioritized = Server::prioritize_class_member_snippet_items(items);

        let container_sources: Vec<Option<&str>> = prioritized
            .iter()
            .filter(|item| item.label == "container")
            .map(|item| item.source.as_deref())
            .collect();
        assert_eq!(container_sources, vec![Some("ClassMemberSnippet/")]);
        assert!(
            prioritized.iter().any(|item| item.label == "other"),
            "non-colliding entries should be preserved"
        );
    }

    #[test]
    fn normalize_class_member_snippet_items_sets_snippet_flags_and_insert_text() {
        let items = vec![
            CompletionItem::new("container".to_string(), CompletionItemKind::Property)
                .with_detail("Container".to_string())
                .with_source("ClassMemberSnippet/".to_string()),
        ];

        let normalized = Server::normalize_class_member_snippet_items(items);
        let item = normalized
            .first()
            .expect("expected normalized class member snippet item");

        assert!(item.has_action);
        assert!(item.is_snippet);
        assert_eq!(item.insert_text.as_deref(), Some("container: Container;"));
    }

    #[test]
    fn merge_class_member_snippet_candidates_prefers_fallback_when_primary_is_not_snippet_ready() {
        let provider = vec![
            CompletionItem::new(
                "execActionWithCount".to_string(),
                CompletionItemKind::Method,
            )
            .with_detail("(count: number): void".to_string()),
        ];
        let fallback = vec![
            CompletionItem::new(
                "execActionWithCount".to_string(),
                CompletionItemKind::Method,
            )
            .with_detail("(count: number) => void".to_string()),
        ];

        let merged = Server::merge_class_member_snippet_candidates(provider, fallback);
        assert_eq!(merged.len(), 1);
        assert_eq!(
            merged[0].detail.as_deref(),
            Some("(count: number) => void"),
            "fallback candidate should replace non-snippet-ready primary candidate"
        );
    }

    #[test]
    fn synthesized_class_member_snippet_candidates_uses_auto_import_items_when_primary_empty() {
        let project_items = vec![
            CompletionItem::new(
                "execActionWithCount".to_string(),
                CompletionItemKind::Function,
            )
            .with_has_action()
            .with_source("@pkg/mod".to_string())
            .with_detail("(count: number) => void".to_string()),
            CompletionItem::new("Container".to_string(), CompletionItemKind::Class)
                .with_has_action()
                .with_source("@pkg/mod".to_string()),
        ];

        let synthesized =
            Server::synthesized_class_member_snippet_candidates_from_project_items(&project_items);
        assert_eq!(synthesized.len(), 1);
        assert_eq!(synthesized[0].label, "execActionWithCount");
        assert_eq!(synthesized[0].kind, CompletionItemKind::Method);
    }

    #[test]
    fn class_member_snippet_synthesized_text_changes_updates_existing_named_import() {
        let source_text = "import { Piece } from \"@sapphire/pieces\";\nclass C extends Piece {}\n";
        let project_items = vec![
            CompletionItem::new("Container".to_string(), CompletionItemKind::Interface)
                .with_has_action()
                .with_source("@sapphire/pieces".to_string()),
        ];

        let changes = Server::class_member_snippet_synthesized_text_changes(
            source_text,
            "container: Container;",
            "container",
            &project_items,
        );

        assert_eq!(changes.len(), 1);
        let change = &changes[0];
        assert_eq!(
            change.get("newText").and_then(serde_json::Value::as_str),
            Some("import { Container, Piece } from \"@sapphire/pieces\";\n")
        );
    }

    #[test]
    fn class_member_snippet_synthesized_text_changes_inserts_after_import_block_for_side_effect_import()
     {
        let source_text = "import \"@sapphire/pieces\";\nimport { Command } from \"@sapphire/framework\";\nclass PingCommand extends Command {\n}\n";
        let project_items = vec![
            CompletionItem::new("Container".to_string(), CompletionItemKind::Interface)
                .with_has_action()
                .with_source("@sapphire/pieces".to_string()),
        ];

        let changes = Server::class_member_snippet_synthesized_text_changes(
            source_text,
            "get container(): Container {\n}",
            "container",
            &project_items,
        );

        assert_eq!(changes.len(), 1);
        let change = &changes[0];
        let expected_start = source_text
            .find("class PingCommand")
            .expect("expected class declaration");
        assert_eq!(
            change
                .get("span")
                .and_then(|span| span.get("start"))
                .and_then(serde_json::Value::as_u64),
            Some(expected_start as u64)
        );
        assert_eq!(
            change.get("newText").and_then(serde_json::Value::as_str),
            Some("import { Container } from \"@sapphire/pieces\";\n")
        );
    }

    #[test]
    fn class_member_snippet_additional_edits_rewrite_default_import_for_underscored_alias() {
        let project_items = vec![
            CompletionItem::new("Document".to_string(), CompletionItemKind::Class)
                .with_has_action()
                .with_source("./document.js".to_string())
                .with_additional_edits(vec![tsz::lsp::rename::TextEdit::new(
                    tsz::lsp::position::Range::new(
                        tsz::lsp::position::Position::new(0, 0),
                        tsz::lsp::position::Position::new(0, 0),
                    ),
                    "import Document from \"./document.js\";\n".to_string(),
                )]),
        ];

        let edits = Server::class_member_snippet_additional_edits(
            "parent: Document_ | undefined;",
            "parent",
            &project_items,
        );
        assert_eq!(edits.len(), 1);
        assert_eq!(
            edits[0].new_text,
            "import Document_ from \"./document.js\";\n"
        );
    }

    #[test]

    fn fallback_class_member_scan_paths_include_external_project_file_paths() {
        let mut open_files = FxHashMap::default();
        open_files.insert("/src/current.ts".to_string(), "class C {}".to_string());
        let mut external_project_files = FxHashMap::default();
        external_project_files.insert(
            "project:/virtual".to_string(),
            vec!["/src/base.ts".to_string(), "/src/current.ts".to_string()],
        );

        let paths = Server::fallback_class_member_scan_paths(&open_files, &external_project_files);

        assert!(paths.iter().any(|path| path == "/src/current.ts"));
        assert!(paths.iter().any(|path| path == "/src/base.ts"));
        assert!(
            !paths.iter().any(|path| path == "project:/virtual"),
            "project names should not be treated as source file paths"
        );
    }

    #[test]
    fn resolve_imported_module_files_finds_relative_and_package_targets_from_open_files() {
        let mut open_files = FxHashMap::default();
        open_files.insert(
            "/workspace/src/base.ts".to_string(),
            "export class Base {}".to_string(),
        );
        open_files.insert(
            "/workspace/node_modules/@scope/pkg/index.d.ts".to_string(),
            "export declare class Piece {}".to_string(),
        );
        let source = "import { Base } from \"./base\";\nimport { Piece } from \"@scope/pkg\";\n";

        let resolved =
            Server::resolve_imported_module_files("/workspace/src/current.ts", source, &open_files);

        assert!(
            resolved.iter().any(|path| path == "/workspace/src/base.ts"),
            "expected relative module candidate to resolve from open files: {resolved:?}"
        );
        assert!(
            resolved
                .iter()
                .any(|path| path == "/workspace/node_modules/@scope/pkg/index.d.ts"),
            "expected package module candidate to resolve from open files: {resolved:?}"
        );
    }

    #[test]
    fn resolve_imported_module_files_maps_js_specifier_to_ts_source() {
        let mut open_files = FxHashMap::default();
        open_files.insert(
            "/workspace/src/node.ts".to_string(),
            "export class Node {}".to_string(),
        );
        let source = "import Node from \"./node.js\";\n";

        let resolved = Server::resolve_imported_module_files(
            "/workspace/src/container.ts",
            source,
            &open_files,
        );

        assert!(
            resolved.iter().any(|path| path == "/workspace/src/node.ts"),
            "expected explicit .js import to resolve sibling TypeScript source: {resolved:?}"
        );
    }
}
