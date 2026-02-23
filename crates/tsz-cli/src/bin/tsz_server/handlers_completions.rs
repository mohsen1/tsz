//! Completions and signature help handlers for tsz-server.

use super::{Server, TsServerRequest, TsServerResponse};
use rustc_hash::FxHashSet;
use std::cmp::Ordering;
use std::path::Path;
use tsz::lsp::Project;
use tsz::lsp::completions::{CompletionItem, CompletionItemKind, Completions, sort_priority};
use tsz::lsp::position::LineMap;
use tsz::lsp::signature_help::SignatureHelpProvider;
use tsz_solver::TypeInterner;

impl Server {
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
        let item = CompletionItem::new(label.to_string(), kind)
            .with_has_action()
            .with_sort_text(sort_priority::AUTO_IMPORT)
            .with_source(module_specifier.to_string())
            .with_source_display(module_specifier.to_string())
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
            if Self::is_identifier(name) && seen.insert(name.to_string()) {
                out.push(name.to_string());
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
                    if Self::is_identifier(head) && seen.insert(head.to_string()) {
                        members.push(head.to_string());
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
            let quote_start = after_imports.find(quote)?;
            let rest = &after_imports[quote_start + 1..];
            let quote_end = rest.find(quote)?;
            let module_specifier = &rest[..quote_end];
            return Some((module_specifier, imports));
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
            let is_class_member_snippet = item.source.as_deref() == Some("ClassMemberSnippet/");
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
            let completion_result = provider.get_completion_result(root, position);
            let provider_items = completion_result
                .as_ref()
                .map(|result| result.entries.clone())
                .unwrap_or_default();
            let project_items = self.project_completion_items(&file, position, Some(preferences));
            let is_member_completion = completion_result
                .as_ref()
                .is_some_and(|result| result.is_member_completion);
            let include_class_member_snippets = Self::bool_pref_or_default(
                Some(preferences),
                "includeCompletionsWithClassMemberSnippets",
                self.include_completions_with_class_member_snippets,
            );
            let snippet_items = if include_class_member_snippets && !is_member_completion {
                self.class_member_snippet_items(
                    &provider,
                    root,
                    position,
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
                Self::merge_non_member_completion_items(provider_items, project_items.clone())
            };
            let mut items = items;
            if !snippet_items.is_empty() {
                items = Self::merge_non_member_completion_items(items, snippet_items.clone());
                items = Self::prioritize_class_member_snippet_items(items);
                items = Self::normalize_class_member_snippet_items(items);
            }
            Self::sort_tsserver_completion_items(&mut items);
            let items = Self::prune_deeper_auto_import_duplicates(items);
            let mut items =
                self.maybe_add_verbatim_commonjs_auto_import_items(&file, &source_text, items);
            Self::sort_tsserver_completion_items(&mut items);
            let items = Self::prune_deeper_auto_import_duplicates(items);

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
            let completion_result = provider.get_completion_result(root, position);
            let provider_items = completion_result
                .as_ref()
                .map(|result| result.entries.clone())
                .unwrap_or_default();
            let mut project_items =
                self.project_completion_items(file, position, Some(preferences));
            let is_member_completion = completion_result
                .as_ref()
                .is_some_and(|result| result.is_member_completion);
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
            if requested_class_member_snippet && project_items.is_empty() {
                let forced_auto_import_prefs =
                    serde_json::json!({ "includeCompletionsForModuleExports": true });
                project_items =
                    self.project_completion_items(file, position, Some(&forced_auto_import_prefs));
            }
            let snippet_items = if (include_class_member_snippets || requested_class_member_snippet)
                && (!is_member_completion || requested_class_member_snippet)
            {
                self.class_member_snippet_items(
                    &provider,
                    root,
                    position,
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
                    let mut item = items.iter().find(|i| {
                        if i.label != name {
                            return false;
                        }
                        if let Some(source) = requested_source.as_deref() {
                            i.source.as_deref() == Some(source)
                        } else {
                            true
                        }
                    });
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
                                    &file,
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
                                        &file,
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
                                for idx in 0..text_changes.len() {
                                    let Some(new_text) = text_changes[idx]
                                        .get("newText")
                                        .and_then(serde_json::Value::as_str)
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

                                    let start = text_changes[idx]
                                        .get("span")
                                        .and_then(|span| span.get("start"))
                                        .and_then(serde_json::Value::as_u64)
                                        .map(|n| n as u32)
                                        .unwrap_or(0);
                                    let length = text_changes[idx]
                                        .get("span")
                                        .and_then(|span| span.get("length"))
                                        .and_then(serde_json::Value::as_u64)
                                        .map(|n| n as u32)
                                        .unwrap_or(0);
                                    if length != 0 || start != existing_start {
                                        continue;
                                    }

                                    if let Some(change_obj) = text_changes[idx].as_object_mut() {
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
                                    &file,
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

#[cfg(test)]
mod tests {
    use super::*;
    use rustc_hash::FxHashMap;
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
